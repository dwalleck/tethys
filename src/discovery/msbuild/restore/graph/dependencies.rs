//! Exact dependency evidence within an already project-bound native restore graph.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::Entry;

fn targets_framework(
    frameworks: &Map<String, Value>,
    target_frameworks: &str,
    framework: &str,
) -> bool {
    let Some((key, _)) = super::framework_key(frameworks, framework) else {
        return false;
    };
    target_frameworks.split(';').map(str::trim).any(|target| {
        super::framework_key(frameworks, target).is_some_and(|(candidate, _)| candidate == key)
    })
}

pub(super) fn package_dependencies_match(
    frameworks: &Map<String, Value>,
    entries: &[&Entry],
) -> bool {
    let mut seen = BTreeMap::<&str, BTreeSet<String>>::new();
    for entry in entries.iter().filter(|entry| entry.kind == "Dependency") {
        if entry.id.is_empty() || entry.target_frameworks.is_empty() {
            return false;
        }
        for framework in entry.target_frameworks.split(';').map(str::trim) {
            let Some((key, value)) = super::framework_key(frameworks, framework) else {
                return false;
            };
            let Some(dependencies) = value["dependencies"].as_object() else {
                return false;
            };
            let Some(dependency) = dependencies
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(&entry.id))
                .map(|(_, value)| value)
            else {
                return false;
            };
            let Some(version) = dependency["version"].as_str() else {
                return false;
            };
            let version_range = if !entry.version_override.is_empty() {
                entry.version_override.as_str()
            } else if !entry.version_range.is_empty() {
                entry.version_range.as_str()
            } else {
                let mut central = None;
                for candidate in entries
                    .iter()
                    .filter(|candidate| candidate.kind == "CentralPackageVersion")
                {
                    if candidate.id.eq_ignore_ascii_case(&entry.id)
                        && !candidate.version_range.is_empty()
                        && targets_framework(frameworks, &candidate.target_frameworks, framework)
                    {
                        if central.is_some() {
                            return false;
                        }
                        central = Some(candidate.version_range.as_str());
                    }
                }
                let Some(central) = central else {
                    return false;
                };
                central
            };
            if dependency["target"]
                .as_str()
                .is_some_and(|target| target != "Package")
                || !super::super::ranges_match(version_range, version)
                || !seen
                    .entry(key.as_str())
                    .or_default()
                    .insert(entry.id.to_ascii_lowercase())
            {
                return false;
            }
            let metadata = [
                (&entry.include_assets, "all"),
                (&entry.exclude_assets, "none"),
                (&entry.private_assets, "contentfiles;analyzers;build"),
            ]
            .map(|(value, default)| {
                Some(if value.is_empty() {
                    default
                } else {
                    value.as_str()
                })
            });
            if !super::super::asset_metadata_matches(dependency, metadata) {
                return false;
            }
        }
    }
    for (framework, value) in frameworks {
        let dependencies = &value["dependencies"];
        let count = if let Some(dependencies) = dependencies.as_object() {
            dependencies
                .values()
                .filter(|dependency| {
                    dependency["target"]
                        .as_str()
                        .is_none_or(|target| target == "Package")
                })
                .count()
        } else if dependencies.is_null() {
            0
        } else {
            return false;
        };
        if count != seen.get(framework.as_str()).map_or(0, BTreeSet::len) {
            return false;
        }
    }
    true
}

pub(super) fn exact_version(range: &str) -> Option<String> {
    let Some(inner) = range
        .trim()
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    else {
        tracing::debug!("restore download range is not exact");
        return None;
    };
    let (lower, upper) = inner.split_once(',').unwrap_or((inner, inner));
    let lower = super::super::normalized_version(lower)?;
    let upper = super::super::normalized_version(upper)?;
    if lower.contains('*') || lower != upper {
        tracing::debug!("restore download range is not exact");
        return None;
    }
    Some(lower)
}
