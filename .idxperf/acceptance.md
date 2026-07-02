# idxperf acceptance record

Date: 2026-06-09/10. Branch `index-correctness`, slices 1–7 committed
(0e21989..6831b5e). Baseline binary: built from d3db7ea
(/tmp/tethys-baseline-bin); frozen comparison tree: `git archive` of d3db7ea.

## Gate results

| Gate | Requirement | Result | Status |
|---|---|---|---|
| C10 self-index | ≥2.0× vs 15.740s ± 0.262s | **442.8ms ± 41.2ms → 35.5×** | PASS |
| C1 batch dump | byte-identical, frozen tree | 19,770 rows, diff empty | PASS |
| C2 streaming dump | byte-identical per-mode | 19,769 rows, diff empty | PASS |
| C3 C# fixture dump | byte-identical | 30 rows, diff empty | PASS |
| C6/C7 resolved count | == baseline | 1,847 == 1,847 (same tree, both binaries) | PASS |
| C11 criterion | no case >10% slower | (see below) | (recorded below) |
| Test suite | all pass | 31 suites, 0 failures (414 lib + integration) | PASS |
| Lints | zero warnings | clippy --all-targets clean | PASS |

## Per-slice oracle log

- Slice 1 (`index_parsed_file_atomic`): commit_hook == 1 per file write;
  FK-violation rollback leaves zero rows; dump unchanged (no call sites yet).
- Slice 2 (batch call site): frozen batch dump diff empty; C# fixture diff
  empty; interim wall time 2.32s.
- Slice 3 (streaming call site): frozen streaming dump diff empty vs
  baseline example binary; bad-file isolation fence added.
- Slice 4 (golden fence): literal cross-checked byte-identical against
  probe-dump.py on the same fixture.
- Slice 5 (Pass 2 memo + batch): dump diff empty; resolved_count 1,847
  identical from baseline and new binaries on the same tree; memo fixture
  distinguishes `alpha` (ambiguous→declined) from `Holder::alpha`
  (resolved) — tail-keyed memo would collapse them.
- Slice 6 (CrateIndex): hand-computed map fixture (overlapping prefixes
  foo/foo-utils + two orphan shapes) passes; dump diff empty.
- Slice 7 (re-index fence): vacuity-checked — disabling the refs DELETE
  makes exactly this test fail.

## Where the 35.5× came from

Probe attribution: the write path was ~96% fdatasync-bound (15.74s disk
vs 0.62s tmpfs). The prototype (transaction-per-file only) measured 6.3×;
Pass 2 batching (1,847 autocommit UPDATEs → 1 transaction) and the
syscall-free crate map took the remainder. Final state beats the tmpfs
ceiling because tmpfs still paid per-statement work that prepare_cached
and batching eliminated outright.

## Regression fences now in CI

- `one_commit_per_file_write` (mechanism fence for C4/C10 — catches any
  reintroduced per-row autocommit deterministically)
- `failed_file_write_leaves_no_rows` (C5)
- `apply_resolutions_batches_in_one_commit` + `_empty_is_a_no_op` (C7)
- `memo_preserves_per_name_outcomes` (C6)
- `fast_crate_map_matches_expected` (C8)
- `idxperf_golden::batch_content_matches_golden_rows` (C1/C3)
- `idxperf_golden::streaming_content_matches_golden_rows_at_both_batch_sizes` (C2/C7)
- `idxperf_golden::reindex_without_rebuild_equals_fresh_rebuild` (C9)
- `bad_file_in_batch_is_isolated` (streaming failure isolation)

C10/C11 timing gates are manual per approved design (decisions log);
the commit-count fence is their deterministic stand-in.

## Criterion comparison (C11) — PASS

**Indexing suite**: improvements across the board — full_index −83.4% to
−94.6% (modules/1 → modules/20; the bigger the workspace, the bigger the
win, consistent with per-row-commit elimination), indexing_phases −80.7%
to −94.4%. Three cases statistically unchanged (worst point estimate
+1.97%, p > 0.05).

**Queries suite** (touches no changed code; read-only SQL on the same
schema): all point estimates within ±7%. Criterion flags four cases as
statistically regressed — get_callers/10 +6.98% (upper CI +10.9%),
get_callers/50 +2.92%, two impact cases +2.90%/+6.34% — and two cases as
statistically improved (−2.10%, −2.80%). The bidirectional drift on
µs-scale cases with unchanged code is binary-layout/measurement noise,
not a systematic regression; no point estimate exceeds the 10% gate.
Honesty note: one case's CI upper bound (10.9%) grazes the gate; its
point estimate (6.98%) does not.

## Deferrals (tracker-verified)

- tethys-q8qw: incremental update()
- tethys-rex0: Pass 2 in-memory name→symbols multimap (layer c)
- tethys-8ya3: batch file_deps inserts
