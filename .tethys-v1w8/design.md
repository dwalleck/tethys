# Design: re-export references (tethys-v1w8)

Status: APPROVED 2026-07-01 (user proceeded to budgeted-plan)
Probe/oracle: `.tethys-v1w8/probe.py`, findings in `probe-findings.md` (agree, 80/80)

## Purpose

A `pub use` site produces no `refs` row today, so a symbol consumed only through
its re-export has zero inbound references — a false positive for dead-code
(tethys-dvsw), a false candidate for visibility tightening (tethys-xoxq), and an
undercount for hotspots (tethys-7p54). Probe evidence: 9 of tethys's own public
re-exported symbols currently report zero inbound refs.

## Architecture (chosen: extractor-emitted refs)

The Rust extractor's `use_declaration` handling (which already computes
`is_reexport` and per-name leaves for the imports path) additionally emits one
`ExtractedReference` per non-glob leaf name when `is_reexport` is true, with a
new ref kind `reexport`, positioned at the declaration site. Pass 2 resolves
these exactly like a bare usage of an imported name — the import row for the
same name/file already exists, so resolution reuses the explicit-import
machinery unchanged.

Rejected alternative: synthesize refs in Pass 2 from stored imports. Killed by
probe finding 1 — the imports table persists neither `is_reexport` nor the line
number, so this path requires a schema migration for strictly less reuse.

Why consumers stay safe by construction (probe findings 4): module-level use
declarations have no enclosing symbol, so re-export refs carry
`in_symbol_id = NULL`; both `populate_call_edges` (`WHERE in_symbol_id IS NOT
NULL`) and panic-points (JOIN on `in_symbol_id`) are structurally blind to them.
Dead-code reads `refs` directly (PRD decision), so it sees them.

## Input shapes

| # | Shape | Covered by claim |
|---|---|---|
| S1 | `pub use m::Name;` (bare) | C1, C2 |
| S2 | `pub(crate) / pub(super) / pub(in p) use` | C1 |
| S3 | `pub use m::Name as Alias;` | C3 |
| S4 | `pub use m::{A, B};` incl. multi-line | C1 |
| S5 | nested group `pub use m::{sub::{C}};` | C4 |
| S6 | glob `pub use m::*;` | C6 (deferred: tethys-pv7w) |
| S7 | external target `pub use serde::Serialize;` | C5 |
| S8 | module target `pub use crate::db;` | C6 (deferred: tethys-pv7w) |
| S9 | path prefixes `self:: / super:: / crate::` | C7 |
| S10 | same name re-exported in two files | C1 |
| S11 | C# | out of scope: C# has no symbol re-export construct (type forwarders are assembly-level, not source-level) |
| S12 | target resolves vs. doesn't | C2 / C5 |

Subtractive sweep (step 2b): the change is additive — it removes no lock,
guard, ordering, or uniqueness property; it inserts new rows of a new kind.
The additive rows are consumer-visible, so consumer invariants are covered as
claims C8–C12 rather than as a removed-invariant sweep.

## Claims

1. **C1** Each non-glob leaf name in a module-level re-export declaration yields exactly one `refs` row with `kind='reexport'` at the declaration site (same name in two files ⇒ two rows, one per file).
2. **C2** A re-export ref whose target is an in-crate symbol resolves in Pass 2 to the same `symbol_id` a bare body-usage of that imported name resolves to.
3. **C3** `pub use m::B as C` (top-level alias) records the ref under original name `B` and resolves to `B`'s symbol; the alias stays on the imports row only. AMENDED during slice 2: group-member aliases (`pub use m::{A, B as C}`) are dropped by current use-list parsing before the emitter sees them — pre-existing extraction gap filed as tethys-rylk; parity preserved, not fixed here.
4. **C4** Nested-group members behave with parity to current import parsing — inheriting tethys-pdea's known intermediate-segment drop, not fixing or worsening it.
5. **C5** A re-export of a non-workspace name stores an unresolved ref (`symbol_id NULL`, `reference_name` populated), per the existing unresolved-ref convention.
6. **C6** Glob (S6) re-exports produce no refs in this change; module (S8) re-exports — AMENDED during slice 2: syntactically indistinguishable from item re-exports at parse time, so they emit a ref resolved like any other — it stays UNRESOLVED (symbol_id NULL) when the target name has no in-crate symbol, but a single-segment module re-export (`pub use m2;`) binds the same-file module declaration symbol (kind='module'), as pinned by `glob_reexport_emits_nothing_and_module_reexport_binds_module`. That module symbol legitimately gains one inbound ref; call_edges and panic-points stay blind either way (the ref carries in_symbol_id NULL). Full glob/module semantics deferred to tethys-pv7w (filed, blocks-linked).
7. **C7** `self::`/`super::`/`crate::` prefixed re-exports resolve with parity to plain imports of the same path — inheriting (not extending) known resolver bugs tethys-nkjd (super::) and tethys-xzdr (bare crate).
8. **C8** Re-export refs carry `in_symbol_id NULL` and produce zero `call_edges` rows.
9. **C9** Re-export refs produce zero panic-point rows even when the re-exported name is `unwrap` or `expect`.
10. **C10** `file_deps` gains the previously-missing edge for a re-export-only import, and re-running does not duplicate existing edges. AMENDED during slice 6: delivered by slice 2's emission itself — the extracted reexport ref's name feeds compute_dependencies' usage-corroboration set, so resolvable import paths gain the edge with no dep-computation change. Two residuals pinned as current behavior with tracker citations: bare single-segment paths still decline (tethys-z9mr — this, not corroboration, explains probe F4's missing lib.rs→unused_imports.rs edge), and aliased re-export-only imports fail the alias-keyed lookup (tethys-sp24). Streaming parity verified by pipeline order: compute_all_dependencies runs before Pass 2 nulls reference_name.
11. **C11** `unused-imports` self-index output is unchanged pre/post.
12. **C12** A fixture symbol whose only reference is its re-export has exactly 1 inbound ref post-change (the dead-code false positive dies).
13. **C13** Re-indexing the same workspace twice yields identical refs / file_deps / call_edges counts.
14. **C14** Counts of ref kinds other than `reexport` on the tethys self-index are unchanged pre/post (the idxperf golden fixture is updated once, deliberately, for the new kind).

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| F1 | Architecture premise: imports table lacks `is_reexport`+line | read schema + INSERT | schema SQL vs. extraction struct | 5m | **passed** (probe) | n/a — design-time premise |
| F2 | Gap exists: 0 refs at real pub-use sites | probe Q1b on fresh self-index | regex oracle proves sites exist | 5m | **passed** (probe) | C12 fixture becomes the fence |
| F3 | Inventory parity: extractor sees every textual pub-use | probe Q1a | regex oracle, string-stripped | 5m | **passed** (probe, 80/80) | `marks_pub_use_as_reexport` (exists) |
| F4 | C10 premise: re-export-only edge missing today | SQL on fresh self-index | `lib.rs→unused_imports.rs` absent, 3 siblings present | 5m | **passed** | new test `reexport_only_import_creates_file_dep` |
| F5 | C8/C9 mechanism: consumers keyed on `in_symbol_id` | read population SQL | `call_edges.rs:55`, `panic_points.rs:51,103` | 5m | **passed** | new tests `reexport_refs_create_no_call_edges`, `reexport_of_expect_creates_no_panic_point` |
| F6 | C1 | fixture with S1,S2,S4,S10; count rows per name | hand-known fixture counts | impl | pending | `reexport_ref_per_leaf_name` |
| F7 | C2 | fixture: same symbol re-exported AND body-used; compare symbol_id | equality of two independently produced rows | impl | pending | `reexport_resolves_like_bare_usage` |
| F8 | C3 | fixture with alias | symbol_id = B's id; name = 'B' | impl | pending | `aliased_reexport_targets_original` |
| F9 | C4 | fixture with nested group | parity vs imports table rows for same stmt | impl | pending | `nested_group_parity_with_imports` |
| F10 | C5 | fixture `pub use serde::Serialize` | symbol_id NULL + reference_name set | impl | pending | `external_reexport_stored_unresolved` |
| F11 | C6 | fixture glob + module re-export | zero `reexport` refs at those sites | impl | pending | `glob_and_module_reexports_emit_no_refs_v1` |
| F12 | C7 | fixture self::/super::/crate:: | resolution outcome equals plain-import control case | impl | pending | `path_prefix_reexports_parity` |
| F13 | C8 | fixture; count call_edges pre/post | SQL count delta = 0 | impl | pending | `reexport_refs_create_no_call_edges` |
| F14 | C9 | fixture re-exporting fn named `expect` | panic-points count = 0 | impl | pending | `reexport_of_expect_creates_no_panic_point` |
| F15 | C10 | fixture: re-export-only import | file_deps edge exists; count stable across re-run | impl | pending | `reexport_only_import_creates_file_dep` |
| F16 | C11 | self-index unused-imports pre/post diff | byte-identical output | 15m | pending | `unused_imports_self_index_unchanged` (self-oracle pattern) |
| F17 | C12 | fixture: symbol only referenced via re-export | inbound refs = 1 | impl | pending | `reexport_only_symbol_not_zero_ref` |
| F18 | C13 | index twice, diff counts | SQL counts equal | impl | pending | `reindex_idempotent_with_reexport_refs` |
| F19 | C14 | self-index kind histogram pre/post | non-reexport counts equal | 15m | pending | updated idxperf golden fixture |

Cheapest falsifiers (F1–F5) have **run and passed** before this design is
presented — including one that killed a draft claim ("file_deps unchanged"
died to F4 and was rewritten as C10).

### Verification results (checkpointed-build, 2026-07-01)

All pending falsifiers ran and passed; fences live in `tests/reexport_refs.rs`
(7 tests), `src/languages/rust.rs` (emission stress test),
`src/types.rs` (round-trip), and `tests/idxperf_golden.rs` (both arms).
Full suite: 748 passed / 0 failed, clippy clean.

- F6–F8, F10–F12, F17: fence tests as named (collision fence renamed
  `reexport_resolves_like_bare_usage_despite_same_named_decoy`, pinning BOTH
  parity arms; TRIPWIREs cite tethys-z9mr). F9 (C4 nested-group parity) is not
  a standalone test — the `i::J` leaf of
  `reexport_refs_one_per_named_leaf_with_parity_gaps` asserts it by shared code
  path with `parse_use_declaration`.
- F13/F14: `reexport_refs_stay_out_of_call_edges_and_panic_points`.
- F15: `reexport_only_import_creates_file_dep` +
  `bare_segment_reexport_only_import_still_lacks_file_dep` (C10 as amended).
- F16 (C11): self-oracle Definite=0 post-change; structural immunity for
  valid Rust (a reexport ref can only mark an import used via a duplicate
  binding, which does not compile); existing fence
  `pub_use_reexport_is_never_reported` green. Worktree pre/post diff waived
  on that argument.
- F19 (C14): kind histogram on the frozen self-index copy EXACT vs baseline
  (call 6007, construct 227, macro 1669, type 2648) + reexport 80; golden
  diff review showed only the one reexport row.
- Wall budget: self-index 353 ms ≤ 500 ms budget (443 ms pre-change baseline).
- Final probe: Q1a 80/80, Q1b one ref per name at every site, Q2 = [] (all
  9 baseline dead-code false positives eliminated).

New issues filed during the build: tethys-rylk (group-alias members dropped
by use-list parsing), tethys-z9mr (single-segment relative path decline),
tethys-sp24 (aliased re-export corroboration gap). Deferred: tethys-pv7w
(glob/module semantics).

Non-vacuity (named buggy implementations per fence): per-statement instead of
per-name emission (F6); qualified reference_name that never resolves (F7);
recording the alias name (F8); accidental nested-group "fix" breaking parity
(F9); skipping external targets entirely (F10); emitting a `*` ref (F11);
bespoke path resolution diverging from ModuleResolver (F12); attributing a
file-level pseudo-symbol to `in_symbol_id` (F13, F14); refs filtered out of
`compute_dependencies`' refs_set by kind (F15); treating reexport refs as body
usage in unused-imports (F16); any emission failure (F17); non-deduped
re-insertion on re-index, the lcb6 family (F18); perturbing sibling extractor
arms (F19).

## Negative space (deliberately not doing)

1. No glob or module re-export refs — tethys-pv7w (blocks-linked, filed with probe evidence).
2. No fixes to existing resolver bugs (tethys-pdea nested groups, tethys-nkjd super::, tethys-xzdr bare-crate, tethys-3i35 crate:: calls) — parity only; their fixes flow through to re-export refs for free when they land.
3. No schema migration — `is_reexport` and line remain unpersisted on imports; the ref row itself carries the site.
4. No C# changes — the language has no source-level symbol re-export.
5. No changes to unused-imports semantics or to the refs_named view.
6. No dead-code analysis — this only removes one of its false-positive classes (tethys-dvsw remains blocked on tethys-ygjx and tethys-j2r1 as well).

## Tracker citations (verified this session)

tethys-pv7w (filed here; blocks-linked to v1w8) · tethys-pdea · tethys-nkjd ·
tethys-xzdr · tethys-3i35 · tethys-ygjx · tethys-j2r1 · tethys-dvsw ·
tethys-xoxq · tethys-7p54 · tethys-l6nt
