//! Nonexecuting restore hints and separately authorized ordinary `NuGet` restore.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::events::Event;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::super::{
    DiscoveryFailure, DiscoveryFailureReason as Reason, DiscoveryRequest,
    DiscoveryRestoreStyle as Style, EvaluationEnvironment, RestoreProvenance,
};
use super::host::{self, EvaluatedProject, HostSelection, ProcessFailure, failure};
use super::input::bounded_read;

mod graph;

const XML_LIMIT: u64 = 4 * 1024 * 1024;
const ASSETS_LIMIT: u64 = 64 * 1024 * 1024;

pub(super) struct ProjectInputs {
    pub uses_sdk: bool,
    pub style: Style,
    pub files: Vec<PathBuf>,
    packages: BTreeMap<String, String>,
    solution_directory: Option<PathBuf>,
    nuget_configs: Vec<PathBuf>,
    external_nuget_configs: Vec<PathBuf>,
    external_config_locations_known: bool,
    uncertain_dependencies: bool,
    restored: RefCell<BTreeMap<String, String>>,
    prior_receipts: RefCell<BTreeMap<String, String>>,
    validated_receipts: RefCell<BTreeMap<String, String>>,
}

pub(super) fn supply_receipts(inputs: &ProjectInputs, receipts: BTreeMap<String, String>) {
    *inputs.prior_receipts.borrow_mut() = receipts;
}

pub(super) fn receipts(inputs: &ProjectInputs) -> BTreeMap<String, String> {
    let mut receipts = inputs.validated_receipts.borrow().clone();
    receipts.extend(
        inputs
            .restored
            .borrow()
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    receipts
}

pub(super) struct RestoreOutcome {
    pub provenance: RestoreProvenance,
    pub performed: bool,
}

pub(super) fn inspect(
    request: &DiscoveryRequest,
    project: &Path,
    solution_paths: &[PathBuf],
) -> Result<ProjectInputs, DiscoveryFailure> {
    let project = project
        .canonicalize()
        .map_err(|error| failure(Reason::MalformedInput, error.to_string()))?;
    if !project.starts_with(&request.workspace_root) {
        return Err(failure(
            Reason::OutsideWorkspaceInput,
            "restore project is outside the workspace",
        ));
    }
    let mut inputs = ProjectInputs {
        uses_sdk: false,
        style: Style::None,
        files: vec![project.clone()],
        packages: BTreeMap::new(),
        solution_directory: None,
        nuget_configs: Vec::new(),
        external_nuget_configs: Vec::new(),
        external_config_locations_known: false,
        uncertain_dependencies: false,
        restored: RefCell::new(BTreeMap::new()),
        prior_receipts: RefCell::new(BTreeMap::new()),
        validated_receipts: RefCell::new(BTreeMap::new()),
    };
    parse_hints(&project, &mut inputs, false)?;
    let directory = project
        .parent()
        .ok_or_else(|| failure(Reason::MalformedInput, "project has no directory"))?;
    for ancestor in directory.ancestors() {
        for name in [
            "Directory.Build.props",
            "Directory.Build.targets",
            "Directory.Packages.props",
            "NuGet.Config",
            "nuget.config",
            "global.json",
        ] {
            let path = ancestor.join(name);
            match path.metadata() {
                Ok(metadata) if metadata.is_file() => {
                    let canonical = path
                        .canonicalize()
                        .map_err(|error| failure(Reason::MalformedInput, error.to_string()))?;
                    inputs.files.push(canonical);
                    if path.extension().is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("props")
                            || extension.eq_ignore_ascii_case("targets")
                    }) {
                        // Imported declarations are hints only; conditions are resolved by native evaluation.
                        parse_hints(&path, &mut inputs, false)?;
                    }
                }
                Ok(_) => {
                    return Err(failure(
                        Reason::MalformedInput,
                        format!("{} is not a regular input file", path.display()),
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(failure(Reason::MalformedInput, error.to_string())),
            }
        }
    }
    for name in ["packages.config", "packages.lock.json"] {
        let path = directory.join(name);
        match path.metadata() {
            Ok(metadata) if metadata.is_file() => {
                inputs.files.push(path.clone());
                if name == "packages.config" {
                    inputs.style = Style::PackagesConfig;
                    inputs.packages.clear();
                    inputs.uncertain_dependencies = false;
                    parse_hints(&path, &mut inputs, true)?;
                }
            }
            Ok(_) => {
                return Err(failure(
                    Reason::MalformedInput,
                    format!("{} is not a regular input file", path.display()),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(failure(Reason::MalformedInput, error.to_string())),
        }
    }
    if inputs.uses_sdk && inputs.style == Style::None {
        inputs.style = Style::PackageReference;
    }
    if inputs.style == Style::PackagesConfig {
        inspect_legacy_context(request, &project, directory, solution_paths, &mut inputs)?;
    }
    inputs.files.sort();
    inputs.files.dedup();
    Ok(inputs)
}

fn inspect_legacy_context(
    request: &DiscoveryRequest,
    project: &Path,
    directory: &Path,
    solution_paths: &[PathBuf],
    inputs: &mut ProjectInputs,
) -> Result<(), DiscoveryFailure> {
    let mut solution_directories = Vec::new();
    for solution in solution_paths {
        let solution = request
            .workspace_root
            .join(solution)
            .canonicalize()
            .map_err(|error| failure(Reason::MalformedInput, error.to_string()))?;
        if !solution.starts_with(&request.workspace_root) {
            return Err(failure(
                Reason::OutsideWorkspaceInput,
                "restore solution is outside the workspace",
            ));
        }
        if let Some(parent) = solution.parent() {
            solution_directories.push(parent.to_path_buf());
        }
        inputs.files.push(solution);
    }
    solution_directories.sort();
    solution_directories.dedup();
    let globals = request.options.context.effective_globals();
    if let Some((_, value)) = globals
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("SolutionDir"))
    {
        if !value.is_empty() {
            inputs.solution_directory = Some(
                native_path(project, value)
                    .canonicalize()
                    .map_err(|error| failure(Reason::MalformedInput, error.to_string()))?,
            );
        }
    } else if solution_directories.len() == 1 {
        inputs.solution_directory = solution_directories.pop();
    }
    // NuGet starts solution settings lookup at SolutionDir/.nuget; without a
    // solution context it uses the command's project working directory.
    let settings_directory = inputs.solution_directory.as_ref().map_or_else(
        || directory.to_path_buf(),
        |solution| solution.join(".nuget"),
    );
    for ancestor in settings_directory.ancestors() {
        for name in ["nuget.config", "NuGet.config", "NuGet.Config"] {
            let path = ancestor.join(name);
            match path.metadata() {
                Ok(metadata) if metadata.is_file() => {
                    let path = path
                        .canonicalize()
                        .map_err(|error| failure(Reason::MalformedInput, error.to_string()))?;
                    if !inputs.nuget_configs.contains(&path) {
                        inputs.nuget_configs.push(path.clone());
                        inputs.files.push(path);
                    }
                    break;
                }
                Ok(_) => {
                    return Err(failure(
                        Reason::MalformedInput,
                        "NuGet configuration is not a regular file",
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(failure(Reason::MalformedInput, error.to_string())),
            }
        }
    }
    inspect_external_configs(inputs, &request.options.environment)
        .map_err(|error| failure(Reason::MalformedInput, error.to_string()))?;
    Ok(())
}

fn inspect_external_configs(
    inputs: &mut ProjectInputs,
    environment: &EvaluationEnvironment,
) -> io::Result<()> {
    let environment_path = |name: &str| {
        environment
            .get(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let (user, machine) = if cfg!(windows) {
        (
            environment_path("APPDATA").map(|path| path.join("NuGet")),
            environment_path("ProgramFiles(x86)")
                .or_else(|| environment_path("ProgramFiles"))
                .map(|path| path.join("NuGet/Config")),
        )
    } else {
        (
            environment_path("HOME").map(|path| path.join(".config/NuGet")),
            Some(
                environment_path("NUGET_COMMON_APPLICATION_DATA")
                    .unwrap_or_else(|| {
                        PathBuf::from(if cfg!(target_os = "macos") {
                            "/Library/Application Support"
                        } else {
                            "/etc/opt"
                        })
                    })
                    .join("NuGet/Config"),
            ),
        )
    };
    inputs.external_config_locations_known = user.is_some() && machine.is_some();
    let mut candidates = Vec::new();
    if let Some(user) = user {
        candidates.push(user.join("NuGet.Config"));
        collect_config_paths(&user.join("config"), &mut candidates)?;
    }
    if let Some(machine) = machine {
        collect_config_paths(&machine, &mut candidates)?;
    }
    for path in candidates {
        match path.metadata() {
            Ok(metadata) if metadata.is_file() => {
                let path = path.canonicalize()?;
                inputs.files.push(path.clone());
                inputs.external_nuget_configs.push(path);
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "NuGet configuration is not a regular file",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn collect_config_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("config"))
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn parse_hints(
    path: &Path,
    inputs: &mut ProjectInputs,
    packages_config: bool,
) -> Result<(), DiscoveryFailure> {
    let malformed = |message: String| {
        failure(
            Reason::MalformedInput,
            format!("{}: {message}", path.display()),
        )
    };
    let bytes = bounded_read(path, XML_LIMIT).map_err(|error| malformed(error.to_string()))?;
    let mut reader = Reader::from_reader(bytes.as_slice());
    let mut depth = 0_usize;
    let mut root_count = 0;
    loop {
        match reader.read_event() {
            Ok(event @ (Event::Start(_) | Event::Empty(_))) => {
                let empty = matches!(event, Event::Empty(_));
                let (Event::Start(element) | Event::Empty(element)) = event else {
                    unreachable!("matched element event");
                };
                if depth == 0 {
                    root_count += 1;
                    let expected: &[u8] = if packages_config {
                        b"packages"
                    } else {
                        b"Project"
                    };
                    if element.local_name().as_ref() != expected {
                        return Err(malformed("unexpected XML root".into()));
                    }
                }
                if !empty {
                    depth += 1;
                }
                if depth > 128 {
                    return Err(malformed("XML nesting exceeds 128 levels".into()));
                }
                let mut attrs = BTreeMap::new();
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|error| malformed(error.to_string()))?;
                    let name = String::from_utf8(attribute.key.as_ref().to_vec())
                        .map_err(|error| malformed(error.to_string()))?;
                    let decoded = reader
                        .decoder()
                        .decode(attribute.value.as_ref())
                        .map_err(|error| malformed(error.to_string()))?;
                    let value = quick_xml::escape::unescape(&decoded)
                        .map_err(|error| malformed(error.to_string()))?
                        .into_owned();
                    attrs.insert(name, value);
                }
                record_hint(
                    element.local_name().as_ref(),
                    &attrs,
                    inputs,
                    packages_config,
                );
            }
            Ok(Event::End(_)) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| malformed("unbalanced XML".into()))?;
            }
            Ok(Event::DocType(_)) => {
                return Err(malformed("DTD declarations are not accepted".into()));
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(malformed(error.to_string())),
        }
    }
    if root_count != 1 || depth != 0 {
        return Err(malformed(
            "project must contain one complete XML root".into(),
        ));
    }
    Ok(())
}

fn record_hint(
    name: &[u8],
    attrs: &BTreeMap<String, String>,
    inputs: &mut ProjectInputs,
    packages_config: bool,
) {
    if attrs.contains_key("Sdk") || name == b"Sdk" {
        inputs.uses_sdk = true;
    }
    if name == b"Import" || attrs.contains_key("Condition") {
        inputs.uncertain_dependencies = true;
    }
    let is_package_reference = name == b"PackageReference";
    let is_package_version = name == b"PackageVersion";
    if is_package_reference || is_package_version || (packages_config && name == b"package") {
        if is_package_reference && !packages_config {
            inputs.style = Style::PackageReference;
        }
        let id = attrs.get(if packages_config { "id" } else { "Include" });
        let version = attrs.get(if packages_config {
            "version"
        } else {
            "Version"
        });
        if let (Some(id), Some(version)) = (id, version) {
            if id.is_empty()
                || version.is_empty()
                || [id, version]
                    .iter()
                    .any(|value| value.contains(['$', '@', '%', '/', '\\']))
            {
                inputs.uncertain_dependencies = true;
            } else {
                inputs
                    .packages
                    .insert(id.to_ascii_lowercase(), version.clone());
            }
        } else {
            inputs.uncertain_dependencies = true;
        }
    }
}

fn property<'a>(evaluated: Option<&'a EvaluatedProject>, name: &str) -> Option<&'a str> {
    evaluated
        .and_then(|project| project.properties.get(name))
        .map(String::as_str)
        .filter(|value| !value.is_empty())
}

fn native_path(project: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_owned()
    } else {
        project.parent().unwrap_or(Path::new(".")).join(path)
    }
}

/// Name a restore artifact the same way however its location was learned.
///
/// `MSBuild` reports an already-plain native path, while the same file derived from
/// the canonicalized project keeps Windows' verbatim `\\?\` prefix. Which branch
/// applies changes within one invocation, because properties like `ProjectAssetsFile`
/// are absent before a restore and present after it. Restore inputs are compared by
/// exact path in three places -- the receipt digest, the captured-restore set and the
/// evaluation input closure -- so a second spelling reads as a different file.
/// Only the generated artifacts below take this: a caller-facing destination such as
/// `repositoryPath` keeps the spelling its own configuration gave it.
fn artifact_path(project: &Path, value: &str) -> PathBuf {
    dunce::simplified(&native_path(project, value)).to_owned()
}

fn style(inputs: &ProjectInputs, evaluated: Option<&EvaluatedProject>) -> Style {
    match property(evaluated, "RestoreProjectStyle") {
        Some("PackageReference") => Style::PackageReference,
        Some("PackagesConfig") => Style::PackagesConfig,
        _ => inputs.style,
    }
}

fn context_key(
    request: &DiscoveryRequest,
    project: &Path,
    target_framework: Option<&str>,
) -> crate::Result<String> {
    serde_json::to_string(&(
        project,
        super::cache::normalized_globals(request),
        target_framework,
    ))
    .map_err(|error| crate::Error::Internal(error.to_string()))
}

fn fresh_files(files: &[PathBuf], generated: &[PathBuf]) -> io::Result<bool> {
    let mut oldest = None;
    for path in generated {
        let metadata = match path.metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if !metadata.is_file() {
            return Ok(false);
        }
        let modified = metadata.modified()?;
        oldest = Some(oldest.map_or(modified, |value: std::time::SystemTime| value.min(modified)));
    }
    let Some(oldest) = oldest else {
        return Ok(false);
    };
    for path in files {
        let metadata = match path.metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        // Equal timestamps cannot establish ordering on coarse-resolution
        // filesystems. A validated content receipt handles legitimate no-op
        // restores; unreceipted bootstrap requires strictly newer outputs.
        if metadata.modified()? >= oldest {
            return Ok(false);
        }
    }
    Ok(true)
}

fn read_assets(path: &Path) -> crate::Result<Option<Value>> {
    match bounded_read(path, ASSETS_LIMIT) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                tracing::debug!(%error, path = %path.display(), "restore metadata is corrupt");
                Ok(None)
            }
        },
        Err(crate::Error::Config(message)) => {
            tracing::debug!(%message, path = %path.display(), "restore metadata exceeds input limit");
            Ok(None)
        }
        Err(crate::Error::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            tracing::debug!(%error, path = %path.display(), "restore metadata unavailable");
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum NuGetRange {
    Any,
    Minimum {
        version: String,
        inclusive: bool,
    },
    Maximum {
        version: String,
        inclusive: bool,
    },
    Exact(String),
    Between {
        lower: String,
        lower_inclusive: bool,
        upper: String,
        upper_inclusive: bool,
    },
}

fn normalized_version(value: &str) -> Option<String> {
    let value = value.trim();
    let value = value.split_once('+').map_or(value, |(value, _)| value);
    let (release, prerelease) = value
        .split_once('-')
        .map_or((value, None), |(value, pre)| (value, Some(pre)));
    let mut components = Vec::new();
    for component in release.split('.') {
        if component.is_empty() {
            return None;
        }
        if component == "*" {
            components.push("*".to_owned());
            break;
        }
        components.push(component.parse::<u64>().ok()?.to_string());
    }
    if components.is_empty() || components.len() > 4 {
        return None;
    }
    if components.last().is_some_and(|component| component == "*") {
        if release.split('.').count() != components.len() {
            return None;
        }
    } else {
        while components.len() < 3 {
            components.push("0".to_owned());
        }
        // NuGet normalizes a four-part release whose revision is zero to the
        // equivalent three-part version.
        while components.len() > 3 && components.last().is_some_and(|component| component == "0") {
            components.pop();
        }
    }
    let mut normalized = components.join(".");
    if let Some(prerelease) = prerelease {
        if prerelease.is_empty() {
            return None;
        }
        let mut identifiers = Vec::new();
        for identifier in prerelease.split('.') {
            if identifier.is_empty() {
                return None;
            }
            identifiers.push(identifier.parse::<u64>().map_or_else(
                |_| identifier.to_ascii_lowercase(),
                |number| number.to_string(),
            ));
        }
        normalized.push('-');
        normalized.push_str(&identifiers.join("."));
    }
    Some(normalized)
}

fn parse_range(value: &str) -> Option<NuGetRange> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with(['[', '(']) {
        let lower_inclusive = value.starts_with('[');
        let upper_inclusive = value.ends_with(']');
        if !(value.ends_with(']') || value.ends_with(')')) {
            return None;
        }
        let inner = &value[1..value.len() - 1];
        if let Some((lower, upper)) = inner.split_once(',') {
            let lower = if lower.trim().is_empty() {
                None
            } else {
                Some(normalized_version(lower)?)
            };
            let upper = if upper.trim().is_empty() {
                None
            } else {
                Some(normalized_version(upper)?)
            };
            return match (lower, upper) {
                (None, None) => Some(NuGetRange::Any),
                (Some(lower), None) => Some(NuGetRange::Minimum {
                    version: lower,
                    inclusive: lower_inclusive,
                }),
                (None, Some(upper)) => Some(NuGetRange::Maximum {
                    version: upper,
                    inclusive: upper_inclusive,
                }),
                (Some(lower), Some(upper))
                    if lower_inclusive && upper_inclusive && lower == upper =>
                {
                    Some(NuGetRange::Exact(lower))
                }
                (Some(lower), Some(upper)) => Some(NuGetRange::Between {
                    lower,
                    lower_inclusive,
                    upper,
                    upper_inclusive,
                }),
            };
        }
        if lower_inclusive && upper_inclusive {
            return Some(NuGetRange::Exact(normalized_version(inner)?));
        }
        return None;
    }
    Some(NuGetRange::Minimum {
        version: normalized_version(value)?,
        inclusive: true,
    })
}

fn ranges_match(left: &str, right: &str) -> bool {
    parse_range(left).is_some_and(|left| parse_range(right).is_some_and(|right| left == right))
}

fn asset_metadata_matches(dependency: &Value, metadata: [Option<&str>; 3]) -> bool {
    let normalize = |value: &str| {
        let mut names: Vec<_> = value
            .split([';', ','])
            .map(|part| part.trim().to_ascii_lowercase())
            .collect();
        names.sort();
        names.dedup();
        names
    };
    for ((field, default), value) in [
        ("include", "all"),
        ("exclude", "none"),
        ("suppressParent", "contentfiles;analyzers;build"),
    ]
    .into_iter()
    .zip(metadata)
    {
        if let Some(value) = value.filter(|value| !value.is_empty())
            && normalize(value) != normalize(dependency[field].as_str().unwrap_or(default))
        {
            return false;
        }
    }
    true
}

fn asset_metadata_matches(dependency: &Value, metadata: [Option<&str>; 3]) -> bool {
    let normalize = |value: &str| {
        let mut names: Vec<_> = value
            .split([';', ','])
            .map(|part| part.trim().to_ascii_lowercase())
            .collect();
        names.sort();
        names.dedup();
        names
    };
    for ((field, default), value) in [
        ("include", "all"),
        ("exclude", "none"),
        ("suppressParent", "contentfiles;analyzers;build"),
    ]
    .into_iter()
    .zip(metadata)
    {
        if let Some(value) = value.filter(|value| !value.is_empty())
            && normalize(value) != normalize(dependency[field].as_str().unwrap_or(default))
        {
            return false;
        }
    }
    true
}

fn dependencies_match(
    assets: &Value,
    evaluated: &EvaluatedProject,
    requested: Option<&str>,
    target_evidence: bool,
) -> bool {
    let Some(frameworks) = assets["project"]["frameworks"].as_object() else {
        return false;
    };
    // Outer multi-target metadata cannot stand in for conditioned inner dependencies.
    // Its complete inner scopes are each checked by ensure before confirmation.
    let framework = match requested {
        Some(name) => graph::framework_key(frameworks, name).map(|(_, value)| value),
        None if frameworks.len() == 1 => frameworks.values().next(),
        None => return true,
    };
    let Some(framework) = framework else {
        return false;
    };
    let native = &evaluated.items["PackageReference"];
    let dependencies = framework["dependencies"].as_object();
    let package_count = dependencies.map_or(0, |values| {
        values
            .values()
            .filter(|value| {
                value["target"]
                    .as_str()
                    .is_none_or(|target| target == "Package")
            })
            .count()
    });
    // Implicitly defined dependencies (for example Microsoft.NETFramework.ReferenceAssemblies)
    // appear only in the assets and the native restore graph. When the caller supplies target
    // evidence it corroborates the complete set against that graph, so the evaluated explicit
    // references must be a subset rather than the whole set; without target evidence the sets
    // must match exactly.
    if package_count < native.len() || (!target_evidence && package_count != native.len()) {
        return false;
    }
    for item in native {
        let Some(dependency) = dependencies.and_then(|values| {
            values
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(&item.include))
                .map(|(_, value)| value)
        }) else {
            return false;
        };
        let version = item
            .metadata_value("VersionOverride")
            .filter(|value| !value.is_empty())
            .or_else(|| {
                item.metadata_value("Version")
                    .filter(|value| !value.is_empty())
            })
            .or_else(|| {
                evaluated
                    .items
                    .get("PackageVersion")
                    .and_then(|items| {
                        items
                            .iter()
                            .find(|version| version.include.eq_ignore_ascii_case(&item.include))
                    })
                    .and_then(|version| version.metadata_value("Version"))
            });
        if version.is_none() && !target_evidence {
            return false;
        }
        let Some(recorded) = dependency["version"].as_str() else {
            return false;
        };
        if version.is_some_and(|version| !ranges_match(version, recorded)) {
            return false;
        }
        if !asset_metadata_matches(
            dependency,
            [
                item.metadata_value("IncludeAssets"),
                item.metadata_value("ExcludeAssets"),
                item.metadata_value("PrivateAssets"),
            ],
        ) {
            return false;
        }
    }
    target_evidence || downloads_match(framework, evaluated)
}

fn downloads_match(framework: &Value, evaluated: &EvaluatedProject) -> bool {
    let downloads = evaluated
        .items
        .get("PackageDownload")
        .map(Vec::as_slice)
        .unwrap_or_default();
    let recorded = framework["downloadDependencies"].as_array();
    if downloads.len() != recorded.map_or(0, Vec::len) {
        return false;
    }
    for item in downloads {
        let Some(version) = item.metadata_value("Version") else {
            return false;
        };
        if !recorded.is_some_and(|values| {
            values.iter().any(|value| {
                value["name"]
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&item.include))
                    && value["version"]
                        .as_str()
                        .is_some_and(|value| ranges_match(value, version))
            })
        }) {
            return false;
        }
    }
    true
}

fn asset_context_matches(
    assets: &Value,
    evaluated: Option<&EvaluatedProject>,
    target_framework: Option<&str>,
    target_evidence: bool,
) -> bool {
    let requested = target_framework.or_else(|| property(evaluated, "TargetFramework"));
    let frameworks = assets["project"]["frameworks"].as_object();
    if let Some(framework) = requested {
        let Some((key, _)) = frameworks.and_then(|values| graph::framework_key(values, framework))
        else {
            return false;
        };
        if let Some(rid) = property(evaluated, "RuntimeIdentifier")
            && assets["targets"].get(format!("{key}/{rid}")).is_none()
        {
            return false;
        }
    } else if let Some(names) = property(evaluated, "TargetFrameworks") {
        for framework in names
            .split(';')
            .map(str::trim)
            .filter(|framework| !framework.is_empty())
        {
            if frameworks
                .and_then(|values| graph::framework_key(values, framework))
                .is_none()
            {
                return false;
            }
        }
    }
    if let Some(evaluated) = evaluated.filter(|value| value.success) {
        evaluated.items.contains_key("PackageReference")
            && dependencies_match(assets, evaluated, requested, target_evidence)
    } else {
        // This is a provisional candidate, not confirmed metadata. Its caller must
        // validate a receipt or fresh native graph; stabilize then requires a
        // successful reevaluation and repeats the strict PackageReference checks.
        target_evidence
    }
}

fn asset_packages_present(assets: &Value) -> crate::Result<bool> {
    // Every library package must still exist in one of NuGet's recorded package folders.
    let Some(libraries) = assets["libraries"].as_object() else {
        return Ok(false);
    };
    let Some(folders) = assets["packageFolders"].as_object() else {
        return Ok(false);
    };
    for library in libraries
        .values()
        .filter(|library| library["type"] == "package")
    {
        let Some(path) = library["path"].as_str() else {
            return Ok(false);
        };
        if Path::new(path).is_absolute()
            || Path::new(path)
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Ok(false);
        }
        let mut populated = false;
        for folder in folders.keys() {
            let package = Path::new(folder).join(path);
            match package.metadata() {
                Ok(metadata) if metadata.is_dir() => {
                    if let Some(files) = library["files"].as_array() {
                        let mut complete = true;
                        for file in files {
                            let Some(file) = file.as_str() else {
                                return Ok(false);
                            };
                            let relative = Path::new(file);
                            if relative.is_absolute()
                                || relative
                                    .components()
                                    .any(|part| matches!(part, std::path::Component::ParentDir))
                            {
                                return Ok(false);
                            }
                            match package.join(relative).metadata() {
                                Ok(metadata) if metadata.is_file() => {}
                                Ok(_) => complete = false,
                                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                                    complete = false;
                                }
                                Err(error) => return Err(error.into()),
                            }
                        }
                        if complete {
                            populated = true;
                            break;
                        }
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        if !populated {
            return Ok(false);
        }
    }
    Ok(true)
}

fn native_project_matches(path: &str, project: &Path) -> io::Result<bool> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Ok(false);
    }
    // Domain identity is canonical; native restore metadata may retain an alias.
    if path.as_os_str() == project.as_os_str() {
        return Ok(true);
    }
    match path.canonicalize() {
        Ok(canonical) => Ok(canonical == project),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound
                    | io::ErrorKind::NotADirectory
                    | io::ErrorKind::InvalidInput
            ) =>
        {
            tracing::debug!(%error, path = %path.display(), "restore project identity unavailable");
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn assets_path(project: &Path, evaluated: Option<&EvaluatedProject>) -> PathBuf {
    property(evaluated, "ProjectAssetsFile").map_or_else(
        || artifact_path(project, "obj/project.assets.json"),
        |value| artifact_path(project, value),
    )
}

fn generated_inputs(
    project: &Path,
    evaluated: Option<&EvaluatedProject>,
    assets: Option<&Value>,
) -> crate::Result<Vec<PathBuf>> {
    let output = assets
        .and_then(|assets| assets["project"]["restore"]["outputPath"].as_str())
        .or_else(|| property(evaluated, "MSBuildProjectExtensionsPath"))
        .map_or_else(
            || artifact_path(project, "obj"),
            |value| artifact_path(project, value),
        );
    let name = project
        .file_name()
        .ok_or_else(|| crate::Error::Config("project has no filename".into()))?
        .to_string_lossy();
    Ok(vec![
        assets_path(project, evaluated),
        output.join(format!("{name}.nuget.g.props")),
        output.join(format!("{name}.nuget.g.targets")),
        output.join(format!("{name}.nuget.dgspec.json")),
    ])
}

fn restore_sources(
    request: &DiscoveryRequest,
    project: &Path,
    evaluated: Option<&EvaluatedProject>,
    assets: Option<&Value>,
    generated: &[PathBuf],
) -> crate::Result<Option<Vec<PathBuf>>> {
    // Recapture conventional paths, including previously absent inputs, rather
    // than treating the initial inspection's path list as a stable closure.
    let mut inspected = match inspect(request, project, &[]) {
        Ok(inputs) => inputs,
        Err(error) => {
            tracing::debug!(?error, "restore source inspection is unavailable");
            return Ok(None);
        }
    };
    inspect_external_configs(&mut inspected, &request.options.environment)?;
    if !cfg!(windows) {
        for home in ["HOME", "DOTNET_CLI_HOME"]
            .into_iter()
            .filter_map(|name| request.options.environment.get(name))
        {
            let fallback = PathBuf::from(home).join(".nuget/NuGet/NuGet.Config");
            if regular_file(&fallback)? {
                inspected.files.push(fallback);
            }
        }
    }
    let mut paths = inspected.files;
    if let Some(evaluated) = evaluated {
        paths.extend(evaluated.imports.iter().cloned());
    }
    if let Some(configs) =
        assets.and_then(|assets| assets["project"]["restore"]["configFilePaths"].as_array())
    {
        for config in configs {
            let Some(path) = config.as_str() else {
                return Ok(None);
            };
            paths.push(native_path(project, path));
        }
    }
    if let Some(config) = property(evaluated, "RestoreConfigFile") {
        paths.push(native_path(project, config));
    }
    let mut generated_identities = Vec::new();
    for path in generated {
        if regular_file(path)? {
            generated_identities.push(path.canonicalize()?);
        }
    }
    let mut sources = Vec::new();
    for path in paths {
        if !regular_file(&path)? {
            tracing::debug!(path = %path.display(), "restore source is unavailable");
            return Ok(None);
        }
        let path = path.canonicalize()?;
        if !generated_identities.contains(&path) {
            sources.push(path);
        }
    }
    sources.sort();
    sources.dedup();
    Ok(Some(sources))
}

fn observe_restore(
    request: &DiscoveryRequest,
    project: &Path,
    evaluated: Option<&EvaluatedProject>,
) -> crate::Result<Option<BTreeMap<PathBuf, String>>> {
    let assets = read_assets(&assets_path(project, evaluated))?;
    let generated = generated_inputs(project, evaluated, assets.as_ref())?;
    let Some(sources) = restore_sources(request, project, evaluated, assets.as_ref(), &generated)?
    else {
        return Ok(None);
    };
    let sources = sources
        .into_iter()
        .map(|path| receipt_digest(std::slice::from_ref(&path)).map(|digest| (path, digest)))
        .collect::<crate::Result<BTreeMap<_, _>>>()?;
    Ok(Some(sources))
}

struct AssetInputs {
    files: Vec<PathBuf>,
    evaluated_dependencies: bool,
    assets: Value,
}

fn asset_inputs(
    request: &DiscoveryRequest,
    project: &Path,
    evaluated: Option<&EvaluatedProject>,
    target_framework: Option<&str>,
    target_evidence: bool,
) -> crate::Result<Option<AssetInputs>> {
    let assets_path = assets_path(project, evaluated);
    let Some(assets) = read_assets(&assets_path)? else {
        return Ok(None);
    };
    if assets["version"].as_u64() != Some(3) {
        return Ok(None);
    }
    let restore = &assets["project"]["restore"];
    let (Some(project_path), Some(project_unique_name)) = (
        restore["projectPath"].as_str(),
        restore["projectUniqueName"].as_str(),
    ) else {
        return Ok(None);
    };
    if !native_project_matches(project_path, project)?
        || !native_project_matches(project_unique_name, project)?
    {
        return Ok(None);
    }
    let Some(output_path) = restore["outputPath"].as_str() else {
        return Ok(None);
    };
    let output = artifact_path(project, output_path);
    let generated = generated_inputs(project, evaluated, Some(&assets))?;
    let Some(spec) = read_assets(&generated[3])? else {
        return Ok(None);
    };
    // The dgspec map retains the validated native spelling, not canonical identity.
    let Some(project_spec) = spec["projects"]
        .as_object()
        .and_then(|projects| projects.get(project_unique_name))
    else {
        return Ok(None);
    };
    if project_spec["frameworks"] != assets["project"]["frameworks"]
        || project_spec["restore"] != *restore
    {
        return Ok(None);
    }
    if !asset_context_matches(&assets, evaluated, target_framework, target_evidence) {
        return Ok(None);
    }
    let Some(mut relevant) =
        restore_sources(request, project, evaluated, Some(&assets), &generated)?
    else {
        return Ok(None);
    };
    if (!target_evidence && !fresh_files(&relevant, &generated)?) || !fresh_files(&[], &generated)?
    {
        return Ok(None);
    }
    if !asset_packages_present(&assets)? {
        return Ok(None);
    }
    let cache = output.join("project.nuget.cache");
    match cache.metadata() {
        Ok(_) => {
            let Some(value) = read_assets(&cache)? else {
                return Ok(None);
            };
            if value["success"] != true {
                return Ok(None);
            }
            if let Some(expected) = value["expectedPackageFiles"].as_array() {
                for path in expected {
                    let Some(path) = path.as_str().map(PathBuf::from) else {
                        return Ok(None);
                    };
                    if !path.is_absolute() || !regular_file(&path)? {
                        return Ok(None);
                    }
                    relevant.push(path);
                }
            } else if !value["expectedPackageFiles"].is_null() {
                return Ok(None);
            }
            relevant.push(cache);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    relevant.extend(generated);
    relevant.sort();
    relevant.dedup();
    Ok(Some(AssetInputs {
        files: relevant,
        evaluated_dependencies: asset_context_matches(&assets, evaluated, target_framework, false),
        assets,
    }))
}

fn receipt_digest(files: &[PathBuf]) -> crate::Result<String> {
    let mut hash = Sha256::new();
    // Versioned, length-framed paths and fixed-width content digests prevent
    // path/content boundaries from aliasing one another. Older receipts also
    // lack the stable before/after source authority required by graph evidence.
    hash.update(b"tethys-restore-inputs-v4\0");
    let mut chunk = [0_u8; 16_384];
    for path in files {
        let path_bytes = path.as_os_str().as_encoded_bytes();
        hash.update((path_bytes.len() as u64).to_le_bytes());
        hash.update(path_bytes);
        let mut content = Sha256::new();
        let mut file = std::fs::File::open(path)?;
        loop {
            let size = file.read(&mut chunk)?;
            if size == 0 {
                break;
            }
            content.update(&chunk[..size]);
        }
        hash.update(content.finalize());
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn collect_package_files(directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_package_files(&entry.path(), files)?;
        } else if kind.is_file() {
            files.push(entry.path());
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "legacy package contains an unverified nonregular input",
            ));
        }
    }
    Ok(())
}

fn package_roots(
    _project: &Path,
    inputs: &ProjectInputs,
    _evaluated: Option<&EvaluatedProject>,
) -> crate::Result<Vec<PathBuf>> {
    // Keep exactly NuGet's destination precedence. MSBuild's
    // RestoreRepositoryPath is not a nuget.exe command-line destination.
    let mut repository = None;
    // External settings are lower priority. Without reproducing NuGet's
    // machine-file ordering, a repositoryPath there is unresolved, not absent.
    // A nearer config assignment/clear can still establish the destination.
    let mut unknown = !inputs.external_config_locations_known;
    for (config, external) in inputs
        .external_nuget_configs
        .iter()
        .map(|path| (path, true))
        .chain(inputs.nuget_configs.iter().rev().map(|path| (path, false)))
    {
        let bytes = bounded_read(config, XML_LIMIT)?;
        let mut reader = Reader::from_reader(bytes.as_slice());
        let mut in_config = false;
        loop {
            match reader.read_event() {
                Ok(Event::Start(element)) if element.local_name().as_ref() == b"config" => {
                    in_config = true;
                }
                Ok(Event::End(element)) if element.local_name().as_ref() == b"config" => {
                    in_config = false;
                }
                Ok(Event::Empty(element) | Event::Start(element))
                    if in_config && element.local_name().as_ref() == b"clear" =>
                {
                    repository = None;
                    if !external {
                        unknown = false;
                    }
                }
                Ok(Event::Empty(element) | Event::Start(element))
                    if in_config && element.local_name().as_ref() == b"add" =>
                {
                    let mut key = None;
                    let mut value = None;
                    for attribute in element.attributes() {
                        let attribute =
                            attribute.map_err(|error| crate::Error::Config(error.to_string()))?;
                        let decoded_value = reader
                            .decoder()
                            .decode(attribute.value.as_ref())
                            .map_err(|error| crate::Error::Config(error.to_string()))?;
                        let decoded = quick_xml::escape::unescape(&decoded_value)
                            .map_err(|error| crate::Error::Config(error.to_string()))?
                            .into_owned();
                        match attribute.key.as_ref() {
                            b"key" => key = Some(decoded),
                            b"value" => value = Some(decoded),
                            _ => {}
                        }
                    }
                    if key
                        .as_deref()
                        .is_some_and(|key| key.eq_ignore_ascii_case("repositoryPath"))
                        && let Some(value) = value
                    {
                        if external {
                            unknown = true;
                        } else {
                            unknown = value.contains('%');
                            repository = (!value.is_empty()).then(|| native_path(config, &value));
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(error) => return Err(crate::Error::Config(error.to_string())),
            }
        }
    }
    if unknown {
        return Ok(Vec::new());
    }
    if let Some(repository) = repository {
        return Ok(vec![repository]);
    }
    Ok(inputs
        .solution_directory
        .iter()
        .map(|directory| directory.join("packages"))
        .collect())
}

fn packages_inputs(
    project: &Path,
    inputs: &ProjectInputs,
    evaluated: Option<&EvaluatedProject>,
) -> crate::Result<Option<Vec<PathBuf>>> {
    if inputs.uncertain_dependencies {
        return Ok(None);
    }
    let mut files = inputs.files.clone();
    let roots = package_roots(project, inputs, evaluated)?;
    for (id, version) in &inputs.packages {
        let mut found = false;
        for root in &roots {
            let entries = match std::fs::read_dir(root) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            for entry in entries {
                let entry = entry?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&format!("{id}.{version}"))
                {
                    let nupkg = entry.path().join(format!("{id}.{version}.nupkg"));
                    // NuGet preserves package-id casing in legacy folders; discover the archive by identity.
                    for package_file in std::fs::read_dir(entry.path())? {
                        let package_file = package_file?;
                        if package_file
                            .file_name()
                            .to_string_lossy()
                            .eq_ignore_ascii_case(
                                &nupkg.file_name().unwrap_or_default().to_string_lossy(),
                            )
                            && package_file.file_type()?.is_file()
                        {
                            files.push(package_file.path());
                            collect_package_files(&entry.path(), &mut files)?;
                            found = true;
                        }
                    }
                }
            }
            if found {
                break;
            }
        }
        if !found {
            return Ok(None);
        }
    }
    if let Some(evaluated) = evaluated {
        for import in &evaluated.imports {
            if !regular_file(import)? {
                return Ok(None);
            }
            files.push(import.clone());
        }
        if let Some(references) = evaluated.items.get("Reference") {
            for reference in references {
                if let Some(path) = reference
                    .metadata_value("HintPath")
                    .filter(|path| !path.is_empty())
                {
                    let path = native_path(project, path);
                    if !regular_file(&path)? {
                        return Ok(None);
                    }
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(Some(files))
}

fn current_inputs(
    request: &DiscoveryRequest,
    project: &Path,
    inputs: &ProjectInputs,
    evaluated: Option<&EvaluatedProject>,
    target_framework: Option<&str>,
    style: Style,
    key: &str,
) -> crate::Result<Option<Vec<PathBuf>>> {
    let restored = inputs.restored.borrow().get(key).cloned();
    let prior = inputs.prior_receipts.borrow().get(key).cloned();
    if let Some(receipt) = restored.as_ref().or(prior.as_ref()) {
        let candidate = match style {
            Style::PackageReference => {
                asset_inputs(request, project, evaluated, target_framework, true)?
                    .map(|inventory| inventory.files)
            }
            Style::PackagesConfig => packages_inputs(project, inputs, evaluated)?,
            Style::None => unreachable!("handled by ensure"),
        };
        if let Some(files) = candidate
            && receipt_digest(&files)? == *receipt
        {
            if restored.is_none() {
                inputs
                    .validated_receipts
                    .borrow_mut()
                    .insert(key.to_owned(), receipt.clone());
            }
            return Ok(Some(files));
        }
        // Neither a claimed prior receipt nor an earlier restore in this invocation
        // authorizes target-derived dependencies after any inventoried bytes change.
        inputs.prior_receipts.borrow_mut().remove(key);
        inputs.validated_receipts.borrow_mut().remove(key);
        if restored.is_some() {
            return Ok(None);
        }
    }
    match style {
        Style::PackageReference => {
            asset_inputs(request, project, evaluated, target_framework, false)
                .map(|inventory| inventory.map(|inventory| inventory.files))
        }
        Style::PackagesConfig => packages_inputs(project, inputs, evaluated),
        Style::None => unreachable!("handled by ensure"),
    }
}

fn run_authorized_restore(
    request: &DiscoveryRequest,
    project: &Path,
    host: &HostSelection,
    style: Style,
    inputs: &ProjectInputs,
) -> crate::Result<Result<Option<graph::RestoreGraphEvidence>, DiscoveryFailure>> {
    let native_project = dunce::simplified(project);
    let mut command = if style == Style::PackagesConfig {
        if package_roots(project, inputs, None)?.is_empty() {
            return Ok(Err(failure(
                Reason::RestoreFailed,
                "packages.config restore destination is unavailable, ambiguous, or depends on unestablished configuration; establish repositoryPath or clear inherited config with an explicit/unique SolutionDir",
            )));
        }
        let nuget = match host::executable("nuget.exe", &request.options.environment) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Err(failure(
                    Reason::ToolchainUnavailable,
                    error.to_string(),
                )));
            }
            Err(error) => return Err(error.into()),
        };
        let mut command = std::process::Command::new(nuget);
        command
            .arg("restore")
            .arg(native_project)
            .arg("-NonInteractive")
            .arg("-MSBuildPath")
            .arg(dunce::simplified(&host.msbuild_path));
        if let Some(directory) = &inputs.solution_directory {
            command
                .arg("-SolutionDirectory")
                .arg(dunce::simplified(directory));
        }
        command
    } else {
        let mut command = host::restore_command(host);
        command.arg(native_project).args([
            "-target:Restore",
            "-nologo",
            "-verbosity:quiet",
            "-getItem:_RestoreGraphEntryFiltered",
        ]);
        let globals = request.options.context.effective_globals();
        for (name, value) in globals {
            if name.is_empty()
                || !name.chars().all(|character| {
                    character.is_alphanumeric() || matches!(character, '_' | '.' | '-' | ':')
                })
            {
                return Err(crate::Error::Config("global property name cannot be represented safely by the MSBuild restore command".into()));
            }
            // MSBuild property CLI treats semicolon/comma as separators; percent-escape values.
            let escaped = value
                .replace('%', "%25")
                .replace(';', "%3B")
                .replace(',', "%2C")
                .replace('"', "%22");
            command.arg(format!("-property:{name}={escaped}"));
        }
        command
    };
    command.current_dir(dunce::simplified(
        project
            .parent()
            .ok_or_else(|| crate::Error::Config("project has no directory".into()))?,
    ));
    let output = match host::run(
        &mut command,
        Vec::new(),
        request.options.timeout,
        &request.options.environment,
    ) {
        Ok(output) => output,
        Err(ProcessFailure::Timeout) => {
            return Ok(Err(failure(
                Reason::Timeout,
                "authorized restore exceeded its deadline",
            )));
        }
        Err(ProcessFailure::Overflow) => {
            return Err(crate::Error::Internal(
                "restore exceeded its output byte limit".into(),
            ));
        }
        Err(ProcessFailure::Io(error)) => return Err(error.into()),
    };
    if !output.status.success() {
        return Ok(Err(failure(
            Reason::RestoreFailed,
            host::diagnostic_text(&output),
        )));
    }
    Ok(Ok(if style == Style::PackageReference {
        graph::RestoreGraphEvidence::parse(&output.stdout)
    } else {
        None
    }))
}

fn restored_asset_inputs(
    request: &DiscoveryRequest,
    project: &Path,
    evaluated: Option<&EvaluatedProject>,
    target_framework: Option<&str>,
    before: Option<&BTreeMap<PathBuf, String>>,
    graph: Option<&graph::RestoreGraphEvidence>,
) -> crate::Result<Option<Vec<PathBuf>>> {
    let Some(before) = before else {
        return Ok(None);
    };
    let Some(candidate) = asset_inputs(request, project, evaluated, target_framework, true)? else {
        return Ok(None);
    };
    let Some(after) = observe_restore(request, project, evaluated)? else {
        return Ok(None);
    };
    if *before != after {
        tracing::debug!("source, import, or configuration inputs changed during restore");
        return Ok(None);
    }
    if !candidate.evaluated_dependencies {
        let corroborated = match graph {
            Some(graph) => graph.matches(&candidate.assets, project)?,
            None => false,
        };
        if !corroborated {
            tracing::debug!("restore target-derived dependency evidence is unavailable");
            return Ok(None);
        }
    }
    Ok(Some(candidate.files))
}

/// Explain why a scope that already restored in this invocation can no longer validate.
fn post_restore_refusal(
    request: &DiscoveryRequest,
    project: &Path,
    inputs: &ProjectInputs,
    evaluated: Option<&EvaluatedProject>,
    target_framework: Option<&str>,
    style: Style,
    key: &str,
) -> crate::Result<String> {
    if style != Style::PackageReference {
        return Ok("the legacy packages.config receipt is unavailable or changed".to_owned());
    }
    let Some(inventory) = asset_inputs(request, project, evaluated, target_framework, true)? else {
        return Ok("the asset inventory is unavailable".to_owned());
    };
    match inputs.restored.borrow().get(key) {
        Some(expected) if receipt_digest(&inventory.files)? == *expected => {
            Ok("the receipt is unchanged".to_owned())
        }
        Some(_) => Ok("the recorded receipt no longer matches the inventoried inputs".to_owned()),
        None => Ok("the recorded receipt is missing".to_owned()),
    }
}

pub(super) fn ensure(
    request: &DiscoveryRequest,
    project: &Path,
    inputs: &ProjectInputs,
    host: &HostSelection,
    evaluated: Option<&EvaluatedProject>,
    target_framework: Option<&str>,
) -> crate::Result<Result<RestoreOutcome, DiscoveryFailure>> {
    let outcome = (|| {
        if !request.options.trust_msbuild {
            return Ok(Err(failure(
                Reason::TrustRequired,
                "MSBuild restore requires explicit evaluation trust",
            )));
        }
        let style = style(inputs, evaluated);
        if style == Style::None {
            return Ok(Ok(RestoreOutcome {
                provenance: RestoreProvenance::default(),
                performed: false,
            }));
        }
        let key = context_key(request, project, target_framework)?;
        let restored_here = inputs.restored.borrow().contains_key(&key);
        let current = current_inputs(
            request,
            project,
            inputs,
            evaluated,
            target_framework,
            style,
            &key,
        )?;
        if let Some(files) = current {
            return reuse_restore(inputs, &key, style, files);
        }
        if !request.options.allow_restore {
            return Ok(Err(failure(
                Reason::RestoreRequired,
                "current restore inputs are unavailable; grant ordinary repository restore separately",
            )));
        }
        if style == Style::PackagesConfig && !cfg!(windows) {
            return Ok(Err(failure(
                Reason::RestoreUnsupportedOnHost,
                "packages.config restoration requires Windows NuGet and the selected MSBuild",
            )));
        }
        if restored_here {
            // Name the cause: an unavailable inventory and a receipt that no longer matches
            // are different platform behaviours, and a generic message hides which one fired.
            let detail = post_restore_refusal(
                request,
                project,
                inputs,
                evaluated,
                target_framework,
                style,
                &key,
            )?;
            return Ok(Err(failure(
                Reason::RestoreFailed,
                format!(
                    "restore inputs changed or could not be validated after the authorized restore: {detail}"
                ),
            )));
        }
        let before = if style == Style::PackageReference {
            observe_restore(request, project, evaluated)?
        } else {
            None
        };
        let graph = match run_authorized_restore(request, project, host, style, inputs)? {
            Ok(graph) => graph,
            Err(failure) => return Ok(Err(failure)),
        };
        let current = match style {
            Style::PackageReference => restored_asset_inputs(
                request,
                project,
                evaluated,
                target_framework,
                before.as_ref(),
                graph.as_ref(),
            )?,
            Style::PackagesConfig => packages_inputs(project, inputs, evaluated)?,
            Style::None => unreachable!("handled above"),
        };
        let Some(files) = current else {
            return Ok(Err(failure(
                Reason::RestoreFailed,
                "NuGet reported success but required restore inputs could not be validated",
            )));
        };
        record_restore(inputs, &key, style, files).map(Ok)
    })();
    match outcome {
        Err(crate::Error::Config(message)) => Ok(Err(failure(Reason::MalformedInput, message))),
        result => result,
    }
}

/// Reuse current inputs only while this invocation's restore authority is intact.
fn reuse_restore(
    inputs: &ProjectInputs,
    key: &str,
    style: Style,
    files: Vec<PathBuf>,
) -> crate::Result<Result<RestoreOutcome, DiscoveryFailure>> {
    if let Some(receipt) = inputs.restored.borrow().get(key)
        && *receipt != receipt_digest(&files)?
    {
        return Ok(Err(failure(
            Reason::RestoreFailed,
            "restore inputs changed after the authorized restore",
        )));
    }
    Ok(Ok(RestoreOutcome {
        provenance: RestoreProvenance {
            style,
            inputs: files,
        },
        performed: false,
    }))
}

/// Record the receipt and outcome for a restore this invocation performed.
fn record_restore(
    inputs: &ProjectInputs,
    key: &str,
    style: Style,
    files: Vec<PathBuf>,
) -> crate::Result<RestoreOutcome> {
    inputs
        .restored
        .borrow_mut()
        .insert(key.to_owned(), receipt_digest(&files)?);
    Ok(RestoreOutcome {
        provenance: RestoreProvenance {
            style,
            inputs: files,
        },
        performed: true,
    })
}

fn regular_file(path: &Path) -> io::Result<bool> {
    match path.metadata() {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::DiscoveryOptions;
    use super::*;

    #[test]
    fn receipt_detects_same_size_content_mutation_with_preserved_timestamp() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("restore-input");
        std::fs::write(&path, "before").unwrap();
        let modified = path.metadata().unwrap().modified().unwrap();
        let before = receipt_digest(std::slice::from_ref(&path)).unwrap();
        std::fs::write(&path, "after!").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_ne!(receipt_digest(&[path]).unwrap(), before);
    }

    #[test]
    fn malformed_xml_and_dtd_are_not_restore_success() {
        let root = tempfile::tempdir().unwrap();
        let request = DiscoveryRequest::new(root.path(), DiscoveryOptions::default()).unwrap();
        let project = root.path().join("bad.csproj");
        for text in [
            "<Project><broken></Project>",
            "<!DOCTYPE Project SYSTEM 'file:///etc/passwd'><Project/>",
        ] {
            std::fs::write(&project, text).unwrap();
            assert!(matches!(
                inspect(&request, &project, &[]),
                Err(DiscoveryFailure {
                    reason: Reason::MalformedInput,
                    ..
                })
            ));
        }
    }

    #[test]
    fn mere_assets_existence_is_not_current_restore() {
        let root = tempfile::tempdir().unwrap();
        let request = DiscoveryRequest::new(root.path(), DiscoveryOptions::default()).unwrap();
        let project = root.path().join("App.csproj");
        std::fs::write(&project, "<Project Sdk=\"Microsoft.NET.Sdk\"/>").unwrap();
        std::fs::create_dir(root.path().join("obj")).unwrap();
        std::fs::write(root.path().join("obj/project.assets.json"), "{}").unwrap();
        assert!(
            asset_inputs(&request, &project, None, None, false)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn nuget_ranges_match_semantically_without_erasing_range_kind() {
        for (requested, recorded) in [
            ("1.0", "[1.0.0, )"),
            ("1.*", "[1.*, )"),
            ("*", "[*, )"),
            ("1.*-*", "[1.*-*, )"),
            ("*-*", "[*-*, )"),
            ("[1.0.0]", "[1.0.0, 1.0.0]"),
            ("1.0.0.0", "[1.0.0, )"),
            ("1.2.3.4", "[1.2.3.4, )"),
            ("1.0.0-beta", "[1.0.0-beta, )"),
        ] {
            assert!(
                ranges_match(requested, recorded),
                "{requested} != {recorded}"
            );
        }
        for (requested, recorded) in [
            ("[1.0.0]", "[1.0.0, )"),
            ("1.0", "[1.0.1, )"),
            ("1.0.0-beta", "[1.0.0, )"),
            ("1.0.0.1", "[1.0.0, )"),
            ("1.2.3.5", "[1.2.3.4, )"),
        ] {
            assert!(
                !ranges_match(requested, recorded),
                "{requested} == {recorded}"
            );
        }
        assert!(!ranges_match("1.2.3.4.5", "1.2.3.4.5"));
    }

    #[test]
    fn unrelated_central_package_version_does_not_select_classic_restore() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("App.csproj");
        std::fs::write(&project, "<Project />").unwrap();
        std::fs::write(
            root.path().join("Directory.Packages.props"),
            r#"<Project><ItemGroup><PackageVersion Include="Unreferenced" Version="1.0.0" /></ItemGroup></Project>"#,
        )
        .unwrap();
        let request = DiscoveryRequest::new(root.path(), DiscoveryOptions::default()).unwrap();
        let inputs = inspect(&request, &project, &[]).unwrap();
        assert_eq!(inputs.style, Style::None);
        assert_eq!(
            inputs.packages.get("unreferenced"),
            Some(&"1.0.0".to_owned())
        );
    }

    #[test]
    fn implicit_package_dependencies_require_target_evidence_without_relaxing_explicit_versions() {
        let assets = serde_json::json!({
            "project": {"frameworks": {"net461": {"dependencies": {
                "Microsoft.NETFramework.ReferenceAssemblies": {
                    "target": "Package", "version": "[1.0.3, )",
                    "suppressParent": "All", "autoReferenced": true
                }
            }}}}
        });
        let evaluated = |packages: &[(&str, &str, Option<&str>)]| EvaluatedProject {
            protocol_version: 1,
            success: true,
            project_path: PathBuf::from("App.csproj"),
            host: None,
            properties: BTreeMap::new(),
            items: BTreeMap::from([(
                "PackageReference".to_owned(),
                packages
                    .iter()
                    .map(|(name, version, private)| host::EvaluatedItem {
                        include: (*name).to_owned(),
                        full_path: PathBuf::new(),
                        metadata: BTreeMap::from([("Version".to_owned(), (*version).to_owned())])
                            .into_iter()
                            .chain(
                                private.map(|value| ("PrivateAssets".to_owned(), value.to_owned())),
                            )
                            .collect(),
                    })
                    .collect(),
            )]),
            imports: Vec::new(),
            glob_patterns: Vec::new(),
            diagnostics: Vec::new(),
            cache_eligible: false,
            cache_ineligibility: Vec::new(),
        };
        // An implicit dependency exists only in the assets and the native graph.
        let implicit_only = evaluated(&[]);
        assert!(dependencies_match(
            &assets,
            &implicit_only,
            Some("net461"),
            true
        ));
        assert!(!dependencies_match(
            &assets,
            &implicit_only,
            Some("net461"),
            false
        ));
        // An explicit reference still has to agree with the recorded version and asset flags.
        let wrong_version = evaluated(&[(
            "Microsoft.NETFramework.ReferenceAssemblies",
            "9.9.9",
            Some("All"),
        )]);
        assert!(!dependencies_match(
            &assets,
            &wrong_version,
            Some("net461"),
            true
        ));
        let wrong_flags = evaluated(&[(
            "Microsoft.NETFramework.ReferenceAssemblies",
            "1.0.3",
            Some("None"),
        )]);
        assert!(!dependencies_match(
            &assets,
            &wrong_flags,
            Some("net461"),
            true
        ));
        let explicit = evaluated(&[(
            "Microsoft.NETFramework.ReferenceAssemblies",
            "1.0.3",
            Some("All"),
        )]);
        assert!(dependencies_match(&assets, &explicit, Some("net461"), true));
    }

    #[test]
    fn legacy_destination_uses_solution_context_and_config_precedence_without_ancestor_guesses() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("one")).unwrap();
        std::fs::create_dir(root.join("two")).unwrap();
        let project = root.join("App.csproj");
        std::fs::write(&project, "<Project/>").unwrap();
        std::fs::write(
            root.join("packages.config"),
            "<packages><package id=\"Example\" version=\"1.0.0\" /></packages>",
        )
        .unwrap();
        std::fs::write(root.join("one/App.sln"), "").unwrap();
        std::fs::write(root.join("two/App.sln"), "").unwrap();
        std::fs::write(
            root.join("NuGet.Config"),
            "<configuration><config><clear /></config></configuration>",
        )
        .unwrap();
        let request = DiscoveryRequest::new(&root, DiscoveryOptions::default()).unwrap();
        let no_solution = inspect(&request, &project, &[]).unwrap();
        assert!(
            package_roots(&project, &no_solution, None)
                .unwrap()
                .is_empty()
        );
        let solutions = [PathBuf::from("one/App.sln"), PathBuf::from("two/App.sln")];
        let single = inspect(&request, &project, &solutions[..1]).unwrap();
        assert_eq!(
            package_roots(&project, &single, None).unwrap(),
            vec![root.join("one/packages")]
        );
        let ambiguous = inspect(&request, &project, &solutions).unwrap();
        assert!(
            packages_inputs(&project, &ambiguous, None)
                .unwrap()
                .is_none()
        );
        assert!(
            package_roots(&project, &ambiguous, None)
                .unwrap()
                .is_empty()
        );
        std::fs::write(root.join("NuGet.Config"),
            "<configuration><config><add key=\"RePoSiToRyPaTh\" value=\"custom\" /></config></configuration>").unwrap();
        let configured = inspect(&request, &project, &solutions).unwrap();
        assert_eq!(
            package_roots(&project, &configured, None).unwrap(),
            vec![root.join("custom")]
        );
        std::fs::create_dir(root.join("one/.nuget")).unwrap();
        std::fs::write(root.join("one/.nuget/NuGet.Config"),
            "<configuration><config><clear /><add key=\"repositoryPath\" value=\"nearby\" /></config></configuration>").unwrap();
        let selected = inspect(&request, &project, &solutions[..1]).unwrap();
        assert_eq!(
            package_roots(&project, &selected, None).unwrap(),
            vec![root.join("one/.nuget/nearby")]
        );
        std::fs::write(
            root.join("NuGet.Config"),
            "<configuration><config><clear /></config></configuration>",
        )
        .unwrap();
        let mut explicit = request;
        explicit.options.context.global_properties.insert(
            "sOlUtIoNdIr".into(),
            root.join("two").to_string_lossy().into_owned(),
        );
        let selected = inspect(&explicit, &project, &solutions).unwrap();
        assert_eq!(
            package_roots(&project, &selected, None).unwrap(),
            vec![root.join("two/packages")]
        );
        assert!(!root.join("packages").exists());
        assert!(!root.join("one/packages").exists());
        assert!(!root.join("two/packages").exists());
        let external = root.join("external.config");
        std::fs::write(&external, "<configuration><config><add key=\"repositoryPath\" value=\"external-packages\" /></config></configuration>").unwrap();
        let mut selected = selected;
        selected.external_nuget_configs = vec![external];
        selected.external_config_locations_known = true;
        std::fs::write(root.join("NuGet.Config"), "<configuration/>").unwrap();
        assert!(package_roots(&project, &selected, None).unwrap().is_empty());
        std::fs::write(
            root.join("NuGet.Config"),
            "<configuration><config><clear /></config></configuration>",
        )
        .unwrap();
        assert_eq!(
            package_roots(&project, &selected, None).unwrap(),
            vec![root.join("two/packages")]
        );
        std::fs::write(root.join("NuGet.Config"), "<configuration><config><add key=\"repositoryPath\" value=\"override\" /></config></configuration>").unwrap();
        assert_eq!(
            package_roots(&project, &selected, None).unwrap(),
            vec![root.join("override")]
        );
    }

    #[test]
    fn stale_generated_inputs_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("project");
        let generated = root.path().join("assets");
        std::fs::write(&generated, "old").unwrap();
        std::fs::write(&input, "new").unwrap();
        let file = std::fs::File::options().write(true).open(&input).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
            .unwrap();
        assert!(!fresh_files(&[input], &[generated]).unwrap());
    }

    #[test]
    fn equal_timestamps_do_not_establish_unreceipted_restore_freshness() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("project");
        let generated = root.path().join("assets");
        let timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for path in [&input, &generated] {
            std::fs::write(path, "content").unwrap();
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(timestamp)
                .unwrap();
        }
        assert!(
            !fresh_files(
                std::slice::from_ref(&input),
                std::slice::from_ref(&generated)
            )
            .unwrap()
        );
        std::fs::File::options()
            .write(true)
            .open(&generated)
            .unwrap()
            .set_modified(timestamp + std::time::Duration::from_secs(1))
            .unwrap();
        assert!(fresh_files(&[input], &[generated]).unwrap());
    }

    #[test]
    fn receipt_distinguishes_paths_from_file_contents() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("input");
        let second = root.path().join("inputx");
        std::fs::write(&first, "xrest").unwrap();
        std::fs::write(&second, "rest").unwrap();
        assert_ne!(
            receipt_digest(&[first]).unwrap(),
            receipt_digest(&[second]).unwrap()
        );
    }
}
