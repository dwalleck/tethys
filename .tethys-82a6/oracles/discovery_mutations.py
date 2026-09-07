#!/usr/bin/env python3
"""S3 named falsifiers, run only against a disposable bounded source copy.

Usage: discovery_mutations.py --repo /path/to/current/tethys
Requires cargo-nextest, TETHYS_SDK_MSBUILD_PATH and TETHYS_WORKER_DISTRIBUTION.
Raw nextest logs, exact mutations, source hashes and results survive under
<repo>/.tethys-82a6/evidence/discovery-mutations-*/. No product files are edited.
Exit 0 means all baseline fences passed and all mutants failed at their named
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
    binary: str
    name: str
    ignored: bool
    assertion: str
    evidence: tuple[str, ...]


@dataclass(frozen=True)
class Mutation:
    name: str
    path: str
    before: str
    after: str
    fences: tuple[Fence, ...]


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
    Mutation('C9-omit-inventory-glob-watch', 'src/discovery/msbuild/cache.rs',
             '        if receipt.recipe != RECIPE || receipt.key != key || receipt.inventory != self.inventory {',
             '        if receipt.recipe != RECIPE || receipt.key != key {', (GLOB,)),
    Mutation('C9-disable-sdk-runtime-eligibility', 'src/discovery/msbuild/host.rs',
             "    pub(super) fn cache_ineligibility(&self) -> Option<&'static str> {\n        if self.kind == EvaluationHostKind::Framework {",
             "    pub(super) fn cache_ineligibility(&self) -> Option<&'static str> {\n        if self.kind == EvaluationHostKind::Sdk { return None; }\n        if self.kind == EvaluationHostKind::Framework {", (HOOK, WRAPPER)),
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
    command = ['cargo', 'nextest', 'run', '--locked', '--color', 'never',
               '--test', fence.binary, '--run-ignored', 'only' if fence.ignored else 'default',
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
        source = (tree / 'tests' / f'{fence.binary}.rs').read_text()
        position = unique(source, fence.assertion, f'{label} assertion')
        line = source[:position].count('\n') + 1
        require(re.search(rf'panicked at (?:[^\n]*[/\\])?tests/{fence.binary}\.rs:{line}:\d+:', text),
                f'{label}: failure is not at the named behavioral assertion line {line}; {log}')
        for expected in fence.evidence:
            require(expected in text, f'{label}: missing assertion evidence {expected!r}; {log}')
        if fence in (HOOK, WRAPPER):
            prefix = 'HOOK' if fence == HOOK else 'WRAPPER'
            require(re.search(rf'left:\s+String\("{prefix}_ONE"\)', text)
                    and re.search(rf'right:\s+"{prefix}_TWO"', text),
                    f'{label}: runtime failure was not stale ONE versus current TWO; {log}')
        record['assertion'] = f'tests/{fence.binary}.rs:{line}'
        record['evidence'] = fence.evidence
    record['result'] = 'FALSIFIED' if mutated else 'BASELINE_PASS'
    print(f'{label}: {record["result"]}: {fence.name}', flush=True)
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--timeout', type=int, default=1800, help='seconds per nextest invocation, including compilation')
    args = parser.parse_args()
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
              'sdk': env['TETHYS_SDK_MSBUILD_PATH'], 'worker': env['TETHYS_WORKER_DISTRIBUTION']}
    try:
        with tempfile.TemporaryDirectory(prefix='tethys-discovery-mutations-') as disposable:
            tree = Path(disposable)
            report['disposable_root'] = str(tree)
            report['source_sha256'] = bounded_copy(repo, tree)
            # Prevent a caller's target override or repository hook installer from
            # touching the original checkout. Native test environment is inherited.
            env['CARGO_TARGET_DIR'] = str(tree / 'target')
            env['CARGO_HUSKY_DONT_INSTALL_HOOKS'] = 'true'
            env['CARGO_TERM_COLOR'] = 'never'
            env['NEXTEST_HIDE_PROGRESS_BAR'] = '1'
            originals = {}
            for mutation in MUTATIONS:
                original = (tree / mutation.path).read_bytes()
                unique(original.decode(), mutation.before, mutation.name)
                originals[mutation.path] = original
                (evidence / f'{mutation.name}.json').write_text(json.dumps({
                    'path': mutation.path, 'before': mutation.before, 'after': mutation.after,
                    'tests': [f.name for f in mutation.fences]}, indent=2) + '\n')
            restore_source = (tree / 'tests/discovery_failures.rs').read_text()
            unique(restore_source, '    let ungranted = discover(root.path(), options());\n    assert_eq!(\n        reason(&ungranted.projects[0].standing),\n        DiscoveryFailureReason::RestoreRequired\n    );\n    assert!(!root.path().join("obj/project.assets.json").exists());', 'C7 native no-grant control')
            fences = tuple(dict.fromkeys(f for m in MUTATIONS for f in m.fences))
            for fence in fences:
                unique((tree / 'tests' / f'{fence.binary}.rs').read_text(), fence.assertion, fence.name)
                report['results'].append(run_fence(tree, evidence, env, fence, 'baseline', args.timeout, False))
            for mutation in MUTATIONS:
                path = tree / mutation.path
                original = originals[mutation.path]
                require(path.read_bytes() == original, f'{mutation.name}: previous source was not restored')
                try:
                    path.write_bytes(original.replace(mutation.before.encode(), mutation.after.encode(), 1))
                    for fence in mutation.fences:
                        report['results'].append(run_fence(tree, evidence, env, fence, mutation.name, args.timeout, True))
                finally:
                    path.write_bytes(original)
                for fence in mutation.fences:
                    report['results'].append(run_fence(
                        tree, evidence, env, fence, mutation.name + '-restored', args.timeout, False))
            report['status'] = 'PASS'
    except Exception as error:
        report['error'] = str(error)
        raise
    finally:
        (evidence / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
    print(f'PASS: 7 named mutations, 8 behavioral falsifiers; evidence {evidence}')


if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        print(f'FAIL CLOSED: {error}', file=sys.stderr)
        sys.exit(1)
