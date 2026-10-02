# Route: tethys-et80

Change: Diagnose and correct the S6 instrumented/control canonical-fact divergence without relaxing fact equality.
Date: 2026-10-01

## Route tests

| # | Test | Evidence | Verdict |
|---|------|----------|---------|
| T1 | Empirical premise | The retained failure is accepted. The faithful original-root replay isolates fifteen differing `qualified_exact` references; all other fact tables agree except their derived `file_deps` and `call_edges`. Every differing target follows first-row selection among duplicate qualified names. `before.json` and `diagnosis.json` retain the exact input, row, binary, and database identities. No external capability or speculative performance premise determines the correction. | no — observed mechanism and existing unique-or-decline contract |
| T2 | Structural module shape | The owner remains `src/db/symbols.rs::Index::get_symbol_by_qualified_name`, with unchanged signature and responsibility. Final production owner size: 1193 → 1205 physical lines (+12), within the projected ≤25 plus 15-line margin. No schema, seam, dependency movement, or unrelated responsibility changes. Edits in the protected facade are documentation only; its shared lookup now declines ambiguous selectors under the existing unique-or-decline contract, as detailed in Impact below. The standalone qualification caller gains its required inherited-environment field; two literal instrumentation anchors follow the current call syntax without moving timer boundaries. Existing-file changes total 218 lines; receipts/scripts plus a 500-line completion margin remain below 3200 lines, under the 4000-line review-size bound. No applicable module-size trigger or god-file growth mechanism introduced. | no — final existing-owner recheck |
| T3 | Production-scale risk | The correction bounds an indexed qualified-name query to two rows, replacing an arbitrary first row. It adds no workspace scan, new collection, concurrency, evaluation-order, workload, cadence, or calibration change. Existing corpus replay covers large-input behavior; no new scale mechanism or budget is introduced. | no |
| T4 | Explicit behavior | Given identical source bytes, pre-run database/cache, evaluation context, and frozen resolver inputs at one shared root, when the retained instrumented and control drivers index with the same request, then every canonical fact table must agree independently of disposable database IDs and already-documented volatile fields. Given a real semantic difference, then the comparison must still reject it. Given the observed failure, when diagnosing, then identify the differing table/row, classify its owning mechanism, correct it or record an evidenced separate product outcome, and retain the same-root reproduction in the ticket. These behaviors come from tethys-et80 acceptance and `.tethys-82a6/plan.md` S6/C12; no new user decision is required for a technical correction that preserves them. | yes |

Unknown route tests: none. The product defect is first-row selection among duplicate qualified names (existing tethys-bvgb); no implementation outside this bounded correction is authorized.

## Selected route

**Local** — a bounded correction under the existing S6/C12 equality contract. Diagnosis precedes implementation. If the observed cause invalidates any route verdict, amend the route and enter the required owning stage before changing the contract.

## Required artifacts

| Artifact | Owner | Status |
|---|---|---|
| route.md | change-workflow | this file; sole workflow/evidence narrative for this ticket |
| spec.md | interrogated-spec | N/A — behavior fully explicit in T4 and existing S6/C12 |
| evidence.md, probe.* | prove-it-prototype | N/A — no new empirical design premise; diagnostic receipts link from this record |
| design.md | falsifiable-design | N/A — Local technical repair; approved S6 ownership and oracle meaning unchanged |
| plan.md | budgeted-plan | N/A — Local route; no new PR partition or production mechanism |

Verification checkpoint in checkpointed-build: N/A — Local route; focused behavior and applicable repository checks still required.

## Downstream sequence

Normal repository diagnosis and fix: retain a same-root table-level comparison, minimize the failure, implement a contract-preserving correction with behavioral regression coverage, then exercise the original comparison and applicable gates.

## Work ownership and provenance

- User request: “claim and implement tethys-et80”.
- Claimed with `rivets claim tethys-et80 --assignee omp -y`, then `rivets start tethys-et80 -y`.
- Branch: `fix/tethys-et80-canonical-transparency`, based on upstream-synchronized `main` at `0179b50c22eb008d095b29c744247d25827f1969` (fetch followed by ahead/behind count `0 0`).
- GitHub PR #49 is MERGED, not the draft described by the September handoff. Its retained binaries/evidence remain diagnostic inputs; they are not proof of current-source acceptance.
- Existing dirty `.rivets/issues.jsonl`, untracked research, skills and other files are user work. Preserve them; no unrelated tracker commit, reset, stash, or publication.
- No qualification driver/lane process was running when checked before diagnosis.

## Diagnosis and verification record

- Existing behavior contract: tethys-bvgb explicitly prescribes bounded unique-or-decline for duplicate qualified names. Its current public-selector notes also forbid an arbitrary winner. This repair addresses that mechanism; module-qualified selector round-tripping remains tethys-70g3 and is not introduced here.
- First replay: `python3 .tethys-et80/replay.py --output target/qualification/tethys-et80-before` retained both databases and full receipts. Reports matched; exactly nine references differed, across `MacHelper::EnsureInitialized`, `VulkanSupport::TryInitialize`, and `ClipboardDataFormatHelper::ToDataFormat`. Candidate sets were identical; winner followed the lowest database ID. **Limit:** relocating the shared root changed restored discovery from 159 to 101 units. This is valid reduced mechanism evidence, not original-corpus acceptance.
- Original-root replay uses a rename/finally-restore guard: the entire retained workspace is preserved unchanged while disposable copies replay at its original absolute path. An attempted private bind mount failed with a worker EOF before useful evidence; no outcome is claimed from it. These are diagnostic command/path corrections, not changed oracle semantics.
- Faithful reproduction: `python3 .tethys-et80/replay.py --output target/qualification/tethys-et80-original-root-before` retained the original absolute root and reset from the same prepared tree between legs. Both reports contain 104 projects / 159 units and are byte-identical. The canonical comparison rejects fifteen differing references across four duplicate-name groups, with differences confined to `refs`, `file_deps`, and `call_edges`. See `before.json` and `diagnosis.json`. The original retained database was restored byte-exactly.
- Hypotheses checked: arbitrary first-row selection predicts the observed duplicate target; eligibility drift is excluded by identical canonical symbol/candidate facts; projection-only ID noise is excluded by differing target source files and public-callee regressions.
- Failing-before regression: `env -u CARGO_TARGET_DIR cargo nextest run --test method_calls -E 'test(csharp_qualified_fallback_declines_duplicate_methods)'` → **FAIL**, expected unbound duplicate `Helper::Ping`, observed `Some("Helper::Ping")`, 0.015s. The initial invocation did not reach tests because ambient `CARGO_TARGET_DIR` was empty; unsetting only that invalid variable corrected the command.
- Test seam: existing `Tethys` indexing and public caller/callee operations, with established reference-row assertions where unresolved-name/provenance must be observed. Unique positive controls prevent vacuous no-bind success; batch/stream and Rust/C# cover the shared lookup.
- Impact: LSP references/definition returned no results despite direct callsites; reported as tool issue. Resolver dogfooding carve-out applies. Source reference search found facade get-symbol/references/callers/callees/impact/reachability and Pass-2 fallback; all already handle `None`. Ambiguous `get_symbol` queries return `None`; existing name-based caller/callee/impact/reachability entry points report `Error::NotFound` rather than silently answering for an arbitrary declaration. This is the shared unique-or-decline correction prescribed by tethys-bvgb, not a new selector format or structured ambiguity error; those are outside this change. No caller signature migration is required.
- Compatibility prerequisites: current-source preparation exposed a missing `DiscoveryOptions.environment` field in `.tethys-82a6/oracles/qualification_driver.rs:54`, followed by stale evaluation/restore literals in the `ANCHORS` table of `.tethys-82a6/oracles/qualification_runtime.py:173-178`. The caller now uses `EvaluationEnvironment::inherited()` inside the controlled subprocess. Both anchor replacements preserve that environment argument and the same process-call-only RAII spans, phase names, deadlines, and exact-one cardinality. These are the only changes to the pre-existing qualification files. Failed attempts remain under `target/qualification/tethys-et80-current` and `-v2`; successful preparation is `-v3`. No canonical projection or normalization changed; `verification.json` records identical before/after projection-function hashes. Local route remains applicable.
- Current-source build: `env -u CARGO_TARGET_DIR python3 .tethys-et80/prepare.py --output target/qualification/tethys-et80-current-v3` → PASS. `preparation.json` pins copied production sources, standalone caller, companion, toolchain, and both binary hashes. The compiled `symbols.rs` hash equals the restored mutation-tested source.
- Regression negative control: disabling only the duplicate guard makes both final valid-language regressions fail with an arbitrary duplicate target. Restoring the source byte-exactly passes all 84 focused method-call, graph, and crate-root tests. Unique positives remain covered. Exact commands and outputs are in `verification.json`.
- Repository gates: clippy with warnings denied, all-feature nextest, doctests, and formatting pass. Test counts and exact commands live in `verification.json`; skipped/ignored tests are not claimed as exercised. An initial `similar_names` lint was corrected by naming incoming-call bindings distinctly, without suppression.
- Independent review: removed an obsolete duplicate-source setup from the graph test while retaining source identity, dangling-edge omission, and corrupt-row error guarantees. Follow-up product review and the separate qualification-compatibility review both report no findings. Review scope/disposition is retained in `verification.json`.
- Repaired consumer smoke: `python3 .tethys-et80/replay.py --output target/qualification/tethys-et80-after --instrumentation target/qualification/tethys-et80-current-v3/instrumentation` → PASS. `after.json` records equality of every canonical table and byte-identical stdout, preserving the original 104 projects / 159 units, shared absolute root, request, and frozen environment. Both executed binary hashes match `preparation.json`. The retained original database was restored byte-exactly.
- Row-level confirmation: all fifteen previously divergent reference sites remain present with their names retained, no symbol binding, and no strategy in both repaired databases. Chained calls can share start coordinates, so the diagnostic query also matches the reference/target name; unrelated calls at the same position are not conflated. The query and results are in `verification.json`.
- Ticket evidence note appended to `tethys-et80`. It remains claimed/in progress, implementation ready for review. AGENTS.md, CONTEXT.md, facade docs, affected fixtures and the user-facing changelog fragment accompany this correction.

## Terminal criterion

PASS — The original failure is reproduced at the original root and classified; the bounded product correction, discriminating regressions, unchanged-projection consumer replay, repository gates, independent reviews, final T2 check, and ticket evidence update are complete. Source/input/environment/binary identities and exact results are retained in the linked receipts. This does not complete tethys-vcxy, tethys-82a6, calibration, or full S6 acceptance; those remain separate tickets.

## Publication authorization

- Requester: “commit the changes and open a PR”.
- Scope: publish this verified correction and its tests, documentation, changelog, qualification compatibility fixes and evidence on `fix/tethys-et80-canonical-transparency`; open a PR against the repository default branch. Do not merge or close this or adjacent tickets.
- Preserve the pre-existing dirty tracker and unrelated research/skills. Exclude `.rivets/issues.jsonl` from this commit; append the PR URL to the local tethys-et80 record after creation.
- Publication verification: PASS — 86 staged and working-tree production/test/caller/oracle/replay hashes match the retained receipts, so the recorded regression, corpus and repository-gate results remain applicable. No runtime behavior, test, fixture, dependency, or configuration changed for publication; only this narrative and the changelog wording changed. Both changelog-lint tests and `git diff --cached --check` pass. The staging allowlist contains exactly the 18 owned files and excludes the dirty tracker.
