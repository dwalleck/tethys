# Route: tethys-82a6

Change: Evaluation-only MSBuild discovery adapter and evaluation-unit schema.
Date: 2026-09-06

## Route tests

| # | Test | Evidence | Verdict |
|---|------|----------|---------|
| 1 | Empirical premise | Current docs/2026-09-06-analysis-csharp-binding-model-review.md §4 G3/G4/G7 and tethys-chlt approved probe records establish evaluation-only SDK/classic metadata extraction and blank VSToolsPath tolerance on Linux. No premise assumes evaluation produces compiler bindings, framework-reference resolution, or arbitrary-host compatibility. Host capability detection and fail-closed unsupported-host outcomes are design requirements; successful Windows/classic qualification remains a mandatory implementation check, not an assumed result. Cache correctness requires a validated dependency/context/provenance closure or reevaluation, not faith in project-file mtimes alone. | no |
| 2 | Structural module shape | src/cargo.rs owns Cargo discovery today; src/indexing.rs directly orchestrates it and arch attribution; src/reindex.rs tracks physical source changes. A new neutral discovery owner with Cargo/MSBuild adapters must supply precomputed context to database-free ModuleResolver. src/db/schema.rs has no snapshot/context/unit schema; arch_file_packages currently assigns each file exactly one package. src/db/architecture.rs and public Tethys/CLI query boundaries need unit-aware evidence. Protected parents: src/indexing.rs, src/resolve.rs, src/batch_writer.rs remain language-neutral; no MSBuild launching from ModuleResolver. Publication ownership must replace independent per-file/architecture commits with a coherent revision boundary. | yes |
| 3 | Production-scale risk | Every index/reindex invocation reevaluates metadata or validates a cache entry. Existing 72-project Linux evaluation is 11.9 seconds versus 3.4-second source indexing (review §4 G7). Process lifetime/RSS, imported/glob inputs, multi-unit participation and coherent publication require measured cold/changed/cache-hit paths on corpus and legacy cluster. | yes |
| 4 | Explicit behavior | Most behavior is approved by tethys-chlt and amended tethys-rvr5 and enumerated in tethys-82a6. One unresolved observable contract: coupling must report evaluation units and declared ProjectReference dependencies, but discovery does not establish referenced target-framework selection. tethys-chlt forbids guessing from equal TFMs; open tethys-rduz explicitly owns command aggregation. No source defines whether unselected references produce project-level aggregate metrics or indeterminate per-unit metrics. Resolve this before architecture. | no |

Unknown tests: none. T4 has a known unresolved scope/aggregation decision, not missing repository evidence.

## Selected route

Structural — public/schema/discovery/publication changes and scale risk; one unresolved query contract. Existing evaluation mechanisms support design without a new external-behavior premise.

## Required artifacts

| Artifact | Owner | Status |
|---|---|---|
| route.md | change-workflow | this file |
| spec.md | interrogated-spec | required — resolve coupling aggregation/target-selection behavior and record requester approval |
| evidence.md, probe.* | prove-it-prototype | N/A — no unverified design premise; existing probe limits remain explicit qualification gates |
| design.md | falsifiable-design | required — discovery boundary, schema/publication, host/trust/cache and performance claims |
| plan.md | budgeted-plan | required — independently green increments and module growth ledger |

Oracle checkpoint in checkpointed-build: required — Structural route.

## Downstream sequence

interrogated-spec → falsifiable-design → budgeted-plan → checkpointed-build

## Terminal criterion

Structural — every downstream artifact satisfies its owning stage's completion criterion, ending with no FAIL in checkpointed-build's recorded gate. All seven tethys-82a6 acceptance criteria remain required; no implementation or qualification criterion is waived.

## Evidence provenance

Issue claimed with `rivets update tethys-82a6 -s in_progress` on 2026-09-06. Read current tethys-chlt, tethys-rvr5, tethys-cmlc and tethys-rduz. Read-only scouts DiscoveryMap, SnapshotMap and DecisionEvidence mapped implementation and prior evidence; no production changes or verification runs performed during routing. Existing unrelated dirty files are preserved.

The blank VSToolsPath evaluation import-tolerance policy is distinct from cmlc's three-property compiler-input profile: targeting-pack injection and FrameworkPathOverride require target-execution authority and are not added to evaluation-only discovery.
