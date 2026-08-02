# Dogfood run: callers of the renamed hydration functions

2026-08-01, on the PR #38 branch after the review-fix restructure. Backfills
the AGENTS.md "Dogfood tethys" step for this slice's rename
(`get_file_dependencies`/`get_file_dependents` → `get_file_dependency_paths`/
`get_file_dependent_paths`), flagged missing by the pre-merge standards
review. `file_deps.rs` is not in the resolution carve-out, so tethys's own
caller output is trustworthy for this slice.

Fresh index first: `cargo run --quiet -- index` → 114 files, 2770 symbols,
22876 references.

## Precision tier (`--exclude-speculative`)

- `tethys callers Index::get_file_dependency_paths --exclude-speculative` →
  no callers
- `tethys callers Index::get_file_dependent_paths --exclude-speculative` →
  no callers

Empty precision is expected here, not dead code: both call sites are
`self.db.<method>(...)` field-receiver calls, which bind in the speculative
band (see AGENTS.md on tethys-53iv receiver binding).

## Recall tier (speculative included)

- `tethys callers Index::get_file_dependency_paths` → 3 callers:
  `Tethys::get_dependencies` (src/lib.rs) + the two unit fence tests in
  `src/db/file_deps.rs`
- `tethys callers Index::get_file_dependent_paths` → 2 callers:
  `Tethys::get_dependents` (src/lib.rs) + the unit statement fence

## Grep oracle

`grep -rn "get_file_dependency_paths\|get_file_dependent_paths"` over `src/`,
`tests/`, `benches/` (definitions excluded) → exactly `src/lib.rs:341` and
`src/lib.rs:359`. Agrees with the recall tier.

## Old names

- `tethys callers Index::get_file_dependencies` → not found: symbol
- `tethys callers Tethys::file_ids_to_paths` → not found: symbol

No orphaned callers of the removed/renamed symbols anywhere in the index.

## Conclusion

The rename is fully wired: the only production callers are the two `Tethys`
facade methods, matching the caller enumeration in `design.md`. No consumer
was missed.

## Note on test locations

After the pre-merge review, the `n8pu_probe` module referenced by
`findings.md` / `design.md` / `plan.md` was split per the house pattern
(`k_hybrid_filter_tests` ↔ `file_deps_corroboration.rs`): statement-count
fences live in `db::file_deps::hydration_fence_tests` (unit, needs the
live-connection trace hook), and the full-pipeline behavior/oracle/
dangling-row fences live in `tests/file_deps_hydration.rs` (public API).
The historical docs intentionally keep the old module name — they record
the state at probe time.
