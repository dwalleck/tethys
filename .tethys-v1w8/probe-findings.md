# tethys-v1w8 probe findings (prove-it-prototype, 2026-07-01)

## Probe

`probe.py` — SQL over a freshly built index of a copy of tethys's own source
(`Cargo.toml` + `src/`, indexed with `tethys index -w <copy>`; DB at
`<copy>/.rivets/index/tethys.db`). Never the ambient repo index (6rlu lesson).

## Oracle

Textual regex scan of the same source tree for `pub use` statements (string
literals stripped) — a mechanism independent of tree-sitter and of the indexer.

Agreement, item by item:

| Slice | Probe (SQL) | Oracle (regex) | Agree |
|---|---|---|---|
| Re-export inventory | imports rows per (file, name) | 80 names / 18 sites | 80/80 ✓ |
| Refs at pub-use sites | 0 rows at all 18 sites | sites exist (non-vacuous) | ✓ gap confirmed |
| Zero-inbound-ref re-exported symbols | 9 symbols | public API by inspection | ✓ headline |

One probe/oracle disagreement occurred and was resolved: the oracle's first
version counted `pub use` text inside a **test string literal**
(`rust.rs:1743 marks_pub_use_as_reexport`). The indexer was right; the oracle
was fixed by stripping string literals (verified by direct inspection first).

## What I learned (non-obvious before probing)

1. **The imports table persists neither `is_reexport` nor the line number** —
   both are dropped at the DB boundary (`files.rs` INSERT: file_id,
   symbol_name, source_module, alias). A pure Pass-2-from-stored-imports
   implementation is impossible without a schema change.
2. **A second latent gap**: re-export-ONLY imports produce no `file_deps` edge
   (`lib.rs → unused_imports.rs` is missing while `lib.rs → cargo.rs/error.rs/
   types.rs` exist, corroborated by body usage). Emitting refs at re-export
   sites will (correctly) add these missing edges — coupling metrics may shift.
3. **9 of tethys's own public re-exported symbols look dead today**:
   DEFAULT_MAX_DEPTH, FILES_COLUMNS, FileAnalysis, ORPHAN_PSEUDO_CRATE_PREFIX,
   REFS_COLUMNS, SCHEMA, SYMBOLS_COLUMNS, row_to_import, row_to_reference.
4. `populate_call_edges` requires `in_symbol_id IS NOT NULL` (no kind filter);
   `panic_points` joins on `in_symbol_id` too. Module-level use declarations
   have no enclosing symbol, so re-export refs are structurally invisible to
   both consumers.
5. Production shape distribution (tethys itself): bare, `pub(crate)`, grouped
   multi-line — **zero glob or module re-exports** (deferral to tethys-pv7w
   costs nothing on this workspace).
