# PR #45 — code review findings

| | |
|---|---|
| **PR** | #45 |
| **Head reviewed** | `5237b91` |
| **Level** | `xhigh` |
| **Date** | 2026-09-08 |
| **Findings** | 15 (14 functional/structural, 1 hygiene) |

> **Verdicts not stamped.** The review read the full diff and the enclosing sources at
> head, but there is no record of a separate adversarial verify pass, so no finding is
> marked CONFIRMED vs PLAUSIBLE. Several cite in-repo evidence that is cheap to check
> directly (the `run34071402251` log line, the follow-on branch's `timeout-minutes: 25`,
> `packages.lock.json` versions).

## Summary

| # | Category | Location | Issue |
|---|---|---|---|
| 1 | correctness | `tools/tethys-msbuild-evaluate/Evaluation.cs:53` | Host self-check ignores effective `MSBuildBinPath` |
| 2 | build-tooling | `.github/workflows/release.yml:101` | Release depends on non-production issue-local script |
| 3 | ci-reliability | `.github/workflows/ci.yml:284` | 10-min timeout too small for required Windows job |
| 4 | ci-reliability | `.tethys-82a6/oracles/worker_qualification.py:46` | 60 s default timeout on cold-runner MSBuild launches |
| 5 | ci-reliability | `.tethys-82a6/oracles/worker_qualification.py:108` | vswhere pinned to VS 17.14 blocks releases |
| 6 | security | `tools/tethys-msbuild-evaluate/Program.cs:39` | `workspace_root` validated but never enforced |
| 7 | correctness | `tools/tethys-msbuild-evaluate/Evaluation.cs:88` | Failed import probes omitted from input closure |
| 8 | correctness | `tools/tethys-msbuild-evaluate/Evaluation.cs:96` | `glob_patterns` omits condition and Update provenance |
| 9 | test-coverage | `.tethys-82a6/oracles/worker_qualification.py:214` | Exit-code fence only checks the success direction |
| 10 | correctness | `.tethys-82a6/oracles/worker_qualification.py:100` | `max()` on `(version, Path)` crashes on tied SDK versions |
| 11 | correctness | `tools/tethys-msbuild-evaluate/Evaluation.cs:26` | Timestamps in metadata make responses non-reproducible |
| 12 | robustness | `.tethys-82a6/oracles/worker_qualification.py:68` | Unbounded stdin write and thread join can hang runner |
| 13 | correctness | `.tethys-82a6/oracles/worker_qualification.py:157` | Publish dir never cleaned; stale DLLs ship in bundle |
| 14 | test-coverage | `.tethys-82a6/oracles/worker_qualification.py:177` | Bundled-runtime fence is a 4-name denylist |
| 15 | simplification | `tests/discovery_cli.rs:1` | Duplicate misnamed tests hardcode `python3` |

---

## 1. Host self-check ignores effective `MSBuildBinPath`

**`tools/tethys-msbuild-evaluate/Evaluation.cs:53`** · correctness

The host self-check compares only the loaded `Microsoft.Build` assembly directory to
`msbuild_path`, never the effective toolset (`MSBuildBinPath`), so the exact substitution
the PR itself diagnosed (S2c) still passes at runtime.

**Failure scenario.** `plan.md` records `run34071402251`: the worker reported
`MSBuildBinPath=Current/Bin/amd64` while the selected host was `Current/Bin` — yet
`Path.GetDirectoryName(typeof(ProjectCollection).Assembly.Location)` equalled
`Current/Bin`, so this guard did NOT fire; only the external Python oracle caught it. The
shipped fix is packaging-only (`PlatformTarget=x86` for net472). Any future AnyCPU/x64
worker, or an SDK host whose `BuildEnvironmentHelper` re-routes to an amd64 sibling,
returns `success:true`, `host.path=<selected>` and silently wrong `DefineConstants` /
`TargetFrameworkVersion` / `Compile` items. `MSBuildBinPath` is already collected into
`response.properties` at line 74; comparing it to `request.msbuild_path` is the one-line
root-cause guard.

## 2. Release depends on non-production issue-local script

**`.github/workflows/release.yml:101`** · build-tooling

The release and CI pipelines now depend on `.tethys-82a6/oracles/worker_qualification.py`,
an issue-local audit script that `plan.md` itself declares non-production, and the
normative docs plus two committed tests point at the same path.

**Failure scenario.** `.tethys-82a6/plan.md` states "Issue-local oracles and test fixtures
are non-production." This PR makes that file the only way to build and package the shipped
companion: 4 invocations in `ci.yml`, 4 in `release.yml`, the documented developer workflow
in `docs/msbuild-evaluation.md` (a doc `AGENTS.md` marks as required reading before
touching companion packaging), and `tests/discovery_cli.rs` / `tests/msbuild_discovery.rs`.
When tethys-82a6 closes and `.tethys-82a6/` is retired the way every other `.tethys-*`
issue directory in this repo is, CI and the release workflow break with
"No such file or directory". The repo already has `scripts/` (`changelog-release.sh`,
`gate.sh`) for exactly this class of durable build tooling.

## 3. 10-min timeout too small for required Windows job

**`.github/workflows/ci.yml:284`** · ci-reliability

`timeout-minutes: 10` on the new required managed-worker job is too small for the Windows
leg, and the job is a hard `needs` of `ci-success`, so a timeout blocks every PR.

**Failure scenario.** The windows-2022 leg does: checkout, `setup-dotnet` for two SDK bands
(~2-3 min on Windows), `setup-python`, `--host windows --prepare` (locked restore + net8
publish + VS MSBuild net472 build), then two full qualification passes that each launch
`MSBuild.exe` roughly 3x per fixture unit (worker, `-getProperty`/`-getItem` oracle,
sentinel target). That is 30+ MSBuild process launches. The follow-on branch in this repo
already raised this to `timeout-minutes: 25` — direct evidence the 10-minute budget was
insufficient. A timeout is `failure`, `ci-success` checks
`needs.managed-worker.result != 'success'`, and the PR is blocked.

## 4. 60 s default timeout on cold-runner MSBuild launches

**`.tethys-82a6/oracles/worker_qualification.py:46`** · ci-reliability

`qualify()` runs every worker and direct-MSBuild invocation on the 60-second default
timeout while `prepare()` explicitly uses 300 — including the sentinel *build*-target
invocations, which routinely exceed 60 s on a cold Windows runner.

**Failure scenario.** `query(project, tfm, target='QualificationSentinel')` at line 335
launches `MSBuild.exe <SDK project> -t:QualificationSentinel`, which loads the whole
`Microsoft.NET.Sdk` import closure. On a cold windows-2022 runner the first such launch
(JIT + SDK resolution + NuGet targets) takes well over 60 s. `process.wait(timeout=60)`
raises `TimeoutExpired`, `run()` kills the child and re-raises, and the required
`managed-worker` job fails — blocking `ci-success` for an unrelated PR. This is the same
60 s cap applied to `worker()`, `query()` and the missing-companion probe.

## 5. vswhere pinned to VS 17.14 blocks releases

**`.tethys-82a6/oracles/worker_qualification.py:108`** · ci-reliability

`selected_windows()` hard-pins vswhere to `[17.14,17.15)` with an explicit no-fallback
failure, and this is now on the release-publishing path, so releases stop being publishable
when the runner image moves past VS 17.14.

**Failure scenario.** GitHub rolls the windows-2022 image to VS 17.15 (or the org's
self-hosted runner upgrades). `vswhere -version [17.14,17.15)` returns `[]`,
`require(installations, "Visual Studio 17.14 MSBuild is required; no fallback to another
host")` raises, `.github/workflows/release.yml:105` fails, the `build` job fails, `release`
(`needs: [verify, build]`) never runs, and pushing a `v*` tag produces no GitHub release at
all — for a Rust CLI change entirely unrelated to MSBuild. The same pin is what makes
`ci.yml`'s managed-worker a required check. A qualification-only pin is defensible; a
release-blocking one is not.

## 6. `workspace_root` validated but never enforced

**`tools/tethys-msbuild-evaluate/Program.cs:39`** · security

`workspace_root` is a `Required.Always` field that is validated to exist and then never
used, and nothing constrains `project_path` to lie inside it — the workspace boundary the
protocol advertises is not enforced anywhere.

**Failure scenario.** Grep the worker: `request.workspace_root` appears only at
`Program.cs:36` (normalized-absolute check) and `Program.cs:39` (`Directory.Exists`).
`Evaluation.Run` never reads it. So a request with `workspace_root=/home/u/proj` and
`project_path=/etc/evil/x.csproj` is accepted and evaluated — and per `CONTEXT.md`/docs,
evaluation executes property functions and SDK resolvers, i.e. arbitrary code. The trust
grant is scoped to a workspace in the caller's model but to nothing in the worker's. Either
enforce `project_path` containment under `workspace_root` (and pass it to MSBuild as the
evaluation anchor), or drop the field from protocol 1 rather than shipping a required
parameter with no effect.

## 7. Failed import probes omitted from input closure

**`tools/tethys-msbuild-evaluate/Evaluation.cs:88`** · correctness

`imports` lists only successfully resolved imports, and `EvaluationLogger` subscribes only
to `ErrorRaised`/`WarningRaised`, so import probes that found nothing are dropped — the
freshness/cache input closure cannot see a file whose future creation would change
evaluation.

**Failure scenario.** A project under a directory with no `Directory.Build.props`: MSBuild
probes upward, finds nothing, and `project.Imports` contains no evidence of the probe. A
caller keys cache validity on `imports` (`docs/msbuild-evaluation.md` calls it the
imported-input list). Someone then adds `Directory.Build.props` in a parent directory;
every input in the recorded closure is unchanged, the cached result is reused, and
`TargetFramework` / `DefineConstants` / `Compile` membership are now wrong. MSBuild raises
`ProjectImportedEventArgs` with `ImportIgnored`/`UnexpectedlyMissing` for exactly this, but
the logger discards non-error/warning events.

## 8. `glob_patterns` omits condition and Update provenance

**`tools/tethys-msbuild-evaluate/Evaluation.cs:96`** · correctness

`glob_patterns` records item declarations from the entire import closure with no
`condition` and no `update` field, so conditioned-out declarations and Update-only elements
are indistinguishable from live include/exclude/remove provenance.

**Failure scenario.** Evaluate any `Microsoft.NET.Sdk` project: `GetLogicalProject()`
yields every `ProjectItemElement` in the SDK closure. `<Compile Update="**/*.cs" ... />`
(how the SDK applies default metadata) produces `{project_path, "Compile", "", "", ""}` — a
row with no provenance at all — and a
`<Compile Include="**/*.cs" Condition="'$(EnableDefaultCompileItems)'=='true'" />` under a
false condition produces a row identical in shape to a live one. An S3 consumer computing
"which files can this project include" from `glob_patterns` will treat dead branches as
live. The Literal fixture assertion (`worker_qualification.py:252`) only exercises a
2-element, import-free project, so neither case is fenced.

## 9. Exit-code fence only checks the success direction

**`.tethys-82a6/oracles/worker_qualification.py:214`** · test-coverage

The exit-code fence only asserts the success direction: `require(not success or status ==
0)` never checks that a failed evaluation exits non-zero, leaving the half of the contract
a process-status-keying caller depends on unfenced.

**Failure scenario.** `Program.cs:57` returns `response.success ? 0 : 1`. Mutate it to
`return 0;` and every negative case in the oracle still passes —
`worker(request(literal, protocol_version=999), success=False)`, the untrusted case, the
missing-host case and the `MissingImport` case all short-circuit the status check because
`not success` is True. A Rust caller that treats exit code 0 as "evaluation usable" would
then accept every failure response. The `if not success:` branch immediately below checks
diagnostics and `cache_eligible` but not `status`.

## 10. `max()` on `(version, Path)` crashes on tied SDK versions

**`.tethys-82a6/oracles/worker_qualification.py:100`** · correctness

`max(entries)` on `(version_tuple, Path)` pairs raises `TypeError` when two SDKs share a
version tuple, and `TypeError` is not in the caught exception tuple at line 363, so the
runner dies with a raw traceback instead of a C5/C14 FAIL.

**Failure scenario.** `dotnet --list-sdks` on a machine with `10.0.100-preview.5.25277.114`
and `10.0.100` installed (or the same SDK version present under both `/usr/share/dotnet`
and `~/.dotnet`) yields two entries whose
`tuple(int(x) for x in version.split('-')[0].split('.'))` are both `(10,0,100)`. `max`
falls through to comparing the second tuple element and `PosixPath` implements no `<`:
`TypeError: '<' not supported between instances of 'PosixPath' and 'PosixPath'`. Line 363
catches only `(OSError, ValueError, RuntimeError, subprocess.SubprocessError, KeyError)`.
Separately, the pre-release strip makes a preview sort equal to its release, so even
without the crash the "highest SDK" selection is not well defined.

## 11. Timestamps in metadata make responses non-reproducible

**`tools/tethys-msbuild-evaluate/Evaluation.cs:26`** · correctness

`BuiltInMetadata` includes `ModifiedTime`, `CreatedTime` and `AccessedTime`, putting
wall-clock filesystem timestamps into every item of every response — non-reproducible
output plus three extra stat calls per item.

**Failure scenario.** Two evaluations of an unchanged project produce different JSON:
`AccessedTime` moves whenever the OS updates atime (and the evaluation's own file reads can
move it). The response is the evidence carrier for `cache_eligible`, so any consumer that
hashes or diffs it sees spurious change. It also makes the oracle's metadata equality check
(`worker_qualification.py:314-317`) flaky by construction — the worker and the
direct-MSBuild oracle stat the same files at different wall-clock times and any atime/ctime
drift between the two runs fails `C5 {kind} metadata mismatch`. Cost is 3 extra `stat`s per
item per evaluation on top of the `FullPath` lookup at line 79.

## 12. Unbounded stdin write and thread join can hang runner

**`.tethys-82a6/oracles/worker_qualification.py:68`** · robustness

`run()` writes the payload with no deadline and then joins the drain threads
unconditionally, so a child that stops reading stdin — or grandchild MSBuild nodes that
inherit the pipes after a kill — hangs the runner past its own 60-second contract.

**Failure scenario.** `process.stdin.write(payload)` at line 68 precedes
`process.wait(timeout=timeout)` at line 70, so nothing bounds the write. `Program.cs:26`
throws `Request exceeds 1 MiB` and stops reading; on a platform where the pipe does not
immediately signal EPIPE the parent blocks forever. Worse: on the timeout path,
`except BaseException` kills only the direct child, then `finally` calls `thread.join()`
with no timeout — `MSBuild.exe` node processes that inherited the stdout/stderr handles keep
the pipes open, `stream.read(65536)` never returns, and the join blocks until the whole
GitHub job is killed. In both cases the 60 s budget is silently a 10-minute (or 6-hour)
budget, and the failure surfaces as an opaque job timeout instead of `C5/C14 FAIL`.

## 13. Publish dir never cleaned; stale DLLs ship in bundle

**`.tethys-82a6/oracles/worker_qualification.py:157`** · correctness

`prepare()` never cleans the distribution directory before publishing, and both the
`dotnet publish -o` and the `MSBuild -t:Build -p:OutputPath=` steps merge into whatever is
already there, so stale artifacts from an earlier prepare ship in the release bundle.

**Failure scenario.** Run `--prepare`, change the csproj (drop a `PackageReference`, bump
Newtonsoft, rename the assembly), run `--prepare` again: the removed/old DLLs remain in
`target/worker-dist/msbuild-evaluate/{sdk,framework}` because neither `publish -o` nor
`-t:Build` deletes unknown files in the output directory. `check_distribution` only asserts
presence of three SDK files and absence of four hardcoded MSBuild DLL names, so the stale
files pass and `release.yml` tars/zips them into `tethys-linux.tar.gz` /
`tethys-windows.zip`. A stale `Newtonsoft.Json.dll` of a different major version shadowing
the current one is the concrete failure mode.

## 14. Bundled-runtime fence is a 4-name denylist

**`.tethys-82a6/oracles/worker_qualification.py:177`** · test-coverage

`check_distribution`'s bundled-runtime fence is a 4-name denylist that omits the transitive
`Microsoft.Build` dependencies that actually break MSBuildLocator hosting when copied
app-local.

**Failure scenario.** `packages.lock.json` pulls `Microsoft.NET.StringTools 17.11.48`,
`System.Collections.Immutable 8.0.0`, `System.Reflection.Metadata 8.0.0` and
`System.Threading.Tasks.Dataflow 8.0.0` under `Microsoft.Build`. `ExcludeAssets="runtime"`
currently keeps them out of the publish output, but nothing fences that: change
`ExcludeAssets` to `PrivateAssets`, add a direct reference to any of them, or add a new
`PackageReference` that depends on a different version, and they land in
`msbuild-evaluate/sdk/`. The denylist (`microsoft.build.dll`,
`microsoft.build.framework.dll`, `microsoft.build.utilities.core.dll`,
`microsoft.build.tasks.core.dll`) passes. At runtime the app-local copy binds before
MSBuildLocator's `AssemblyResolve` handler ever fires (that handler only runs on probe
failure), so the worker loads StringTools 17.11 against a selected MSBuild 18.0
installation and throws `TypeLoadException`. An allowlist of expected output files would
catch the general case.

## 15. Duplicate misnamed tests hardcode `python3`

**`tests/discovery_cli.rs:1`** · simplification

`tests/discovery_cli.rs` and `tests/msbuild_discovery.rs` are near-identical 21-line
duplicates that differ only by a CLI flag, are both misnamed, and both hardcode `python3`
which is a Microsoft Store alias stub on Windows.

**Failure scenario.** The two files differ only in `--installed-only` and the assertion
message; everything else — `CARGO_MANIFEST_DIR`, the `PYTHON`/`TETHYS_QUALIFICATION_HOST`
env lookups, the `Command` construction, the output assertion — is copy-pasted, and each is
a separate integration-test binary (extra link + compile cost on every `cargo test`).
Neither name matches its content: `discovery_cli.rs` never invokes the tethys CLI and
`msbuild_discovery.rs` never touches discovery; both just shell out to the same Python
script. And `unwrap_or_else(|| "python3".into())`: on a Windows dev box without `PYTHON`
set, `python3.exe` resolves to the App Execution Alias, which opens the Microsoft Store and
exits 9009, so `cargo test -- --ignored` fails with an unrelated error. One parameterized
test (or one file with two `#[ignore]` fns) removes the duplication.

---

## Provenance

Produced by `/code-review xhigh PR #45` on 2026-09-08 as a single forked agent
(`a1a5f1a9a88484457`), 26 Bash calls, no subagents. The agent emitted exactly these 15
findings and no intermediate candidate list; reasoning content is not persisted to the
subagent transcript, so any candidates it considered and discarded left no trace.
