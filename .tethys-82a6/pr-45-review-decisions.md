# PR #45 review decisions

Source: `docs/pr-45-code-review.md` (15 findings, head `5237b91`, `/code-review xhigh`).
Reviewed by Main on 2026-09-08 against the reviewed head **and** the current stack head,
using three read-only verification scouts plus independent checks. Finding IDs are
`P45-n` to avoid collision with this workflow's own F1–F33 rows in `review-decisions.md`.

Method notes: `src/discovery/**` does not exist at `5237b91` — the Rust adapter, cache
closure and containment checks are later-stack additions, so several findings are real in
the worker but already closed one layer up at the current head. No fix has been applied
yet; the repair plan below is prepared for approval and the applicable gate.

Corrections made during verification: `P45-10` was split after running the example — the
`TypeError` crash claim is **refuted** (`pathlib.PurePath` is orderable, and
`selected_sdk` builds only same-flavour paths), while the selection ambiguity is real and
resolves *against* the stable release. `P45-3` was refuted by the reviewed head's own CI
timing. `P45-6`/`P45-7`/`P45-8` retain verified mechanisms but refuted impact or severity.

## Decision log

| finding-id | finding | reviewer | evidence-state | evidence | decision | fix | note |
|---|---|---|---|---|---|---|---|
| P45-1 | Host self-check ignores the effective `MSBuildBinPath`; fix: compare it to `msbuild_path` | code-review xhigh | Verified | `Evaluation.cs:52-55` compares only the loaded assembly directory; `plan.md:206` records worker `MSBuildBinPath=Current/Bin/amd64` vs oracle `Current/Bin` | Modify | Run the comparison after the properties loop (not at line 53 — properties are unavailable before `LoadProject`), canonicalizing both sides as `host.rs:1023-1033` does; add a divergent-toolset fixture | The stack already rejects divergence in the Rust adapter as `toolchain-unavailable`; the worker guard is defence-in-depth and must not canonicalize differently or one layer will accept what the other rejects |
| P45-2 | Release/CI depend on the issue-local `worker_qualification.py`; fix: move it to durable tooling | code-review xhigh | Verified (dependency) / Refuted (retirement premise) | `release.yml:101/105/108/112`, `ci.yml:301/304/306/309`; `plan.md:75` "non-production"; 25 `.tethys-*` trees tracked and none ever removed | Modify (tracked at `tethys-yqqb`) | Split the packaging driver into `scripts/`; keep qualification oracles issue-local and remove their CI steps at issue close | Placement change: design/plan update required before implementing |
| P45-3 | `timeout-minutes: 10` is too small for the required Windows job; fix: raise to 25 | code-review xhigh | Refuted | CI run `34089139689` at the reviewed head: `Managed Worker (windows-2022)` completed in **2m32s** of 10m (`setup-dotnet` 5s, `--host windows --prepare` 1m58s); the 25-minute value belongs to the S6 corpus job | Reject | `N/A` | Measured evidence contradicts the premise; ~4x headroom, and `design.md:11` sets a 10-minute PR semantic cap |
| P45-4 | 60 s default timeout on qualification launches; fix: raise | code-review xhigh | Verified | `worker_qualification.py:46` default 60; call sites `:209` worker, `:228` sentinel build, `:230-231` oracle, `:266` missing-companion; `prepare()` uses 300 at `:154/:158/:163` | Modify | Pass 300 explicitly at the MSBuild/worker sites; keep 60 for `--list-sdks`, vswhere and the missing-companion probe | The cold-runner "routinely exceeds 60 s" sub-claim is unverified; the asymmetry is real. Meaningless without P45-12 |
| P45-5 | vswhere hard-pinned to `[17.14,17.15)` blocks releases; fix: relax | code-review xhigh | Verified | `:108-111`; `release.yml:105` → `prepare()` → `selected_windows()`; `release` needs `[verify, build]` | Modify | Keep the strict pin and hard failure inside `qualify()`; give `prepare()` a tolerant `-latest` selector | The pin encodes the x86/amd64 host contract from this PR; move the assertion, do not delete it |
| P45-6 | `workspace_root` validated but never enforced; fix: enforce containment or drop the field | code-review xhigh | Verified (mechanism) / Refuted (security framing) | `Program.cs:36-39` only normalizes + `Directory.Exists`; `docs/msbuild-evaluation.md:3` "the evaluation grant is trust, not a sandbox"; the current head enforces containment as `OutsideWorkspaceInput` | Modify | Enforce `project_path` containment in `Program.cs` using canonical identity and accepting both spellings; update the protocol doc | Dropping the field is a breaking protocol change; the field is already consumed by the cache key |
| P45-7 | Failed import probes are omitted from the input closure; fix: subscribe to import events | code-review xhigh | Verified (mechanism) / Refuted (impact) | `Evaluation.cs:88-89` + logger `:189-198`; importing projects are already cache-ineligible (`:139`, `:105-106`); the named `UnexpectedlyMissing` event does not exist and `ImportIgnored` excludes the glob-no-match case | Reject | `N/A` | The proposed fix cannot observe the claimed case, and the claimed stale-reuse scenario is unreachable in the eligible (import-free) class |
| P45-8 | `glob_patterns` omits condition and Update provenance; fix: add the fields | code-review xhigh | Verified (mechanism) / Refuted (impact) | `Evaluation.cs:96-100`; `Contract.cs:51-58`; no consumer reads dead branches as live, and any condition already disqualifies the receipt | Reject (tracked at `tethys-w25h`) | `N/A` | Protocol change with no consumer today; recorded as backlog |
| P45-9 | Exit-code fence asserts only the success direction; fix: assert non-zero on failure | code-review xhigh | Verified | `:214` `require(not success or status == 0)`; every negative case passes `success=False`; `Program.cs:57` | Accept | Add `require(status != 0, …)` inside the `if not success:` branch | Pure regression fence; all negative cases already exit non-zero |
| P45-10a | `max()` over `(version, Path)` crashes on tied SDK versions (`TypeError`) | code-review xhigh | Refuted | Ran a tied-version example: `max([((10,0,100), Path('…/10.0.100-preview.5.25277.114')), ((10,0,100), Path('…/10.0.100'))])` returns the preview path, no exception — `PurePath` is orderable (pathlib "General properties"), and `selected_sdk` builds only same-flavour `Path` values | Reject | `N/A` | No crash to fix; adding `TypeError` to the fatal-only handler would mask genuine bugs |
| P45-10b | The same tie makes "highest SDK" selection ill-defined (preview vs its release) | code-review xhigh | Verified | Same example: the preview sorts **after** its release (`Path('…/10.0.100-preview…') < Path('…/10.0.100')` is `False`), so `max` picks the prerelease; the pre-release strip (`version.split("-")[0]`) collapses both to `(10,0,100)` | Modify | Rank by `(core_version, 0 if prerelease else 1, prerelease_parts, str(path))` so a release outranks its preview and remaining ties are explicit | Low severity — a machine must hold both a preview and its release — but the selection should be intentional |
| P45-11 | Timestamps in metadata make responses non-reproducible; fix: remove them | code-review xhigh | Verified | `Evaluation.cs:26` + `:82-83`; the oracle compares the direct-host keys at `:313-317`, so removal alone fails CI; the stack already strips the same three names | Modify | Remove the three names **and** skip them in the oracle comparison in the same commit | No test asserts them as expected values |
| P45-12 | Unbounded stdin write and unconditional thread join can hang the runner | code-review xhigh | Verified (structure) / Plausible (hang) | `:68`/`:70`, kill path `:71-73`, `thread.join()` `:76-77`; MSBuild node reuse is a known pipe-holding hazard | Modify | Daemon drain threads + `join(timeout)` + process group / `start_new_session` + `-nodeReuse:false` on direct MSBuild invocations | Prerequisite for P45-4: a longer `wait` timeout does not bound the write or the join |
| P45-13 | Publish directory never cleaned; stale DLLs ship | code-review xhigh | Verified | `prepare()` `:152-165` has no cleanup; `publish -o` and `-t:Build -p:OutputPath` merge; `check_distribution` asserts presence only | Accept | `shutil.rmtree(destination / "msbuild-evaluate", ignore_errors=True)` before publishing | The staged `tethys` binary sits outside that directory and is untouched |
| P45-14 | Bundled-runtime fence is a 4-name denylist | code-review xhigh | Verified | `:177-180` vs `packages.lock.json` (`Microsoft.NET.StringTools`, `System.Collections.Immutable`, `System.Reflection.Metadata`, `System.Threading.Tasks.Dataflow`, …); `ExcludeAssets="runtime"` currently keeps them out | Modify | Derive the forbidden set from the `Microsoft.Build` closure in `packages.lock.json` instead of a hand-maintained list | Self-maintaining when a transitive dependency or `ExcludeAssets` changes |
| P45-15 | Duplicate misnamed tests hardcode `python3`; fix: merge them | code-review xhigh | Verified (duplication, `python3`) / Refuted (misnaming at head) | Both files are 21-line wrappers differing by one flag; the `python3` fallback is at line 7 in both; at the current head both files carry real tests and CI excludes these two from `--run-ignored all` | Modify | One file with both ignored fns sharing an interpreter resolver (`PYTHON` → `python` on Windows → `python3`) that panics actionably; update the CI exclusion filter | Merging alone does not fix the `python3` fallback; do not delete either case — they fence different contracts |

## Repair plan (applied)

Landed on `feat/tethys-82a6-worker` (PR #45) per the maintainer's decision, then merged
forward so the stack head carries the same fixes:

| Commit | Branch | Findings |
|---|---|---|
| `24479dd` | worker | `P45-1`, `P45-6`, `P45-11` (worker protocol) |
| `fdd7b59` | worker | `P45-4`, `P45-5`, `P45-9`, `P45-10b`, `P45-12`, `P45-13`, `P45-14` (oracle + fences) |
| `0dd4940` | worker | `P45-15` (shared interpreter resolver) |
| `72e973a` | discovery | merge of the above |
| `0471d04` | publication | merge |
| `a91816c` | coupling | merge |
| `ce23669` | corpus | merge (docs conflict resolved by combining both texts) |

Verification (all on this host, `--host sdk`):

- Full C5/C14 qualification passes: `python3 .tethys-82a6/oracles/worker_qualification.py --host sdk --prepare` then `--host sdk` → `C5/C14 PASS: independent manifests + direct MSBuild, no targets, clean distribution`.
- `P45-1`/`P45-6`/`P45-11` fences, each proven red then restored green:
  - timestamps re-added to `BuiltInMetadata` → `Filesystem timestamps are not contract metadata and break response reproducibility`;
  - containment removed from `Program.cs` → `A project outside workspace_root was not refused`;
  - exit contract broken (`return 0`) → `C5 failure response with success exit: 0`.
- `P45-12` reproduced and fixed: a 200 KB payload to a non-reading child raised after 2.0 s; a grandchild holding the pipes raised after 2.0 s (previously unbounded).
- `P45-13`: a planted `stale-artifact.dll` does not survive `--prepare`.
- `P45-14`: a planted `System.Text.Json.dll` in the bundle fails `check_distribution`; removing it passes. The derived set is unioned with the explicit names because the lock graph at this version does not list `microsoft.build.utilities.core`/`tasks.core`.
- `P45-10b`: `sdk_rank` orders `10.0.102 > 8.0.417`, `10.0.100 > 10.0.100-preview…`, `10.0.100-preview… > 9.0.310`.
- `P45-15`: both wrapper tests pass via the shared resolver; the 11 native tests pass on the merged stack head with the CI environment.
- Quality: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and both worker target frameworks (`net8.0`, `net472`) build.

Not fenced locally: `P45-1`'s divergent-toolset path (needs a host whose effective toolset
differs — the Windows qualification leg and the oracle's own `MSBuildBinPath` identity
assertion cover it) and `P45-5` (Windows-only selection split).

`P45-2` is a placement change and `P45-8` is backlog; both are tracked and excluded from
this repair set until their design/plan update lands.

## Landing decision (resolved)

Maintainer: **"Land them on 45"** — repairs landed on `feat/tethys-82a6-worker` and were
merged forward through discovery → publication → coupling → corpus, so every PR in the
stack carries them without a history rewrite.
