# tethys-9z7i slice 3 plan (2026-07-04) — design-slice3.md, approved

Ship conventions bind. Three build slices:

## P1: refs_banded view (C1, C2)
**Claim:** view exists, band CASE matches ADR verbatim, refs_named untouched.
**Oracle:** ADR-0003 table text vs per-strategy view readback (10 asserts incl. NULL); sqlite_schema.
**Stress fixture:** schema_tests inserts one ref per strategy via the typed helper + one unresolved; readback bands. Kills: a strategy in the wrong CASE arm; band non-NULL on unresolved.
**Loop budget:** none (DDL + tests). **Files:** src/db/schema.rs (view + tests).

## P2: callers exclusion (C3, C4, C5)
**Claim:** exclude_speculative drops only all-speculative-support edges, transitively; default unchanged.
**Oracle:** fixture source hand-read (which calls are import-backed vs bare-cross-crate is written in the fixture).
**Stress fixture:** tests/strategy.rs: chain fixture — A -[explicit]-> B -[bare cross-crate unique]-> C, plus D with BOTH an import-backed and a bare call to E (mixed support). Expected with flag: B in A's callers-of-B sense... concretely: callers(C, exclude)= {} (B's edge to C is speculative-only, and transitively A must not appear); callers(C, no flag) = {B direct, A transitive}; callers(E, exclude) keeps D (mixed support). Kills: EXISTS inverted; recursive arm unfiltered; mixed-edge dropped.
**Loop budget:** EXISTS subquery per candidate edge inside the CTE — probes idx_refs_symbol; O(edges × log refs) at production (edges ≈ 10^5) ≪ budget.
**Files:** src/graph/mod.rs + src/db/graph.rs (trait + impl param), src/lib.rs facade (wiring line). Impact: all get_callers callers updated (grep-listed in commit).

## P3: CLI flag (C6 + C5's default fence)
**Claim:** --exclude-speculative surfaces in table+json; default output byte-identical (existing tests unmodified).
**Oracle:** run_cli output text (mechanical).
**Stress fixture:** binary-level run on the P2 fixture workspace, flag on/off diff. Kills: flag parsed-not-threaded.
**Loop budget:** none. **Files:** src/main.rs (arg), src/cli/callers.rs (thread + display).

Self-review: no new loops beyond the budgeted EXISTS; fixtures adversarial per-claim; no doc-contract preconditions added; data->stdout unchanged; tracker refs: 53iv/msn0/3i35 (verified open, non-goals), panic-points skip recorded in design + epic amendment at close-out. Coverage C1-C6 complete.
