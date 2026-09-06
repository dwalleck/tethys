# Spec: Evaluation-only discovery and coherent evaluation-unit snapshots

Status: behavior resolved; requester approved the coupling clarification on 2026-09-06. Architecture approval remains separate.

## Request (verbatim)
> claim and implement tethys-82a6

## What this is
Add trusted evaluation-only MSBuild discovery alongside unchanged Cargo discovery. Persist one coherent revision of project/unit metadata, source participation and typed failures, and expose C# discovery evidence through coupling without inventing compiler-selected reference targets.

## Roles
- **CLI/CI maintainer**: grants evaluation and optionally restore authority, selects evaluation context, inspects partial coverage and coupling.
- **Library consumer**: invokes indexing/reindexing and queries the published revision without triggering evaluation.

## Behavior

### Cargo preservation
- **Given**: an existing Rust fixture and unchanged options.
- **When**: indexing runs through the Cargo adapter.
- **Then**: existing Rust source attribution and canonical query dumps are byte-identical.

### Candidate discovery and authority
- **Given**: solution, solution-filter, solution-XML or standalone project candidates inside the workspace.
- **When**: index/reindex runs without evaluation trust.
- **Then**: candidates are parsed without executing project code, source-only syntax remains available and evidence records trust-required with nonzero indexing status.

### Evaluated snapshot
- **Given**: evaluation trust and an available compatible host under the recorded context.
- **When**: index/reindex evaluates projects/frameworks or validates reusable evaluation inputs.
- **Then**: the snapshot records path-based project identity, framework identifier/version/profile/platform (including classic empty SDK TFM properties), effective Compile membership and Link metadata, parse properties, assembly metadata, declared references, host/toolchain/restore provenance and typed evidence, without executing build/design-time targets or creating resolved symbol bindings.

### Publication and failure
- **Given**: a previously published revision.
- **When**: an invocation completes with unit successes and bounded typed failures.
- **Then**: it publishes one coherent replacement containing those outcomes, not stale successes from earlier revisions; a fatal mid-run failure leaves the previous revision intact.

### Query and schema boundaries
- **Given**: an old-schema index or a current published index.
- **When**: a command opens it.
- **Then**: old schema is refused with explicit index --rebuild guidance; current query commands read persisted evidence and never evaluate projects.

### Performance evidence
- **Given**: pinned corpus and classic fixture cluster.
- **When**: discovery/evaluation, changed-input reindex and unchanged cache-hit runs are measured.
- **Then**: records include separate discovery/evaluation, downstream reindex and total wall-clock seconds and peak RSS, including the 72-project legacy comparison where available; cache correctness is checked against forced reevaluation.

### Evidence-scoped coupling
- **Given**: a pure-C# workspace with evaluated units and declared ProjectReference items, including references whose target evaluation unit is unproved.
- **When**: the CLI/CI maintainer runs coupling or a library consumer requests the equivalent report.
- **Then**: the report lists evaluation units and declared project dependencies separately from selected-unit edges. Ca/Ce and instability that depend on an unproved selection are explicitly indeterminate, never zero or guessed from equal TFMs, assembly names or all-to-all projection. Unaffected evidence-complete metrics remain available. Project-level framework-collapsed metrics are not introduced.

## Success criteria

All seven acceptance criteria in tethys-82a6 remain required: byte-identical Cargo proof; SDK/multi-target/classic/imported/linked/solution-filter/standalone fixture snapshots; every amended rvr5 reason exercised; atomic revision and failure retention proof; schema refusal and evaluation-free query proof; pure-C# coupling unit/reference proof; corpus/legacy/cache timing and RSS measurements. The coupling check must show an unselected multi-framework reference separately and indeterminate dependent metrics, with no fabricated selected edge. Design must name executable checks before implementation.

## Out of scope

Resolved compiler symbol bindings and semantic ingestion (tethys-yjok, tethys-e2jx); target-executing compiler-input acquisition and targeting-pack profile (tethys-cmlc); inferred target-framework selection; expanded Rust target/feature semantics. Existing source-only syntax does not become confirmed unit-scoped semantics through membership alone.

## Related issues

Prior-art search: `rivets list -l csharp-support -n 100`; direct reads and read-only evidence investigation of the relevant decision chain.
- tethys-82a6: complete requested acceptance contract.
- tethys-chlt: approved identity, ownership, publication, cache amendment and unchanged Cargo behavior.
- tethys-rvr5: evaluation authority, context, trust/restore, stable failure reasons and import-tolerance amendment.
- tethys-teax: evaluation mechanism research.
- tethys-cmlc: classic profile authority distinction and unsupported packages.config restore.
- tethys-rduz: unresolved architecture command aggregation; expressly not settled by chlt.
- tethys-yjok and tethys-e2jx: semantic provider and ingestion ownership, not this discovery slice.

## Decisions

| Question | Decision | Rationale | Implication |
|---|---|---|---|
| One context or history? | One active recorded context | tethys-chlt | Context changes invalidate affected facts; no historical query selector |
| Project/framework identity? | Workspace-relative project path and evaluated framework identity | tethys-chlt | Moves remove/discover; empty SDK TFM is valid for classic projects |
| Physical-file sharing? | Many unit memberships; explicit source-only scope | tethys-chlt | No invented project, no copied syntax double counting |
| Authority and permissions? | Explicit evaluation trust; separate restore authority | tethys-rvr5 | Evaluation can execute imports/property functions; no build/design-time targets |
| Partial unit failure? | Publish successful units plus typed failure evidence; nonzero indexing status | tethys-rvr5/chlt | No mixed old/new facts and no fabricated enumeration success |
| Cache invalidation? | Validate context/provenance/input closure each invocation; otherwise reevaluate | tethys-chlt cache amendment plus unchanged freshness contract | Project/import mtimes alone cannot justify reuse after source-glob, restore or host changes |
| Which classic profile applies here? | Recorded blank VSToolsPath evaluation import-tolerance only | amended tethys-rvr5 and explicit tethys-82a6 scope | Targeting-pack injection and FrameworkPathOverride remain target-execution work |
| Unselected referenced framework in coupling? | Preserve per-unit metrics; dependent metrics indeterminate; show declared reference separately | Requester approved the recommendation: “yes, I approve” on 2026-09-06 | No guessed unit edges, zero-valued unknowns or implicit project-level aggregation |

## Edge checklist

| Dimension | Decision | Rationale / observable check |
|---|---|---|
| Empty set | No project candidates means source-only scope; failed enumeration is not empty success | chlt; empty-workspace and malformed-enumeration fixtures |
| Maximum scale | Qualify on pinned parity corpus and legacy cluster; record seconds/RSS rather than claim an unsupported size ceiling | 82a6; design names measured fixtures and inherited parity thresholds |
| Null / missing field | Empty SDK TFM properties are valid for classic frameworks; missing required evaluated identity produces typed incomplete evidence | chlt; classic Client-profile fixture versus failed enumeration |
| Concurrent writes | Serialize publication; a competing writer cannot mix its revision with another invocation | coherent revision contract; concurrent-writer reader fixture |
| Permission denied | No evaluation without trust; no restore without its separate grant; process/filesystem failures retain diagnostics | rvr5; grant and access-denial fixtures |
| Partial failure | Publish successes and explicit failures; fatal run failure preserves previous revision | chlt/rvr5; failpoint and partial-TFM fixtures |
| Retries / idempotency | Each invocation reevaluates or validates reusable inputs; repeating unchanged inputs preserves canonical facts | chlt cache amendment; repeat and forced-evaluation comparison |
| Soft-deleted records | N/A — no soft-delete API; removed projects are absent from the new active revision | chlt path identity; project-move fixture |
| Multi-tenancy boundaries | Workspace containment applies to linked/imported sources; no outside-workspace indexing | rvr5; canonical path and symlink escape fixture |
| Time-zone / DST | N/A — no civil-time policy; staleness uses filesystem timestamps | timestamp/context invalidation comparison |
| Replication lag | N/A — local SQLite index, no replication subsystem | query reads only coherent published revision |
| Cache invalidation | Changed or unverified input/context/provenance closure forces evaluation | chlt; project/import/glob/context/restore invalidation fixtures |

## Open-question queue

Empty for observable behavior. Concrete APIs, schema layout, module placement, cache eligibility and qualification commands belong to falsifiable-design and require architecture approval.

## Approval

Requester approval (verbatim): "yes, I approve"
Date: 2026-09-06
Approval scope: the presented coupling behavior and acceptance clarification. Existing model, authority and lifecycle decisions are adopted from the approved tracker records above; this does not approve an unseen implementation design.
