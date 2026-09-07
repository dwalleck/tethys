# Plan: tethys-82a6

Approved inputs: route.md, spec.md, design.md (architecture approval verbatim “yes, I approve”, 2026-09-06). Structural route; no review re-entry. Design C0 passed including early-commit negative control; C1–C14 have named independent oracles, mutations and pending checkpoint owners. No risk waiver. All seven issue acceptance criteria remain mandatory.

Baseline: repository upstream discovered as refs/remotes/origin/main. Integration branch: feat/tethys-82a6-evaluation. Writers use isolated workspaces and disjoint ownership; Main integrates and owns shared seam edits/validation. No worker runs formatters, builds, lint or tests concurrently. No slice is declared complete here.

## Fixed cross-slice contracts

The managed evaluator uses protocol_version=1 and one JSON request on stdin / one JSON response on stdout; stderr is diagnostics only. Request: workspace_root (canonical absolute), project_path (absolute), target_framework (nullable SDK selector), global_properties (string map), msbuild_path (selected installation directory), trust_granted (bool). Host is registered before Microsoft.Build types load. Response: protocol_version, success, project_path, host {kind,path,version,runtime}, properties (string map), items (Compile/ProjectReference/Reference/PackageReference/PackageVersion/PackageDownload arrays of {include,full_path,metadata}), imports (absolute paths), glob_patterns (include/exclude/remove evidence), diagnostics ({code,exception_type,message,file,line,column,severity}), cache_eligible and cache_ineligibility. Failure still returns diagnostics; no guessed target-unit binding. Native codes are data, never inferred from human substrings. SDK/Framework worker packaging supplies the correct runtime flavor. Contract adjustments inside the approved fields are coordinated before implementation, never inferred separately.

Property projection returns required framework/identity/compiler/output/restore/toolchain fields plus explicit caller-global keys, not the ambient environment property bag. An ambient secret-shaped sentinel must not appear in returned properties. Companion build outputs under tools/tethys-msbuild-evaluate/bin and obj are ignored; generated lock metadata remains tracked.

Neutral DiscoveryRequest/Snapshot types live only in src/discovery/types.rs; coordinator/mod owns adapter dispatch; Cargo adapter stays in src/cargo.rs. Library discovery can be exercised independently before index wiring. All integration callers consume the one canonical type, no duplicated wire/schema models outside managed serialization DTOs.

Revision owner uses live BEGIN IMMEDIATE, nested savepoints and a scoped BatchWriter borrowing Index. The owner never holds its MutexGuard while the writer locks it. Fatal storage/channel/architecture errors roll back; bounded source/evaluation diagnostics remain distinct. Rebuild performs DDL under the transaction, not pre-delete. Work on the public facade is serialized through Main.

## Review-size partition

| PR increment | Slices | Estimate | Independently mergeable definition |
|---|---|---:|---|
| Atomic index revisions | S1 | 1400 | Existing Rust/C# syntax indexing uses atomic batch/stream/rebuild publication; old/new SQL manifests prove it without discovery code |
| Evaluation companion | S2 | 1600 | Real packaged SDK/Framework worker evaluates fixture metadata against direct-MSBuild/hand manifests; no Rust consumer needed |
| Neutral discovery | S3 | 9700 | Cargo/MSBuild discovery interface returns complete metadata/failures/cache evidence under grants, independently exercised through its public seam |
| Persisted evaluated discovery | S4 | 1600 | Index/reindex store and publish contexts/projects/units/membership and return typed partial coverage; snapshots readable without coupling |
| Evidence-aware coupling | S5 | 1400 | CLI/library coupling reports units/declarations/unknown metrics from persisted snapshot; all queries remain evaluation-free |
| Corpus qualification | S6 | 800 | Reproducible pinned qualification/measurement tooling and records, requiring all earlier behavior; no product placeholder |

Sum: 16,500 changed lines (implementation+tests+fixtures/docs/evidence). Churn margin: 25%=4,125, for managed host packaging and schema/fixture integration; total20,625 >4,000. Each increment has its own observable seam and no dependence on later increments. S3's final scoped checkpoint contains9,611 changed lines across55 files, including committed raw mutation/provenance evidence; the estimate is revised to9,700. The approved candidate/host/restore/cache owners remain distinct, and every production footprint is within its reviewed range. This is not a reason to create unused or placeholder modules. Ownership changes still return to design approval. This partition is not permission to close the issue before S6.

## Module growth ledger

Counts use design's physical-production footprint convention; all new paths start at zero. Ranges are review tripwires, not padding targets. Rationale: move architecture bodies out of parents, keep orchestration thin, keep process/metadata/cache/SQL ownership separate.

| Module | Baseline | Projected final | Responsibility/interface change | Protected-parent rule |
|---|---:|---:|---|---|
| src/lib.rs | 1142 | 1140–1200 | facade/reexports/discovery access | wiring only |
| src/indexing.rs | 1312 | 1250–1400 | revision/discovery/scoped writer; remove architecture body | no language-specific bodies |
| src/reindex.rs | 372 | 340–400 | transactional rebuild/update | no evaluation logic |
| src/batch_writer.rs | 266 | 240–310 | scoped shared Index and propagated failures | no Index::open/commit/discovery |
| src/cargo.rs | 426 | 430–520 | real neutral adapter reusing algorithms | Cargo-only |
| src/languages/module_resolver.rs | 412 | 415–455 | immutable discovery context | no DB/process |
| src/resolve.rs | 1337 | 1337–1370 | context plumbing | no language rules |
| src/db/mod.rs | 358 | 280–390 | lifecycle delegation | no discovery |
| src/db/revision.rs | 0 | 180–350 | transaction/rebuild owner | concrete SQLite only |
| src/db/files.rs | 572 | 570–620 | savepoint/shared-reference receiver | existing file owner |
| src/db/references.rs | 266 | 266–290 | savepoint | existing ref owner |
| src/db/call_edges.rs | 349 | 349–380 | savepoint | no discovery logic |
| src/db/schema.rs | 213 | 300–450 | revision/context/unit/membership/cache DDL | single DDL owner |
| src/db/discovery.rs | 0 | 300–650 | snapshot/cache persistence | no process |
| src/db/architecture.rs | 437 | 450–650 | unit nodes/read-snapshot/coupling persistence | no host policy |
| src/architecture.rs | 0 | 300–600 | moved records/phase + metric evidence | no project parsing |
| src/types.rs | 1737 | 1530–1650 | moved architecture records; owned options | no duplicate records |
| src/main.rs | 364 | 380–440 | flags/dispatch | no implementation bodies |
| src/cli/index.rs | 189 | 180–300 | grants/context/output/transactional rebuild call | library delegation |
| src/cli/coupling.rs | 358 | 380–550 | typed evidence rendering | no aggregation logic |
| src/cli/mod.rs | 121 | 121–150 | shared CLI presentation if needed | no evaluator |
| src/error.rs | 211 | 211–240 | fatal typed cause if required | no standing policy |
| src/discovery/mod.rs | 0 | 30–100 | adapter interface/coordinator | no SQL |
| src/discovery/types.rs | 0 | 250–550 | canonical immutable metadata/options | no execution |
| src/discovery/msbuild/mod.rs | 0 | 550–750 | trust/evaluation workflow | no SQL |
| src/discovery/msbuild/candidates.rs | 0 | 440–550 | non-executing candidate/path parsing | no execution |
| src/discovery/msbuild/host.rs | 0 | 850–1100 | host/process/protocol deadline, runtime qualification and native wire lookup | no inferred grants |
| src/discovery/msbuild/cache.rs | 0 | 300–450 | qualified recipe/input validation and opaque restore receipts | no universal purity claim |
| src/discovery/msbuild/restore.rs | 0 | 1100–1400 | separately authorized restore, actual solution/config provenance and current-input corroboration | no feed/lock override |
| tools/tethys-msbuild-evaluate/Program.cs | 0 | 40–100 | host registration/protocol dispatch | no evaluation bodies |
| tools/tethys-msbuild-evaluate/Evaluation.cs | 0 | 250–500 | evaluated items/properties/imports/diagnostics | no target APIs |
| tools/tethys-msbuild-evaluate/Contract.cs | 0 | 80–180 | JSON DTOs | data only |
| tools/tethys-msbuild-evaluate/Tethys.MSBuild.Evaluate.csproj | 0 | 25–90 | SDK/Framework builds | pinned deps |
| tools/tethys-msbuild-evaluate/packages.lock.json | 0 | generated | managed dependency locks | license gate |
| Cargo.toml | 85 | 85–100 | XML/hash/safe process dependencies as needed | vetted licenses |
| Cargo.lock | existing generated | generated | dependency resolution | no manual version guesses |
| .github/workflows/ci.yml | 311 | 330–410 | focused qualification | no runtime policy |
| .github/workflows/release.yml | 150 | 170–250 | companion package/smoke | no implicit runtime downloads |

Issue-local oracles and test fixtures are non-production. Named test paths occur in the slices below. Required atomic behavior documentation: AGENTS.md, CONTEXT.md, existing CLI/reference docs and per-increment changelog.d/tethys-82a6-<increment>.changed.md; new worker/protocol instructions only where explicitly required by approved design. Documentation source claims change in the same commit as behavior. No unrelated dirty files are staged.

## S1: Publish complete index revisions atomically

**Claim IDs:** C0, C1, C2, C13.
**Review-fix purpose (F1):** incompatible-schema refusal preserves database bytes and sidecars, including a legacy DELETE-mode cache. Open unconfigured, check currency, then configure accepted/rebuild connections inside the existing revision owner. The C2 fence compares bytes as well as rows/schema; its additional mutation configures WAL before the check and must fail. No responsibility or public contract changes.
**Expected behavior:** batch/stream indexing and explicit rebuild publish all facts once; old/schema/data remain intact on injected fatal failure; concurrent readers see old or new; streaming cannot use a second connection.
**Oracle:** independent sqlite3 connection/process over authored old/new row manifests; C0 standalone SQLite protocol oracle; parser-based module ledger.
**Stress fixture:** two writers, a pinned reader, source/ref/arch changes, injected failures after file and DDL writes, streaming queue disconnect. Expected old complete revision until successful commit; second writer blocked, later succeeds after release.
**Regression fence:** tests/revision_publication.rs; existing tests/indexing.rs, tests/idxperf_golden.rs; .tethys-82a6/oracles/revision_transaction.py and module_shape.py.
**Named mutation:** design C1 premature commit/second writer open; C2 pre-delete/pre-BEGIN DDL; C13 forbidden evaluate_msbuild function in protected indexing parent. Red must identify the claim; restoration green.
**Complexity/production scale:** no full-index copy; existing O(files+symbols+refs+edges) writes, 15,124-file stress. Savepoint/phase overhead O(files). Writer serialization is intentional. Maximum accepted RSS 6 GiB, DB+WAL/SHM 10 GiB at pinned stress; no all-row clone introduced. Composite reads have one transaction, not per-row queries.
**Wall budget/phase:** indexing/reindex always-on per invocation; measured corpus total ≤30 minutes under nightly qualification, per approved source. Writer-lock contention uses existing bounded busy timeout; no unbounded retry. One-off explicit rebuild shares the same limits for qualification.
**Module shape:** db/revision owns begin/commit/rollback/schema lifecycle; existing mutators retain their algorithms. Protected deltas: indexing ≤+88 and architecture body not moved until S5; batch_writer ≤+44; reindex ≤+28; lib ≤+58 wiring. `python3 .tethys-82a6/oracles/module_shape.py --stage S1` → C13 PASS; forbidden body mutation red.
**Files:** src/db/revision.rs, db/mod.rs, db/schema.rs, db/files.rs, db/references.rs, db/call_edges.rs, db/architecture.rs, indexing.rs, batch_writer.rs, reindex.rs, lib.rs, cli/index.rs; tests/revision_publication.rs; named existing tests; issue-local module_shape.py/ledger.json; atomic docs/fragment.
**Estimate:** one substantial integration increment; estimate is a signal, not a stop condition.
**Diff estimate:** 1400 lines.
**PR increment:** Atomic index revisions.
**Commands and expected results:**
- `python3 .tethys-82a6/oracles/revision_transaction.py` → C0 old/pinned/rollback/new assertions agree; `--mutate-early-commit` → C0 red.
- `cargo nextest run --test revision_publication --test indexing --test idxperf_golden` → C1/C2 exact old/new/rebuild manifests and existing successful canonical indexing agree.
- `python3 .tethys-82a6/oracles/module_shape.py --stage S1` → approved owners only; named C13 mutation fails.
- `cargo build --lib --bin tethys && rustc --edition=2024 .tethys-82a6/oracles/revision_stream.rs --extern tethys=target/debug/libtethys.rlib -L dependency=target/debug/deps -o /tmp/tethys-revision-stream`; run `python3 .tethys-82a6/oracles/revision_smoke.py --files 15124` with and without `--streaming-driver /tmp/tethys-revision-stream` → real CLI/library publication agrees with independent SQL manifests and measured resource caps.

## S1a: Repair canonical-path fixture portability

**Claim IDs:** C1 repair; no new production claim owner.
**Expected behavior:** the unreadable-source fence accepts the exact canonical file identity on Linux alias roots, macOS temporary-directory aliases and Windows extended paths; stale-fact and unaffected-source checks remain unchanged.
**Oracle:** filesystem canonical identity plus observed Windows/macOS CI failures; a symlinked TMPDIR reproduces the same mismatch locally.
**Stress fixture:** existing two-source fixture under a symlinked temporary root, one unreadable source and one valid sibling; exactly one diagnostic, failed facts absent, valid sibling retained.
**Regression fence:** existing tests/indexing.rs::reindex_reports_unreadable_source_without_publishing_stale_facts, exercised with aliased TMPDIR and by platform CI.
**Named mutation:** restore the fixture's uncanonical expected path; the alias-root run must fail with two spellings of the same file. This repairs an existing fence, not a production behavior change.
**Complexity/production scale:** N/A — no production loop or storage change; fixture still contains two files and one canonicalization.
**Wall budget/phase:** N/A — no always-on phase changed; focused fixture must remain deterministic and bounded.
**Module shape:** production owners/deltas unchanged; module_shape.py --stage S1 must remain PASS.
**Files:** tests/indexing.rs and this plan amendment. No production/docs contract or changelog change is needed.
**Estimate:** one fixture-correctness repair.
**Diff estimate:** 25 lines including gate planning.
**PR increment:** repair in Atomic index revisions PR #44 before resuming S2.
**Commands and expected results:**
- With a created symlink to an isolated temporary directory, `TMPDIR=<alias> cargo nextest run --test indexing -E 'test(reindex_reports_unreadable_source)'` → exact canonical identity and stale-fact assertions pass; original assertion fails.
- `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo nextest run && cargo test --doc` → existing gates green.
- GitHub CI Windows/macOS test jobs on PR #44 → original failing fixture and remaining suites pass before the next increment resumes.

## S2: Deliver an evaluation-only managed worker

**Claim IDs:** C5, C14.
**Expected behavior:** packaged SDK and Windows Framework worker evaluates SDK/classic per-framework items/properties/import metadata with exact selected host and no targets; clean install needs no checkout or implicit build.
**Oracle:** direct `dotnet msbuild` / VS MSBuild `-getProperty/-getItem` with identical globals plus handwritten fixture membership/framework manifest. This is independent of worker serialization/extraction.
**Stress fixture:** SDK multi-target, classic v4.0 Client/v4.7.2/v4.8, linked+imported items, assembly collision, spaces/Unicode, target sentinel. Expected exact evaluated sets and no sentinel; explicitly authorized direct target run creates sentinel as positive control. Framework worker selects actual VS installation, not SDK-only Locator discovery.
**Regression fence:** tests/msbuild_discovery.rs::sdk_classic_metadata (worker seam harness), tests/discovery_cli.rs::installed_companion and managed fixture/launch runner under tests/fixtures/msbuild; release smoke.
**Named mutation:** Evaluation.cs omit imported Compile/use outer values; Program.cs select default rather than requested host; release workflow omit worker. Expected C5 metadata mismatch/C14 install or host mismatch.
**Complexity/production scale:** O(evaluated items+properties+imports) streaming one project request/response, no compilation; 18 TFMs and 802-unit corpus; per-unit RSS included in process-tree 6 GiB limit; output has explicit configured byte bound and oversized response failure, never unbounded buffering.
**Wall budget/phase:** always-on evaluation per cache miss ≤60 seconds/unit; total corpus qualification ≤30 minutes. Host registration once per worker process; no process-wide dependency scanning per item.
**Module shape:** new managed tool owns only evaluation; Program.cs protected entry max100 lines; Evaluation no Build/BuildManager/Roslyn APIs. `python3 .tethys-82a6/oracles/module_shape.py --stage S2` → C13 PASS.
**Files:** tools/tethys-msbuild-evaluate/{Program.cs,Evaluation.cs,Contract.cs,Tethys.MSBuild.Evaluate.csproj,packages.lock.json}; .github/workflows/{ci.yml,release.yml}; tests/fixtures/msbuild; tests/msbuild_discovery.rs; tests/discovery_cli.rs; issue-local worker qualification runner; managed protocol/deployment docs and fragment.
**Estimate:** one managed-host implementation/qualification increment.
**Diff estimate:** 1600 lines.
**PR increment:** Evaluation companion.
**Commands and expected results:**
- `python3 .tethys-82a6/oracles/worker_qualification.py --host sdk --prepare` (Windows: `--host windows --prepare`) → explicit locked restore, dependency-license verification and complete runtime-flavor distribution.
- `python3 .tethys-82a6/oracles/worker_qualification.py --host sdk` → C5 exact fixture manifests; target sentinel negative+positive controls.
- `python3 .tethys-82a6/oracles/worker_qualification.py --host windows` → C5/C14 actual VS17.14 classic profile/import/link metadata and clean-installed worker.
- `python3 .tethys-82a6/oracles/module_shape.py --stage S2` → no target APIs and approved placement.

**Local checkpoint evidence (Windows gate pending):** Both net8.0 and net472 compile with warnings treated as errors, zero warnings/errors. C5/C14 clean-install qualification passes with explicitly selected SDK8.0.417/MSBuild17.11.48.46605, SDK9.0.310/MSBuild17.14.37.60402 and SDK10.0.102/MSBuild18.0.7.61305, running the worker on .NET10.0.2. The initial host-version oracle compared the shorter MSBuildVersion; a direct native-property probe established MSBuildFileVersion as the full ProjectCollection.Version identity, and the oracle now compares that native property. No version string is fabricated. All 25 locked dependencies pass MIT/license-allowlist verification. C13 S2 passes; Program remains93 lines.

**Mutation evidence:** Isolated disposable companion copies (never parent production files) dropped imported items → C5 Compile differs from MSBuild; selected SDK8 instead of requested SDK10, with the worker self-check deliberately neutralized to test the independent oracle → C14 selected host was substituted; removed the bundled companion DLL → missing artifact failure. Unmodified distribution then passed C5/C14 again; disposable mutation directory removed. Fresh-context WorkerReview found no concrete S2 blocker. Actual VS17.14 classic/packaging evidence must pass Windows CI before S3 advances. CONTEXT vocabulary and agent/deployment guidance are updated atomically with this slice.

**Final local gates:** `cargo fmt`, all-target/all-feature clippy with `-D warnings`, nextest1131 passed/10 skipped, doctests18 passed/2 ignored. Explicit ignored-worker run: both msbuild_discovery and discovery_cli fences passed with the packaged real worker. Throwaway boundary probes rejected a valid JSON request padded to1MiB+1 and a64MiB item-metadata response; the latter returned one parseable594-byte failure response, not partial JSON. Probe files were removed. Rust pre-commit checklist reviewed. Platform CI remains the S2 acceptance gate.

**Checkpoint size/upstream:** scoped staged increment1480 changed lines across54 files, within1600-line estimate. Fresh fetch leaves upstream main at the pinned0a2753d50dd8fb335660b247c00e0efa355a8fcc; no reconciliation needed. Only explicit S2 paths are staged; unrelated skills/notes/tracker edits remain outside the checkpoint.

## S2a: Enable required CI for stacked PR bases

**Claim IDs:** C5, C14 (platform gate execution).
**Expected behavior:** every pull-request base runs existing CI, including managed Windows qualification; push CI remains main-only.
**Oracle:** actual GitHub Actions run attached to stacked PR45.
**Stress fixture:** PR45 targets feat/tethys-82a6-evaluation, not main.
**Regression fence:** native pull_request trigger has no base-branch restriction.
**Named mutation:** restore branches:[main] → observed no workflow run after93 seconds/21 polls.
**Complexity/production scale:** workflow trigger configuration only; no runtime changes.
**Wall budget/phase:** existing CI job timeouts unchanged.
**Module shape:** no product owner changes.
**Files:** .github/workflows/ci.yml; this audit record.
**Estimate:** one trigger correction.
**Diff estimate:** 25 lines.
**PR increment:** repair in Evaluation companion PR45 before advancing S3.
**Commands and expected results:** push correction; GitHub run_watch must discover a real run and mandatory managed Windows/SDK jobs must pass.

## S2b: Make the managed lock graph portable

**Claim IDs:** C14.
**Expected behavior:** locked restore succeeds on Linux/macOS and Windows without weakening lock enforcement or changing Framework execution/output defaults.
**Oracle:** Windows managed-worker CI plus local locked restore/license/package qualification.
**Stress fixture:** net472 executable; SDK10 infers win-x86 only on Windows.
**Regression fence:** explicit net472 RuntimeIdentifiers includes win-x86 in the lock graph on every build OS.
**Named mutation:** remove the graph entry → Windows NU1004 observed in run34070722708/job101587441492.
**Complexity/production scale:** build metadata only.
**Wall budget/phase:** existing restore/build deadlines unchanged.
**Module shape:** existing companion project/lock owners only.
**Files:** tools/tethys-msbuild-evaluate/{Tethys.MSBuild.Evaluate.csproj,packages.lock.json}; this audit record.
**Estimate:** one cross-platform dependency-graph correction.
**Diff estimate:** 30 lines plus generated lock graph.
**PR increment:** repair in Evaluation companion PR45 before S3.
**Commands and expected results:** explicit development --force-evaluate refreshes the lock; normal --prepare still uses --locked-mode and license checks; both TFMs compile and SDK oracle passes; actual Windows job must restore and evaluate successfully.

Root cause is native SDK RuntimeIdentifierInference.targets55–63: Windows Framework executables infer win-x86, while the Linux-generated lock had no RID graph. RuntimeIdentifiers requests that graph without forcing RuntimeIdentifier/PlatformTarget or changing the output directory. S2a successfully triggered all stacked-PR CI gates; this is the resulting platform failure, not missing CI.

**Local repair proof:** locked restore succeeds both normally and with explicit RuntimeIdentifier=win-x86. NuGet records the RID graph for both target frameworks; existing package versions remain locked. --prepare revalidates all25 MIT dependencies, SDK clean-install C5/C14 passes, and both target frameworks compile with zero warnings/errors. Actual Windows qualification remains required.

## S2c: Diagnose Framework effective-toolset identity

**Claim IDs:** C5, C14.
**Expected behavior:** classic effective properties agree with the direct selected VS host, without hiding a different toolset behind assembly identity.
**Oracle:** actual Windows direct-MSBuild property values and worker values.
**Stress fixture:** Client40 with explicit ToolsVersion=Current.
**Regression fence:** existing exact property comparison; report both values on failure.
**Named mutation:** divergent MSBuildBinPath already fails run34071104686/job101588538892; no assertion is relaxed for diagnosis.
**Complexity/production scale:** diagnostic-only probe first.
**Wall budget/phase:** existing CI timeouts.
**Module shape:** existing qualification owner; any product fix follows observed evidence.
**Files:** issue-local worker qualification runner; this audit record.
**Estimate:** one platform diagnosis/repair checkpoint.
**Diff estimate:** 30 lines before any evidence-routed fix.
**PR increment:** Evaluation companion PR45.
**Commands and expected results:** push value-reporting probe; read actual Windows mismatch; repair only its demonstrated cause and re-run the full Windows oracle.

S2b now passes actual Windows locked restore, licensing, both runtime-flavor packaging and SDK qualification. Framework assembly path/version and literal controls also pass. First classic unit differs at MSBuildBinPath; exact values are required before choosing between lexical path equivalence and a real effective-toolset mismatch.

**Observed cause and repair:** run34071402251/job101589359326 reports worker MSBuildBinPath=Current/Bin/amd64 versus direct oracle=Current/Bin. This is a real toolset mismatch, not lexical spelling. Primary MSBuild BuildEnvironmentHelper source routes an externally hosted AnyCPU/x64 process to the VS amd64 toolset; Locator1.7.8 does not pin it for VS17.14, and MSBUILD_EXE_PATH is architecture-normalized. Pin net472 PlatformTarget=x86 to match the already-qualified root MSBuild.exe authority. Do not change the oracle or manufacture a Toolset. Existing RuntimeIdentifiers remains a restore-graph setting, not process architecture. Deployment guidance is updated atomically.

**S2 platform acceptance:** commit10d156aabfa176850eea8fdd96c5b26198ed4f74 passes all17 CI jobs in https://github.com/dwalleck/tethys/actions/runs/34072027967. Actual VS17.14/MSBuild17.14.51.32402 evaluates Client40, Classic472 and Classic48 with exact native properties/items and target negative/positive controls from clean packages; Windows SDK10.0.400/MSBuild18.9.6.38015 also passes. Linux/macOS managed jobs and every Rust gate pass. PR45 remains draft, stacked on PR44; S3 can now advance.

## S3: Implement the neutral discovery adapters and freshness contract

**Claim IDs:** C3, C4, C6, C7, C9.
**Review-fix purpose (F2–F8):** preserve native case-insensitive metadata; honor explicit SDK selection; exclude physical generated/internal identities through case variants and arbitrary aliases; reject unqualified runtime cache reuse; preserve actual solution context for normal legacy NuGet restore. These repair C4/C6/C7/C9 within existing owners. Metadata and alias regressions are red, host precedence and runtime staleness are independently reproduced, and native NuGet destination semantics establish the legacy gap. Public DTOs and ownership remain unchanged.
**Expected behavior:** public discovery seam returns unchanged Cargo attribution plus candidate/project/unit metadata or typed failures; missing trust cannot launch, restore is separate, paths contained, reusable eligible hits equal forced evaluation.
**Oracle:** frozen Rust dumps/Cargo expectations; handwritten candidate/reason/exit manifests; direct worker/MSBuild metadata; independent filesystem/process-start log and forced fresh evaluation.
**Stress fixture:** every S1–S8/S11 variant in design; every 11 reason reachable; two valid/failed TFMs; no-candidate versus failed enumeration; link escape and inside twin; missing packages.config on Linux/Windows; changed project/import/glob/context/restore/host and unknown function. Eligible unchanged fixture avoids launch, mutation forces launch and expected changed metadata.
**Regression fence:** tests/cargo_discovery.rs, tests/module_path_integration.rs, tests/discovery_candidates.rs, tests/discovery_failures.rs, tests/discovery_cache.rs, tests/discovery_runtime.rs; relevant Rust golden projection.
Additional review fences: native_metadata_names_are_case_insensitive_without_rewriting_spelling; restored_package_metadata_names_are_case_insensitive; explicit-host precedence and filesystem directory-case public fixtures. Runtime hooks and forwarding muxers change same-path external inputs and require changed native DefineConstants; native host tracing, not a forwarding wrapper or cache flag, proves zero evaluator launches on eligible hits.
**Named mutation:** discard custom Cargo lib path; bypass trust; failed unit→empty confirmed; string-prefix containment; restore grant bypass; omit glob/negative-import watch set or accept unknown recipe. Exact C3/C4/C6/C7/C9 fence red.
**Complexity/production scale:** candidate walk O(paths+solution entries), dedup using keyed sets; 480 projects/802 units. Cache validation O(watched paths+input bytes), key no host work per file. No project×all-files repeated scans: one workspace membership inventory per invocation. Maximum accepted process-tree RSS6GiB/storage10GiB, corpus30min; perunit60s. Cache hit must execute zero evaluations for eligible unchanged requests, an exact deterministic bound.
**Wall budget/phase:** discovery/cache validation always-on per invocation; fit corpus30min total; evaluation misses follow60s/unit. Measured evaluation/cache overhead recorded separately against 11.9s/72-project observation; it is a comparison, not an invented ratio baseline.
**Module shape:** src/discovery/* owns canonical DTOs/coordinator/MSBuild; Cargo stays src/cargo.rs. No facade/indexing integration body until S4. `python3 .tethys-82a6/oracles/module_shape.py --stage S3` → C13 PASS.
**Files:** src/discovery/mod.rs, types.rs, msbuild/{mod.rs,candidates.rs,host.rs,cache.rs,restore.rs}; src/cargo.rs; lib.rs module/reexports; Cargo.toml/Cargo.lock; named discovery/Cargo/runtime tests; tools/tethys-msbuild-evaluate/{Contract.cs,Evaluation.cs} and existing worker oracle/fixture for native package metadata; issue-local discovery/Cargo/mutation oracles; .github/workflows/ci.yml native qualification; atomic docs/fragment.
**Estimate:** one substantial discovery correctness increment.
**Diff estimate:** 9700 lines, revised against the final scoped checkpoint including committed mutation/provenance evidence.
**PR increment:** Neutral discovery.
**Commands and expected results:**
- `cargo nextest run --all-features --lib --test discovery_candidates --test discovery_failures --test discovery_cache --test discovery_runtime --test cargo_discovery --test module_path_integration --run-ignored all` → exact reason/path/metadata and unchanged Rust manifests, positive controls reached.
- `python3 .tethys-82a6/oracles/discovery_smoke.py` → real discovery under no trust/trust/changed/eligible-hit/forced modes matches authored manifests; zero unauthorized execution.
- `python3 .tethys-82a6/oracles/module_shape.py --stage S3` → canonical records and no forbidden owner.
- Native real-worker fences run explicitly with `--run-ignored all` and selected SDK/packaged companion environment; default ignored status is not qualification.
- `python3 .tethys-82a6/oracles/discovery_mutations.py --repo <checkout>` → baseline fences pass; named mutants fail at their behavioral assertions, not compilation/setup; disposable sources are restored and removed.

**Integrated local proof:** artifact357 records all-target/all-feature Clippy with -D warnings,674 native nextest tests (0 skipped),18 doctests, actual C3 Cargo output comparison, native C6/C7/C9 public smoke and C13 S3 PASS. Runtime fences now check changed native metadata before confirming absence of reusable receipts, without pinning internal reason wording; the offline restore fixture explicitly proves the no-grant→grant transition before existing currentness checks. Final gate reruns those refinements.

**Quality and footprint review:** host selection, protocol decoding and process supervision were factored into private responsibilities rather than suppressing Clippy's function-size warnings. Evaluation uses nested typed Results, matching restore and avoiding a boxed success; hashing uses16KiB stack buffers. Final production footprints excluding cfg(test): candidates542, host1,067 and restore1,342 lines. Runtime eligibility/bounded supervision and legacy solution/config currentness explain the amended tripwires; owners and protected parents are unchanged.

**Mutation proof:** `.tethys-82a6/evidence/discovery-mutations-2o13m48p/results.json` records eight successful unmutated controls and seven named mutants caught by eight behavioral assertions against final source hashes. Compiler/setup errors and timeouts are rejected as evidence. The runtime mutants specifically returned stale ONE metadata after the external value became TWO. The disposable source/build tree was removed; parent production files were never mutated.

**Final-review repairs:** S3TransportReview found no concrete transport/cache blocker. S3RestoreReview found generated/internal aliases bypassing exclusions (native public fence red, artifact376) and NuGet project restore missing default solution destination (primary CLI/source evidence). Candidate now privately retains actual solution paths separately from discovery containers; filters preserve referenced solution origin. Restore inspection consumes those paths, honors established context/config precedence and declines ambiguous defaults. SDK restore behavior remains unchanged. Windows proof includes the old destination-less command as a negative control before successful granted discovery.

**Impact evidence:** Fresh self-index indexed163 files/3,526 symbols/30,277 references; both caller tiers found only the same-file inspect caller. Narrow source search recovered all four actual callsites (orchestrator, host unit fence and two restore unit fences). LSP's stale reference positions were reported to the tool issue channel; caller migration uses current source plus the integrated compiler gate.

**F8 placement review:** retaining actual solution context and applicable NuGet configuration adds197 production lines within the existing restore-policy owner after integration simplification. The restore tripwire is amended1200→1400; this is not a new responsibility or wider public seam. Source-only membership documentation now accurately distinguishes evaluated participation from successful syntax indexing. Final integrated Clippy passes and676 native tests pass with zero skipped (artifact392), including the previously red alias fence. Actual Windows NuGet success remains pending.

**Final local checkpoint:** artifact397 records fmt,1,155 nextest passes (34 intentionally ignored native/platform fences),18 doctests (2 ignored), actual frozen Cargo CLI output and real public discovery smoke, and C13 S3 PASS. The separate native run executes676 tests with zero skipped. Final mutation replay passes all seven/eight named mutation/falsifier cases in102.28 seconds. Rust pre-commit checklist reviewed; no warning suppressions or compatibility shims were added. Evaluation/candidate/cache fixture deadlines hold; pinned corpus resource qualification belongs to S6/C12, not an invented local measurement. Final assembled integration is N/A — S4–S6 remain. Native Windows F8 negative/positive control is the mandatory next checkpoint before S4 advances.

## S4: Persist and publish evaluated metadata during index and reindex

**Claim IDs:** C8.
**Expected behavior:** one physical syntax set participates in multiple units; contexts/project moves/failures replace old metadata coherently; index/reindex reevaluates or validates on every invocation; options/grants reach adapter; all bounded evaluation failures give nonzero status without suppressing source-only data.
**Oracle:** authored project/unit/physical-file identities plus independent SQL joins and complete revision manifests.
**Stress fixture:** shared source with different defines, assembly-name collision, moved project, failed outer enumeration, partial inner failure, membership-only change with unchanged source bytes, no trust source-only. Expected one physical source row and exact distinct membership sets; removed unit facts absent, failure evidence explicit.
**Regression fence:** tests/msbuild_discovery.rs::membership_replacement, tests/discovery_cli.rs indexing outcomes, tests/revision_publication.rs integration rerun.
**Named mutation:** participation uniqueness=file_id or retain removed unit snapshot → C8 missing membership/stale unit; positive second physical file protects against collapsed file observation.
**Complexity/production scale:** O(units+memberships+refs+inputs) bulk snapshot insert/replace; 802 units,15,124 physical files; memberships can multiply across units but syntax isn't copied. Maximum6GiB RSS/10GiB database+sidecars; no row-by-row repeated full query. Serialization bounded by snapshot input records.
**Wall budget/phase:** every index/reindex invocation; corpus total30min qualification, perunit60s. No query-triggered work.
**Module shape:** db/discovery owns SQL, discovery owns execution; IndexOptions becomes owned Clone and all callers migrate atomically; ModuleContext immutable context only. Protected deltas indexing final≤1400, lib≤1200, resolve≤1370, reindex≤400. `python3 .tethys-82a6/oracles/module_shape.py --stage S4` → C13 PASS.
**Files:** src/db/discovery.rs, db/schema.rs, db/mod.rs; types.rs, lib.rs, indexing.rs, reindex.rs, resolve.rs, languages/module_resolver.rs; main.rs, cli/index.rs; every IndexOptions consumer found by LSP; tests/msbuild_discovery.rs, discovery_cli.rs, revision_publication.rs and broken contract callers; atomic docs/fragment.
**Estimate:** one persistence/integration increment.
**Diff estimate:** 1600 lines.
**PR increment:** Persisted evaluated discovery.
**Commands and expected results:**
- `cargo nextest run --test msbuild_discovery --test discovery_cli --test revision_publication --test idxperf_golden --test mixed_language_dispatch` → exact memberships/revisions/status, existing Rust projection unchanged.
- `python3 .tethys-82a6/oracles/discovery_smoke.py --index` → real CLI fixture SQL source-only/trusted/changed results agree with manifest.
- `python3 .tethys-82a6/oracles/module_shape.py --stage S4` → neutral parent wiring only.

## S5: Expose evidence-aware per-unit coupling

**Claim IDs:** C10, C11.
**Expected behavior:** pure-C# coupling lists units and declarations; affected Ca/Ce/instability indeterminate for unselected target; independent complete zero metrics retained. Rust-only JSON/human output unchanged. All query commands read persisted snapshots and launch no evaluator.
**Oracle:** handwritten unit/declaration/proven-edge graph with exact known/unknown values and output/exit manifest; independent writer barrier and process marker.
**Stress fixture:** App/net8→Core with two frameworks and no selected unit; Core same AssemblyName as independent project; isolated complete unit; incoming uncertainty; duplicate references; query during publication. Expected no guessed selected edges, AppCe and applicable CoreCa unknown, isolated zero stable. Known-edge control has exact counts.
**Regression fence:** tests/csharp_coupling.rs, tests/discovery_cli.rs::queries_are_read_only, existing tests/architecture.rs and CLI coupling tests.
**Named mutation:** equal-TFM selection/unknown→0; query invokes discovery or composite SQL loses read snapshot. Expected C10 false-known value/C11 launch or mixed revision.
**Complexity/production scale:** one snapshot/adjacency of O(units+declared edges), no all-to-all target expansion; 802 units; sort O(units log units). Query p95≤1s/max2s at pinned corpus qualification; known instability formula retained. Memory O(units+edges) under6GiB whole-process cap.
**Wall budget/phase:** query always-on per invocation; p95≤1s/max2s inherited umjq gate; no external process/evaluation. Architecture rebuild part of index30min cap.
**Module shape:** move architecture records+phase to src/architecture.rs; db/architecture existing SQL owner; CLI renders, no second aggregator. Protected indexing loses architecture body; types shrinks by moved records; lib wiring only. `python3 .tethys-82a6/oracles/module_shape.py --stage S5` → C13 PASS.
**Files:** src/architecture.rs, db/architecture.rs, db/schema.rs, types.rs, lib.rs, indexing.rs, cli/coupling.rs, main.rs; all LSP-located architecture consumers; tests/csharp_coupling.rs, discovery_cli.rs, architecture.rs; atomic docs/fragment.
**Estimate:** one query contract increment.
**Diff estimate:** 1400 lines.
**PR increment:** Evidence-aware coupling.
**Commands and expected results:**
- `cargo nextest run --test csharp_coupling --test discovery_cli --test architecture` → C10 exact known/unknown graph and C11 no-launch/read-snapshot manifest.
- Actual CLI `coupling --json` on isolated mixed-SDK/classic fixture → explicit unknown metric evidence/declarations, no fabricated edge; Rust comparison byte-identical.
- `python3 .tethys-82a6/oracles/module_shape.py --stage S5` → moved phase/records in approved owner only.

## S6: Qualify corpus, legacy and cache-hit performance

**Claim IDs:** C12.
**Expected behavior:** reproducible pinned corpus/classic outputs and measurements satisfy all inherited gates; wrong SHA, missing proof, drift or threshold breach fails qualification, not just warns.
**Oracle:** OS process-tree peak RSS, monotonic wall-clock/file sizes, immutable four-repo SHA/expected-standing manifests and arithmetic threshold checks; direct host evaluation and exact fixture manifests remain fact authority.
**Stress fixture:** 18 TFMs, 480 projects/802 units, largest-repo profile, two fresh batch/stream runs, changed metadata and genuine eligible cache hits/forced evaluation; at/above deadline/RSS/storage controls. Expected exact repeat canonical answers, ≤60s/unit, ≤30min corpus nightly and ≤6GiB/10GiB caps; documented14-run baseline calibration before ratios used.
**Regression fence:** issue-local oracles/qualification.py and deterministic threshold-boundary fixture; timeout fence retained in tests/discovery_failures.rs.
**Named mutation:** replace qualification cap rejection with unconditional acceptance → C12 above-cap control accepted; remove worker deadline → controlled silent-worker timeout fails.
**Complexity/production scale:** measurement collection O(process samples+output bytes), deterministic evaluator O(records), no quadratic joins across corpus outputs. Maximum sample storage bounded by explicit runner interval/deadline; qualification budget30min nightly/60min release; all workload counts recorded.
**Wall budget/phase:** one-off qualification, runtime evaluation/query budgets inherited above; measure rather than automatically rebaseline.
**Module shape:** nonproduction runner/corpus manifests only, no new product owner. `python3 .tethys-82a6/oracles/module_shape.py --stage S6` → C13 PASS for assembled tree.
**Files:** .tethys-82a6/oracles/{qualification.py,worker_qualification.py,discovery_smoke.py,module_shape.py,ledger.json}; tests/fixtures/msbuild corpus/expectation manifests; existing workflow qualification wiring; measurement records; approved atomic docs/fragment as relevant.
**Estimate:** full host/corpus qualification increment.
**Diff estimate:** 800 lines.
**PR increment:** Corpus qualification.
**Commands and expected results:**
- `python3 .tethys-82a6/oracles/qualification.py --fixtures --corpus --legacy --cache --repeat 2` → exact manifests/SHAs/standing/determinism and measured threshold PASS; every run reports evaluation/reindex/total seconds, peakRSS/index sidecar sizes.
- `python3 .tethys-82a6/oracles/qualification.py --check-boundaries` → limits accepted exactly at cap; above-cap fixtures rejected with C12.
- `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo nextest run && cargo test --doc` → assembled repository gates green once workers finish.
- Independent fresh reviewer reconstructs production owners/interfaces before receiving design; final C13 conformance comparison has no unresolved mismatch.

## Tracker taxonomy and self-review

C0–C14 each have exactly one owning slice; all pending falsifiers are assigned, with regression fence and mutation created alongside behavior. Dependent slices rerun prior fences without claiming duplicate ownership. Every slice lists14 fields, production-scale costs and budget rationale. No provisional implementation is an accepted final deliverable. Adjacent intended semantic/target/profile/aggregation work remains at verified yjok/e2jx/cmlc/rduz/dsh1 as classified in design; all82a6 acceptance remains here. All touched production paths are covered by the ledger; new paths require explicit placement, not opportunistic additions. No slice is marked complete by this planning artifact.
