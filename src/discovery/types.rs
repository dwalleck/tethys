//! Owned records at the language-neutral workspace discovery seam.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::types::path_wire;
use crate::{CrateInfo, Error, Result};

/// A canonical workspace-relative C# project-file identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectKey(pub(crate) String);

impl ProjectKey {
    /// Return the workspace-relative project path, using `/` separators.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An opaque project/framework/context identity, never an assembly name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EvaluationUnitKey(pub(crate) String);

impl EvaluationUnitKey {
    /// Return the stable serialized identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Explicit, recorded evaluation-import policy; this is not target execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImportToleranceProfile {
    /// Preserve ordinary `MSBuild` imports.
    #[default]
    Strict,
    /// Supply an empty `VSToolsPath` unless the caller explicitly overrides it.
    BlankVsToolsPath,
}

/// Requested context shared by every project in one discovery invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationContext {
    /// Requested `Configuration`; absent means project/`MSBuild` defaults.
    pub configuration: Option<String>,
    /// Requested `Platform`; absent means project/`MSBuild` defaults.
    pub platform: Option<String>,
    /// Requested `RuntimeIdentifier`; absent means project/`MSBuild` defaults.
    pub runtime_identifier: Option<String>,
    /// Explicit global properties; these take precedence over convenience selectors.
    pub global_properties: BTreeMap<String, String>,
    /// Recorded import-tolerance policy.
    pub import_profile: ImportToleranceProfile,
}

impl EvaluationContext {
    pub(crate) fn effective_globals(&self) -> BTreeMap<String, String> {
        let mut values = BTreeMap::new();
        if self.import_profile == ImportToleranceProfile::BlankVsToolsPath {
            values.insert("VSToolsPath".to_owned(), String::new());
        }
        for (name, value) in [
            ("Configuration", &self.configuration),
            ("Platform", &self.platform),
            ("RuntimeIdentifier", &self.runtime_identifier),
        ] {
            if let Some(value) = value {
                values.insert(name.to_owned(), value.clone());
            }
        }
        for (name, value) in &self.global_properties {
            if let Some(default) = [
                "Configuration",
                "Platform",
                "RuntimeIdentifier",
                "VSToolsPath",
            ]
            .into_iter()
            .find(|candidate| candidate.eq_ignore_ascii_case(name))
            {
                values.remove(default);
            }
            values.insert(name.clone(), value.clone());
        }
        values
    }
}

/// Ambient process settings that evaluation depends on but cannot receive as arguments.
///
/// `MSBuild` reads environment variables as properties, and a selected host's native
/// loader reads some of them as code-injection points. Both are therefore evaluation
/// cache identity. Discovery takes the environment as an explicit value rather than
/// reading the calling process's ambient state, so the environment that is
/// fingerprinted is exactly the environment the evaluation host is given.
#[derive(Clone, PartialEq, Eq)]
pub struct EvaluationEnvironment {
    variables: BTreeMap<OsString, OsString>,
}

/// Values may hold credentials, so only the binding count is rendered.
impl fmt::Debug for EvaluationEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EvaluationEnvironment")
            .field("variables", &self.variables.len())
            .finish()
    }
}

impl EvaluationEnvironment {
    /// Settings that let a selected host load arbitrary code, disqualifying reuse.
    ///
    /// Diagnostic-only `COREHOST_TRACE` / `DOTNET_HOST_TRACE` inputs deliberately do
    /// not appear here: unlike these injection points, tracing does not load arbitrary
    /// code. `PATH` is likewise absent. It is the Windows loader's search path, but it
    /// is also required for ordinary operation and is already an explicit input to host
    /// selection, so disqualifying on it would make every Windows evaluation ineligible.
    /// The native-loader entries cover glibc's `LD_PRELOAD`, `LD_AUDIT` and
    /// `LD_LIBRARY_PATH`, and dyld's `DYLD_INSERT_LIBRARIES`, `DYLD_LIBRARY_PATH`,
    /// `DYLD_FALLBACK_LIBRARY_PATH`, `DYLD_FRAMEWORK_PATH` and
    /// `DYLD_FALLBACK_FRAMEWORK_PATH`. This finite list is not a sandbox or a complete
    /// model of arbitrary environment-dependent code.
    pub const RUNTIME_CODE_EXTENSIONS: &'static [&'static str] = &[
        "DOTNET_STARTUP_HOOKS",
        "DOTNET_ADDITIONAL_DEPS",
        "DOTNET_SHARED_STORE",
        "DOTNET_ENABLE_PROFILING",
        "DOTNET_PROFILER",
        "DOTNET_PROFILER_PATH",
        "DOTNET_PROFILER_PATH_32",
        "DOTNET_PROFILER_PATH_64",
        "DOTNET_PROFILER_PATH_ARM32",
        "DOTNET_PROFILER_PATH_ARM64",
        "CORECLR_ENABLE_PROFILING",
        "CORECLR_PROFILER",
        "CORECLR_PROFILER_PATH",
        "CORECLR_PROFILER_PATH_32",
        "CORECLR_PROFILER_PATH_64",
        "CORECLR_PROFILER_PATH_ARM32",
        "CORECLR_PROFILER_PATH_ARM64",
        "COR_ENABLE_PROFILING",
        "COR_PROFILER",
        "COR_PROFILER_PATH",
        "COR_PROFILER_PATH_32",
        "COR_PROFILER_PATH_64",
        "APPDOMAIN_MANAGER_ASM",
        "APPDOMAIN_MANAGER_TYPE",
        "LD_PRELOAD",
        "LD_AUDIT",
        "LD_LIBRARY_PATH",
        "DYLD_INSERT_LIBRARIES",
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "DYLD_FRAMEWORK_PATH",
        "DYLD_FALLBACK_FRAMEWORK_PATH",
    ];

    /// Capture the calling process's environment. This is the default for discovery.
    #[must_use]
    pub fn inherited() -> Self {
        Self {
            variables: std::env::vars_os().collect(),
        }
    }

    /// An environment that binds no variables at all.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            variables: BTreeMap::new(),
        }
    }

    /// Look up one binding using the platform's own environment-block name comparison.
    ///
    /// Windows compares names case-insensitively; Unix compares them exactly. This
    /// matches what a spawned evaluation host would itself observe.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&OsStr> {
        if let Some(value) = self.variables.get(OsStr::new(name)) {
            return Some(value);
        }
        if !cfg!(windows) {
            return None;
        }
        self.variables.iter().find_map(|(key, value)| {
            key.as_encoded_bytes()
                .eq_ignore_ascii_case(name.as_bytes())
                .then_some(value.as_os_str())
        })
    }

    /// Bind one variable, replacing any existing binding of the same platform name.
    pub fn insert(&mut self, name: impl Into<OsString>, value: impl Into<OsString>) {
        let name = name.into();
        self.remove_platform_name(&name);
        self.variables.insert(name, value.into());
    }

    /// Remove one variable, returning its previous value.
    pub fn remove(&mut self, name: &str) -> Option<OsString> {
        self.remove_platform_name(OsStr::new(name))
    }

    /// Bind one variable, consuming and returning the environment.
    #[must_use]
    pub fn with(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.insert(name, value);
        self
    }

    /// Remove several variables, consuming and returning the environment.
    ///
    /// Pass [`Self::RUNTIME_CODE_EXTENSIONS`] to build a caller environment that a
    /// harness's own loader settings cannot disqualify.
    #[must_use]
    pub fn without(mut self, names: &[&str]) -> Self {
        for name in names {
            self.remove(name);
        }
        self
    }

    /// Iterate every binding in a stable order, name before value.
    pub fn iter(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.variables
            .iter()
            .map(|(name, value)| (name.as_os_str(), value.as_os_str()))
    }

    /// Whether any binding lets a selected host load arbitrary code.
    pub(crate) fn loads_runtime_code(&self) -> bool {
        Self::RUNTIME_CODE_EXTENSIONS
            .iter()
            .any(|name| self.get(name).is_some_and(|value| !value.is_empty()))
    }

    /// Remove every spelling the platform considers the same name.
    fn remove_platform_name(&mut self, name: &OsStr) -> Option<OsString> {
        let mut removed = self.variables.remove(name);
        if cfg!(windows) {
            let recased: Vec<_> = self
                .variables
                .keys()
                .filter(|key| {
                    key.as_encoded_bytes()
                        .eq_ignore_ascii_case(name.as_encoded_bytes())
                })
                .cloned()
                .collect();
            for key in recased {
                removed = self.variables.remove(&key).or(removed);
            }
        }
        removed
    }
}

/// Whether eligible evaluations may be reused after input validation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiscoveryCachePolicy {
    /// Validate qualified cached inputs; otherwise evaluate again.
    #[default]
    Enabled,
    /// Force evaluation and do not emit reusable cache entries.
    Disabled,
}

/// Explicit authority granted for this discovery invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryGrants {
    /// Permission to execute project evaluation code.
    pub trust_msbuild: bool,
    /// Separate permission to use the repository's normal restore policy.
    pub allow_restore: bool,
}

/// Authority, context and installed-tool selection for discovery.
#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    /// Explicit permission to execute project evaluation code.
    pub trust_msbuild: bool,
    /// Separate permission to use the repository's normal restore policy.
    pub allow_restore: bool,
    /// Requested evaluation context.
    pub context: EvaluationContext,
    /// Optional selected installed `MSBuild` directory; never silently substituted.
    pub msbuild_path: Option<PathBuf>,
    /// Optional directory containing packaged `sdk/` and `framework/` companions.
    pub companion_directory: Option<PathBuf>,
    /// Per-process deadline, greater than zero and at most sixty seconds.
    pub timeout: Duration,
    /// Conservative evaluation-cache policy.
    pub cache_policy: DiscoveryCachePolicy,
    /// Ambient settings evaluation depends on; fingerprinted and given to the host.
    pub environment: EvaluationEnvironment,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            trust_msbuild: false,
            allow_restore: false,
            context: EvaluationContext::default(),
            msbuild_path: None,
            companion_directory: None,
            timeout: Duration::from_secs(60),
            cache_policy: DiscoveryCachePolicy::Enabled,
            environment: EvaluationEnvironment::inherited(),
        }
    }
}

/// Opaque reusable evaluation evidence; its contents are validated by the cache owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationCacheEntry {
    pub(crate) key: String,
    pub(crate) payload: String,
}

/// A validated invocation at the workspace discovery seam.
#[derive(Debug, Clone)]
pub struct DiscoveryRequest {
    pub(crate) workspace_root: PathBuf,
    pub(crate) options: DiscoveryOptions,
    pub(crate) cache: Vec<EvaluationCacheEntry>,
}

impl DiscoveryRequest {
    /// Canonicalize the workspace and validate invocation-level configuration.
    ///
    /// # Errors
    /// Returns an I/O error for an inaccessible root or a configuration error for
    /// a non-directory root, invalid timeout, or ambiguous global-property names.
    pub fn new(workspace_root: &Path, options: DiscoveryOptions) -> Result<Self> {
        let workspace_root = workspace_root.canonicalize()?;
        if !workspace_root.is_dir() {
            return Err(Error::Config(
                "discovery workspace must be a directory".into(),
            ));
        }
        if options.timeout.is_zero() || options.timeout > Duration::from_secs(60) {
            return Err(Error::Config(
                "discovery timeout must be greater than zero and at most 60 seconds".into(),
            ));
        }
        let mut names = BTreeSet::new();
        for name in options.context.global_properties.keys() {
            if name.is_empty() || name.contains('\0') || !names.insert(name.to_ascii_lowercase()) {
                return Err(Error::Config(
                    "global property names must be nonempty and case-insensitively unique".into(),
                ));
            }
        }
        Ok(Self {
            workspace_root,
            options,
            cache: Vec::new(),
        })
    }

    /// Supply previously published opaque cache entries; reuse still requires validation.
    #[must_use]
    pub fn with_cache(mut self, entries: Vec<EvaluationCacheEntry>) -> Self {
        self.cache = entries;
        self
    }
}

/// Stable discovery reasons approved by tethys-rvr5 and its amendment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveryFailureReason {
    /// Project evaluation has not been authorized.
    TrustRequired,
    /// Current restore-dependent inputs are unavailable without a restore grant.
    RestoreRequired,
    /// The separately authorized repository restore failed.
    RestoreFailed,
    /// This host cannot perform the required packages.config restore.
    RestoreUnsupportedOnHost,
    /// A compatible selected host, runtime or companion is unavailable.
    ToolchainUnavailable,
    /// Repository SDK, workload or custom SDK resolution failed.
    SdkUnresolved,
    /// A candidate, project or evaluation input is malformed.
    MalformedInput,
    /// Evaluation failed without a more specific approved reason.
    EvaluationFailed,
    /// A bounded process exceeded its deadline.
    Timeout,
    /// An indexed source or project input escapes the workspace.
    OutsideWorkspaceInput,
    /// At least one required target-framework outcome is indeterminate.
    PartialTargetFrameworks,
}

/// Severity emitted by evaluation and restore diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryDiagnosticSeverity {
    /// Evaluation cannot confirm the affected scope.
    #[default]
    Error,
    /// Native warning retained alongside the outcome.
    Warning,
}

/// Diagnostic detail, separate from stable reason classification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryDiagnostic {
    /// Native code when supplied by `MSBuild`, the SDK or `NuGet`.
    pub code: Option<String>,
    /// Native exception type when available.
    pub exception_type: Option<String>,
    /// Human-readable detail; never used to classify a failure.
    pub message: String,
    /// Native source path when available.
    #[serde(with = "path_wire::option")]
    pub file: Option<PathBuf>,
    /// One-based native source line, or zero when unavailable.
    pub line: u32,
    /// One-based native source column, or zero when unavailable.
    pub column: u32,
    /// Native diagnostic severity.
    pub severity: DiscoveryDiagnosticSeverity,
}

/// A bounded failure that can be published as explicit incomplete coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryFailure {
    /// Stable reason, independent of message wording.
    pub reason: DiscoveryFailureReason,
    /// Retained native diagnostics and actionable detail.
    pub diagnostics: Vec<DiscoveryDiagnostic>,
}

/// Standing of a project or evaluation unit, not compiler-binding confidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "standing", content = "failure", rename_all = "kebab-case")]
pub enum DiscoveryStanding {
    /// Evaluation-time metadata is authoritative for its complete input scope.
    Confirmed,
    /// Required evidence is incomplete; empty data is not a clean result.
    Indeterminate(DiscoveryFailure),
}

/// A workspace-level candidate issue that cannot be attached to a valid project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryIssue {
    /// Candidate/container path associated with the issue.
    #[serde(with = "path_wire")]
    pub path: PathBuf,
    /// Typed incomplete-coverage evidence.
    pub failure: DiscoveryFailure,
}

/// A C# project candidate and its aggregate evaluation standing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDiscovery {
    /// Canonical workspace-relative project-file identity.
    pub key: ProjectKey,
    /// Solutions and solution filters declaring membership, relative to the workspace.
    #[serde(with = "path_wire::paths")]
    pub containers: Vec<PathBuf>,
    /// Aggregate standing, including failure before framework enumeration.
    pub standing: DiscoveryStanding,
}

/// Evaluated framework identity; empty SDK shorthand does not erase classic identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FrameworkIdentity {
    /// SDK `TargetFramework` shorthand, if supplied by evaluation.
    pub short_name: Option<String>,
    /// Evaluated `TargetFrameworkIdentifier`.
    pub identifier: String,
    /// Evaluated `TargetFrameworkVersion`.
    pub version: String,
    /// Evaluated `TargetFrameworkProfile`, empty when inapplicable.
    pub profile: String,
    /// Evaluated `TargetPlatformIdentifier`, empty when inapplicable.
    pub platform_identifier: String,
    /// Evaluated `TargetPlatformVersion`, empty when inapplicable.
    pub platform_version: String,
}

/// Qualified managed host flavor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvaluationHostKind {
    /// `MSBuild` supplied by a selected .NET SDK.
    Sdk,
    /// Visual Studio `MSBuild` on the .NET Framework runtime.
    Framework,
}

/// Actual installed host/runtime provenance, not an executable-name guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostProvenance {
    /// Evaluated host flavor (`sdk` or `framework`).
    pub kind: EvaluationHostKind,
    /// Canonical loaded `MSBuild` installation path.
    #[serde(with = "path_wire")]
    pub path: PathBuf,
    /// Actual `MSBuild` file version.
    pub version: String,
    /// Actual managed runtime description.
    pub runtime: String,
}

/// Repository restore mechanism, not a permission grant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveryRestoreStyle {
    /// No restore-dependent evaluation inputs.
    #[default]
    None,
    /// PackageReference/project.assets.json evaluation inputs.
    PackageReference,
    /// Classic packages.config and populated package-directory inputs.
    PackagesConfig,
}

/// Restore input provenance; no feeds, credentials or lock policy are invented.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreProvenance {
    /// Required repository restore mechanism.
    pub style: DiscoveryRestoreStyle,
    /// Existing validated restore inputs, with absolute provenance paths where necessary.
    #[serde(with = "path_wire::paths")]
    pub inputs: Vec<PathBuf>,
}

/// An evaluated physical source participating in one unit, independently of syntax indexing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMembership {
    /// Canonical workspace-relative physical source path.
    #[serde(with = "path_wire")]
    pub path: PathBuf,
    /// Evaluated Link spelling, when present.
    pub link: Option<String>,
    /// Other evaluated item metadata, not compiler bindings.
    pub metadata: BTreeMap<String, String>,
}

/// A declared project reference, never a guessed target evaluation-unit edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredProjectReference {
    /// Workspace-relative target project path, even if not yet available.
    pub target: ProjectKey,
    /// Evaluated item spelling.
    pub include: String,
    /// Evaluated metadata, including `ReferenceOutputAssembly` when present.
    pub metadata: BTreeMap<String, String>,
}

/// An evaluated assembly-reference declaration, not a compiler-resolved assembly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredAssemblyReference {
    /// Evaluated `Reference` include value.
    pub include: String,
    /// Evaluated metadata, including `HintPath` when present.
    pub metadata: BTreeMap<String, String>,
}

/// One evaluated or explicitly failed project/framework outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationUnit {
    /// Project/framework/context identity; failures never masquerade as confirmed keys.
    pub key: EvaluationUnitKey,
    /// Owning project identity.
    pub project: ProjectKey,
    /// Requested SDK selector, if one was known before evaluation.
    pub target_framework: Option<String>,
    /// Full native identity; absent for a known selector whose evaluation failed.
    pub framework: Option<FrameworkIdentity>,
    /// Evidence standing for this outcome.
    pub standing: DiscoveryStanding,
    /// Effective framework/compiler/assembly/output/global properties.
    pub properties: BTreeMap<String, String>,
    /// Physical source membership; a file may participate in many units.
    pub sources: Vec<SourceMembership>,
    /// Declared project references without invented target-unit selection.
    pub project_references: Vec<DeclaredProjectReference>,
    /// Declared assembly references without compiler resolution.
    pub assembly_references: Vec<DeclaredAssemblyReference>,
    /// Actual host evidence, absent when host loading/evaluation was unavailable.
    pub host: Option<HostProvenance>,
    /// Required restore-input provenance.
    pub restore: RestoreProvenance,
}

/// Cache observations are invocation evidence, not semantic metadata or unit identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryCacheObservation {
    /// Project associated with this observation.
    pub project: ProjectKey,
    /// SDK selector, or outer/classic scope.
    pub target_framework: Option<String>,
    /// Whether evaluation was avoided by validated reuse.
    pub reused: bool,
    /// Why reuse was bypassed, when applicable.
    pub bypass_reasons: Vec<String>,
}

/// A captured evaluation input, not evidence of cache eligibility or currentness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationInput {
    /// Original path spelling captured by the evaluation scope.
    #[serde(with = "path_wire")]
    pub path: PathBuf,
    /// Canonical absolute identity observed when the input was captured.
    #[serde(with = "path_wire")]
    pub canonical_path: PathBuf,
    /// Observed byte length.
    pub length: u64,
    /// Observed filesystem modification time.
    #[serde(with = "system_time")]
    pub modified: SystemTime,
    /// Digest of the bytes observed by the existing input fingerprint.
    pub digest: String,
}

/// Lossless signed-epoch serialization for the public [`SystemTime`] field.
///
/// Positive timestamps retain serde's existing wire shape. Pre-epoch values use
/// an explicit signed-offset shape because serde's `SystemTime` serializer rejects
/// them rather than encoding the side of the epoch.
mod system_time {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PositiveTime {
        secs_since_epoch: u64,
        nanos_since_epoch: u32,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct OffsetTime {
        before_epoch: bool,
        duration: Duration,
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum WireTime {
        Positive(PositiveTime),
        Offset(OffsetTime),
    }

    pub fn serialize<S>(value: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value.duration_since(UNIX_EPOCH) {
            Ok(duration) => PositiveTime {
                secs_since_epoch: duration.as_secs(),
                nanos_since_epoch: duration.subsec_nanos(),
            }
            .serialize(serializer),
            Err(error) => OffsetTime {
                before_epoch: true,
                duration: error.duration(),
            }
            .serialize(serializer),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let time = match WireTime::deserialize(deserializer)? {
            WireTime::Positive(value) => {
                if value.nanos_since_epoch >= 1_000_000_000 {
                    return Err(serde::de::Error::custom(
                        "invalid nanoseconds in persisted SystemTime",
                    ));
                }
                UNIX_EPOCH.checked_add(Duration::new(
                    value.secs_since_epoch,
                    value.nanos_since_epoch,
                ))
            }
            WireTime::Offset(value) if value.before_epoch => UNIX_EPOCH.checked_sub(value.duration),
            WireTime::Offset(value) => UNIX_EPOCH.checked_add(value.duration),
        };
        time.ok_or_else(|| serde::de::Error::custom("persisted SystemTime is out of range"))
    }
}

/// One captured evaluation scope, retained even if later validation invalidates it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryInputScope {
    /// Owning project identity.
    pub project: ProjectKey,
    /// Captured observations; distinct outer and inner scopes remain separate.
    pub inputs: Vec<EvaluationInput>,
}

/// Owned discovery output; publication belongs to the index revision owner.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoverySnapshot {
    /// Cargo attribution in its existing order, with unchanged Cargo semantics.
    pub crates: Vec<CrateInfo>,
    /// One requested context; actual host/restore provenance belongs to its unit outcomes.
    pub context: EvaluationContext,
    /// Explicit evaluation and restore authority for this invocation.
    pub grants: DiscoveryGrants,
    /// Captured input scopes, independent of cache eligibility and final standing.
    pub inputs: Vec<DiscoveryInputScope>,
    /// Project candidates and aggregate outcomes.
    pub projects: Vec<ProjectDiscovery>,
    /// Successful and explicitly failed framework outcomes.
    pub units: Vec<EvaluationUnit>,
    /// Malformed or out-of-workspace candidates without a valid project owner.
    pub issues: Vec<DiscoveryIssue>,
    /// Reusable evidence to publish atomically with this snapshot.
    pub cache: Vec<EvaluationCacheEntry>,
    /// Invocation-level cache reuse/bypass evidence, excluded from semantic comparisons.
    pub cache_observations: Vec<DiscoveryCacheObservation>,
}

impl DiscoverySnapshot {
    /// Whether all discovered coverage has confirmed standing and no top-level issues.
    ///
    /// This checks only observed projects and units; it does not invent coverage
    /// for projects that were not discovered.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.issues.is_empty()
            && self
                .projects
                .iter()
                .all(|project| project.standing == DiscoveryStanding::Confirmed)
            && self
                .units
                .iter()
                .all(|unit| unit.standing == DiscoveryStanding::Confirmed)
    }
}
#[cfg(test)]
mod tests {
    use super::EvaluationEnvironment;

    #[test]
    fn without_removes_every_runtime_code_extension() {
        let mut environment = EvaluationEnvironment::empty();
        for name in EvaluationEnvironment::RUNTIME_CODE_EXTENSIONS {
            environment.insert(*name, "Probe.dll");
        }
        environment.insert("DefineConstants", "KEPT");
        assert!(environment.loads_runtime_code());

        let scrubbed = environment.without(EvaluationEnvironment::RUNTIME_CODE_EXTENSIONS);
        assert!(!scrubbed.loads_runtime_code());
        assert_eq!(scrubbed.get("DefineConstants").unwrap(), "KEPT");
        assert_eq!(scrubbed.iter().count(), 1);
    }

    #[test]
    fn insert_replaces_rather_than_binding_a_name_twice() {
        let environment = EvaluationEnvironment::empty()
            .with("DefineConstants", "FIRST")
            .with("DefineConstants", "SECOND");
        assert_eq!(environment.get("DefineConstants").unwrap(), "SECOND");
        assert_eq!(environment.iter().count(), 1);
    }

    /// Name comparison follows the platform's own environment block, so a recased
    /// lookup resolves on Windows and stays distinct everywhere else.
    #[test]
    fn name_comparison_follows_the_platform() {
        let environment = EvaluationEnvironment::empty().with("DefineConstants", "VALUE");
        assert_eq!(environment.get("DefineConstants").unwrap(), "VALUE");
        assert_eq!(
            environment.get("defineconstants").is_some(),
            cfg!(windows),
            "recased lookup must match the platform's environment block",
        );
    }

    /// A captured environment holds credentials on any CI runner, and
    /// `DiscoveryOptions` derives `Debug`.
    #[test]
    fn debug_redacts_values() {
        let environment = EvaluationEnvironment::empty().with("TOKEN", "s3cret");
        let rendered = format!("{environment:?}");
        assert!(!rendered.contains("s3cret"), "{rendered}");
        assert!(!rendered.contains("TOKEN"), "{rendered}");
        assert!(rendered.contains('1'), "{rendered}");
    }
}
