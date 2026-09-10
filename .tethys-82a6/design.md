# Design: Evaluation-only discovery and coherent publication

Status: proposed for architecture approval; production implementation has not started.

## Route and inputs

Route: **Structural**, from `route.md`. Behavior source: every Given/When/Then entry in `spec.md`: unchanged Cargo outcomes; non-executing candidates without trust; per-framework evaluated metadata; coherent successful/partial revisions and fatal rollback; schema refusal and evaluation-free queries; corpus/cache performance evidence; per-unit coupling with unknown target selection exposed, not guessed. Its edge checklist is incorporated. Requester approved the coupling clarification on 2026-09-06, verbatim: “yes, I approve”. This is not approval of this architecture.

Empirical-stage artifacts: **N/A — Structural route**. Existing authority: approved tethys-chlt, tethys-rvr5 and tethys-teax; September 6 binding-model review §4 G3/G4/G7. Evaluation-only metadata does not imply compiler references or semantic bindings. Windows Roslyn BuildHost results are not evaluation-only qualification.

Qualification source: tethys-umjq. Exact bounded manifests; separate authorities for MSBuild metadata, filesystem membership and tethys-specific coupling; two fresh batch/streaming repetitions; all four pinned repositories; classic Windows success; 18-TFM and 480-project/802-unit stress shapes. Per-unit product deadline is at most 60 seconds. PR semantic qualification cap is 10 minutes; corpus nightly/release caps are 30/60 minutes. RSS maximum is 6 GiB on an 8 GiB runner, index plus sidecars maximum 10 GiB. Calibrated ratios (2× indexing time, 1.5× RSS/storage) apply only to reviewed immutable baselines established by the approved 14 observations, not invented measurements. Record separate evaluation, reindex and total seconds plus peak RSS, including cache hits and forced reevaluation.

### Selected architecture in brief

1. A concrete discovery coordinator calls two real adapters, Cargo and MSBuild, through a small neutral discovery interface. It returns owned immutable metadata and evidence; it never writes the index.
2. MSBuild runs in a packaged, first-party **evaluation-only companion**, using `ProjectCollection.LoadProject` and evaluated-project APIs. It does not use Roslyn, BuildHost, BuildManager, `Project.Build`, or target execution. This avoids both CLI-version dependence and treating `MSBuildAllProjects` as a complete import list.
3. One SQLite `BEGIN IMMEDIATE` transaction covers the entire index revision. Existing file/dependency/architecture transactions become savepoints. The streaming writer borrows the same Index through a scoped thread; it never opens another SQLite writer. No full-index staging copy, file swap, or revision predicate in every query.
4. Discovery/project/unit/membership/failure/cache records and syntax/graph/architecture facts commit together. Fatal infrastructure failure rolls back; bounded evaluation failure publishes explicit partial coverage.
5. Coupling uses evaluation-unit identity and explicit evidence for selected edges. Declarations without selected-unit proof are reported separately; affected metrics remain indeterminate. Rust output keeps its existing spelling, ordering and values.

## Input shapes

Each family includes empty, singleton, distinct-multiple and duplicate collections where applicable. Optional fields cover both presence branches and their reachable combinations, not only one fully populated record.

| Shape | Production-reachable variants / boundaries | Coverage |
|---|---|---|
| S1 Candidates | No Cargo/project; Cargo root/virtual/nested workspace; standalone csproj; sln/slnx/slnf; duplicate entries; filter subset; missing/invalid referenced solution; mixed SDK/classic and referenced non-C# project metadata | C3, C4 |
| S2 Text and paths | Empty invalid path, ASCII, Unicode, spaces, XML escapes, relative/absolute, Windows separators/drives/UNC, case-sensitive/insensitive identity, canonical symlink aliases, missing path, directory instead of file, unreadable path, linked/imported source escaping root | C4, C7 |
| S3 Framework identity | SDK single/multiple/18 TFMs; classic empty SDK shorthand with identifier/version/profile/platform; v4.0 Client/v4.7.2/v4.8; duplicate framework aliases; failed outer enumeration; one failed inner framework; no fabricated unit for unknown identity | C5, C6 |
| S4 Metadata | Absent/present/empty Link, DefineConstants, LangVersion, AssemblyName, output metadata, reference HintPath and metadata; imported/linked/globbed/removed/conditioned Compile; same file in zero/one/multiple units; same assembly name in distinct projects | C5, C8 |
| S5 Authority | Evaluation trust absent/present × restore grant absent/present; no restore inputs/current inputs/stale/missing assets; packages.config prepopulated/missing; Windows/non-Windows; offline/auth/restore failure; caller property overrides versus defaults versus recorded import profile | C4, C6, C7 |
| S6 Hosts | Selected/discovered SDK or VS MSBuild; present/missing companion or runtime; supported/unsupported protocol; unresolved SDK/workload/global.json/roll-forward policy; supported classic host versus unavailable compatible host; custom resolver; timeout/exit/broken pipe/oversized or malformed output | C5, C6, C14 |
| S7 Evidence sum | Confirmed; trust-required; restore-required; restore-failed; restore-unsupported-on-host; toolchain-unavailable; sdk-unresolved; malformed-input; evaluation-failed; timeout; outside-workspace-input; partial-target-frameworks | C6 |
| S8 Cache | Disabled/ineligible/miss/hit; project/import mtime change; same-path changed content; missing/new conditional import; glob addition/deletion/rename; context/global/env/host/SDK/restore/profile change; unknown external I/O; changed inputs during evaluation; corrupted/stale cache entry | C9 |
| S9 Publication | Fresh/current/outdated schema; first index/reindex/rebuild; zero/multiple units; bounded failures; failure after file writes/before architecture/during commit; killed writer; two writers; old pinned reader/new reader; source-only parse failure | C0, C1, C2, C8 |
| S10 Coupling | Isolated unit; declaration with proven target/without selected target; multi-TFM destination; duplicate declarations; ReferenceOutputAssembly metadata; failed source/destination; incoming/outgoing uncertainty; complete independent subgraph; mixed Cargo and units; name collisions; sort and detail lookup | C10, C11 |
| S11 Counts and resources | Zero counts, identifier overflow rejected, negative/zero timeout rejected, supported deadline boundary 60 seconds, invalid batch size rejected, 480 projects/802 units, 18 TFMs, bounded stdout/stderr, RSS/storage at/above caps | C6, C12 |
| S12 Structure and distribution | Batch and scoped streaming, library and CLI, package install/source build/release bundle, fresh install without checkout paths, Linux/macOS/Windows, all optional trust/settings combinations | C3, C11, C13, C14 |

N/A: query-selectable historical contexts (permanent non-goal, one active context); arbitrary workspace escape indexing (permanent prohibition); compiler-selected bindings, generator output and target-generated references (tethys-yjok/e2jx, not discovery).

## Removed invariants

This is partly subtractive: remove the one-package-per-physical-file assumption for evaluated C# participation, construction-only discovery freshness, and independent commit points. The following previously impossible states become possible:

- One file participates in two units: identity and counts must not duplicate physical syntax (C8); existing Rust one-crate attribution remains unchanged (C3).
- A project exists without a known framework, or a known unit fails: absence is not empty success and old facts cannot fill the gap (C6/C8).
- An indexing writer spans file/dependency/architecture phases: the old second streaming connection would deadlock and early commits would leak mixed facts (C1).
- A coupling node has unselected outgoing references: numeric zero cannot encode unknown (C10).
- A query overlaps publication: one composite query must pin a SQLite read snapshot; separate invocations may legitimately observe different revisions (C1/C11).

Readers/mutators identified: `index_parsed_file_atomic`, `delete_files`, `apply_resolutions`, `populate_file_deps_from_call_edges`, `repopulate_architecture`; indexing's direct clear/insert calls; BatchWriter's separate Index open; Tethys coupling wrappers and both JSON writers. Current `rebuild`/CLI `--rebuild` delete the index before indexing: that must be removed, not wrapped in a transaction after deletion (C2). The original assumption that all pre-existing source-only facts used physical-file identity was falsified during S4 (F23): Rust retains logical symlink identities. The approved F23 amendment below preserves C3 while enforcing C# physical identity.

## Placement

### Discovery interface and configuration

`WorkspaceDiscovery::discover(&DiscoveryRequest) -> Result<DiscoverySnapshot>` is the adapter interface. Two shipping adapters implement it: `CargoDiscovery` reuses `src/cargo.rs`; `MsBuildDiscovery` owns candidates, host selection, evaluation, restore policy and reusable evaluation evidence. A concrete coordinator merges their outputs deterministically. It does not expose host command lines through the neutral interface.

`DiscoveryRequest` carries canonical workspace, requested context, explicit grants, timeout and cache policy. `DiscoverySnapshot` carries Cargo attribution unchanged plus project/evaluation records and source membership, with typed bounded failures distinct from fatal I/O/protocol/storage errors. Keys are newtypes; a project key is the normalized workspace-relative path; a unit key includes evaluated framework identifier/version/profile/platform and context identity, not AssemblyName or a required SDK shorthand. Requested settings and per-unit effective settings are both retained. Environment secrets are never persisted verbatim: provenance stores names and a keyed/opaque comparison digest, not credential values.

Extend existing `IndexOptions` with owned discovery options and migrate its consumers; it becomes `Clone`, not `Copy`. Do not introduce a second competing indexing options convention or shim. CLI flags live on `index`: explicit `--trust-msbuild`, `--allow-restore`, context selectors/global properties, host override, and a recorded import-tolerance profile selector. A selected profile sets blank VSToolsPath; explicit caller global properties win and are reported. Neither grants nor host installation happen implicitly. Query commands do not expose execution grants.

`ModuleContext` receives immutable precomputed discovery context. All resolvers remain database-free and cannot launch processes. Membership does not upgrade existing syntax bindings into compiler-confirmed semantic facts; e2jx owns that transition.

### Managed evaluation companion

`tools/tethys-msbuild-evaluate/` contains a small console worker and versioned JSON protocol. One worker process evaluates one project/framework request; an outer request enumerates target frameworks when needed, never standing in for inner evaluation. It registers the explicitly selected MSBuild installation before loading Microsoft.Build types. SDK-host and Windows Framework-host builds are packaged; the adapter checks protocol, actual host/runtime/toolset and policy before accepting results. Minimum qualified host bands are pinned in the build/fixture manifest, not guessed from an executable name. Missing/incompatible host is typed evidence with an installation/selection remedy, not silent substitution.

Use `ProjectCollection.LoadProject`, `Project.Items`, evaluated properties, `Project.Imports` and glob/logical-project metadata. Preserve native exception type/code/path/span (`InvalidProjectFileException.ErrorCode`) separately from human detail. Do not invoke targets to obtain missing compiler references. `Project.Imports` excludes false-condition imports; cache eligibility must account for those conditions and candidate files separately. No blanket missing-import ignore mode. The blank VSToolsPath policy is a recorded evaluation context, not a claim of equivalence to imported VS targets.

The Rust MSBuild adapter owns bounded process lifecycle and authority. Drain stdout/stderr while enforcing deadline and protocol bounds; cancellation terminates/reaps the worker/process group using a safe platform implementation (crate-wide unsafe remains forbidden). No unbounded blocking read waiting for EOF. Restore is a distinct, separately granted external operation using the repository's normal NuGet policy; never add feeds/credentials/lock behavior. Non-Windows packages.config restore is never attempted. Targeting-pack injection and FrameworkPathOverride are outside this worker (cmlc/yjok).

Package the worker with release assets and provide an explicit companion-path override for source/Cargo installations. Do not build, restore or download a helper on an index invocation. Release CI checks managed dependency locks/licenses against the repository's allowed-license policy as well as Rust dependencies. The worker is not the Roslyn semantic tool and has no target-execution mode.

### Cache contract

Cache reuse is **eligibility-gated**, not universal. A persisted successful evaluation includes input provenance, import closure, glob/conditional-import watch sets, restore assets, context/profile and selected host/toolchain identity. Reuse only when a qualified evaluation recipe can account for all evaluation-time reads; unknown custom SDK resolvers, unaudited property functions/external I/O or an incomplete closure force reevaluation. Record that bypass reason.

Key project/import mtimes plus content identity, source-membership inventory, negative dependencies, requested/effective context, relevant environment digest, host/toolchain/SDK resolver identity, restore state and profile. Audited toolchain recipes are tied to exact toolchain/import content and have mutation fixtures; a version change invalidates the recipe instead of expanding trust. Validate the input set before and after evaluation; changing inputs cannot publish Confirmed reused metadata. Failures do not become reusable successes. Source mtimes still drive source parsing separately. Measure a genuine eligible hit, ineligible bypass, changed-input run and forced reevaluation on the corpus/legacy fixtures; zero eligible corpus projects is not proof of the required cache-hit path.

### Persistence and publication

`src/db/revision.rs` owns an RAII revision session over the existing Index connection. `BEGIN IMMEDIATE` serializes indexing writers. Do not hold the connection MutexGuard across worker execution: begin/finish use short locks; all writes use the same connection. The Tethys indexing entry remains exclusive (`&mut self`); the worker is scoped and joined before commit or rollback. Storage/channel/architecture failure aborts the session. Source parsing's bounded errors retain their documented diagnostics, but affected stale facts cannot be retained as current facts. Typed evaluation failures are intentional revision contents, not transaction errors.

The five existing write-transaction sites become savepoints, which also work atomically when used outside a revision. The run owner decides commit exactly once, after parser writes, both resolution passes, call/file edges, architecture, discovery rows and revision metadata succeed. Join failures and disconnection propagate rather than logging-and-continuing. All exceptional exits roll back; rollback failure is logged and invalidates the connection for further writes. Ordinary independent queries use WAL; multi-statement composite reads pin a read transaction on their own connection.

Schema remains one DDL source in `src/db/schema.rs`, adding:

- `index_revision`: one published revision identity and schema version; no historical query interface.
- `evaluation_context`: requested properties and non-secret host/restore/profile provenance.
- `projects`: path identity plus candidate/enumeration evidence, including failure before a framework is known.
- `evaluation_units`: project, full evaluated framework identity, effective properties/assembly metadata and success/failure evidence.
- `file_participation`: physical file × evaluation unit and evaluated source/Link metadata; no uniqueness on file alone.
- `declared_project_references` and `declared_assembly_references`: declaration metadata distinct from optional proven target selection.
- `evaluation_inputs`/`evaluation_cache`: validated input evidence and successful reusable payload tied to context/toolchain.
- bounded run/source diagnostics necessary to distinguish absent facts from failed processing in this revision.

All rows describe the same committed active revision; no old-unit fallback. Existing syntax tables remain source-only and are not copied per membership. Foreign-key and unique constraints enforce identity. C# architecture nodes derive from units; the existing Cargo mapping remains specific to Cargo rather than attributing nested C# files to Rust packages.

Normal opening checks schema currency before applying DDL and refuses old indexes with `tethys index --rebuild`. Explicit rebuild uses a raw SQLite connection under the same revision session: drop/recreate schema inside the transaction, index, then commit. Remove the pre-delete/reset-and-reopen path from rebuild callers. A failure rolls back even DDL, leaving the old schema/data intact. An unreadable/non-SQLite file produces an actionable error, never an automatic destructive repair. Explicit standalone deletion, if still independently used, is not the rebuild mechanism.

### Coupling interface

Move architecture-specific public records beside their owner in `src/architecture.rs`, keeping the canonical public re-export through lib.rs; migrate all internal imports without aliases or duplicate records. Extend the existing coupling result contract with typed metric evidence and declared references rather than adding a second coupling implementation. Public Rust consumers must handle indeterminate metrics explicitly; the CLI retains the existing Cargo-only JSON/human shape and numbers, while evaluated-unit rows carry identity, standing/reasons, nullable unavailable values and separate declarations. Unknowns sort deterministically after known numeric values; detail lookup uses unambiguous unit identity rather than AssemblyName.

For an unresolved destination selection, Ce of the source is indeterminate; candidate target-unit Ca is indeterminate wherever the declaration could contribute. Instability is indeterminate if either required count is indeterminate. No selected edge arises merely because target TFMs match, only one indexed name exists, or a project has multiple units. Isolated evidence-complete units can retain known zero metrics. Use the existing Rust-owned instability formula for known counts. A declaration is not a compiler-resolved call dependency.

Owner: `src/architecture.rs` assembles neutral architecture inputs/report and owns evidence propagation; `src/db/architecture.rs` persists/queries them; the discovery adapters own language-specific attribution. Indexing only invokes the architecture interface. No MSBuild/project-file knowledge enters indexing/resolve/batch_writer.

## Module shape

### Current cluster inventory

The baseline census is recorded below; counts are physical source footprint signals, not executable LOC or architecture quality. Inline test-module ranges are excluded; any separately gated helper/re-export conservatively included is identified. Tests/fixtures are separate from production.

| Existing path | Baseline footprint | Current interface, dependencies, callers/test surface | Decision |
|---|---:|---|---|
| src/lib.rs | 1142 | Tethys constructor and query facade; owns Index/Cargo state; called by CLI and integration tests | retain protected facade |
| src/indexing.rs | 1312 | Exclusive index orchestration, physical discovery, parse/dependency passes and inline architecture; calls Cargo/Index/resolver/writer; golden/streaming tests | deepen orchestration; move architecture ownership |
| src/reindex.rs | 372 | Source mtime/size staleness, update/full index and destructive rebuild; staleness tests | deepen revision entry wiring |
| src/batch_writer.rs | 266 | Background second-connection writer; channel/send/finish, error stats; streaming fixtures | deepen existing actor to scoped shared-Index lifetime |
| src/cargo.rs | 426 | Crate discovery/module attribution; filesystem/Cargo manifest; cargo_discovery/module_path_integration | retain as real adapter implementation |
| src/languages/module_resolver.rs | 412 | DB-free ModuleResolver/ModuleContext; Cargo/filesystem context; seam_lint/mixed dispatch | deepen context only |
| src/resolve.rs | 1337 | Neutral DB candidate lookup and resolver delegation; pass2 fixtures | retain protected driver |
| src/unused_imports.rs | 350 | Rust unused-import query already constructs ModuleContext and selects source files; current351 in S4 census | retain Rust query algorithm; migrate context and canonical source-selection callers |
| src/db/mod.rs | 358 | Index connection Mutex, schema open/reset; gated reexports excluded (spans 1–58, 69–368); all DB callers | move revision lifecycle to concrete DB owner |
| src/db/files.rs | 572 | Atomic per-file mutations/delete; gated helper excluded (spans 1–57, 80–594); batch+stream callers | retain owner; nested savepoints/shared borrowing |
| src/db/references.rs | 266 | Apply reference resolutions atomically; includes gated test helper; resolve caller | retain; nested savepoint |
| src/db/call_edges.rs | 349 | Call-edge/reference corroboration and derived file deps; indexing caller | retain; nested savepoint only |
| src/db/schema.rs | 213 | Single authoritative DDL; Index open and schema tests | deepen schema |
| src/db/architecture.rs | 437 | Atomic Cargo architecture replacement and multi-read query details; production spans 1–258 and 313–491; architecture integration tests | deepen existing persistence/query owner |
| src/types.rs | 1737 | Shared records/options plus architecture records after test module; spans 1–1563 and 2572–2745; all public callers | move architecture cluster; options discovery field only |
| src/main.rs | 364 | Clap commands and dispatch; CLI tests | protected declarations/dispatch |
| src/cli/index.rs | 189 | Flags→Tethys, output/exit, destructive preclear; CLI tests | deepen presentation, remove preclear |
| src/cli/coupling.rs | 358 | Sort/detail/table/JSON rendering; CLI tests | deepen evidence rendering only |
| src/cli/mod.rs | 121 | CLI helpers/availability; existing LSP paths | retain; no evaluation executor |
| src/error.rs | 211 | Typed top-level errors, result aliases | retain or add narrowly typed fatal discovery error; no error-message parsing |
| Cargo.toml | 85 | Dependency/features/lints and build metadata | dependency declarations only |
| .github/workflows/release.yml | 150 | Rust binary build/distribution | add companion packaging/smoke/license wiring |
| .github/workflows/ci.yml | 311 | Rust quality/CI gates | add focused managed/evaluation qualification wiring |

### Alternatives and selection

**A — selected: deep discovery adapters + live revision session.** Caller: `snapshot = discovery.discover(request); revision.run(indexing_work)` with actual discovery performed within the serialized invocation and validated before publication. Small caller interface hides candidates/hosts/failure/cache; DB revision owns transaction lifetime. Cargo/MSBuild are real adapters; writer mediation is an intrinsic connection/lifetime seam, not a fake backend trait. Tests invoke the same discovery/index interfaces. Highest locality for authority and zero full-index copy; tradeoff is a longer SQLite writer lock and mandatory savepoint/scoped-thread cutover.

**B — default-caller optimized monolithic coordinator with private staging Index.** Caller: `workspace.refresh(options)` owns discovery, temporary SQLite snapshot, full existing index pipeline and publication. Existing BatchWriter can keep an independent connection against staging. Hidden complexity includes backup/swap and query connection pinning. Fewer changes to per-file transaction code but full-index copy/RSS/storage and Windows/open-reader publication handling duplicate persistence responsibilities. Rejected: caller convenience does not justify avoidable copies or a multi-responsibility coordinator.

**C — extension-first revision-keyed repository interfaces.** Caller: `session = store.begin_revision(); provider.populate(session); store.activate(session.id)`. Every table/query is revision-keyed and providers write through a generic store interface. Easy multiple backends/history but spreads ownership and revision knowledge across all SQL/query callers; no second store adapter exists. Rejected: hypothetical storage seam, largest query cutover, and historical contexts are not required.

**Evaluator alternatives:** (A) selected managed evaluation worker, one metadata/import contract across SDK/VS hosts, with packaging cost; (B) modern CLI `-getItem/-getProperty`, easy common path but cannot claim complete import/read closure and excludes older query-less hosts; (C) Roslyn MSBuildWorkspace/BuildHost, already solves semantic acquisition but runs targets and violates this grant. The worker's installation/host-loading correctness is a mandatory C14 gate, not an assumed existing capability.

**Coupling alternatives:** (A) selected extend the existing report owner with typed unknown metrics/declarations; (B) separate C# coupling command duplicates aggregation; (C) project-collapsed metrics conceal unit uncertainty. The requester explicitly approved A's behavior.

### Proposed module ledger (approval requested)

| Module/path | Interface | Owns | Hides/reuses | Must not own | Adapters | Tests through | Change |
|---|---|---|---|---|---|---|---|
| src/discovery/mod.rs | discover(request)→snapshot; fatal versus bounded outcomes | neutral invocation and adapter coordination | Cargo/MSBuild implementations | SQL, parser/graph algorithms | Cargo, MSBuild | discovery entry | create |
| src/discovery/types.rs | immutable request/snapshot/evidence records | identity/context/source-membership contracts | owned maps/newtypes | execution/DB | N/A — records | discovery serialization/consumers | create |
| src/cargo.rs | existing discovery plus neutral adapter implementation | Cargo discovery/attribution | existing CrateInfo algorithms | MSBuild/fake TFMs | Cargo | existing Cargo integration | deepen without algorithm change |
| src/discovery/msbuild/mod.rs | adapter discover implementation | evaluation workflow/authority | candidates/host/cache/restore | SQL/semantic binding | MSBuild | neutral discovery entry | create |
| src/discovery/msbuild/candidates.rs | parse candidate set | sln/slnx/slnf/project containment/enumeration | safe XML/JSON parsers | project execution | N/A — concrete private owner | parse-only discovery | create |
| src/discovery/msbuild/host.rs | evaluate(request)→bounded response | selected host, protocol, process lifetime | std process + safe cancellation support | policy-grant invention/targets | SDK/VS worker hosts | discovery host integration | create |
| src/discovery/msbuild/cache.rs | reusable(input evidence)→decision | eligible recipe/key/watch validation | evaluated closure and filesystem | universal arbitrary-code purity claim | N/A — concrete cache | forced-evaluation comparison | create |
| src/discovery/msbuild/restore.rs | restore(grant, host, project)→evidence | separately authorized normal restore | host's NuGet mechanism | feeds/credentials/lock changes | supported repository restore hosts | restore fixtures | create |
| tools/tethys-msbuild-evaluate/Program.cs | versioned request/result process | handshake/host registration/dispatch | Evaluation/Contract | evaluation algorithm/targets | N/A — protected entry | launched worker | create |
| tools/tethys-msbuild-evaluate/Evaluation.cs | evaluate project/framework request | evaluated metadata/native diagnostics | Microsoft.Build.Evaluation APIs | BuildManager/Project.Build/Roslyn | selected MSBuild installation | real worker vs CLI oracle | create |
| tools/tethys-msbuild-evaluate/Contract.cs | versioned DTOs | managed wire fields | JSON serialization | host/evaluation policy | N/A — data | worker handshake/records | create |
| tools/tethys-msbuild-evaluate/Tethys.MSBuild.Evaluate.csproj | SDK/Framework worker builds | pinned dependencies and runtime packaging | installed MSBuild via loader | project builds at indexing time | SDK/VS distribution variants | clean-install smoke | create |
| src/db/revision.rs | begin/commit/rollback/open-for-rebuild | whole-run transaction and schema lifecycle | Index connection/savepoints | discovery or file-specific algorithms | N/A — intrinsic serialized writer seam | full index/failure tests | create |
| src/db/discovery.rs | replace/load snapshot | normalized snapshot/cache persistence | schema/Index | process launches/metadata guessing | N/A — SQLite only | persisted discovery query | create |
| src/db/schema.rs | SCHEMA and currency contract | all DDL/version/FK constraints | existing schema | execution | N/A — SQLite | old/fresh/rebuild fixtures | deepen |
| src/db/files.rs; src/db/references.rs; src/db/call_edges.rs | existing writes | current file/ref/call-dependency responsibilities | nested savepoints | run publication/MSBuild | N/A — existing concrete owners | current+revision integration | deepen transaction nesting only |
| src/db/architecture.rs | replace/query architecture | SQL storage/read snapshot | existing Ca/Ce aggregation | host/execution | N/A — SQLite | coupling integration | deepen |
| src/architecture.rs | architecture inputs→report | architecture-specific records, evidence propagation, phase assembly | DB query and existing instability formula | project parsing/host policy | neutral discovery inputs | coupling report | create; move existing cluster |
| src/batch_writer.rs | scoped writer send/finish | channel/lifetime/write batching | borrowed Index | opening DB, commit owner, discovery | N/A — intrinsic actor seam | streaming index | deepen |
| src/lib.rs; src/indexing.rs; src/reindex.rs; src/resolve.rs | existing facade/drivers | existing neutral orchestration | new deep owners | new MSBuild/schema/cache bodies | existing language resolvers | library/CLI | retain wiring; move architecture body |
| src/languages/module_resolver.rs | ModuleContext/ModuleResolver, private SourcePathIdentity policy | DB-free precomputed language context and language-path identity policy | snapshot/CrateInfo, existing Rust/CSharp static registry | SQL/processes | Rust/CSharp existing implementations | seam+dispatch and public file/freshness/dependency queries | deepen context and approved F23 identity policy |
| src/unused_imports.rs | existing unused-import query | existing Rust query analysis | precomputed ModuleContext and shared language-aware source selection | discovery execution, new language policy | existing Rust resolver | unused-import query fixtures | retain; caller migration only |
| src/types.rs | shared records | existing shared domain, IndexOptions | discovery/architecture-owned records | new architecture implementation | N/A — data | consumers | move architecture declarations; options cutover |
| src/main.rs; src/cli/index.rs; src/cli/coupling.rs; src/cli/mod.rs | command parse/render/exit | CLI presentation | library interfaces | direct host invocation for discovery | N/A — CLI | actual CLI smoke | deepen presentation, protected main |
| src/error.rs | Error/Result/IndexError | fatal and bounded source-error representation | existing categories and serde | unit standing policy | N/A — error type | consumer/storage error handling | representation extension only |
| Cargo.toml; managed lock/runtime files; .github/workflows/ci.yml; .github/workflows/release.yml | build/package configuration | dependencies and qualification/distribution | existing Rust gates | runtime policy | OS release packages | clean-install qualification | deepen |

Every new source has baseline zero. Required test owners: existing Cargo/golden/architecture suites plus tests/discovery_candidates.rs, tests/msbuild_discovery.rs, tests/discovery_failures.rs, tests/discovery_cache.rs, tests/revision_publication.rs, tests/csharp_coupling.rs and tests/discovery_cli.rs. The issue-local structural oracle is not a production behavioral test.

### Protected parents and seam tests

| Protected parent | Baseline responsibilities | Allowed change | Forbidden change | Exit condition |
|---|---|---|---|---|
| src/lib.rs | facade/reexports/state | typed delegation, state/options/reexports | evaluation/cache/SQL/aggregation bodies | only named facade wiring additions |
| src/indexing.rs | physical parse/index orchestration | discovery/revision interface calls; scoped writer wiring; remove run_architecture_phase body | project XML, host commands, target/framework logic | no language-specific new branch and moved architecture body absent |
| src/resolve.rs | neutral reference resolution | immutable context plumbing | process/DB access in resolvers, MSBuild rules | resolver interface only for language decisions |
| src/reindex.rs | source freshness/update entry | invoke complete revision path; remove reset-before-run | MSBuild-specific invalidation logic | metadata invalidation delegated to discovery |
| src/batch_writer.rs | existing generic write actor | shared-Index scoped lifetime/error propagation | discovery/context parsing, Index::open, COMMIT ownership | no independent connection, joined before revision ends |
| src/main.rs | CLI declaration/dispatch | flags and typed dispatch | discovery or aggregation bodies | parse/dispatch only |
| tools/tethys-msbuild-evaluate/Program.cs | new entry | host registration/handshake/delegation | evaluation loops/target calls | metadata evaluation only in Evaluation.cs |

Deletion test: removing discovery spreads host/grant/cache rules into CLI/index/reindex; removing revision spreads commit/error/lifetime rules into every phase; removing architecture spreads unknown-metric propagation into CLI/DB. All earn depth. Interface test: production callers and fixtures use discover/index/coupling, not private maps. Adapter test: discovery has two real adapters; SQLite revision is a concrete intrinsic lifetime owner, not a generic store. Locality test: one owner per ledger responsibility; records have a canonical definition and one lib re-export.

Mechanical fence C13: `python3 .tethys-82a6/oracles/module_shape.py --base <discovered-upstream-ref>` checks an issue-local ledger manifest: exact production path allowlist, declared imports/visibility, forbidden process/MSBuild ownership, absence of second writer connection, protected-parent added-symbol allowlists, and production growth tripwires. Baseline/upstream is discovered from Git, not hard-coded. Planned new parent-body additions beyond wiring are errors; numeric growth within an unchanged responsibility triggers inspection, not arbitrary splitting. The parser-based census includes production after inline test modules. Each diagnostic names C13/path/symbol/delta. No production code reads this oracle.

Projected cumulative change is above 4,000 lines: approximately 5,500–8,000 including implementation, managed companion, fixtures and docs, plus 25% churn margin (6,875–10,000). This is a review-size signal, not an implementation quota. The approved-design planning stage must produce independently green/mergeable increments; internal-only increments cannot claim the issue complete. Exact slice growth and order belong to plan.md after approval.

## Claims

- C0: SQLite released savepoints remain unpublished until their enclosing transaction commits.
- C1: An index invocation publishes exactly one coherent revision or leaves the previous revision intact.
- C2: Ordinary old-schema opening is non-mutating and explicit rebuild rolls back its schema replacement on failure.
- C3: The Cargo adapter preserves existing Rust discovery, attribution and canonical outcomes.
- C4: Candidate enumeration executes no project code and missing trust cannot launch an evaluator.
- C5: Evaluation returns the exact supported SDK/classic per-unit metadata under the recorded host/context without targets.
- C6: Every incomplete evaluation preserves its typed/native evidence and never becomes empty success.
- C7: Restore and workspace containment cannot be bypassed by evaluation or linked inputs.
- C8: One physical syntax record can participate in multiple units without mixed-revision or duplicate semantic ownership.
- C9: Cache hits preserve the forced-evaluation answer and unknown dependency closures force reevaluation.
- C10: Unproved target selection cannot produce known dependent coupling metrics or fabricated unit edges.
- C11: Query commands read one persisted evidence snapshot and never evaluate projects.
- C12: Qualification reports and enforces the approved deadline, determinism, time, RSS and storage gates.
- C13: Production responsibility placement and dependency directions match the module ledger.
- C14: Packaged evaluators run against the recorded supported host from a clean installation without checkout paths or implicit builds.

## Falsification

Status `PENDING` names Main/checkpointed-build as discharge owner; plan.md must assign each to the slice implementing it. All commands/tests below other than C0 are proposed and not yet executed. Every absence assertion has a positive control. Expected and mutated outcomes identify the claim, rather than accepting any generic error.

| # / Claim (defined verbatim above) | Input shape | Falsifier and confounder control | Independent oracle | Named mutation → expected red | Regression fence | Cost | Status |
|---|---|---|---|---|---|---|---|
| C0 | S9 savepoint/reader/writer | Run revision_transaction.py; verify old rows during writes/rollback, new rows after commit, pinned reader old. Positive contender after release distinguishes writer exclusion from broken connection. | Separate SQLite observer/reopened connection and literal three-table revision manifest | Existing oracle --mutate-early-commit commits after files → C0 mixed revision assertion | .tethys-82a6/oracles/revision_transaction.py | local, <1 second observed | PASS — 2026-09-06; see log |
| C1 | S9; writer lifetime | Pause real batch/stream index after file writes; independent reader sees old exact file/ref/arch/unit manifest; kill/fail writer and compare old; successful control publishes new. Ensure changed source actually reaches writer. | Separate-process SQL snapshot + authored old/new manifests, not Tethys query self-comparison alone | src/db/revision.rs commit before architecture/snapshot persistence → C1 mixed rows; src/batch_writer.rs reopen path → C1 streaming timeout/rollback | tests/revision_publication.rs::atomic_batch_and_streaming | local SQLite/integration | PENDING — Main/checkpointed-build |
| C2 | S9 old/current/rebuild | Open saved old schema and compare schema/data before/after refusal; fail rebuild after DDL then reopen old raw SQLite; successful rebuild control opens new. Failure injection must reach after DDL. | sqlite_master/user_version + literal schema/data fixture | src/cli/index.rs remove DB before rebuild or db/revision.rs apply DDL before BEGIN → C2 old revision lost | tests/revision_publication.rs::schema_refusal_and_rebuild_rollback | local SQLite/integration | PENDING — Main/checkpointed-build |
| C3 | S1/S12 Cargo | Compare Rust-only canonical file/symbol/ref/arch outputs prechange and new batch/stream plus existing Cargo edge fixtures; nested-crate control distinguishes attribution from no discovery. Mixed fixture Rust projection unchanged; intentional C# ownership changes explicit. | Frozen prechange Rust dump and hand-authored cargo_discovery/module-path expectations | src/cargo.rs adapter discards custom lib path → C3 module-path mismatch | tests/cargo_discovery.rs; tests/module_path_integration.rs; tests/idxperf_golden.rs | existing focused suites | PENDING — Main/checkpointed-build |
| C4 | S1/S2/S5 | Parse all solution/filter shapes with evaluator sentinel; without trust sentinel absent and trust-required exists; trusted control reaches sentinel and valid worker response. XML external entity input cannot make a network/filesystem read. | Authored candidate/path manifest + executable marker process and network-denial control | src/discovery/msbuild/mod.rs bypass grant check → C4 launch observed | tests/discovery_candidates.rs; tests/discovery_failures.rs::trust_gate | local fixtures | PENDING — Main/checkpointed-build |
| C5 | S3/S4/S6 | Real SDK and Windows classic fixtures: compare framework/profile, imported/linked Compile/Link/properties/references exactly. Target sentinel absent; explicit separately authorized target invocation creates it. Distinguish failed evaluation from successful no-target metadata. | Direct query-capable MSBuild CLI evaluation under identical host/context plus hand-authored fixture manifest; different wrapper/serialization path from worker | tools/tethys-msbuild-evaluate/Evaluation.cs omit imported Compile or use outer evaluation for inner request → C5 per-unit manifest mismatch | tests/msbuild_discovery.rs::sdk_classic_metadata | real SDK + Windows Build Tools | PENDING — Main/checkpointed-build |
| C6 | S3/S5/S6/S7/S11 | One reachable fixture for each 11 stable reason; assert phase/native code and source/unit coverage. Custom SDK failure reaches resolver; timeout host starts and stays alive; malformed input distinguished from unavailable tool. Positive valid sibling must remain available. | Hand-authored reason/exit/coverage manifest; real malformed/custom SDK/restore fixtures and bounded scripted transport fault provider | src/discovery/msbuild/mod.rs map failed unit to empty confirmed list → C6 expected reason/coverage absent | tests/discovery_failures.rs::reason_matrix | local + host fixtures | PENDING — Main/checkpointed-build |
| C7 | S2/S5 | Outside-root linked source and symlink rejected before parsing; inside-root twin indexed. Grantless restore marker absent, granted supported restore marker present; packages.config non-Windows records unsupported without launching restore. | Canonical filesystem manifest + independent restore invocation log | src/discovery/msbuild/candidates.rs replace canonical containment with string prefix → C7 escaped file indexed; restore.rs skip grant → C7 unauthorized restore | tests/discovery_failures.rs::containment_and_restore | local/platform fixtures | PENDING — Main/checkpointed-build |
| C8 | S4/S9 | Linked file in two units with different defines, same assembly names, project move, failed unit and orphan syntax; SQL counts exactly one physical syntax set, correct memberships, no stale removed unit. Distinct second file positive control detects a manifest ignoring files. | Authored physical-file/participation identities and SQL joins | src/db/schema.rs make participation unique on file_id or db/discovery.rs retain removed unit → C8 missing membership/stale unit | tests/msbuild_discovery.rs::membership_replacement | local + SDK fixture | PENDING — Main/checkpointed-build |
| C9 | S8 | Compare cached vs forced evaluation after every key dimension; unchanged eligible fixture must avoid process spawn, changed/custom-I/O fixture must spawn and change expected metadata. Verify input reached cache check, not an earlier failure. | Forced fresh host evaluation plus explicit filesystem/context mutation manifest; external process-start log | src/discovery/msbuild/cache.rs omit glob/negative-import watch set or accept unknown recipe → C9 stale Compile/property manifest | tests/discovery_cache.rs::cache_input_closure | real host and mutation fixtures | PENDING — Main/checkpointed-build |
| C10 | S10 | App unit→multi-TFM Core declaration with no selection: source Ce/affected Core Ca/instability indeterminate, no selected edges; unrelated isolated unit remains known zero; separate proven-edge fixture has exact counts. | Hand-authored graph/evidence manifest, not MSBuild-derived target inference | src/architecture.rs map unselected reference to equal-TFM unit or unknown count to 0 → C10 false known metric | tests/csharp_coupling.rs::unselected_target_metrics | deterministic DB/report fixture | PENDING — Main/checkpointed-build |
| C11 | S10/S12 | After indexing, replace host with launch marker and invoke CLI query commands; no launch and exact persisted report/exit. A trusted index control launches it. Concurrent publish during coupling detail must return one revision, never split reads. | Golden query/exit manifest + marker process + separate writer barrier | src/cli/coupling.rs invoke discovery before read or db/architecture.rs drop read snapshot → C11 process/mixed revision | tests/discovery_cli.rs::queries_are_read_only | actual CLI fixtures | PENDING — Main/checkpointed-build |
| C12 | S11/S12 | Measure all four pinned corpora/legacy and eligible-hit/bypass/changed paths; verify SHAs, exact manifest, repeated canonical answers, separate seconds/RSS/DB+WAL, 18/802-unit stress. Inject at/above-threshold measurement controls to prove qualification rejects breaches, not merely prints metrics. | OS timing/process-tree RSS and file sizes; immutable corpus manifest; arithmetic threshold oracle | qualification checker changes > cap to unconditional acceptance → C12 above-cap accepted; host.rs removes deadline → timeout fixture exceeds deadline | issue-local oracles/qualification.py plus deterministic limit-boundary fixture and tests/discovery_failures.rs timeout | corpus/Windows/pinned runner | PENDING — Main/checkpointed-build |
| C13 | S12 + ledger | Census source/diff against approved manifest; mutate a protected driver with an MSBuild helper and restore. Existing approved wiring positive control allowed. Require localized path/symbol/delta. | Standalone parser-based ledger oracle independent of production code | Add fn evaluate_msbuild to src/indexing.rs or Index::open in batch_writer.rs → C13 forbidden owner | issue-local oracles/module_shape.py | local source census | PENDING — Main/checkpointed-build |
| C14 | S6/S12 | Install produced bundle in clean directory with no source tree; select SDK and Windows MSBuild; evaluate known fixture; absent companion/runtime/protocol mismatch must be typed unavailable, never build/download. Actual host identity must match request. | Packaged-file manifest + direct selected-host CLI version/evaluation output | tools/.../Program.cs load default host instead of selected installation or release.yml omit worker → C14 host/install mismatch | tests/discovery_cli.rs::installed_companion plus release/Windows smoke | package + real host | PENDING — Main/checkpointed-build |

All rows with production mutations are applied only after the named implementation exists, then restored and rerun. C0's mutation edits the design experiment itself; it does not claim to mutate future production. No regression-fence waiver is requested.

## Non-goals and future work

Permanent non-goals: guessed project continuity after moves; AssemblyName identity; all-to-all/equal-TFM target inference; silently supplied trust; feeding source-only syntax into confirmed semantic scope; arbitrary outside-root indexing; query-time evaluation; a generic storage backend; historical context selection. These contradict the approved model or have no second adapter.

Verified intended work owned elsewhere, not omitted acceptance here:

- tethys-yjok: target-granted Roslyn provider and semantic bindings, not this evaluation worker.
- tethys-e2jx: semantic ingestion and unit-owned symbol/declaration schema; this issue completes discovery/unit/membership persistence it consumes.
- tethys-cmlc: full compiler-input targeting-pack profile; blank VSToolsPath evaluation profile is included here.
- tethys-rduz: broader architecture semantics/project-level aggregation; this issue includes the explicitly approved per-unit coupling behavior.
- tethys-dsh1: shared legacy oracle corpus/Windows leg ownership; this issue must still execute its own required classic success/negative/performance fixtures before completion, not waive them because dsh1 is open.

AGENTS.md/CONTEXT.md and relevant command documentation must be updated atomically with the behavior they describe, plus changelog fragment(s). New companion/protocol/deployment docs are required implementation deliverables. Rust-only output promises apply to existing valid Rust fixtures, not obsolete C# pseudo-package attribution in mixed fixtures.

## Falsifier run log

2026-09-06:

- `python3 .tethys-82a6/oracles/revision_transaction.py` → exit 0. Output: `C0 PASS: released savepoints remain unpublished; rollback preserves old rows; writer exclusion and reader pinning hold; commit/reopen observes all new rows.` SQLite **3.53.4** (Python runtime).
- `python3 .tethys-82a6/oracles/revision_transaction.py --mutate-early-commit` → expected exit 1: `AssertionError: C0 mixed revision visible before publish`.

The protocol assertions are labelled C0, separate from pending tethys integration C1. Production qualification will also exercise rusqlite's bundled SQLite, not substitute the Python version. This experiment is the cheapest falsifier and survived; the negative control demonstrates it detects early publication.

## Self-review and approval

Input families map to claims; removed invariants have claims; independent oracles and positive controls are named; every pending claim has Main/checkpointed-build ownership; all behavior-changing placement has a mechanical fence; no fail gate is waived. Census includes production after inline test modules; conservative gated helpers are identified. The projected review-size threshold requires increments in plan.md. LSP references run on 2026-09-06 found 26 index_with_options locations (including declaration) and 10 get_coupling_metrics locations (including declaration), covering CLI, reindex and integration consumers; implementation repeats impact discovery before exported changes.

**Requester architecture approval (verbatim): "yes, I approve"**
Date: 2026-09-06.

Approved scope: the module ledger, protected parents, whole-run transaction/scoped writer, managed evaluation companion and packaging, explicit metric-evidence contract, conservative cache eligibility, and the stated verification gates.

Approved risk acceptances: **None that waive correctness or qualification.** Approved operational tradeoffs: longer serialized writer lifetime (readers continue via WAL), added managed companion distribution, possible cache bypass for arbitrary MSBuild code, and public Rust options/coupling type changes with complete caller migration. Planning and checkpointed implementation may proceed under this design.

## Approved F23 amendment — language-specific source identity

**Approved and implemented; local qualification PASS, committed native platform acceptance pending.** The original approval above remains valid for unaffected decisions. The final local checkpoint and evidence disposition are recorded in review-decisions.md; S5 remains blocked until native S4 acceptance.

### Evidence and unchanged requirements

The global canonicalization added to S4 fails three existing Rust symlink fences (artifact714). The untouched pre-S4 tree2c002f5 passes all seven original symlink fences (artifact722). This is the cheapest falsifier of the assumed pre-existing identity model; logical Rust identity, not global physical identity, survives that comparison. C3's Rust compatibility requirement and C7/C8's C# containment/physical-membership requirements are unchanged.

Relevant input shapes: Rust/C#; relative/absolute and lexically equivalent spelling; ordinary file, in-root file alias, non-cyclic directory alias, outside-root target, missing target; one/multiple memberships and retained syntax after membership withdrawal. Existing loop/dangling-link behavior remains covered by the Rust baseline. Unknown extensions retain unsupported-source/query handling, rather than selecting a language by guess. All are covered by C3/C7/C8 and their additional fences below.

### Inventory and alternatives

Read-only inventory: S4IdentityInventory. `module_resolver.rs` is416 production lines with a crate-private trait and nonallocating static Rust/CSharp dispatch; `discovery/mod.rs` is43 lines with a public trait and aggregate dispatcher; `cargo.rs` is443 lines; `discovery/msbuild/mod.rs` is772 lines. `languages/mod.rs` is74 lines with a public syntax-only trait and nonallocating static extraction dispatch. Counts follow the C13 exclusion convention. No existing general source-file identity policy was found. Rejected extraction-owner whole-file counts were not established; those files are not proposed for mutation.

| Alternative | Interface and caller | Hidden policy/adapters | Tradeoff |
|---|---|---|---|
| A — existing language-path seam, proposed | Private `ModuleResolver::source_path_identity() -> SourcePathIdentity`; source selection and query normalization consume `get_module_resolver(language).source_path_identity()` | Rust returns Logical; CSharp returns Physical. Follows the existing GlobPolicy pattern; registry already has both real adapters and allocates nothing. | Smallest interface change; keeps language-path rules local, but explicitly expands the resolver's documented responsibility beyond import-path translation. |
| B — discovery adapters | Add a source-identity operation to WorkspaceDiscovery and a language-specific discovery dispatcher; selection calls the dispatcher | Cargo and MSBuild adapters own their respective file-identity rules | Keeps intake policy with discovery; widens the public discovery interface and adds another dispatch path alongside aggregate metadata discovery. |
| C — extraction seam | Add source-identity policy to LanguageSupport; selection calls the static language registry | RustLanguage/CSharpLanguage choose logical/physical identity | Straightforward for parsers, but gives the syntax-only interface workspace-path responsibilities and expands the changed owner set to both extraction implementations. |

Proposed A passes the seam tests: deletion would duplicate the language distinction across publication/freshness/query callers; consumer-facing file/dependency/freshness tests exercise the same policy as production; two existing adapters implement it; one module owns the distinction. No new module, generic adapter, allocation-bearing factory, database access or process call is introduced.

### Proposed ledger delta

| Module/path | Interface | Owns | Reuses | Must not own | Adapters / tests | Change |
|---|---|---|---|---|---|---|
| src/languages/module_resolver.rs | Existing private trait plus SourcePathIdentity and source_path_identity | DB-free language-path identity policy as well as existing resolution semantics | Existing static registry and language implementations | SQL, evaluator execution, run publication | Rust/CSharp; public file/dependency/freshness fences | Deepen;416 current, approximately430–470 projected |
| src/indexing.rs | Existing discover_files | Existing shared candidate selection, applying the declared identity policy | Existing walker, canonicalization, diagnostics and resolver registry | Rust/CSharp branches or a duplicate policy | Index/reindex/source freshness | Neutral wiring only;1395 current, approximately1405–1440 projected |
| src/lib.rs | Existing relative_path and public queries | Existing lexical normalization and query-path conversion, applying the same identity policy | Existing lexical normalizer and resolver registry | Rust/CSharp branches or a duplicate policy | Relative/absolute file and dependency queries | Neutral wiring only;1183 current, approximately1190–1220 projected |
| src/reindex.rs; src/unused_imports.rs | Existing shared-selection callers | Existing freshness / Rust unused-import behavior | discover_files | Discovery execution or another identity rule | Existing freshness/unused-import fences | Retain caller contract |

Protected-parent limits do not increase: indexing remains within baseline1313+150 and lib within baseline1143+100. Other ledger rows, schemas, transaction ownership, discovery grants, metadata evaluation and module-path binding are unchanged. No repository-wide Rust symlink hardening is added: preserving existing external Rust targets is an explicit compatibility requirement, not a new C# escape allowance.

### Additional falsification obligations

| Claim | Falsifier and independent oracle | Named mutation | Regression fence | Cost / status |
|---|---|---|---|---|
| C3 — preserve logical Rust identities | Compare ordinary/aliased/external Rust files with the pre-S4 filesystem manifest; distinct logical aliases stay distinct and known external symbols remain available. Relative/absolute and dotted spellings identify the expected logical row. | In the Rust resolver implementation, return Physical instead of Logical; expect the external-target or distinct-alias assertion to fail. | Existing tests/symlink_boundary.rs and affected_tests_cli::path_forms_equivalent; strengthen alias identity assertions rather than changing them to canonical behavior. | Local; baseline PASS artifact722, implementation PENDING — Main/S4 repair |
| C7/C8 — retain physical contained C# identities | Hand-authored C# file/directory aliases share one physical row, symbol set and dependency identity; an outside-root C# twin is absent while an inside-root positive control is present. Native evaluated-only membership/freshness still agrees with SQL. | In the CSharp resolver implementation, return Logical instead of Physical; expect duplicate alias identity or escaped-source assertion to fail. | Convert the newly introduced Rust-canonical fixture in concurrency_and_filesystem.rs into the intended C# identity fence; retain native msbuild_discovery membership/freshness fences. | Local/native; PENDING — Main/S4 repair |
| C13 — keep the policy at the approved seam | Source/dependency census and existing language-neutral seam lint reject policy ownership outside module_resolver.rs; public entry points remain unchanged. | Move effective source_path_identity policy into src/lib.rs and route consumers through it; expect the shape fence to identify that exact forbidden owner. | Extend .tethys-82a6/oracles/module_shape.py ownership check; existing seam_lint.rs | Local; PENDING — Main/S4 repair |

Run new mutation red/restored-green checks, the complete ordinary/native suites, affected packaged CLI/SQL and production-scale checks, and renewed isolated conformance before reconciling all nine S4 gates. Retain prior evidence only with explicit applicability under the workflow contract. Keep the full S5/S6 scope and all platform/resource limits.

**Requester amendment approval (verbatim): "Approve existing seam"**
Date:2026-09-07. Selected alternative A and its ledger/falsification obligations. No new risk waiver. Main updates the affected S4 plan before production edits.
