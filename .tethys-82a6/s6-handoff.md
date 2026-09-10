# S6 hand-off: corpus qualification, methodology and evidence

Date: 2026-09-08. Author: Main. Companion artifacts: `plan.md` (S6 slice, cadence
contract, corpus authority), `evidence.md` (empirical premises P1–P9 and cadence
applicability), `review-decisions.md` (bounded repairs R1–R8), `design.md`
(performance amendment), `evidence/s6/` (retained reports).

`tethys-82a6` is **blocked** on seven stories; this file is the shared context they
reference so nothing is rediscovered from scratch.

## Where the branch stands

- Branch `feat/tethys-82a6-corpus`, pushed, PR #49 (draft, base
  `feat/tethys-82a6-coupling`).
- CI is green at `1133495` (all jobs, including the Windows Managed Worker leg).
  Later pushes: `b43669a` (lane evidence retention).
- S1–S5 are implemented and reviewed. S6 (qualification) is not accepted.

## The stories this hand-off serves

| Story | Needs from this document |
|---|---|
| `tethys-et80` | Transparency reproduction recipe; the identical-stdout hashes; why per-root comparisons are invalid |
| `tethys-vcxy` | Corpus lane command, environment, cadence roster, the four fixed defects, adjudication rule |
| `tethys-7sme` | Density capture command, the `density_baseline` contract, what a review must record |
| `tethys-nmzo` | Calibration requirements (14 observations, pinned runner, explicit review) |
| `tethys-9vio` | Windows VM harness and the two classic tests that need the CI fixture environment |
| `tethys-gw9c` | The VM provisioning steps below, verbatim |
| `tethys-hucm` | Suite arithmetic inputs (index counts and measured per-index seconds) |

## Qualification runner

`.tethys-82a6/oracles/qualification.py` enforces a per-cadence lane roster before any
runtime work and binds the cadence cap to the aggregate invocation (argument validation
through the final lane, including preparation, transparency controls and canonical
projection). Repetitions are exact for nightly/release. Fences:
`qualification_cadence_fence.py` (8 cases) and `qualification_deadline_fence.py`
(5 cases), each with a named mutation proven red and restored green.

Corpus lane (Linux, the acceptance evidence):

```
target/qualification/venv/bin/python .tethys-82a6/oracles/qualification.py \
  --corpus --cadence qualification --repeat 2 \
  --environment target/qualification/corpus-environment-v5 \
  --sdk /home/dwalleck/.local-dotnet/sdk/10.0.102 \
  --companion target/worker-dist/msbuild-evaluate \
  --cli target/debug/tethys --allow-restore \
  --manifest tests/fixtures/msbuild/qualification/corpus.json \
  --output target/qualification/corpus-qualification-N/report.json
```

Per repository the lane acquires the pinned SHA, captures direct-host metadata, verifies
it against the committed authority, then runs `2 repetitions × 2 modes` product indexes
plus one same-root transparency control each. Four repositories therefore mean 16
measured indexes and 16 controls.

Density capture (qualification cadence, capture-only, unreviewed by definition):

```
... qualification.py --capture-density --cadence qualification --repeat 2 \
  --environment target/qualification/fixture-environment --allow-restore --output ...
```

## Environment and data

- `tests/fixtures/msbuild/qualification/corpus.json` pins four public repositories by
  immutable SHA (Avalonia 11.3.7, xUnit v3-3.2.2, Newtonsoft.Json 13.0.4, MSBuild
  v18.8.2) with their capture files and reviewed expected-standing manifests, all
  committed (~59 MB of captures).
- The corpus lane must use `corpus-environment-v5`; its frozen fingerprint matches the
  captures byte-for-byte. `fixture-environment` is a different resolver state and will
  fail the reviewed-environment check.
- Reviewed standing (reviewer `Daryl Walleck`, 2026-09-08): avalonia 104 projects/159
  units, msbuild 85/80, xunit 47/83, newtonsoft-json 4/21; `expected_exit` is 0 for the
  invoked driver, which exits non-zero only on a fatal discovery failure.
- Derivation and review rules live in `.tethys-82a6/oracles/corpus_review.py`. Units are
  **not** derived for a project-level Restore failure (`docs/msbuild-evaluation.md`: a
  failed outer evaluation is an indeterminate project; only failed *inner* evaluations
  retain selectors).

## Product defects the lane found (all repaired)

| ID | Defect | Commit | Fence |
|---|---|---|---|
| R1/R2 | Authorized Restore inherited the inner-evaluation `TargetFramework`, overwriting shared multi-target assets; currentness accepted target-derived downloads without graph evidence | `fe0c5aa` | native `authorized_restore_*` fences |
| R3/R4 | Cadence cap checked per lane; runtime deadline snapshotted per lane | `e6f1fe7` | `qualification_{cadence,deadline}_fence.py` |
| R5 | An explicit installed host was rejected when a repo `global.json` resolved to a different SDK | `15fafe4` | `explicit_host_is_not_rejected_by_a_conflicting_global_json` |
| R6 | Implicitly defined package dependencies (e.g. `Microsoft.NETFramework.ReferenceAssemblies`) were refused under target evidence | `a49349c` | `implicit_package_dependencies_require_target_evidence_without_relaxing_explicit_versions` |
| R7 | Platform framework aliases (`net8.0-tizen8.0` vs graph `net8.0-tizen`, authoritative `targetAlias`) failed corroboration | `e9ddbd6` | `platform_framework_aliases_resolve_to_their_assets_key` |
| R8 | Restore input identity was spelling-sensitive (`\\?\C:\…` vs `C:\…`) in the receipt, closure membership and restored-path lookup | `91c3d12`, `1133495` | `receipt_digest_ignores_verbatim_spelling_and_order` (Windows-only) |

Reviewed-expectation correction found by the same lane: units are not published for
project-level Restore failures (avalonia 178→159, msbuild 86→80, xunit 85→83).

## Open blocker: transparency (`tethys-et80`)

The corpus lane aborts when the instrumented build's canonical facts differ from the
uninstrumented control's. At `corpus-avalonia-stream-1` the two runs published **byte-
identical discovery reports** (sha256 `b781764664449e334031cd415a3c289ee139bf9201dafab97ed7188e49cf7b42`,
20,218,897 bytes each, 104 projects / 159 units) yet the canonical snapshots differed.

Reproduction rules learned the hard way:

- Both runs **must share one root**, reset to the pristine snapshot between them.
  Generated `obj` files (`project.assets.json`, `*.nuget.dgspec.json`,
  `project.nuget.cache`) embed the absolute root, so comparing two different roots
  produces a false difference.
- Retained binaries for one such pair:
  `target/qualification/corpus-qualification-5/runtime/instrumentation/{instrumented,control}-tethys-qualification-driver`,
  pristine snapshot at
  `target/qualification/corpus-qualification-5/work/corpus/corpus-avalonia-prepared`,
  payload at `.../runtime/run-00002/input.json`.
- `canonical_snapshot` (`.tethys-82a6/oracles/qualification_runtime.py`) drops
  `evaluation_inputs.modified`, normalizes the root to `$WORKSPACE`, sorts rows and
  excludes `evaluation_cache`; the diff therefore has to be per table.

## Windows VM harness (`tethys-gw9c`, used by `tethys-9vio`)

The local VM `resourcefs-win11` (Windows 11 Pro, 2 vCPU, 16 GB) is reachable over WinRM
at `127.0.0.1:55985` via `docs/2026-09-06-legacy-probe-sidecar/vmps.py`, which needs
`PROBE_VM_CREDENTIALS=/home/dwalleck/.config/resourcefs/windows-vm/credentials.env` and
`uv run` (the script declares `pywinrm` inline).

Provisioned on 2026-09-08 on request:

1. C++ build tools workload (the MSVC linker Rust needs):
   `C:\probe\vs_BuildTools.exe --quiet --wait --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended`
   → MSVC 14.44.35207.
2. Rust: download `https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe`
   and run `-y --default-toolchain stable-x86_64-pc-windows-msvc --profile minimal`
   → Rust 1.98.1.
3. Stage sources: `git archive --format=zip -o /tmp/tethys-src.zip HEAD`, serve `/tmp`
   over HTTP from the host, download on the guest from the gateway address, and
   `Expand-Archive -Force` to `C:\tethys`.
4. Companion: `dotnet restore --locked-mode` then
   `dotnet publish -t:... -c Release -f net8.0 -o target\worker-dist\msbuild-evaluate\sdk`.
5. Build/run with the VS environment imported:
   `cmd /c "vcvars64.bat && set"` piped into PowerShell before `cargo`.
6. Test environment: `TETHYS_SDK_MSBUILD_PATH=C:\Program Files\dotnet\sdk\9.0.317`,
   `TETHYS_WORKER_DISTRIBUTION=C:\tethys\target\worker-dist\msbuild-evaluate`.

Caveats that cost time:

- `Expand-Archive` resets file timestamps, so cargo may skip a rebuild. Touch the
  re-staged file (`(Get-Item …).LastWriteTime = Get-Date`) or clean the package.
- `cargo test` (libtest) rejects nextest-only flags such as `--no-fail-fast`.
- Two classic tests need the CI fixture environment my ad-hoc run lacked
  (`TETHYS_NUGET_FIXTURE_SOURCE`, `NUGET_CERT_REVOCATION_MODE`):
  `windows_filter_solution_default_restore_and_ambiguity` and
  `windows_packages_config_restore_requires_grant_and_confirms_native_inputs`.
  All 21 other ignored Windows tests passed.
- Never `pkill -f tethys-qualification-driver` while a lane runs; it killed two
  captures this session. The chain `corpus → density` in one shell is safer than a
  `pgrep` waiter (a waiter's own command line matches its own pattern).

## Gotchas worth not relearning

- Production never reads `TETHYS_WORKER_DISTRIBUTION`; the companion resolves from
  `DiscoveryOptions.companion_directory` or `<exe_dir>/msbuild-evaluate`. To drive the
  CLI by hand, copy the binary and the companion into one directory.
- CI's native fixtures select an **8.x** SDK (`TETHYS_SDK_MSBUILD_PATH` from the setup
  step), while the corpus lane uses 10.0.102. Fence fixtures must behave on both bands:
  `ManagePackageVersionsCentrally` + `VersionOverride` needs a declared central
  `PackageVersion` or SDK 8 ignores the override and fails NU1605.
- The cadence cap binds the aggregate invocation, so a multi-hour lane breaches it by
  design. Lane records are now persisted before the cap check, so a breach retains
  evidence instead of discarding it.

## Evidence inventory

Committed (tracked, durable):

- `tests/fixtures/msbuild/qualification/` — four captures + `corpus.json` + reviewed
  expected-standing manifests + `limits.json` + `shapes.json`.
- `.tethys-82a6/evidence/s6/` — corpus lane report, per-run timing stderr, density
  capture report, reviewed-projection proposals and summary, baseline phase summary,
  and `transparency-stdout.sha256.json`.
- `.tethys-82a6/evidence/` (earlier slices) — mutation runs, platform acceptances,
  S4/S5 qualification records.
- `.tethys-82a6/oracles/` — runner, corpus review derivation, fences, driver.
- `.tethys-82a6/probe-*.py`, `probe-performance.cs` — the performance probe and oracles.

Gitignored (`target/qualification/…`, large, regenerate rather than rely on):

- `corpus-qualification-5/` (~439 MB) — full run stdout, instrumented/control binaries,
  pristine prepared snapshot. The instrumented/control pair is the reproduction input
  for `tethys-et80`; if it is cleaned, re-run the corpus lane to rebuild it.
- `density-capture-2/` (~201 MB) — failed capture (driver killed locally).
- `baseline-f34-v1/` — the retained 240-project/401-unit failure measurement.
