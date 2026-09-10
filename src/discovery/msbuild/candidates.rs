//! Non-executing container enumeration and canonical workspace containment.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use quick_xml::{Reader, events::Event};
use serde::Deserialize;

use super::input::bounded_read;

use super::super::{
    DiscoveryDiagnostic, DiscoveryFailure, DiscoveryFailureReason, DiscoveryIssue, ProjectKey,
};

pub(super) struct Candidate {
    pub key: ProjectKey,
    pub path: PathBuf,
    pub containers: Vec<PathBuf>,
    /// Canonical workspace-relative actual solutions, not solution filters.
    pub(super) solution_paths: Vec<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct WorkspaceInventory {
    pub paths: Vec<PathBuf>,
    pub has_symlinks: bool,
    pub issues: Vec<DiscoveryIssue>,
}

pub(super) struct Candidates {
    pub projects: Vec<Candidate>,
    pub issues: Vec<DiscoveryIssue>,
    pub inventory: WorkspaceInventory,
}

fn failure(
    path: &Path,
    reason: DiscoveryFailureReason,
    message: impl Into<String>,
) -> DiscoveryFailure {
    DiscoveryFailure {
        reason,
        diagnostics: vec![DiscoveryDiagnostic {
            message: message.into(),
            file: Some(path.to_path_buf()),
            ..DiscoveryDiagnostic::default()
        }],
    }
}

impl WorkspaceInventory {
    /// Retain traversal uncertainty without hiding readable siblings.
    fn observe<T>(&mut self, path: &Path, result: std::io::Result<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "Incomplete discovery inventory");
                self.issues.push(DiscoveryIssue {
                    path: path.to_path_buf(),
                    failure: failure(
                        path,
                        DiscoveryFailureReason::EvaluationFailed,
                        error.to_string(),
                    ),
                });
                None
            }
        }
    }
}

fn malformed(path: &Path, message: impl Into<String>) -> DiscoveryFailure {
    failure(path, DiscoveryFailureReason::MalformedInput, message)
}

/// Candidate containers are untrusted before any `MSBuild` trust grant. Keep
/// their cap aligned with the existing bounded XML input convention. The
/// bounded read consumes one byte beyond the cap so an exact-boundary file is
/// accepted without a metadata/read TOCTOU check.
const SOLUTION_CONTAINER_LIMIT: u64 = 4 * 1024 * 1024;

enum CandidateError {
    Failure(DiscoveryFailure),
    Fatal(crate::Error),
}

impl From<DiscoveryFailure> for CandidateError {
    fn from(failure: DiscoveryFailure) -> Self {
        Self::Failure(failure)
    }
}

pub(super) fn relative_path(
    root: &Path,
    path: &Path,
    allow_missing: bool,
) -> Result<PathBuf, DiscoveryFailure> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let resolved = match absolute.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Resolve each existing component before applying '..': lexical cleanup
            // before resolving symlinks would authorize a different physical path.
            let mut resolved = PathBuf::new();
            for component in absolute.components() {
                match component {
                    Component::Prefix(prefix) => {
                        // A Windows prefix is not yet the rooted path: canonicalizing
                        // it can fail or resolve a drive's current directory instead.
                        // Accumulate it until RootDir before accessing the filesystem.
                        resolved.push(prefix.as_os_str());
                    }
                    Component::CurDir => {}
                    Component::ParentDir => {
                        resolved.pop();
                    }
                    component => {
                        resolved.push(component.as_os_str());
                        match resolved.canonicalize() {
                            Ok(canonical) => resolved = canonical,
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                match fs::symlink_metadata(&resolved) {
                                    Ok(metadata) if metadata.is_symlink() => {
                                        return Err(malformed(path, "Unresolved symbolic link"));
                                    }
                                    Ok(_) => {}
                                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                                    Err(error) => return Err(malformed(path, error.to_string())),
                                }
                            }
                            Err(error) => return Err(malformed(path, error.to_string())),
                        }
                    }
                }
            }
            resolved
        }
        Err(error) => return Err(malformed(path, error.to_string())),
    };
    let relative = resolved.strip_prefix(root).map_err(|_| {
        failure(
            path,
            DiscoveryFailureReason::OutsideWorkspaceInput,
            "Path escapes the workspace",
        )
    })?;
    if !allow_missing {
        let metadata =
            fs::metadata(&resolved).map_err(|error| malformed(path, error.to_string()))?;
        if !metadata.is_file() {
            return Err(malformed(path, "Expected a regular file"));
        }
    }
    Ok(relative.to_path_buf())
}

pub(super) fn project_key(root: &Path, path: &Path) -> Result<ProjectKey, DiscoveryFailure> {
    let relative = relative_path(root, path, true)?;
    let text = relative
        .to_str()
        .ok_or_else(|| malformed(path, "Project path is not UTF-8"))?;
    Ok(ProjectKey(text.replace(std::path::MAIN_SEPARATOR, "/")))
}

fn extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn candidate_path(path: &Path) -> bool {
    ["csproj", "sln", "slnx", "slnf"]
        .iter()
        .any(|suffix| extension(path, suffix))
}

fn directory_name_matches(
    directory: &Path,
    actual: &std::ffi::OsStr,
    expected: &str,
) -> std::io::Result<bool> {
    if actual == expected {
        return Ok(true);
    }
    if !actual.eq_ignore_ascii_case(expected) {
        return Ok(false);
    }
    // Compare existing identities instead of assuming an OS-wide casing policy:
    // Windows directories may be case-sensitive, and Unix volumes may not be.
    let alias = match directory.join(expected).canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    Ok(alias == directory.join(actual).canonicalize()?)
}

fn excluded_physical_ancestry(root: &Path, directory: &Path) -> std::io::Result<bool> {
    for ancestor in directory.ancestors().take_while(|path| *path != root) {
        let (Some(parent), Some(name)) = (ancestor.parent(), ancestor.file_name()) else {
            break;
        };
        for excluded in ["bin", "obj", "target", ".git", ".rivets"] {
            if directory_name_matches(parent, name, excluded)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn symlink_target(root: &Path, path: &Path) -> Option<PathBuf> {
    match path.canonicalize() {
        Ok(physical) if physical.starts_with(root) => Some(physical),
        Ok(_) => None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            tracing::warn!(path = %path.display(), error = %error, "Skipping unresolved symlink; inventory is not cache-qualified");
            None
        }
    }
}

enum WalkFrame {
    Enter {
        logical: PathBuf,
        physical: PathBuf,
        generated: bool,
    },
    Leave(PathBuf),
}

/// Advance depth-first traversal while keeping only physical ancestors active.
fn next_directory(
    pending: &mut Vec<WalkFrame>,
    ancestors: &mut std::collections::HashSet<PathBuf>,
) -> Option<(PathBuf, PathBuf, bool)> {
    while let Some(frame) = pending.pop() {
        match frame {
            WalkFrame::Leave(physical) => {
                ancestors.remove(&physical);
            }
            WalkFrame::Enter {
                logical,
                physical,
                generated,
            } => {
                if ancestors.insert(physical.clone()) {
                    pending.push(WalkFrame::Leave(physical.clone()));
                    return Some((logical, physical, generated));
                }
            }
        }
    }
    None
}

fn walk(root: &Path) -> crate::Result<(WorkspaceInventory, Vec<PathBuf>)> {
    let mut inventory = WorkspaceInventory {
        paths: Vec::new(),
        has_symlinks: false,
        issues: Vec::new(),
    };
    let mut candidates = Vec::new();
    let mut ancestors = std::collections::HashSet::new();
    let mut pending = vec![WalkFrame::Enter {
        logical: root.to_path_buf(),
        physical: root.to_path_buf(),
        generated: false,
    }];
    while let Some((directory, physical_directory, generated)) =
        next_directory(&mut pending, &mut ancestors)
    {
        let Some(entries) = inventory.observe(&directory, fs::read_dir(&directory)) else {
            continue;
        };
        for entry in entries {
            let Some(entry) = inventory.observe(&directory, entry) else {
                continue;
            };
            let name = entry.file_name();
            let excluded = (|| {
                Ok(directory_name_matches(&directory, &name, ".git")?
                    || directory_name_matches(&directory, &name, ".rivets")?)
            })();
            match inventory.observe(&entry.path(), excluded) {
                Some(false) => {}
                Some(true) | None => continue,
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|error| crate::Error::Config(error.to_string()))?
                .to_path_buf();
            let Some(kind) = inventory.observe(&path, entry.file_type()) else {
                continue;
            };
            let automatic_candidate = candidate_path(&path) && !generated && !kind.is_dir();
            let physical = if kind.is_symlink() {
                inventory.has_symlinks = true;
                symlink_target(root, &path)
            } else {
                Some(physical_directory.join(&name))
            };
            let Some(metadata) =
                inventory.observe(&path, physical.as_ref().map(fs::metadata).transpose())
            else {
                continue;
            };
            if kind.is_symlink()
                && let (Some(physical), Some(metadata)) = (&physical, &metadata)
            {
                let directory = if metadata.is_dir() {
                    physical.as_path()
                } else {
                    physical.parent().unwrap_or(root)
                };
                match inventory.observe(&path, excluded_physical_ancestry(root, directory)) {
                    Some(false) => {}
                    Some(true) | None => continue,
                }
            }
            // Keep unresolved/outside candidate aliases so discovery emits its
            // typed failure instead of silently dropping the project.
            if automatic_candidate {
                candidates.push(relative.clone());
            }
            let (Some(physical), Some(metadata)) = (physical, metadata) else {
                continue;
            };
            if metadata.is_dir() {
                let generated = (|| {
                    Ok(generated
                        || directory_name_matches(&directory, &name, "bin")?
                        || directory_name_matches(&directory, &name, "obj")?
                        || directory_name_matches(&directory, &name, "target")?)
                })();
                let Some(generated) = inventory.observe(&path, generated) else {
                    continue;
                };
                pending.push(WalkFrame::Enter {
                    logical: path,
                    physical,
                    generated,
                });
            } else if metadata.is_file() {
                inventory.paths.push(relative);
            }
        }
    }
    inventory.paths.sort();
    inventory
        .issues
        .sort_by(|left, right| left.path.cmp(&right.path));
    candidates.sort();
    Ok((inventory, candidates))
}

pub(super) fn inventory(root: &Path) -> crate::Result<WorkspaceInventory> {
    walk(root).map(|(inventory, _)| inventory)
}

fn declared_path(base: &Path, text: &str) -> Result<PathBuf, DiscoveryFailure> {
    if text.trim().is_empty() || text.contains('\0') {
        return Err(malformed(base, "Empty or invalid declared path"));
    }
    let portable = text.replace('\\', "/");
    #[cfg(not(windows))]
    if portable.starts_with("//") || portable.as_bytes().get(1) == Some(&b':') {
        return Err(failure(
            base,
            DiscoveryFailureReason::OutsideWorkspaceInput,
            "Foreign absolute path is outside this workspace",
        ));
    }
    Ok(base.join(portable))
}

fn read_container(root: &Path, path: &Path) -> Result<String, CandidateError> {
    let relative = relative_path(root, path, false)?;
    let bytes = match bounded_read(&root.join(relative), SOLUTION_CONTAINER_LIMIT) {
        Ok(bytes) => bytes,
        Err(crate::Error::Config(message)) => {
            return Err(CandidateError::Failure(malformed(path, message)));
        }
        Err(error) => return Err(CandidateError::Fatal(error)),
    };
    String::from_utf8(bytes).map_err(|error| {
        CandidateError::Failure(malformed(path, format!("Container is not UTF-8: {error}")))
    })
}

fn supported_solution_project_path(text: &str) -> bool {
    // Keep malformed C# declarations visible to the typed path diagnostics;
    // only path-shaped entries with a known unsupported extension are skipped.
    if text.trim().is_empty() || text.contains('\0') {
        return true;
    }
    let portable = text.replace('\\', "/");
    extension(Path::new(&portable), "csproj")
}

fn supported_solution_project_kind(kind: &str) -> bool {
    [
        // Legacy and SDK-style C# project types used by Visual Studio solutions.
        "{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}",
        "{9A19103F-16F7-4668-BE54-9A1E7A4F7556}",
    ]
    .iter()
    .any(|supported| kind.eq_ignore_ascii_case(supported))
}

fn sln_paths(path: &Path, text: &str) -> Result<Vec<String>, DiscoveryFailure> {
    if !text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with("Microsoft Visual Studio Solution File, Format Version")
    {
        return Err(malformed(path, "Missing solution format header"));
    }
    let mut projects = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| {
        line.starts_with("Project(") || line.starts_with("Project ") || line.starts_with("Project=")
    }) {
        let Some((kind, declaration)) = line
            .strip_prefix("Project(\"")
            .and_then(|line| line.split_once("\")"))
        else {
            return Err(malformed(path, "Malformed solution project declaration"));
        };
        let Some(mut rest) = declaration.trim_start().strip_prefix('=') else {
            return Err(malformed(path, "Missing solution project assignment"));
        };
        let mut fields = Vec::with_capacity(3);
        for index in 0..3 {
            let Some(quoted) = rest.trim_start().strip_prefix('"') else {
                return Err(malformed(path, "Unquoted solution project field"));
            };
            let Some((field, remaining)) = quoted.split_once('"') else {
                return Err(malformed(path, "Unterminated solution project field"));
            };
            fields.push(field);
            rest = remaining.trim_start();
            if index < 2 {
                rest = rest
                    .strip_prefix(',')
                    .ok_or_else(|| malformed(path, "Missing solution project separator"))?;
            }
        }
        if !rest.trim().is_empty() {
            return Err(malformed(
                path,
                "Trailing solution project declaration text",
            ));
        }
        // Solution folders and every non-C# project kind are not candidates.
        // Classify from the declaration GUID before validating its path: a
        // missing vcxproj or URL-style WebSite entry must not create a
        // spurious path diagnostic, while supported C# paths still do.
        if supported_solution_project_kind(kind) {
            projects.push(fields[1].to_owned());
        }
    }
    Ok(projects)
}

fn slnx_paths(path: &Path, text: &str) -> Result<Vec<String>, DiscoveryFailure> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().expand_empty_elements = true;
    let mut depth = 0usize;
    let mut seen_root = false;
    let mut projects = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(path, error.to_string()))?
        {
            Event::Start(element) => {
                if depth == 0 {
                    if seen_root || element.name().as_ref() != b"Solution" {
                        return Err(malformed(path, "Expected one Solution XML root"));
                    }
                    seen_root = true;
                }
                let mut declared = None;
                for attribute in element.attributes() {
                    let attribute =
                        attribute.map_err(|error| malformed(path, error.to_string()))?;
                    // Native solution parsing preserves literal attribute whitespace.
                    let decoded = reader
                        .decoder()
                        .decode(attribute.value.as_ref())
                        .map_err(|error| malformed(path, error.to_string()))?;
                    let value = quick_xml::escape::unescape(&decoded)
                        .map_err(|error| malformed(path, error.to_string()))?;
                    if attribute.key.as_ref() == b"Path" {
                        declared = Some(value.into_owned());
                    }
                }
                if element.name().as_ref() == b"Project" {
                    let declared =
                        declared.ok_or_else(|| malformed(path, "Solution Project lacks Path"))?;
                    if supported_solution_project_path(&declared) {
                        projects.push(declared);
                    }
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| malformed(path, "Unbalanced solution XML"))?;
            }
            Event::DocType(_) | Event::GeneralRef(_) | Event::CData(_) => {
                return Err(malformed(
                    path,
                    "DTD and entity references are not supported in solution XML",
                ));
            }
            Event::Text(text) if !text.iter().all(u8::is_ascii_whitespace) => {
                return Err(malformed(path, "Unexpected solution XML text"));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !seen_root || depth != 0 {
        return Err(malformed(path, "Incomplete solution XML"));
    }
    Ok(projects)
}

#[derive(Deserialize)]
struct Filter {
    solution: FilterSolution,
}
#[derive(Deserialize)]
struct FilterSolution {
    path: String,
    projects: Vec<String>,
}

fn solution_paths(root: &Path, path: &Path) -> Result<Vec<String>, CandidateError> {
    let text = read_container(root, path)?;
    let paths = if extension(path, "sln") {
        sln_paths(path, &text)
    } else if extension(path, "slnx") {
        slnx_paths(path, &text)
    } else {
        Err(malformed(
            path,
            "Filter must reference a solution or solution XML file",
        ))
    };
    paths.map_err(CandidateError::Failure)
}

fn filter_membership(
    root: &Path,
    base: &Path,
    members: Vec<String>,
    container: &Path,
    issues: &mut Vec<DiscoveryIssue>,
) -> BTreeSet<ProjectKey> {
    let mut membership = BTreeSet::new();
    for member in members {
        match declared_path(base, &member).and_then(|declared| project_key(root, &declared)) {
            Ok(key) => {
                membership.insert(key);
            }
            Err(failure) => issues.push(DiscoveryIssue {
                path: container.to_path_buf(),
                failure,
            }),
        }
    }
    membership
}

pub(super) fn discover(root: &Path) -> crate::Result<Candidates> {
    let (inventory, paths) = walk(root)?;
    let mut projects = BTreeMap::<ProjectKey, Candidate>::new();
    let mut issues = inventory.issues.clone();
    let mut add = |path: &Path, origin: Option<(&Path, &Path)>| -> Result<(), DiscoveryFailure> {
        // Do not validate paths for unsupported solution project kinds. This
        // is also a defensive guard for path-based solution formats.
        if !extension(path, "csproj") {
            return Ok(());
        }
        let key = project_key(root, path)?;
        let relative = relative_path(root, path, false)?;
        fs::File::open(root.join(&relative)).map_err(|error| malformed(path, error.to_string()))?;
        let candidate = projects.entry(key.clone()).or_insert_with(|| Candidate {
            key,
            path: root.join(relative),
            containers: Vec::new(),
            solution_paths: Vec::new(),
        });
        if let Some((container, solution)) = origin {
            candidate.containers.push(container.to_path_buf());
            candidate.solution_paths.push(solution.to_path_buf());
        }
        Ok(())
    };
    for logical in paths {
        let path = root.join(&logical);
        let result = (|| -> Result<(), CandidateError> {
            if extension(&path, "csproj") {
                return add(&path, None).map_err(CandidateError::Failure);
            }
            let container = relative_path(root, &path, false)?;
            let parent = path
                .parent()
                .ok_or_else(|| malformed(&path, "Container has no parent"))?;
            let (base, declarations, solution) = if extension(&path, "slnf") {
                let filter: Filter = serde_json::from_str(&read_container(root, &path)?)
                    .map_err(|error| malformed(&path, error.to_string()))?;
                let solution = declared_path(parent, &filter.solution.path)?;
                let members = solution_paths(root, &solution)?;
                let base = solution
                    .parent()
                    .ok_or_else(|| malformed(&solution, "Solution has no parent"))?;
                let membership = filter_membership(root, base, members, &logical, &mut issues);
                for member in &filter.solution.projects {
                    let selected = declared_path(base, member)?;
                    if !membership.contains(&project_key(root, &selected)?) {
                        return Err(CandidateError::Failure(malformed(
                            &path,
                            "Filter project is not declared by its referenced solution",
                        )));
                    }
                }
                (
                    base.to_path_buf(),
                    filter.solution.projects,
                    relative_path(root, &solution, false)?,
                )
            } else {
                (
                    parent.to_path_buf(),
                    solution_paths(root, &path)?,
                    container.clone(),
                )
            };
            for declaration in declarations {
                match declared_path(&base, &declaration)
                    .and_then(|declared| add(&declared, Some((&container, &solution))))
                {
                    Ok(()) => {}
                    Err(failure) => issues.push(DiscoveryIssue {
                        path: logical.clone(),
                        failure,
                    }),
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {}
            Err(CandidateError::Failure(failure)) => issues.push(DiscoveryIssue {
                path: logical,
                failure,
            }),
            Err(CandidateError::Fatal(error)) => return Err(error),
        }
    }
    for candidate in projects.values_mut() {
        candidate.containers.sort();
        candidate.containers.dedup();
        candidate.solution_paths.sort();
        candidate.solution_paths.dedup();
    }
    Ok(Candidates {
        projects: projects.into_values().collect(),
        issues,
        inventory,
    })
}
