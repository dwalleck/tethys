# tethys-9z7i slice 2 design: strategy through every write path (2026-07-04)

Per ADR-0003 (merged, PR #12) and the probe facts in `findings.md`. The
design may not contradict the probe; every fact below that shapes a claim
cites one.

## Architecture

- **Schema**: `refs.strategy TEXT` (nullable) joins the CREATE TABLE for
  fresh DBs. NO migration (approved Option C — the index is a disposable
  derived cache and tethys has no users yet): `Index::open` performs a
  cheap column-presence check (`PRAGMA table_info(refs)`) and returns a
  clear error on a pre-column DB — "index schema is outdated; run
  `tethys index --rebuild`". This delivers tethys-xvlw's last unmet AC
  (clear feedback on incompatible schema); xvlw closes with this slice.
  No index on the column (the query surface is tethys-9z7i slice 3's
  scope; it measures before adding one). Follows the
  never-written-nullable precedent (probe fact 7: `end_line`/`end_column`).
- **NULL semantics (approved)**: `strategy IS NULL ⇔ symbol_id IS NULL`
  holds BY CONSTRUCTION on every DB the new code accepts — NULL means
  unresolved, exactly as ADR-0003 states. No sentinel values; the enum
  stays the nine real arms.
- **Types**: `ResolutionStrategy` enum in `src/types.rs` (language-neutral
  — the seam is untouched; `ModuleResolver` still never sees the DB), with
  `as_str()` emitting the ADR's snake_case spellings.
- **Threading**:
  - Pass 1 (`src/db/files.rs` INSERT): rows bound by the same-file maps
    (incl. the macro map) stamp `same_file`; unresolved rows stay NULL.
  - Pass 2 (`src/resolve.rs`): `try_resolve_reference` returns
    `Option<(SymbolId, ResolutionStrategy)>`; each arm tags itself
    (`explicit_import`, `glob_import`, `import_union`,
    `qualified_module_fallback`); `fallback_symbol_search` returns which
    sub-path fired (`qualified_exact` / `same_crate` /
    `unique_workspace`). The per-file memo widens to cache the tuple
    (probe fact 5 — the memo fans one resolution out to duplicate refs);
    `resolutions` becomes `(ref_id, symbol_id, strategy)`.
  - Pass 3 LSP: `Index::resolve_reference` gains the strategy parameter
    (`lsp`), still through the ONE widened `RESOLVE_REFERENCE_SQL` —
    the seam widens, never forks.
  - The `#[cfg(test)]` `insert_reference` fixture helper gains the field.
- **Golden fences**: `tests/idxperf_golden.rs` dumps refs via an explicit
  column list (verified at design time), so the schema change alone
  changes nothing; the dump SELECT and expected literals are DELIBERATELY
  extended with `strategy`, which is also the epic's "new fence pins
  strategy values on a fixture crate" AC. Both batch and streaming
  variants inherit it.

## Input shapes

- Write shapes: Pass-1-resolved insert, Pass-1-unresolved insert, each
  Pass-2 arm (7 labels across 5 arms incl. the fallback's 3 sub-paths),
  Pass-3 LSP single-row, test-only helper (mechanical).
- Ref kinds: call/type/construct (C3 fixture), macro (memo bypass, C5),
  reexport (v1w8 rows, C9).
- DB states: fresh (column present); pre-column opened (clear error
  naming --rebuild); pre-column with `--rebuild` (fresh via reset()).
- Row states: NULL (unresolved), arm values (resolved) — nothing else.
- Memo states: miss, hit (fan-out), negative-cache (None cached — no
  strategy involved), macro bypass.
- Modes: batch and streaming (same atomic write path; both golden
  variants).
- Multi-round Pass 2 (pending-dependency retries): same mechanics; C1's
  fixture includes a forward reference so at least two rounds run.

## Subtractive sweep

Purely additive: a nullable column, wider tuples, one extra bind on the
existing UPDATE — no lock, guard, ordering, or uniqueness property is
removed or relaxed.

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| 1 | Fresh index: `strategy IS NULL ⇔ symbol_id IS NULL`, both directions (fixture includes a forward ref so multi-round Pass 2 runs) | index fixture; count violations | raw SQL counts (schema-level, not analysis code) | 15m | pending | `strategy_null_iff_unresolved`. Buggy: any arm or the LSP path forgetting the bind → resolved-with-NULL rows |
| 2 | Every Pass-1 bind stamps `same_file`, and count(strategy='same_file') equals the same-file spatial JOIN count | self-index; compare label count vs `refs r JOIN symbols s ON r.symbol_id=s.id AND r.file_id=s.file_id` count | spatial join (independent of the label) — the probe's exact reconciliation (1564 == 1564), now in-DB | 10m | pending | `same_file_label_matches_spatial_join` (fixture-scale). Buggy: Pass 2 stamping same_file, or Pass 1 stamping a wrong label |
| 3 | Each Pass-2 arm stamps its own label (7 labels; fixture fires every arm incl. C# import_union) | crafted mixed fixture; per-ref label asserts | RUST_LOG trace events name the arm per (file,name) — independent log stream | 45m | pending | `every_arm_stamps_its_label`, per-arm asserts. Buggy: swapped labels; fallback sub-paths collapsed to one label |
| 4 | Memo fan-out: N same-name refs in one file share the first resolution's strategy | fixture: 3 calls to one imported fn | trace shows ONE arm event; DB shows 3 stamped rows | 15m | pending | `memo_fans_strategy_to_duplicates`. Buggy: memo caches SymbolId only → duplicates NULL |
| 5 | Macro refs (memo bypass) still stamp | fixture: `write!()` macro + `write()` fn same file | DB rows per kind vs trace | 15m | pending | `macro_bypass_stamps_strategy`. Buggy: bypass branch misses the tuple |
| 6 | LSP single-row path stamps `lsp` through the widened seam | db-level test via fixture helper calling `resolve_reference(ref, sym, Lsp)` | SQL readback | 10m | pending | `lsp_path_stamps_strategy`. Buggy: single-row SQL forked from batch and missed the column |
| 7 | Opening a pre-column DB fails with a clear error naming `--rebuild`; a fresh DB opens fine | schema_tests: hand-build a pre-column DB via raw SQL, open → expect the error; open a fresh DB → ok | error text + `PRAGMA table_info` via raw sqlite | 15m | pending | `outdated_schema_open_errors_clearly` + `fresh_db_has_strategy_column`. Buggy: check inverted (fresh DB rejected); raw 'no such column' leaking instead of the guidance |
| 8 | `--rebuild` on a pre-column DB yields fresh schema | probe1.sh(c) canary — RAN, PASSED | sqlite PRAGMA before/after | 5m | **passed** (probe) | `open_fresh_db_has_strategy_column` (schema_tests) + existing reset tests |
| 9 | Reexport-kind refs carry strategy when resolved | fixture with `pub use inner::item` (unique name) | SQL readback + trace arm event | 10m | pending | `reexport_refs_carry_strategy`. Buggy: kind-filtered stamping |
| 10 | Existing analysis outputs are byte-identical (strategy invisible in slice 2) | old-vs-new binary on the golden fixture: deprecated-callers + visibility-tightening `--json` diff | byte diff (mechanical) | 20m | pending | the existing CLI fences that pin exact outputs (deprecated_callers, visibility tests) keep passing unmodified |
| 11 | Golden dump extension: post-change dump diff vs regolded expectations = strategy fields ONLY | extend dump SELECT + literals; diff old-vs-new dump line-by-line | canonical dump text diff (existing golden machinery, both batch + streaming variants) | 30m | pending | the extended `idxperf_golden` tests themselves — this IS the epic's "fence pins strategy values" AC |
| 12 | `refs_named` view shape is untouched | read the view definition | schema.rs source text — explicit column list | 5m | **passed** (design time) | existing `refs_named` tests + schema_tests view assert |
| 13 | Indexing wall-clock delta ≤ 5% (one TEXT bind per resolution + one PRAGMA at open) | hyperfine self-index, old vs new binary, ≥10 runs | hyperfine stats (external tool) | 20m | pending | **manual** (one-shot measurement recorded in `.tethys-9z7i/perf-slice2.md`; `benches/indexing.rs` exists for deeper regressions) — requires approval at the design pause |

Cheapest falsifiers already run: #12 (view column list — passed), #8
(probe1.sh canary — passed), plus #7's ALTER mechanics (probe1.sh(b)).

## Negative space

1. **No query surface**: no band view, no CLI exposure — tethys-9z7i
   slice 3 (in-epic).
2. **No consumer changes**: dead-code suppression is tethys-9z7i slice 4,
   itself deferred to the dead-code work as an acceptance criterion.
3. **Does NOT fix the phantom class**: tethys-53iv / tethys-msn0 /
   tethys-3i35 stay open; this labels their bindings (`unique_workspace`
   / `qualified_module_fallback` land speculative when slice 3 bands
   them).
4. **No general schema-version framework** (settled, approved): the
   index is a disposable derived cache with no compatibility promise and
   no users; a column-presence check is the entire requirement, and it
   completes tethys-xvlw's AC list — xvlw closes with this slice's PR.
   Index compatibility, if ever promised, is a new product decision.
5. **No strategy column on call_edges**: joins through refs per ADR-0003.
6. **No demote/un-resolve handling**: no production path un-resolves a
   ref today (per-file DELETE+reinsert re-stamps naturally); the future
   incremental design owns that lifecycle — tethys-q8qw (verified open).

## Decisions (approved at the design pause, 2026-07-04)

1. **Option C — no migration**: column-presence check + clear
   run-`--rebuild` error; no ALTER, no backfill, no sentinel (user
   rationale: no current users; the index is a disposable cache).
2. **Claim 13's `manual` fence approved** (one-shot hyperfine, recorded
   in `.tethys-9z7i/perf-slice2.md`).
3. **`ResolutionStrategy` in `src/types.rs`** (domain home, re-exported).
