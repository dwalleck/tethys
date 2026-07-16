# Triage: 5 issues recovered from a February 2026 tracker snapshot

**Task for a tethys agent**: decide, for each issue below, whether it has been
superseded by later work. These records are from **2026-02-20** — five months
old at recovery time. The codebase has changed substantially since (repo split,
db module restructure, resolver rework), so treat each as a *hypothesis about a
problem*, not a confirmed work item.

## Provenance

Recovered 2026-07-04 from a git stash in the **rivets** repo containing a
2026-02-20 working-tree snapshot of the pre-split shared tracker. These five
issue records were **never committed to git history anywhere** — they existed
only in working trees and survived only in that stash. They carry original
`rivets-*` IDs because they predate the tethys repo split, but their content is
tethys-domain. Most were filed from PR reviews in the *old rivets repo* — PR
numbers in descriptions refer to that repo, **not** tethys PRs.

A companion audit already accounted for 14 sibling records (fixed, migrated, or
subsumed — see appendix). These five had no tracker match and no evidence of a
code fix as of 2026-07-04.

## Triage instructions

For each issue: search the tethys tracker (`rivets list` / `rivets show`,
including closed issues), inspect the current code, and check the PRD
(tethys-l6nt) scope decisions. Then either:

- **File it**: create a tethys issue, copying the fields below and adding
  `recovered-2026-02` to labels plus a provenance line pointing at this doc; or
- **Mark it superseded**: record the evidence (issue ID, commit, or PRD scope
  decision) in the Outcome line below. Do not delete the entry.

Check the "Preliminary evidence" notes first — they are starting points from
the 2026-07-04 recovery, not conclusions.

---

## 1. Document database schema migration strategy (`rivets-nnft`)

- **Type/Priority**: task / P3 · **Labels**: documentation, pr-review · **Created**: 2026-02-06
- **Description**: Document how schema migrations work as index schema evolves. Users need upgrade path.
- **Design**: Document schema version tracking, migration approach, what happens to existing indexes on upgrade.
- **Acceptance**: Migration strategy documented; user-facing upgrade docs; version tracking defined
- **Preliminary evidence (2026-07-04)**: No tracker match for "schema migration" / "migration strategy". Note the rivets tracker has a known related bug from the same era: "--rebuild flag fails on schema changes" — check whether tethys inherited or fixed that path (schema version handling may have changed with the db module restructure).
- **Outcome**: _pending_

## 2. Parallelize staleness metadata checks for large repos (`rivets-hqd1`)

- **Type/Priority**: feature / P3 · **Created**: 2026-02-10
- **Description**: For large repositories, staleness checks could be parallelized using rayon to speed up metadata collection. Currently staleness detection does a single-threaded filesystem walk checking mtime/size for each file. For repos with thousands of files, parallelizing these metadata checks could significantly reduce the time before indexing begins. Suggested in PR #51 review (old rivets repo).
- **Design**: Use rayon's par_iter over the file list to collect filesystem metadata in parallel. The staleness report aggregation would need to handle concurrent results (e.g., collect into a concurrent HashMap or merge thread-local results).
- **Preliminary evidence (2026-07-04)**: `tethys-gkt2` "Implement proper staleness check" is in progress but is about *correctness*, not parallelization — adjacent, not superseding. Verify whether the staleness walk is still single-threaded after gkt2 lands; if gkt2 redesigns the walk, fold this in there instead of filing separately.
- **Outcome**: _pending_

## 3. Add explicit error path tests (`rivets-x015`)

- **Type/Priority**: task / P3 · **Labels**: testing, pr-review · **Created**: 2026-02-06
- **Description**: Add tests for error paths: malformed Rust files, permission denied, non-UTF8, empty files, symlink edge cases.
- **Design**: Write tests verifying parser does not panic on bad input and returns proper Result::Err.
- **Acceptance**: Malformed file tests; permission error tests; edge case file tests; all return Result::Err
- **Preliminary evidence (2026-07-04)**: No tracker match. The test suite has grown a lot since February — check whether `tests/` now covers malformed input, non-UTF8, and permission errors before filing. The PRD's "findings are suppressions, not accusations" error posture makes parser robustness load-bearing, so gaps here matter.
- **Outcome**: _pending_

## 4. Add CI performance regression tracking (`rivets-3cx2`)

- **Type/Priority**: task / P4 · **Labels**: ci, performance, pr-review · **Created**: 2026-02-06
- **Description**: Add CI performance regression tracking to catch indexing slowdowns early.
- **Design**: Use criterion benchmarks with cargo-criterion in CI or hyperfine for e2e timing. Compare against baseline per PR.
- **Acceptance**: Benchmark suite in CI; results compared against baseline; regressions flagged
- **Preliminary evidence (2026-07-04)**: `benches/` and BENCHMARKS.md exist, and the idxperf golden tests are referenced in the PRD — the *benchmark suite* half may be done. The unverified half is CI integration with per-PR baseline comparison. Check `.github/workflows/` before filing; scope down to just the CI half if benchmarks already cover the rest.
- **Outcome**: _pending_

## 5. Move column constants to db/schema.rs (`rivets-xxg0`)

- **Type/Priority**: chore / P4 · **Labels**: refactor, pr-review · **Created**: 2026-02-06
- **Description**: FILES_COLUMNS, SYMBOLS_COLUMNS, REFS_COLUMNS should live in a dedicated db/schema.rs module.
- **Design**: Create db/schema.rs with column constants and schema types. Re-export from db/mod.rs.
- **Acceptance**: Column constants in db/schema.rs; all tests pass
- **Preliminary evidence (2026-07-04)**: `db/schema.rs` does not exist; `FILES_COLUMNS` still lives in `db/mod.rs`. But the db module was restructured into ~10 files since this was filed — the complaint's context has shifted. Likely candidates: fold into an existing tech-debt/polish issue, or discard as stale if the current layout is deliberate.
- **Outcome**: _pending_

---

## Appendix: sibling records already accounted for (do not re-triage)

From the same recovered snapshot, verified 2026-07-04:

| Original ID | Title (short) | Resolution |
|---|---|---|
| rivets-znph | Hardcoded src/ crate-root assumption | Fixed — tethys-m4wt (closed) parses Cargo.toml; remaining `join("src")` hits are test fixtures |
| rivets-fd1i | Unused --depth flag in impact CLI | Fixed — depth is wired through `src/cli/impact.rs` into the queries |
| rivets-sdid | Blanket cast-lint allows in db/mod.rs | Fixed at named site — db/mod.rs clean; residual `languages/*.rs` allows fall under tethys-l8ur polish backlog |
| rivets-9658 | r2d2 connection pooling | Subsumed by tethys-ed9y (open) |
| rivets-4soq | Single transaction for batch indexing | Subsumed by tethys-ed9y (open) |
| rivets-e7fg | Adopt proptest | Substantially done — proptest is a dependency (Cargo.toml) |
| rivets-e7fp | Content hashing for change detection | Migrated — tethys-3l14 |
| rivets-j9bu | Drift-inspired perf/architecture epic | Migrated — tethys-j9bu |

(The remaining six siblings were rivets-domain or disposable test records; handled in the rivets repo.)
