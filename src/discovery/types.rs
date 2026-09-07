//! Owned records at the language-neutral workspace discovery seam.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

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

/// Whether eligible evaluations may be reused after input validation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiscoveryCachePolicy {
    /// Validate qualified cached inputs; otherwise evaluate again.
    #[default]
    Enabled,
    /// Force evaluation and do not emit reusable cache entries.
    Disabled,
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
    pub inputs: Vec<PathBuf>,
}

/// An evaluated physical source participating in one unit, independently of syntax indexing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMembership {
    /// Canonical workspace-relative physical source path.
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

/// Owned discovery output; publication belongs to the index revision owner.
#[derive(Debug, Clone, Default)]
pub struct DiscoverySnapshot {
    /// Cargo attribution in its existing order, with unchanged Cargo semantics.
    pub crates: Vec<CrateInfo>,
    /// One requested context; actual host/restore provenance belongs to its unit outcomes.
    pub context: EvaluationContext,
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
