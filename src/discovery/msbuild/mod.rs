//! Evaluation orchestration; process, candidate and cache mechanics have separate owners.
mod cache;
mod candidates;
mod host;
mod input;
mod restore;

use std::collections::{BTreeMap, BTreeSet};

use super::{
    DeclaredAssemblyReference, DeclaredProjectReference, DiscoveryCacheObservation,
    DiscoveryDiagnostic, DiscoveryFailure, DiscoveryFailureReason, DiscoveryGrants,
    DiscoveryRequest, DiscoverySnapshot, DiscoveryStanding, EvaluationUnit, EvaluationUnitKey,
    FrameworkIdentity, HostProvenance, ProjectDiscovery, ProjectKey, RestoreProvenance,
    SourceMembership, WorkspaceDiscovery,
};
use cache::Inputs;
use candidates::Candidate;
use host::{EvaluatedProject, HostSelection, HostSelector};

/// Discover evaluated C# projects under explicit evaluation and restore authority.
#[derive(Debug, Clone, Copy, Default)]
pub struct MsBuildDiscovery;

struct Scope {
    evaluation: EvaluatedProject,
    restore: RestoreProvenance,
    inputs: Inputs,
}

struct ScopeFailure {
    failure: DiscoveryFailure,
    host: Option<HostProvenance>,
}

impl WorkspaceDiscovery for MsBuildDiscovery {
    fn discover(&self, request: &DiscoveryRequest) -> crate::Result<DiscoverySnapshot> {
        let found = candidates::discover(&request.workspace_root)?;
        let mut snapshot = DiscoverySnapshot {
            context: request.options.context.clone(),
            grants: DiscoveryGrants {
                trust_msbuild: request.options.trust_msbuild,
                allow_restore: request.options.allow_restore,
            },
            issues: found.issues,
            ..DiscoverySnapshot::default()
        };
        if found.projects.is_empty() {
            return Ok(snapshot);
        }
        let cache = cache::Cache::new(request, &found.inventory)?;
        let mut selector = HostSelector::new(request);
        let mut validations = Vec::new();
        let mut hosts = BTreeMap::new();
        for candidate in found.projects {
            let start = snapshot.units.len();
            let result = if request.options.trust_msbuild {
                discover_project(
                    request,
                    &candidate,
                    &mut selector,
                    &cache,
                    &mut snapshot,
                    &mut validations,
                    &mut hosts,
                )?
            } else {
                Err(failure(
                    DiscoveryFailureReason::TrustRequired,
                    "MSBuild evaluation requires an explicit trust grant",
                ))
            };
            let standing = if let Err(error) = result {
                DiscoveryStanding::Indeterminate(error)
            } else {
                project_standing(&snapshot.units[start..])
            };
            snapshot.projects.push(ProjectDiscovery {
                key: candidate.key,
                containers: candidate.containers,
                standing,
            });
        }
        validate_snapshot(
            request,
            &found.inventory,
            &validations,
            &hosts,
            &mut snapshot,
        )?;
        snapshot.inputs = validations
            .into_iter()
            .map(|(project, inputs)| cache::input_scope(project, inputs))
            .collect::<crate::Result<Vec<_>>>()?;
        Ok(snapshot)
    }
}

fn project_standing(units: &[EvaluationUnit]) -> DiscoveryStanding {
    let failed: Vec<_> = units
        .iter()
        .filter_map(|unit| match &unit.standing {
            DiscoveryStanding::Indeterminate(failure) => Some(failure),
            DiscoveryStanding::Confirmed => None,
        })
        .collect();
    if failed.is_empty() {
        DiscoveryStanding::Confirmed
    } else if units.len() > 1 {
        DiscoveryStanding::Indeterminate(DiscoveryFailure {
            reason: DiscoveryFailureReason::PartialTargetFrameworks,
            diagnostics: failed
                .into_iter()
                .flat_map(|failure| failure.diagnostics.clone())
                .collect(),
        })
    } else {
        DiscoveryStanding::Indeterminate(failed[0].clone())
    }
}

fn validate_snapshot(
    request: &DiscoveryRequest,
    before: &candidates::WorkspaceInventory,
    validations: &[(ProjectKey, Inputs)],
    hosts: &BTreeMap<String, HostSelection>,
    snapshot: &mut DiscoverySnapshot,
) -> crate::Result<()> {
    // Exactly one final inventory, shared by every scope. Validated restore
    // inputs were present before authoritative reevaluation and are fingerprinted.
    let after = candidates::inventory(&request.workspace_root)?;
    let before_paths: BTreeSet<_> = before.paths.iter().collect();
    let after_paths: BTreeSet<_> = after.paths.iter().collect();
    let mut restore_paths = BTreeMap::<ProjectKey, BTreeSet<_>>::new();
    for unit in &snapshot.units {
        restore_paths
            .entry(unit.project.clone())
            .or_default()
            .extend(unit.restore.inputs.iter().cloned());
    }
    let mut host_changed = false;
    for host in hosts.values() {
        if !host.is_current(&request.options.environment)? {
            host_changed = true;
        }
    }
    let changed_paths: Vec<_> = before_paths
        .symmetric_difference(&after_paths)
        .map(|path| request.workspace_root.join(path))
        .collect();
    let uncertainty_changed = before.issues != after.issues;
    // Both inventories order traversal issues by path. Merge observations
    // without rescanning every earlier diagnostic for each final failure.
    let mut earlier = before.issues.iter().peekable();
    for issue in &after.issues {
        while earlier.peek().is_some_and(|old| old.path < issue.path) {
            earlier.next();
        }
        let duplicate = earlier.peek().is_some_and(|old| old.path == issue.path)
            && earlier.next() == Some(issue);
        if !duplicate {
            snapshot.issues.push(issue.clone());
        }
    }
    if !before.issues.is_empty() || !after.issues.is_empty() {
        snapshot.cache.clear();
    }
    let mut invalid = BTreeSet::new();
    for (project, inputs) in validations {
        // An artifact observed by a later project's restore cannot excuse a
        // changed glob in an earlier scope. Each scope must have fingerprinted
        // this exact input before its own authoritative evaluation.
        let inventory_changed = before.has_symlinks != after.has_symlinks
            || uncertainty_changed
            || (!changed_paths.is_empty() && {
                let captured_restore_paths = restore_paths
                    .get(project)
                    .map(|paths| cache::captured_restore_paths(inputs, paths))
                    .unwrap_or_default();
                changed_paths
                    .iter()
                    .any(|path| !captured_restore_paths.contains(dunce::simplified(path)))
            });
        if inventory_changed || host_changed || !cache::unchanged(inputs) {
            invalid.insert(project);
        }
    }
    if !invalid.is_empty() {
        let error = failure(
            DiscoveryFailureReason::EvaluationFailed,
            "Evaluation inputs changed during discovery; rerun with stable inputs",
        );
        for project in &mut snapshot.projects {
            if invalid.contains(&project.key) {
                project.standing = DiscoveryStanding::Indeterminate(error.clone());
            }
        }
        for unit in &mut snapshot.units {
            if invalid.contains(&unit.project) && unit.standing == DiscoveryStanding::Confirmed {
                unit.standing = DiscoveryStanding::Indeterminate(error.clone());
                unit.sources.clear();
            }
        }
        // Entries are an atomic invocation envelope: no receipt from a moving tree.
        snapshot.cache.clear();
        for observation in &mut snapshot.cache_observations {
            if invalid.contains(&observation.project) {
                observation.reused = false;
                observation
                    .bypass_reasons
                    .push("inputs_changed_during_invocation".into());
            }
        }
    }
    Ok(())
}

fn discover_project(
    request: &DiscoveryRequest,
    candidate: &Candidate,
    selector: &mut HostSelector,
    cache: &cache::Cache<'_>,
    snapshot: &mut DiscoverySnapshot,
    validations: &mut Vec<(ProjectKey, Inputs)>,
    hosts: &mut BTreeMap<String, HostSelection>,
) -> crate::Result<std::result::Result<(), DiscoveryFailure>> {
    let inputs = match restore::inspect(request, &candidate.path, &candidate.solution_paths) {
        Ok(inputs) => inputs,
        Err(error) => return Ok(Err(error)),
    };
    let host = match selector.select(&candidate.path, inputs.uses_sdk) {
        Ok(host) => host,
        Err(error) => return Ok(Err(error)),
    };
    hosts
        .entry(host.fingerprint.clone())
        .or_insert_with(|| host.clone());
    let restore_key = cache.key(&candidate.path, None, &host)?;
    match cache.restore_receipts(&restore_key) {
        Ok(receipts) => restore::supply_receipts(&inputs, receipts),
        Err(reason) => snapshot.cache_observations.push(DiscoveryCacheObservation {
            project: candidate.key.clone(),
            target_framework: None,
            reused: false,
            bypass_reasons: vec![reason],
        }),
    }
    let result = (|| {
        let outer = match scope(request, candidate, &inputs, &host, None, cache, snapshot)? {
            Ok(scope) => scope,
            Err(error) => {
                if let Some(selector) = cache::normalized_globals(request)
                    .get("targetframework")
                    .filter(|value| !value.is_empty())
                {
                    snapshot
                        .units
                        .push(failed_unit(request, candidate, selector, &host, error)?);
                    return Ok(Ok(()));
                }
                return Ok(Err(error.failure));
            }
        };
        validations.push((candidate.key.clone(), outer.inputs.clone()));
        let shorthand = property(&outer.evaluation, "TargetFramework");
        let frameworks = property(&outer.evaluation, "TargetFrameworks");
        if !shorthand.is_empty() || frameworks.is_empty() {
            let selector = (!shorthand.is_empty()).then(|| shorthand.to_owned());
            let unit = unit(request, candidate, selector.as_deref(), outer)?;
            snapshot.units.push(unit);
        } else {
            let mut seen = BTreeSet::new();
            for framework in frameworks
                .split(';')
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if !seen.insert(framework.to_owned()) {
                    continue;
                }
                match scope(
                    request,
                    candidate,
                    &inputs,
                    &host,
                    Some(framework),
                    cache,
                    snapshot,
                )? {
                    Ok(scope) => {
                        validations.push((candidate.key.clone(), scope.inputs.clone()));
                        snapshot
                            .units
                            .push(unit(request, candidate, Some(framework), scope)?);
                    }
                    Err(error) => snapshot
                        .units
                        .push(failed_unit(request, candidate, framework, &host, error)?),
                }
            }
            if seen.is_empty() {
                return Ok(Err(failure(
                    DiscoveryFailureReason::MalformedInput,
                    "TargetFrameworks contains no selectors",
                )));
            }
        }
        Ok(Ok(()))
    })();
    if let Some(entry) = cache.restore_entry(&restore_key, restore::receipts(&inputs))? {
        snapshot.cache.push(entry);
    }
    result
}

struct ScopeRequest<'a, 'cache> {
    request: &'a DiscoveryRequest,
    candidate: &'a Candidate,
    inputs: &'a restore::ProjectInputs,
    host: &'a HostSelection,
    selector: Option<&'a str>,
    cache: &'a cache::Cache<'cache>,
}

fn scope(
    request: &DiscoveryRequest,
    candidate: &Candidate,
    inputs: &restore::ProjectInputs,
    host: &HostSelection,
    selector: Option<&str>,
    cache: &cache::Cache<'_>,
    snapshot: &mut DiscoverySnapshot,
) -> crate::Result<std::result::Result<Scope, ScopeFailure>> {
    let scope_request = ScopeRequest {
        request,
        candidate,
        inputs,
        host,
        selector,
        cache,
    };
    let mut actual_host = None;
    scope_request
        .evaluate(snapshot, &mut actual_host)
        .map(|outcome| {
            outcome.map_err(|failure| ScopeFailure {
                failure,
                host: actual_host,
            })
        })
}

impl ScopeRequest<'_, '_> {
    fn evaluate(
        &self,
        snapshot: &mut DiscoverySnapshot,
        actual_host: &mut Option<HostProvenance>,
    ) -> crate::Result<std::result::Result<Scope, DiscoveryFailure>> {
        let key = self
            .cache
            .key(&self.candidate.path, self.selector, self.host)?;
        let mut observation = DiscoveryCacheObservation {
            project: self.candidate.key.clone(),
            target_framework: self.selector.map(str::to_owned),
            reused: false,
            bypass_reasons: Vec::new(),
        };
        match self.cache.lookup(&key, &self.candidate.path, self.host) {
            Ok((evaluation, previous_restore, watched)) => {
                actual_host.clone_from(&evaluation.host);
                match self.ensure_restore(&evaluation)? {
                    Ok(current)
                        if !current.performed
                            && current.provenance == previous_restore
                            && cache::unchanged(&watched) =>
                    {
                        observation.reused = true;
                        snapshot.cache_observations.push(observation);
                        if let Some(entry) = self.cache.receipt(
                            key,
                            &evaluation,
                            &current.provenance,
                            &watched,
                            self.host,
                        )? {
                            snapshot.cache.push(entry);
                        }
                        return Ok(Ok(Scope {
                            evaluation,
                            restore: current.provenance,
                            inputs: watched,
                        }));
                    }
                    Ok(_) => observation
                        .bypass_reasons
                        .push("restore_state_changed".into()),
                    Err(error) => {
                        snapshot.cache_observations.push(observation);
                        return Ok(Err(error));
                    }
                }
            }
            Err(reason) => observation.bypass_reasons.push(reason),
        }
        let (provisional, restored, mut observation) =
            match self.provisional(snapshot, observation, actual_host)? {
                Ok(value) => value,
                Err(error) => return Ok(Err(error)),
            };
        let scope = match self.stabilize(provisional, restored, actual_host)? {
            Ok(value) => value,
            Err(error) => return Ok(Err(error)),
        };
        observation
            .bypass_reasons
            .extend(self.cache.ineligibility(&scope.evaluation, self.host));
        snapshot.cache_observations.push(observation);
        if let Some(entry) = self.cache.receipt(
            key,
            &scope.evaluation,
            &scope.restore,
            &scope.inputs,
            self.host,
        )? {
            snapshot.cache.push(entry);
        }
        Ok(Ok(scope))
    }

    fn ensure_restore(
        &self,
        evaluation: &EvaluatedProject,
    ) -> crate::Result<std::result::Result<restore::RestoreOutcome, DiscoveryFailure>> {
        restore::ensure(
            self.request,
            &self.candidate.path,
            self.inputs,
            self.host,
            Some(evaluation),
            self.selector,
        )
    }

    fn provisional(
        &self,
        snapshot: &mut DiscoverySnapshot,
        observation: DiscoveryCacheObservation,
        actual_host: &mut Option<HostProvenance>,
    ) -> crate::Result<
        std::result::Result<(Scope, bool, DiscoveryCacheObservation), DiscoveryFailure>,
    > {
        let mut paths = self.inputs.files.clone();
        paths.extend([self.candidate.path.clone(), self.host.worker_path.clone()]);
        let watched = match cache::capture(paths) {
            Ok(value) => value,
            Err(error) => return Ok(Err(input_failure(&error))),
        };
        let evaluation =
            match host::evaluate(self.request, self.host, &self.candidate.path, self.selector)? {
                Ok(value) => value,
                Err(error) => {
                    snapshot.cache_observations.push(observation);
                    return Ok(Err(error));
                }
            };
        actual_host.clone_from(&evaluation.host);
        if !evaluation.success {
            let error = host::classify_failure(&evaluation);
            if matches!(
                error.reason,
                DiscoveryFailureReason::SdkUnresolved
                    | DiscoveryFailureReason::MalformedInput
                    | DiscoveryFailureReason::ToolchainUnavailable
            ) {
                snapshot.cache_observations.push(observation);
                return Ok(Err(error));
            }
        }
        let restored = match self.ensure_restore(&evaluation)? {
            Ok(value) => value,
            Err(error) => {
                snapshot.cache_observations.push(observation);
                return Ok(Err(error));
            }
        };
        if !evaluation.success && !restored.performed {
            snapshot.cache_observations.push(observation);
            return Ok(Err(host::classify_failure(&evaluation)));
        }
        Ok(Ok((
            Scope {
                evaluation,
                restore: restored.provenance,
                inputs: watched,
            },
            restored.performed,
            observation,
        )))
    }

    fn stabilize(
        &self,
        mut scope: Scope,
        restored: bool,
        actual_host: &mut Option<HostProvenance>,
    ) -> crate::Result<std::result::Result<Scope, DiscoveryFailure>> {
        // First native evaluation identifies its input closure. Capture that closure,
        // then reevaluate against it; provisional metadata is never published.
        if !cache::unchanged(&scope.inputs) {
            return Ok(Err(failure(
                DiscoveryFailureReason::EvaluationFailed,
                "Project inputs changed during evaluation",
            )));
        }
        if let Some(items) = scope.evaluation.items.get("Compile") {
            for item in items {
                if let Err(error) =
                    candidates::relative_path(&self.request.workspace_root, &item.full_path, false)
                {
                    return Ok(Err(error));
                }
            }
        }
        if let Some(items) = scope.evaluation.items.get("ProjectReference") {
            for item in items {
                if let Err(error) =
                    candidates::project_key(&self.request.workspace_root, &item.full_path)
                {
                    return Ok(Err(error));
                }
            }
        }
        let discovered = cache::evaluation_paths(&scope.evaluation, &scope.restore);
        let needs_validation_evaluation = restored
            || discovered
                .iter()
                .any(|path| !scope.inputs.contains_key(path));
        scope.inputs.extend(match cache::capture(discovered) {
            Ok(value) => value,
            Err(error) => return Ok(Err(input_failure(&error))),
        });
        if needs_validation_evaluation {
            *actual_host = None;
            scope.evaluation =
                match host::evaluate(self.request, self.host, &self.candidate.path, self.selector)?
                {
                    Ok(value) => value,
                    Err(error) => return Ok(Err(error)),
                };
            actual_host.clone_from(&scope.evaluation.host);
        }
        if !scope.evaluation.success {
            return Ok(Err(host::classify_failure(&scope.evaluation)));
        }
        let current = match self.ensure_restore(&scope.evaluation)? {
            Ok(value) => value,
            Err(error) => return Ok(Err(error)),
        };
        if current.performed
            || !cache::unchanged(&scope.inputs)
            || cache::evaluation_paths(&scope.evaluation, &current.provenance)
                .iter()
                .any(|path| !scope.inputs.contains_key(path))
        {
            return Ok(Err(failure(
                DiscoveryFailureReason::EvaluationFailed,
                "Evaluation input closure did not stabilize",
            )));
        }
        scope.restore = current.provenance;
        Ok(Ok(scope))
    }
}

fn unit(
    request: &DiscoveryRequest,
    candidate: &Candidate,
    selector: Option<&str>,
    scope: Scope,
) -> crate::Result<EvaluationUnit> {
    let evaluation = scope.evaluation;
    let identifier = property(&evaluation, "TargetFrameworkIdentifier");
    let version = property(&evaluation, "TargetFrameworkVersion");
    let framework = (!identifier.is_empty() && !version.is_empty()).then(|| FrameworkIdentity {
        short_name: nonempty(property(&evaluation, "TargetFramework")),
        identifier: identifier.to_owned(),
        version: version.to_owned(),
        profile: property(&evaluation, "TargetFrameworkProfile").into(),
        platform_identifier: property(&evaluation, "TargetPlatformIdentifier").into(),
        platform_version: property(&evaluation, "TargetPlatformVersion").into(),
    });
    let mut standing = if framework.is_some() {
        DiscoveryStanding::Confirmed
    } else {
        DiscoveryStanding::Indeterminate(failure(
            DiscoveryFailureReason::MalformedInput,
            "Evaluation did not establish a complete framework identity",
        ))
    };
    let mut sources = Vec::new();
    let mut project_references = Vec::new();
    let mut assembly_references = Vec::new();
    if let Some(items) = evaluation.items.get("Compile") {
        for item in items {
            match candidates::relative_path(&request.workspace_root, &item.full_path, false) {
                Ok(path) if item.full_path.is_file() => sources.push(SourceMembership {
                    path,
                    link: item.metadata_value("Link").map(str::to_owned),
                    metadata: semantic_metadata(&item.metadata),
                }),
                Ok(_) => {
                    standing = DiscoveryStanding::Indeterminate(failure(
                        DiscoveryFailureReason::MalformedInput,
                        "Compile input is not a regular file",
                    ));
                }
                Err(error) => standing = DiscoveryStanding::Indeterminate(error),
            }
        }
    }
    if let Some(items) = evaluation.items.get("ProjectReference") {
        for item in items {
            match candidates::project_key(&request.workspace_root, &item.full_path) {
                Ok(target) => project_references.push(DeclaredProjectReference {
                    target,
                    include: item.include.clone(),
                    metadata: semantic_metadata(&item.metadata),
                }),
                Err(error) => standing = DiscoveryStanding::Indeterminate(error),
            }
        }
    }
    if let Some(items) = evaluation.items.get("Reference") {
        assembly_references.extend(items.iter().map(|item| DeclaredAssemblyReference {
            include: item.include.clone(),
            metadata: semantic_metadata(&item.metadata),
        }));
    }
    let key = unit_key(
        request,
        candidate,
        selector,
        framework.as_ref(),
        &evaluation.properties,
        evaluation.host.as_ref(),
    )?;
    Ok(EvaluationUnit {
        key,
        project: candidate.key.clone(),
        target_framework: selector.map(str::to_owned),
        framework,
        standing,
        properties: evaluation.properties,
        sources,
        project_references,
        assembly_references,
        host: evaluation.host,
        restore: scope.restore,
    })
}

fn failed_unit(
    request: &DiscoveryRequest,
    candidate: &Candidate,
    selector: &str,
    host: &HostSelection,
    error: ScopeFailure,
) -> crate::Result<EvaluationUnit> {
    let properties = BTreeMap::new();
    let key = EvaluationUnitKey(cache::digest(
        &serde_json::to_vec(&(
            candidate.key.as_str(),
            "unconfirmed-selector",
            selector,
            cache::normalized_globals(request),
            request.options.context.import_profile,
            &host.fingerprint,
            &error.host,
        ))
        .map_err(|error| {
            crate::Error::Internal(format!("discovery evidence serialization failed: {error}"))
        })?,
    ));
    Ok(EvaluationUnit {
        key,
        project: candidate.key.clone(),
        target_framework: Some(selector.into()),
        framework: None,
        standing: DiscoveryStanding::Indeterminate(error.failure),
        properties,
        sources: Vec::new(),
        project_references: Vec::new(),
        assembly_references: Vec::new(),
        host: error.host,
        restore: RestoreProvenance::default(),
    })
}

fn unit_key(
    request: &DiscoveryRequest,
    candidate: &Candidate,
    selector: Option<&str>,
    framework: Option<&FrameworkIdentity>,
    properties: &BTreeMap<String, String>,
    host: Option<&HostProvenance>,
) -> crate::Result<EvaluationUnitKey> {
    let effective: BTreeMap<_, _> = properties
        .iter()
        .filter(|(name, _)| {
            [
                "Configuration",
                "Platform",
                "RuntimeIdentifier",
                "VSToolsPath",
            ]
            .iter()
            .any(|known| known.eq_ignore_ascii_case(name))
        })
        .map(|(name, value)| (name.to_ascii_lowercase(), value))
        .collect();
    Ok(EvaluationUnitKey(cache::digest(
        &serde_json::to_vec(&(
            candidate.key.as_str(),
            framework,
            framework.is_none().then_some(selector),
            cache::normalized_globals(request),
            request.options.context.import_profile,
            effective,
            host,
        ))
        .map_err(|error| {
            crate::Error::Internal(format!("discovery evidence serialization failed: {error}"))
        })?,
    )))
}

fn property<'a>(evaluation: &'a EvaluatedProject, name: &str) -> &'a str {
    evaluation.properties.get(name).map_or("", String::as_str)
}
fn nonempty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.into())
    }
}
fn input_failure(error: &std::io::Error) -> DiscoveryFailure {
    failure(
        DiscoveryFailureReason::EvaluationFailed,
        &format!("Cannot validate evaluation input: {error}"),
    )
}
fn failure(reason: DiscoveryFailureReason, message: &str) -> DiscoveryFailure {
    DiscoveryFailure {
        reason,
        diagnostics: vec![DiscoveryDiagnostic {
            message: message.into(),
            ..DiscoveryDiagnostic::default()
        }],
    }
}

fn semantic_metadata(metadata: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    metadata
        .iter()
        .filter(|(name, _)| {
            !["ModifiedTime", "CreatedTime", "AccessedTime"]
                .iter()
                .any(|volatile| volatile.eq_ignore_ascii_case(name))
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}
