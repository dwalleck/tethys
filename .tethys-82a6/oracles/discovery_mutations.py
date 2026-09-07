#!/usr/bin/env python3
"""S3/S4 named falsifiers, run only against a disposable bounded source copy.

Usage: discovery_mutations.py --repo /path/to/current/tethys
Requires cargo-nextest, TETHYS_SDK_MSBUILD_PATH and TETHYS_WORKER_DISTRIBUTION.
Raw nextest logs, exact mutations, source hashes and results survive under
<repo>/.tethys-82a6/evidence/discovery-mutations-*/. No product files are edited.
Exit 0 means all platform-applicable baseline fences passed and all mutants failed at their named
behavioral oracle; any setup/compiler/timeout/unexpected failure exits 1.
"""
import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile


@dataclass(frozen=True)
class Fence:
    binary: str | None
    name: str
    ignored: bool
    assertion: str
    evidence: tuple[str, ...]
    source: str | None = None


@dataclass(frozen=True)
class Mutation:
    name: str
    path: str
    before: str
    after: str
    fences: tuple[Fence, ...]
    platform: str | None = None


REASON = '        DiscoveryStanding::Confirmed => panic!("expected explicit incomplete coverage"),'
CARGO = Fence('cargo_discovery', 'neutral_cargo_adapter_preserves_member_order_custom_targets_and_root_last', False,
              '        assert_eq!(\n            info.lib_path,', ('assertion `left == right` failed', 'None', 'custom code/λ_library.rs'))
FAILED = Fence('discovery_failures', 'reason_matrix_native_failures_preserve_successful_siblings', True,
               REASON, ('expected explicit incomplete coverage',))
CONTAIN = Fence('discovery_candidates', 'missing_directory_empty_and_outside_declarations_are_typed_not_dropped', False,
                '    assert_eq!(\n        snapshot\n            .issues\n            .iter()\n            .map(|issue| issue.failure.reason)',
                ('assertion `left == right` failed', 'MalformedInput', 'OutsideWorkspaceInput'))
TRUST = Fence('discovery_failures', 'trust_gate_precedes_host_and_restore', False,
              '    assert_eq!(\n        reason(&snapshot.projects[0].standing),\n        DiscoveryFailureReason::TrustRequired',
              ('assertion `left == right` failed', 'ToolchainUnavailable', 'TrustRequired'))
RESTORE = Fence('discovery_failures', 'authorized_restore_rechecks_metadata_before_confirmation', True,
                REASON, ('expected explicit incomplete coverage',))
GLOB = Fence('discovery_cache', 'cache_input_closure_matches_forced_and_invalidates_globs_content_context', True,
             '    assert_eq!(paths, names);', ('assertion `left == right` failed', 'src/First.cs', 'src/Second.cs'))
HOOK = Fence('discovery_runtime', 'startup_hook_external_reads_never_reuse_literal_metadata', True,
             '    assert_eq!(\n        snapshot["units"][0]["properties"]["DefineConstants"],',
             ('assertion `left == right` failed', 'HOOK_ONE', 'HOOK_TWO'))
WRAPPER = Fence('discovery_runtime', 'forwarding_muxer_external_reads_never_reuse_literal_metadata', True,
                HOOK.assertion, ('assertion `left == right` failed', 'WRAPPER_ONE', 'WRAPPER_TWO'))
ALIAS = Fence('discovery_failures', 'existing_restore_requires_same_physical_project', True,
              '    assert_eq!(\n        same_project.projects[0].standing,',
              ('assertion `left == right` failed', 'RestoreRequired', 'Confirmed'))
CAPTURED_RESTORE = Fence('discovery_failures', 'authorized_restore_preserves_aliased_artifact_identity', True,
                         '    assert_eq!(\n        aliased_restore.projects[0].standing,',
                         ('assertion `left == right` failed', 'EvaluationFailed', 'Confirmed'))
MEMBERSHIP = Fence('msbuild_discovery', 'membership_replacement', True,
                   '    assert_eq!(\n        membership(sql),\n        [\n            (\n                "A.csproj".into(),\n                "Second.cs".into(),\n                Some("Second.cs".into())\n            ),\n            (\n                "A.csproj".into(),',
                   ('assertion `left == right` failed', 'A.csproj', 'B.csproj', 'Shared.cs', 'Second.cs'))
REMOVED_UNIT = Fence('msbuild_discovery', 'membership_replacement', True,
                    '    assert_eq!(\n        strings(\n            &sql,\n            "SELECT project_key FROM evaluation_units ORDER BY project_key"',
                    ('assertion `left == right` failed', 'removed project/unit identities must not survive replacement',
                     'A.csproj', 'B.csproj', 'Moved.csproj'))
RUST_SOURCE_IDENTITY = Fence(
    'symlink_boundary',
    'symlink_to_file_outside_workspace_is_indexed_through_logical_path', False,
    '    assert_eq!(\n        stats.files_indexed, 1,\n        "symlinked file outside workspace is currently indexed via logical path"',
    ('assertion `left == right` failed', 'left: 0', 'right: 1'))
CSHARP_SOURCE_IDENTITY = Fence(
    'concurrency_and_filesystem',
    'canonical_publication_resolves_relative_and_absolute_file_aliases', False,
    '    assert_eq!(stats.files_indexed, 2);',
    ('assertion `left == right` failed', 'left: 6', 'right: 2'))
CSHARP_SOURCE_CONTAINMENT = Fence(
    'concurrency_and_filesystem',
    'csharp_external_file_and_directory_aliases_are_not_published', False,
    '    assert_eq!(stats.files_indexed, 1);\n    let inside = index.get_file(Path::new("src/Inside.cs")).unwrap().unwrap();',
    ('assertion `left == right` failed', 'left: 3', 'right: 1'))
REAPED_GROUP = Fence(
    None, 'discovery::msbuild::host::tests::already_exited_process_group_is_reaped', False,
    '        terminate_and_reap(&mut child).expect("an exited group must be reaped successfully");',
    ('an exited group must be reaped successfully', 'PermissionDenied'),
    'src/discovery/msbuild/host.rs')

# Keep the replacement compilable and transactional: only old, removed projects
# survive. Shift ordinals into a disjoint range before inserting current rows;
# deleting each current project cascades its old children, avoiding key failures.
REPLACE_PREFIX = '''        tx.execute_batch("DELETE FROM projects; DELETE FROM evaluation_context; DELETE FROM evaluation_cache; DELETE FROM discovery_issues; DELETE FROM source_diagnostics;")?;
        tx.execute(
            "INSERT INTO evaluation_context VALUES (1, ?1, ?2, ?3, ?4)",
            params![
                encode(&snapshot.crates)?,
                encode(&snapshot.context)?,
                encode(&snapshot.grants)?,
                encode(&snapshot.cache_observations)?,
            ],
        )?;
        {
            let mut statement = tx.prepare("INSERT INTO projects VALUES (?1, ?2, ?3, ?4)")?;
            for (ordinal, project) in snapshot.projects.iter().enumerate() {'''
RETAIN_REMOVED = REPLACE_PREFIX.replace(
    'DELETE FROM projects;',
    'UPDATE projects SET ordinal = ordinal + 1000000; '
    'UPDATE evaluation_units SET ordinal = ordinal + 1000000; '
    'UPDATE evaluation_inputs SET ordinal = ordinal + 1000000;'
) + '''
                tx.execute("DELETE FROM projects WHERE project_key = ?1", params![project.key.as_str()])?;'''

MUTATIONS = (
    Mutation('C3-discard-custom-library', 'src/cargo.rs',
             '            crates: discover_crates(&request.workspace_root),',
             '            crates: discover_crates(&request.workspace_root).into_iter().map(|mut info| { info.lib_path = None; info }).collect(),', (CARGO,)),
    Mutation('C4-confirm-failed-project', 'src/discovery/msbuild/mod.rs',
             '            let standing = if let Err(error) = result {\n                DiscoveryStanding::Indeterminate(error)',
             '            let standing = if let Err(_error) = result {\n                DiscoveryStanding::Confirmed', (FAILED,)),
    # The approved contained boundary is now named relative_path. A string-prefix
    # check wrongly admits workspace-sibling, then normal file checks classify it
    # as malformed rather than an escape. Inside and missing controls stay live.
    Mutation('C4-C7-string-prefix-containment', 'src/discovery/msbuild/candidates.rs',
             '''    let relative = resolved.strip_prefix(root).map_err(|_| {
        failure(
            path,
            DiscoveryFailureReason::OutsideWorkspaceInput,
            "Path escapes the workspace",
        )
    })?;''',
             '''    let resolved_text = resolved.to_string_lossy();
    let root_text = root.to_string_lossy();
    let relative = resolved_text.strip_prefix(root_text.as_ref()).map(|suffix| {
        PathBuf::from(suffix.trim_start_matches(std::path::MAIN_SEPARATOR))
    }).ok_or_else(|| {
        failure(
            path,
            DiscoveryFailureReason::OutsideWorkspaceInput,
            "Path escapes the workspace",
        )
    })?;''', (CONTAIN,)),
    Mutation('C6-overwrite-trust-grant', 'src/discovery/types.rs',
             '    pub fn new(workspace_root: &Path, options: DiscoveryOptions) -> Result<Self> {\n        let workspace_root',
             '    pub fn new(workspace_root: &Path, mut options: DiscoveryOptions) -> Result<Self> {\n        options.trust_msbuild = true;\n        let workspace_root', (TRUST,)),
    Mutation('C7-overwrite-restore-grant', 'src/discovery/types.rs',
             '    pub fn new(workspace_root: &Path, options: DiscoveryOptions) -> Result<Self> {\n        let workspace_root',
             '    pub fn new(workspace_root: &Path, mut options: DiscoveryOptions) -> Result<Self> {\n        options.allow_restore = true;\n        let workspace_root', (RESTORE,)),
    Mutation('C7-lexical-project-identity', 'src/discovery/msbuild/restore.rs',
             '        Ok(canonical) => Ok(canonical == project),',
             '        Ok(_canonical) => Ok(path == project),', (ALIAS,)),
    Mutation('C7-lexical-generated-identity', 'src/discovery/msbuild/restore.rs',
             '''    let generated_identities = generated
        .iter()
        .map(|path| path.canonicalize())''',
             '''    let generated_identities = generated
        .iter()
        .map(|path| Ok(path.clone()))''', (ALIAS,)),
    Mutation('C7-lexical-captured-restore', 'src/discovery/msbuild/cache.rs',
             '        .flat_map(|(path, stamp)| [dunce::simplified(path), dunce::simplified(&stamp.canonical)])',
             '        .flat_map(|(path, _stamp)| [dunce::simplified(path), dunce::simplified(path)])', (CAPTURED_RESTORE,)),
    Mutation('C9-omit-inventory-glob-watch', 'src/discovery/msbuild/cache.rs',
             '        if receipt.recipe != RECIPE || receipt.key != key || receipt.inventory != self.inventory {',
             '        if receipt.recipe != RECIPE || receipt.key != key {', (GLOB,)),
    Mutation('C9-disable-sdk-runtime-eligibility', 'src/discovery/msbuild/host.rs',
             "    pub(super) fn cache_ineligibility(&self) -> Option<&'static str> {\n        if self.kind == EvaluationHostKind::Framework {",
             "    pub(super) fn cache_ineligibility(&self) -> Option<&'static str> {\n        if self.kind == EvaluationHostKind::Sdk { return None; }\n        if self.kind == EvaluationHostKind::Framework {", (HOOK, WRAPPER)),
    # Model a last-writer-wins file_id uniqueness policy without provoking a
    # constraint error: the distinct Second.cs control survives, shared A loses.
    Mutation('C8-membership-unique-file-id', 'src/db/discovery.rs',
             '                sources.execute(params![\n                    key,',
             '''                conn.execute("DELETE FROM file_participation WHERE file_id = ?1", params![file_ids.get(&path)])?;
                sources.execute(params![
                    key,''', (MEMBERSHIP,)),
    Mutation('C8-retain-removed-unit', 'src/db/discovery.rs',
             REPLACE_PREFIX, RETAIN_REMOVED, (REMOVED_UNIT,)),
    Mutation('C3-physical-rust-source-identity', 'src/languages/module_resolver.rs',
             '    fn source_path_identity(&self) -> SourcePathIdentity {\n        SourcePathIdentity::Logical\n    }',
             '    fn source_path_identity(&self) -> SourcePathIdentity {\n        SourcePathIdentity::Physical\n    }',
             (RUST_SOURCE_IDENTITY,)),
    Mutation('C7-C8-logical-csharp-source-identity', 'src/languages/module_resolver.rs',
             '    fn source_path_identity(&self) -> SourcePathIdentity {\n        SourcePathIdentity::Physical\n    }',
             '    fn source_path_identity(&self) -> SourcePathIdentity {\n        SourcePathIdentity::Logical\n    }',
             (CSHARP_SOURCE_IDENTITY, CSHARP_SOURCE_CONTAINMENT)),
    Mutation('C6-reject-zombie-group-reaping', 'src/discovery/msbuild/host.rs',
             'if cfg!(target_os = "macos")',
             'if false', (REAPED_GROUP,), platform='darwin'),
)


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def unique(text, anchor, label):
    require(text.count(anchor) == 1, f'{label}: expected exactly one anchor, found {text.count(anchor)}')
    return text.index(anchor)


def bounded_copy(repo, destination):
    # Only compilation inputs; never clone target, .git, cache, notes or tools.
    # Refuse symlinks rather than accidentally copying data outside this scope.
    hashes = {}
    paths = [repo / name for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml')]
    for directory in ('src', 'tests', 'benches'):
        root = repo / directory
        require(root.is_dir() and not root.is_symlink(), f'missing/linked source root: {root}')
        for base, dirs, files in os.walk(root, followlinks=False):
            for name in dirs:
                path = Path(base) / name
                require(not path.is_symlink(), f'linked source directory: {path}')
            dirs[:] = [name for name in dirs if name not in {'target', '.git', 'cache', '__pycache__', 'bin', 'obj'}]
            paths.extend(Path(base) / name for name in files)
    for path in paths:
        require(path.is_file() and not path.is_symlink(), f'missing/linked source file: {path}')
        relative = path.relative_to(repo)
        data = path.read_bytes()
        require(len(data) < 32 * 1024 * 1024, f'unexpected source input size: {relative}')
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        hashes[str(relative)] = hashlib.sha256(data).hexdigest()
    return hashes


def run_fence(tree, evidence, env, fence, label, timeout, mutated):
    log = evidence / f'{label}--{fence.name}.log'
    target = ['--lib'] if fence.binary is None else ['--test', fence.binary]
    command = ['cargo', 'nextest', 'run', '--locked', '--color', 'never', *target,
               '--run-ignored', 'only' if fence.ignored else 'default',
               '-E', f'test(={fence.name})', '--no-tests', 'fail', '--no-fail-fast',
               '--retries', '0', '--test-threads', '1', '--status-level', 'all',
               '--final-status-level', 'all', '--failure-output', 'immediate-final']
    record = {'command': command, 'cwd': str(tree), 'log': str(log), 'phase': label, 'mutated': mutated}
    with log.open('wb') as output:
        process = subprocess.Popen(command, cwd=tree, env=env, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
        except (subprocess.TimeoutExpired, KeyboardInterrupt):
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            raise RuntimeError(f'{label}: interrupted/timed out; NOT falsified; log {log}')
    text = log.read_text(errors='replace')
    record['exit_code'] = code
    require(re.search(r'^\s*Starting 1 tests? across 1 binar(?:y|ies)\b', text, re.M),
            f'{label}: did not run exactly one test; {log}')
    status = 'FAIL' if mutated else 'PASS'
    require(re.search(rf'^\s*{status}\s+\[.*\]\s+(?:\(\s*1/1\)\s+)?\S+\s+{re.escape(fence.name)}\s*$', text, re.M),
            f'{label}: no explicit {status} record for {fence.name}; {log}')
    require(code == (100 if mutated else 0), f'{label}: unexpected nextest exit {code}; {log}')
    require(not re.search(r'^\s*(?:TIMEOUT|XFAIL|XPASS|ABORT|LEAK|EXECFAIL)\s', text, re.M),
            f'{label}: non-assertion termination; {log}')
    if mutated:
        source_path = fence.source or f'tests/{fence.binary}.rs'
        source = (tree / source_path).read_text()
        position = unique(source, fence.assertion, f'{label} assertion')
        line = source[:position].count('\n') + 1
        require(re.search(rf'panicked at (?:[^\n]*[/\\])?{re.escape(source_path)}:{line}:\d+:', text),
                f'{label}: failure is not at the named behavioral assertion line {line}; {log}')
        for expected in fence.evidence:
            require(expected in text, f'{label}: missing assertion evidence {expected!r}; {log}')
        if fence in (HOOK, WRAPPER):
            prefix = 'HOOK' if fence == HOOK else 'WRAPPER'
            require(re.search(rf'left:\s+String\("{prefix}_ONE"\)', text)
                    and re.search(rf'right:\s+"{prefix}_TWO"', text),
                    f'{label}: runtime failure was not stale ONE versus current TWO; {log}')
        record['assertion'] = f'{source_path}:{line}'
        record['evidence'] = fence.evidence
    record['result'] = 'FALSIFIED' if mutated else ('RESTORED_PASS' if label.endswith('-restored') else 'BASELINE_PASS')
    print(f'{label}: {record["result"]}: {fence.name}', flush=True)
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--timeout', type=int, default=1800, help='seconds per nextest invocation, including compilation')
    parser.add_argument('--mutation', action='append', choices=[mutation.name for mutation in MUTATIONS],
                        help='run only named mutations; repeat to select several (default: all)')
    args = parser.parse_args()
    selected = tuple(mutation for mutation in MUTATIONS
                     if not args.mutation or mutation.name in args.mutation)
    unsupported = tuple(m for m in selected if m.platform and m.platform != sys.platform)
    require(not args.mutation or not unsupported,
            f'explicit mutations require another platform: {[m.name for m in unsupported]}')
    mutations = tuple(m for m in selected if m not in unsupported)
    require(os.name == 'posix', 'POSIX process groups and forwarding-muxer fence are required')
    require(args.timeout > 0, '--timeout must be positive')
    repo = args.repo.resolve(strict=True)
    env = dict(os.environ)
    for name in ('TETHYS_SDK_MSBUILD_PATH', 'TETHYS_WORKER_DISTRIBUTION'):
        require(env.get(name), f'{name} must select the installed SDK / packaged worker explicitly')
        env[name] = str(Path(env[name]).resolve(strict=True))
        require(Path(env[name]).is_dir(), f'{name} must be a directory')
    require((Path(env['TETHYS_SDK_MSBUILD_PATH']) / 'MSBuild.dll').is_file(), 'selected SDK lacks MSBuild.dll')
    require((Path(env['TETHYS_WORKER_DISTRIBUTION']) / 'sdk/Tethys.MSBuild.Evaluate.dll').is_file(), 'packaged SDK worker missing')
    require(shutil.which('cargo') is not None, 'cargo missing from PATH')
    evidence_parent = repo / '.tethys-82a6/evidence'
    evidence_parent.mkdir(parents=True, exist_ok=True)
    evidence = Path(tempfile.mkdtemp(prefix='discovery-mutations-', dir=evidence_parent))
    print(f'Evidence: {evidence}', flush=True)
    report = {'repo': str(repo), 'results': [], 'status': 'FAILED',
              'sdk': env['TETHYS_SDK_MSBUILD_PATH'], 'worker': env['TETHYS_WORKER_DISTRIBUTION'],
              'mutations': [mutation.name for mutation in mutations],
              'platform_skipped': [mutation.name for mutation in unsupported]}
    try:
        with tempfile.TemporaryDirectory(prefix='tethys-discovery-mutations-') as disposable:
            tree = Path(disposable)
            report['disposable_root'] = str(tree)
            report['source_sha256'] = bounded_copy(repo, tree)
            if any(m.name.startswith('C8-') for m in mutations):
                require('src/db/discovery.rs' in report['source_sha256'],
                        'C8 requires the current uncommitted S4 persistence source')
            # Prevent a caller's target override or repository hook installer from
            # touching the original checkout. Native test environment is inherited.
            env['CARGO_TARGET_DIR'] = str(tree / 'target')
            env['CARGO_HUSKY_DONT_INSTALL_HOOKS'] = 'true'
            env['CARGO_TERM_COLOR'] = 'never'
            env['NEXTEST_HIDE_PROGRESS_BAR'] = '1'
            originals = {}
            for mutation in mutations:
                original = (tree / mutation.path).read_bytes()
                unique(original.decode(), mutation.before, mutation.name)
                originals[mutation.path] = original
                (evidence / f'{mutation.name}.json').write_text(json.dumps({
                    'path': mutation.path, 'before': mutation.before, 'after': mutation.after,
                    'tests': [f.name for f in mutation.fences]}, indent=2) + '\n')
            restore_source = (tree / 'tests/discovery_failures.rs').read_text()
            unique(restore_source, '    let ungranted = discover(root.path(), options());\n    assert_eq!(\n        reason(&ungranted.projects[0].standing),\n        DiscoveryFailureReason::RestoreRequired\n    );\n    assert!(!root.path().join("obj/project.assets.json").exists());', 'C7 native no-grant control')
            fences = tuple(dict.fromkeys(f for m in mutations for f in m.fences))
            for fence in fences:
                source_path = fence.source or f'tests/{fence.binary}.rs'
                unique((tree / source_path).read_text(), fence.assertion, fence.name)
                # Assertion identity distinguishes obligations sharing one nextest
                # selector, independent of selection order or selected mutations.
                case_key = hashlib.sha256(fence.assertion.encode()).hexdigest()
                baseline_label = f'baseline-{fence.binary}-{case_key}'
                report['results'].append(run_fence(
                    tree, evidence, env, fence, baseline_label, args.timeout, False))
            for mutation in mutations:
                path = tree / mutation.path
                original = originals[mutation.path]
                require(path.read_bytes() == original, f'{mutation.name}: previous source was not restored')
                try:
                    path.write_bytes(original.replace(mutation.before.encode(), mutation.after.encode(), 1))
                    mutant_hash = hashlib.sha256(path.read_bytes()).hexdigest()
                    for fence in mutation.fences:
                        result = run_fence(tree, evidence, env, fence, mutation.name, args.timeout, True)
                        result['mutated_source_sha256'] = mutant_hash
                        report['results'].append(result)
                finally:
                    path.write_bytes(original)
                    require(path.read_bytes() == original, f'{mutation.name}: explicit restoration failed')
                    report.setdefault('restorations', []).append({
                        'mutation': mutation.name, 'path': mutation.path,
                        'restored_source_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                    })
                for fence in mutation.fences:
                    report['results'].append(run_fence(
                        tree, evidence, env, fence, mutation.name + '-restored', args.timeout, False))
            report['final_source_sha256'] = {
                relative: hashlib.sha256((tree / relative).read_bytes()).hexdigest()
                for relative in report['source_sha256']
            }
            require(report['final_source_sha256'] == report['source_sha256'],
                    'disposable source differs from the fresh baseline after explicit restoration')
            report['status'] = 'PASS'
    except Exception as error:
        report['error'] = str(error)
        raise
    finally:
        (evidence / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
    cases = sum(len(mutation.fences) for mutation in mutations)
    print(f'PASS: {len(mutations)} named mutations, {cases} behavioral falsifier cases; evidence {evidence}')


if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        print(f'FAIL CLOSED: {error}', file=sys.stderr)
        sys.exit(1)
