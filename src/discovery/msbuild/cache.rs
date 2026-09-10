//! Opaque, qualified evaluation receipts and invocation-wide freshness checks.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::super::{
    DiscoveryCachePolicy, DiscoveryInputScope, DiscoveryRequest, EvaluationCacheEntry,
    EvaluationInput, ProjectKey, RestoreProvenance,
};
use super::candidates::WorkspaceInventory;
use super::host::{EvaluatedProject, HostSelection};

// Recipe 2 receipts encoded `SystemTime` with serde's unsigned epoch shape.
// Recipe 3 stores the epoch side and a lossless `Duration` offset, including
// subsecond timestamps before the epoch. The key and payload checks below
// deliberately reject older receipts.
const EVALUATION_RECIPE: u32 = 3;
const RESTORE_RECIPE: u32 = 3;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Stamp {
    canonical: PathBuf,
    length: u64,
    modified: EpochOffset,
    digest: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EpochOffset {
    before_epoch: bool,
    duration: Duration,
}

impl EpochOffset {
    fn from_system_time(value: SystemTime) -> Self {
        match value.duration_since(UNIX_EPOCH) {
            Ok(duration) => Self {
                before_epoch: false,
                duration,
            },
            Err(error) => Self {
                before_epoch: true,
                duration: error.duration(),
            },
        }
    }

    fn into_system_time(self) -> crate::Result<SystemTime> {
        let value = if self.before_epoch {
            UNIX_EPOCH.checked_sub(self.duration)
        } else {
            UNIX_EPOCH.checked_add(self.duration)
        };
        value.ok_or_else(|| {
            crate::Error::Internal(
                "persisted evaluation timestamp exceeds the SystemTime range".into(),
            )
        })
    }
}

pub(super) type Inputs = BTreeMap<PathBuf, Stamp>;

/// Consume already captured stamps without rereading inputs or interpreting receipts.
pub(super) fn input_scope(
    project: ProjectKey,
    inputs: Inputs,
) -> crate::Result<DiscoveryInputScope> {
    let inputs = inputs
        .into_iter()
        .map(|(path, stamp)| {
            Ok(EvaluationInput {
                path,
                canonical_path: stamp.canonical,
                length: stamp.length,
                modified: stamp.modified.into_system_time()?,
                digest: stamp.digest,
            })
        })
        .collect::<crate::Result<Vec<_>>>()?;
    Ok(DiscoveryInputScope { project, inputs })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    recipe: u32,
    key: String,
    inventory: String,
    inputs: Inputs,
    evaluation: EvaluatedProject,
    restore: RestoreProvenance,
    content_digest: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreReceipt {
    recipe: u32,
    key: String,
    receipts: BTreeMap<String, String>,
    content_digest: String,
}

pub(super) struct Cache<'a> {
    request: &'a DiscoveryRequest,
    entries: BTreeMap<&'a str, &'a EvaluationCacheEntry>,
    inventory: String,
    symlinks: bool,
    incomplete_inventory: bool,
    environment: String,
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn stamp(path: &Path) -> std::io::Result<Stamp> {
    let canonical = path.canonicalize()?;
    let before = fs::metadata(&canonical)?;
    if !before.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "evaluation input is not a file",
        ));
    }
    let mut file = fs::File::open(&canonical)?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 16_384];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let digest = format!("{:x}", hash.finalize());
    let after = fs::metadata(&canonical)?;
    if before.len() != after.len()
        || before.modified()? != after.modified()?
        || path.canonicalize()? != canonical
    {
        return Err(std::io::Error::other(
            "evaluation input changed while fingerprinting",
        ));
    }
    Ok(Stamp {
        canonical,
        length: after.len(),
        modified: EpochOffset::from_system_time(after.modified()?),
        digest,
    })
}

pub(super) fn capture(paths: impl IntoIterator<Item = PathBuf>) -> std::io::Result<Inputs> {
    paths
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|path| {
            let value = stamp(&path)?;
            Ok((path, value))
        })
        .collect()
}

pub(super) fn unchanged(inputs: &Inputs) -> bool {
    inputs.iter().all(|(path, old)| match stamp(path) {
        Ok(current) => current == *old,
        Err(error) => {
            tracing::debug!(path = %path.display(), %error, "input validation failed");
            false
        }
    })
}

pub(super) fn captured_restore_paths<'a>(
    inputs: &'a Inputs,
    restored: &BTreeSet<PathBuf>,
) -> BTreeSet<&'a Path> {
    restored
        .iter()
        .filter_map(|path| inputs.get_key_value(path))
        .flat_map(|(path, stamp)| [dunce::simplified(path), dunce::simplified(&stamp.canonical)])
        .collect()
}

pub(super) fn normalized_globals(request: &DiscoveryRequest) -> BTreeMap<String, String> {
    request
        .options
        .context
        .effective_globals()
        .into_iter()
        .map(|(key, value)| (key.to_ascii_lowercase(), value))
        .collect()
}

impl<'a> Cache<'a> {
    pub(super) fn new(
        request: &'a DiscoveryRequest,
        inventory: &WorkspaceInventory,
    ) -> crate::Result<Self> {
        // OS strings are hashed without lossy conversion and never persisted verbatim.
        let mut hash = Sha256::new();
        for (name, value) in request.options.environment.iter() {
            for bytes in [name.as_encoded_bytes(), value.as_encoded_bytes()] {
                hash.update(bytes.len().to_le_bytes());
                hash.update(bytes);
            }
        }
        Ok(Self {
            request,
            entries: request
                .cache
                .iter()
                .map(|entry| (entry.key.as_str(), entry))
                .collect(),
            inventory: digest(&serde_json::to_vec(&inventory.paths).map_err(|error| {
                crate::Error::Internal(format!("discovery evidence serialization failed: {error}"))
            })?),
            symlinks: inventory.has_symlinks,
            incomplete_inventory: !inventory.issues.is_empty(),
            environment: format!("{:x}", hash.finalize()),
        })
    }

    pub(super) fn key(
        &self,
        project: &Path,
        selector: Option<&str>,
        host: &HostSelection,
    ) -> crate::Result<String> {
        Ok(digest(
            &serde_json::to_vec(&(
                EVALUATION_RECIPE,
                &self.request.workspace_root,
                project,
                selector,
                normalized_globals(self.request),
                self.request.options.context.import_profile,
                &host.fingerprint,
                &self.environment,
            ))
            .map_err(|error| {
                crate::Error::Internal(format!("discovery evidence serialization failed: {error}"))
            })?,
        ))
    }

    pub(super) fn lookup(
        &self,
        key: &str,
        project: &Path,
        host: &HostSelection,
    ) -> std::result::Result<(EvaluatedProject, RestoreProvenance, Inputs), String> {
        if self.incomplete_inventory {
            return Err("incomplete_workspace_inventory".into());
        }
        if self.request.options.cache_policy == DiscoveryCachePolicy::Disabled {
            return Err("cache_disabled".into());
        }
        if let Some(reason) = host.cache_ineligibility(&self.request.options.environment) {
            return Err(reason.into());
        }
        if self.symlinks {
            return Err("unqualified_symlink_inventory".into());
        }
        let entry = self.entries.get(key).ok_or("cache_miss")?;
        let receipt: Receipt =
            serde_json::from_str(&entry.payload).map_err(|_| "corrupt_receipt")?;
        if receipt.recipe != EVALUATION_RECIPE
            || receipt.key != key
            || receipt.inventory != self.inventory
        {
            return Err("stale_recipe_or_context_or_inventory".into());
        }
        let content = serde_json::to_vec(&(&receipt.inputs, &receipt.evaluation, &receipt.restore))
            .map_err(|_| "invalid_receipt_content")?;
        if digest(&content) != receipt.content_digest {
            return Err("corrupt_receipt_content".into());
        }
        if receipt.evaluation.protocol_version != 1
            || receipt.evaluation.host.as_ref().is_none_or(|actual| {
                actual.kind != host.kind
                    || actual.path != host.msbuild_path
                    || actual.version.is_empty()
                    || actual.runtime.is_empty()
            })
        {
            return Err("unqualified_cached_host".into());
        }
        if receipt.evaluation.project_path != project
            || !receipt.evaluation.success
            || !receipt.evaluation.cache_eligible
            || !receipt.evaluation.cache_ineligibility.is_empty()
            || !receipt.evaluation.imports.is_empty()
        {
            return Err("unqualified_receipt".into());
        }
        let required = evaluation_paths(&receipt.evaluation, &receipt.restore);
        if required
            .iter()
            .any(|path| !receipt.inputs.contains_key(path))
            || !unchanged(&receipt.inputs)
        {
            return Err("changed_or_incomplete_inputs".into());
        }
        if !self.ineligibility(&receipt.evaluation, host).is_empty() {
            return Err("unqualified_recipe".into());
        }
        Ok((receipt.evaluation, receipt.restore, receipt.inputs))
    }

    pub(super) fn ineligibility(
        &self,
        evaluation: &EvaluatedProject,
        host: &HostSelection,
    ) -> Vec<String> {
        let mut reasons = evaluation.cache_ineligibility.clone();
        if self.incomplete_inventory {
            reasons.push("incomplete_workspace_inventory".into());
        }
        if let Some(reason) = host.cache_ineligibility(&self.request.options.environment) {
            reasons.push(reason.into());
        }
        if !evaluation.cache_eligible {
            reasons.push("native_recipe_not_eligible".into());
        }
        let property = |name| {
            evaluation
                .properties
                .get(name)
                .is_some_and(|value| !value.trim().is_empty())
        };
        if !(property("TargetFrameworks")
            || property("TargetFrameworkIdentifier") && property("TargetFrameworkVersion"))
        {
            reasons.push("incomplete_framework_identity".into());
        }
        if self.symlinks {
            reasons.push("unqualified_symlink_inventory".into());
        }
        if !evaluation.imports.is_empty() {
            reasons.push("unqualified_import_closure".into());
        }
        for pattern in &evaluation.glob_patterns {
            for expression in [&pattern.include, &pattern.exclude, &pattern.remove] {
                if expression.contains("**")
                    || expression.contains("$(")
                    || expression.contains("@(")
                    || expression.contains("%(")
                    || expression
                        .split(['/', '\\', ';'])
                        .any(|part| matches!(part, ".." | ".git" | ".rivets"))
                    || Path::new(expression).is_absolute()
                {
                    reasons.push("unqualified_glob_watch_set".into());
                }
            }
        }
        reasons.sort();
        reasons.dedup();
        reasons
    }

    pub(super) fn receipt(
        &self,
        key: String,
        evaluation: &EvaluatedProject,
        restore: &RestoreProvenance,
        inputs: &Inputs,
        host: &HostSelection,
    ) -> crate::Result<Option<EvaluationCacheEntry>> {
        if self.request.options.cache_policy == DiscoveryCachePolicy::Disabled
            || !self.ineligibility(evaluation, host).is_empty()
            || !unchanged(inputs)
        {
            return Ok(None);
        }
        let payload = serde_json::to_string(&Receipt {
            recipe: EVALUATION_RECIPE,
            key: key.clone(),
            inventory: self.inventory.clone(),
            inputs: inputs.clone(),
            evaluation: evaluation.clone(),
            restore: restore.clone(),
            content_digest: digest(&serde_json::to_vec(&(inputs, evaluation, restore)).map_err(
                |error| {
                    crate::Error::Internal(format!(
                        "discovery evidence serialization failed: {error}"
                    ))
                },
            )?),
        })
        .map_err(|error| {
            crate::Error::Internal(format!("discovery evidence serialization failed: {error}"))
        })?;
        Ok(Some(EvaluationCacheEntry { key, payload }))
    }

    pub(super) fn restore_receipts(
        &self,
        key: &str,
    ) -> std::result::Result<BTreeMap<String, String>, String> {
        let key = format!("restore:{key}");
        let Some(entry) = self.entries.get(key.as_str()) else {
            return Ok(BTreeMap::new());
        };
        let receipt: RestoreReceipt =
            serde_json::from_str(&entry.payload).map_err(|_| "corrupt_restore_receipt")?;
        if receipt.recipe != RESTORE_RECIPE
            || receipt.key != key
            || digest(
                &serde_json::to_vec(&receipt.receipts).map_err(|_| "invalid_restore_receipt")?,
            ) != receipt.content_digest
        {
            return Err("stale_or_corrupt_restore_receipt".into());
        }
        Ok(receipt.receipts)
    }

    pub(super) fn restore_entry(
        &self,
        key: &str,
        receipts: BTreeMap<String, String>,
    ) -> crate::Result<Option<EvaluationCacheEntry>> {
        if self.request.options.cache_policy == DiscoveryCachePolicy::Disabled
            || receipts.is_empty()
        {
            return Ok(None);
        }
        let key = format!("restore:{key}");
        let content_digest = digest(&serde_json::to_vec(&receipts).map_err(|error| {
            crate::Error::Internal(format!("discovery evidence serialization failed: {error}"))
        })?);
        let payload = serde_json::to_string(&RestoreReceipt {
            recipe: RESTORE_RECIPE,
            key: key.clone(),
            receipts,
            content_digest,
        })
        .map_err(|error| {
            crate::Error::Internal(format!("discovery evidence serialization failed: {error}"))
        })?;
        Ok(Some(EvaluationCacheEntry { key, payload }))
    }
}

pub(super) fn evaluation_paths(
    evaluation: &EvaluatedProject,
    restore: &RestoreProvenance,
) -> Vec<PathBuf> {
    let mut paths = vec![evaluation.project_path.clone()];
    paths.extend(evaluation.imports.iter().cloned());
    paths.extend(restore.inputs.iter().cloned());
    if let Some(items) = evaluation.items.get("Compile") {
        paths.extend(items.iter().map(|item| item.full_path.clone()));
    }
    paths
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_epoch_input_scope_round_trips_through_public_serialization() {
        let modified = UNIX_EPOCH
            .checked_sub(Duration::new(7, 11))
            .expect("test timestamp should be representable");
        let inputs = BTreeMap::from([(
            PathBuf::from("App.csproj"),
            Stamp {
                canonical: PathBuf::from("/workspace/App.csproj"),
                length: 12,
                modified: EpochOffset::from_system_time(modified),
                digest: "digest".into(),
            },
        )]);
        let scope = input_scope(ProjectKey("App.csproj".into()), inputs).expect("scope");
        assert_eq!(scope.inputs[0].modified, modified);

        let encoded = serde_json::to_string(&scope).expect("serialize scope");
        let decoded: DiscoveryInputScope =
            serde_json::from_str(&encoded).expect("deserialize scope");
        assert_eq!(decoded, scope);
    }
}
