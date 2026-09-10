# Route: tethys-82a6

Change: Evaluation-only MSBuild discovery; S6 performance-design amendment.
Date: 2026-09-08

## Route tests

| # | Test | Evidence | Verdict |
|---|------|----------|---------|
| 1 | Empirical premise | P1–P9 are discharged in evidence.md. Process reuse leaks state; removing reevaluation loses byte proof; concurrency is fast on the bounded independent shape but changes side-effect order. The full warm repeat preserved output yet was slower, so no beneficial warm-run effect or representative baseline is assumed. Cadence authority separately assigns PR semantics and full-corpus timing work. | no — prior empirical evidence retained |
| 2 | Structural module shape | The overall change alters public interface, schema, seams and responsibility owners per the approved design ledger (route T2 was already yes for the main change). The performance amendment itself adds **no** production module, interface or owner: it changes only the nonproduction qualification runner and its lane assignment/accounting. host.rs, Evaluation/Program/Contract, scope/cache/restore and the protected lib/indexing/resolve/batch_writer + Cargo owners are unchanged. | yes — overall change; amendment adds no production shape |
| 3 | Production-scale risk | The approved240/401 and480/802 shapes, full corpus, repeated instrumented/control runs,60s evaluation deadline, qualification wall/RSS/storage caps and unchanged fresh-input behavior all remain binding. Process lifetime, concurrency and input-observation changes carry latency, memory and correctness risk. | yes |
| 4 | Explicit behavior | Approved metadata, authority, grants, input closure, failures, cleanup, workloads, repetitions, cadence-specific time/RSS/storage gates and unchanged Rust behavior remain binding. The requester resolved the amendment on2026-09-08 by selecting **serial product evaluation** (verbatim: “Lets proceed with your recommendation”), so cross-project evaluation order is unchanged and no execution-order risk acceptance exists. | yes — behavior fully explicit |

Unknown tests: none.

## Selected route

**Structural** — the main change alters public interface/schema/ownership (T2 yes) under production-scale risk (T3 yes); T1 has no remaining unverified premise (P1–P9 discharged). The performance amendment is resolved with **no product change**: serial evaluation retained, cadence orchestration corrected under Main's existing S6 ownership.

## Required artifacts

| Artifact | Owner | Status |
|---|---|---|
| route.md | change-workflow | this file |
| spec.md | interrogated-spec | existing approved spec.md remains the behavior source; no new unresolved behavior (T4 yes) |
| evidence.md, probe.* | prove-it-prototype | retained PASS — diagnostic P1–P9 and cadence authority; not calibration or suite acceptance |
| design.md | falsifiable-design | approved architecture plus resolved performance amendment (no product change); orchestration correction recorded |
| plan.md | budgeted-plan | existing slices retained; S6 cadence assignment/accounting contract recorded |

Oracle checkpoint in checkpointed-build: required after any approved implementation.

## Downstream sequence

falsifiable-design (amendment resolved, no product change) → budgeted-plan → checkpointed-build

## Terminal criterion

Structural — every downstream artifact satisfies its owning stage's completion criterion, ending with no FAIL in checkpointed-build's recorded gate. The performance amendment contributes no production change: serial evaluation is retained, the cadence orchestration correction is verified by `oracles/qualification_cadence_fence.py` (named mutation red, restored green), and the remaining S6 obligations are the assembled qualification under the correctly assigned cadence plus a reviewed fourteen-observation calibration authority. Retained P1–P9 are diagnostic evidence, not qualification or calibration.

## Evidence provenance

Issue claimed with `rivets update tethys-82a6 -s in_progress` on 2026-09-06. Read current tethys-chlt, tethys-rvr5, tethys-cmlc and tethys-rduz. Read-only scouts DiscoveryMap, SnapshotMap and DecisionEvidence mapped implementation and prior evidence; no production changes or verification runs performed during routing. Existing unrelated dirty files are preserved.

The blank VSToolsPath evaluation import-tolerance policy is distinct from cmlc's three-property compiler-input profile: targeting-pack injection and FrameworkPathOverride require target-execution authority and are not added to evaluation-only discovery.

## Performance-design authorization

On2026-09-08 the requester selected **“Design performance repair”**: preserve workloads, repetitions, transparency and caps; investigate evaluation costs, present a falsifiable design, and pause before production changes. This authorizes empirical/design work, not implementation or stack publication. Baseline evidence is retained at `target/qualification/baseline-f34-v1/phase-summary.json`; completed S6 authority observations remain separate from qualification acceptance.
