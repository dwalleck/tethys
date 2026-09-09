//! Corroborate dependencies against the graph returned by the authorized Restore.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

mod dependencies;

#[derive(Deserialize)]
struct Response {
    #[serde(rename = "Items")]
    items: Items,
}

#[derive(Deserialize)]
struct Items {
    #[serde(rename = "_RestoreGraphEntryFiltered")]
    graph: Vec<Entry>,
}

// Only retain evidence used here, never arbitrary task metadata or feed credentials.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Entry {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(default)]
    project_unique_name: String,
    #[serde(default)]
    project_path: String,
    #[serde(default)]
    project_style: String,
    #[serde(default)]
    output_path: String,
    #[serde(default)]
    target_framework: String,
    #[serde(default)]
    target_frameworks: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    version_range: String,
    #[serde(default)]
    version_override: String,
    #[serde(default)]
    include_assets: String,
    #[serde(default)]
    exclude_assets: String,
    #[serde(default)]
    private_assets: String,
}

pub(super) struct RestoreGraphEvidence {
    entries: Vec<Entry>,
}

/// Match a native graph framework name to an assets framework key.
///
/// `NuGet` records the platform version in the assets key (`net8.0-tizen8.0`) and the
/// unversioned alias in `targetAlias` (`net8.0-tizen`), which is the spelling the
/// restore graph reports. Only an exact key or a unique alias match is accepted; any
/// other difference, or an ambiguous alias, is a mismatch rather than a guess.
pub(super) fn framework_key<'a>(
    frameworks: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Option<(&'a String, &'a Value)> {
    if let Some(entry) = frameworks.get_key_value(name) {
        return Some(entry);
    }
    let mut matched = None;
    for (key, value) in frameworks {
        let Some(alias) = value["targetAlias"].as_str() else {
            continue;
        };
        if !alias.eq_ignore_ascii_case(name) {
            continue;
        }
        if matched.is_some() {
            return None;
        }
        matched = Some((key, value));
    }
    matched
}

impl RestoreGraphEvidence {
    pub(super) fn parse(stdout: &[u8]) -> Option<Self> {
        // Native warnings may precede the getItem JSON even at quiet verbosity.
        // Require a complete trailing JSON document, not a brace scraped from a log.
        let start = stdout
            .split_inclusive(|byte| *byte == b'\n')
            .scan(0, |offset, line| {
                let start = *offset;
                *offset += line.len();
                Some((start, line))
            })
            .find_map(|(offset, line)| (line.trim_ascii() == b"{").then_some(offset));
        let Some(start) = start else {
            tracing::debug!("authorized restore did not return a graph document");
            return None;
        };
        if let Ok(response) = serde_json::from_slice::<Response>(&stdout[start..]) {
            Some(Self {
                entries: response.items.graph,
            })
        } else {
            tracing::debug!("authorized restore graph document is unavailable or malformed");
            None
        }
    }

    pub(super) fn matches(&self, assets: &Value, project: &Path) -> crate::Result<bool> {
        let Some(frameworks) = assets["project"]["frameworks"].as_object() else {
            return Ok(false);
        };
        let Some(output) = assets["project"]["restore"]["outputPath"].as_str() else {
            return Ok(false);
        };
        let mut restore_spec = false;
        let mut project_spec = false;
        let mut downloads = BTreeMap::<&str, Vec<(String, String)>>::new();
        let mut selected = Vec::new();
        for entry in &self.entries {
            if !super::native_project_matches(&entry.project_unique_name, project)? {
                continue;
            }
            selected.push(entry);
            match entry.kind.as_str() {
                "RestoreSpec" => restore_spec = true,
                "ProjectSpec" => {
                    if project_spec
                        || entry.project_style != "PackageReference"
                        || !super::native_project_matches(&entry.project_path, project)?
                        || !output_matches(project, &entry.output_path, output)?
                    {
                        return Ok(false);
                    }
                    project_spec = true;
                }
                "TargetFrameworkInformation" => {
                    if framework_key(frameworks, &entry.target_framework).is_none()
                        || downloads
                            .insert(&entry.target_framework, Vec::new())
                            .is_some()
                    {
                        return Ok(false);
                    }
                }
                _ => {}
            }
        }
        if !restore_spec || !project_spec || downloads.len() != frameworks.len() {
            return Ok(false);
        }
        if !dependencies::package_dependencies_match(frameworks, &selected) {
            return Ok(false);
        }
        for entry in selected {
            if entry.kind != "DownloadDependency" {
                continue;
            }
            if entry.id.is_empty() || entry.target_frameworks.is_empty() {
                return Ok(false);
            }
            for framework in entry.target_frameworks.split(';') {
                let Some(recorded) = downloads.get_mut(framework) else {
                    return Ok(false);
                };
                for range in entry.version_range.split(';') {
                    let Some(version) = dependencies::exact_version(range) else {
                        return Ok(false);
                    };
                    recorded.push((entry.id.to_ascii_lowercase(), version));
                }
            }
        }
        for (framework, mut native) in downloads {
            let Some((_, value)) = framework_key(frameworks, framework) else {
                return Ok(false);
            };
            let recorded = &value["downloadDependencies"];
            let mut expected = Vec::new();
            if let Some(recorded) = recorded.as_array() {
                for download in recorded {
                    let (Some(name), Some(range)) =
                        (download["name"].as_str(), download["version"].as_str())
                    else {
                        return Ok(false);
                    };
                    let Some(version) = dependencies::exact_version(range) else {
                        return Ok(false);
                    };
                    expected.push((name.to_ascii_lowercase(), version));
                }
            } else if !recorded.is_null() {
                return Ok(false);
            }
            native.sort_unstable();
            expected.sort_unstable();
            if native != expected {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn output_matches(project: &Path, actual: &str, expected: &str) -> io::Result<bool> {
    if actual.is_empty() || expected.is_empty() {
        return Ok(false);
    }
    match (
        super::native_path(project, actual).canonicalize(),
        super::native_path(project, expected).canonicalize(),
    ) {
        (Ok(actual), Ok(expected)) => Ok(actual == expected),
        (Err(error), _) | (_, Err(error))
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound
                    | io::ErrorKind::NotADirectory
                    | io::ErrorKind::InvalidInput
            ) =>
        {
            tracing::debug!("restore graph output identity is unavailable");
            Ok(false)
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_framework_aliases_resolve_to_their_assets_key() {
        let assets = serde_json::json!({
            "net8.0-tizen8.0": {"targetAlias": "net8.0-tizen", "dependencies": {}},
            "net8.0": {"targetAlias": "net8.0", "dependencies": {}}
        });
        let frameworks = assets.as_object().unwrap();
        assert_eq!(
            framework_key(frameworks, "net8.0-tizen").unwrap().0,
            "net8.0-tizen8.0"
        );
        assert_eq!(
            framework_key(frameworks, "net8.0-tizen8.0").unwrap().0,
            "net8.0-tizen8.0"
        );
        assert_eq!(framework_key(frameworks, "net8.0").unwrap().0, "net8.0");
        // A different platform is never an alias match.
        assert!(framework_key(frameworks, "net8.0-android").is_none());
    }
    #[test]
    fn ambiguous_or_wrong_aliases_withhold_framework_evidence() {
        let ambiguous = serde_json::json!({
            "net8.0-a": {"targetAlias": "net8.0", "dependencies": {}},
            "net8.0-b": {"targetAlias": "net8.0", "dependencies": {}}
        });
        let frameworks = ambiguous.as_object().unwrap();
        assert!(framework_key(frameworks, "net8.0").is_none());
        assert!(framework_key(frameworks, "net8.0-android").is_none());
    }

    #[test]
    fn unavailable_graph_output_withholds_evidence_without_infrastructure_failure() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("App.csproj");
        std::fs::write(&project, "<Project/>").unwrap();
        let output = root.path().join("obj");
        std::fs::create_dir(&output).unwrap();
        let project = project.canonicalize().unwrap();
        let output = output.canonicalize().unwrap();
        let corroborates = |native_output: &Path, assets_output: &Path| {
            let response = serde_json::json!({
                "Items": {"_RestoreGraphEntryFiltered": [
                    {"Type": "RestoreSpec", "ProjectUniqueName": project},
                    {"Type": "ProjectSpec", "ProjectUniqueName": project,
                     "ProjectPath": project, "ProjectStyle": "PackageReference",
                     "OutputPath": native_output},
                    {"Type": "TargetFrameworkInformation", "ProjectUniqueName": project,
                     "TargetFramework": "net8.0"}
                ]}
            });
            let assets = serde_json::json!({"project": {
                "restore": {"outputPath": assets_output},
                "frameworks": {"net8.0": {}}
            }});
            RestoreGraphEvidence::parse(&serde_json::to_vec_pretty(&response).unwrap())
                .unwrap()
                .matches(&assets, &project)
                .unwrap()
        };
        assert!(corroborates(&output, &output));
        assert!(!corroborates(Path::new(""), project.parent().unwrap()));
        assert!(!corroborates(&root.path().join("missing"), &output));
        assert!(!corroborates(&output, &root.path().join("missing")));
        assert!(!corroborates(&project.join("not-a-directory"), &output));
    }
}
