# idxperf design: behavior-preserving index performance (v1)

Inputs: `.idxperf/spec.md` (signed), `.idxperf/probe-findings.md`.
Probe constraints this design may not contradict: write path is ~96%
fdatasync-bound; canonical content is deterministic in both modes; Pass 2
performs no refs reads after `get_unresolved_references`.

## Architecture

**Change A — one transaction per file write.** A new DB-layer method
`Index::index_parsed_file_atomic(path, language, mtime_ns, size_bytes,
content_hash, symbols, references, imports) -> (FileId, Vec<SymbolId>, usize)`
absorbs the whole per-file write: file upsert (including the refs/symbols/
imports DELETEs on the update path), symbol+attribute inserts, same-file
reference resolution (the name/span maps move into the DB layer), reference
inserts, import inserts — all under one transaction with `prepare_cached`
statements. Both call sites (`Tethys::write_parsed_file`,
`BatchWriter::write_single_file`) convert to it, which also deletes the
current duplicated store_references/store_imports logic in batch_writer.rs.
`compute_dependencies` (module resolution, needs Tethys state) stays outside.

**Change B — Pass 2 memoization + batched updates.**
`resolve_refs_for_file` gains a per-file `HashMap<String, Option<SymbolId>>`
memo keyed by `reference_name` (positive AND negative outcomes cached; the
probe established outcomes depend only on (file imports, reference_name)).
The loop stops calling `resolve_reference` per ref; it collects
`(ref_id, symbol_id)` pairs, and `resolve_cross_file_references` applies them
via a new `Index::apply_resolutions(&[(RefId, SymbolId)])` in one transaction
after the scan — sidestepping the non-reentrant connection mutex entirely.
Trace-level logging per ref is preserved.

**Change C — fast crate map.** Extract the ancestor-walk file→crate
assignment from `run_architecture_phase` into a shared helper
(`Tethys::file_crate_assignments` or similar); `build_file_crate_map` uses it
plus the existing orphan-pseudo-crate naming, eliminating the per-file
`canonicalize()` + O(crates) scan.

## Input shapes

- **ParsedFileData**: symbols ∅ / 1 / N / duplicate names; refs ∅ /
  same-file-resolved / unresolved / top-level (`in_symbol_id NULL`) /
  duplicates; imports ∅ / explicit / glob / aliased / re-export flag;
  attributes ∅ / N; file NEW (insert path) vs EXISTING (update path).
- **Write modes**: batch; streaming with batch_size 1, =files, >files.
- **Pass 2 ref sets per file**: ∅ (file skipped); repeated reference_name
  (memo hit); distinct names; qualified vs simple; resolving vs declining
  names; missing file record (warn path); Rust vs C# (namespace arm).
- **Crate map files**: inside a crate; inside nested crates with overlapping
  prefixes; orphan in a subdirectory; orphan at workspace root; empty crates
  list (all orphans); absolute-path file row (defensive branch).
- **Failure shapes**: constraint violation mid-file (rollback); one bad file
  in a streaming batch (others survive).

Out-of-scope shapes: non-UTF-8 files (rejected upstream at parse, unchanged);
LSP Pass 3 (untouched).

## Claims and falsifiers

C1. Batch-mode canonical dump of the frozen tethys tree is byte-identical
    between baseline and changed binaries.
C2. Streaming-mode canonical dump likewise (per-mode comparison).
C3. C# fixture (`.idxperf/fixtures/csharp-ws`) canonical dump likewise.
C4. During the per-file write phase, SQLite commits exactly once per file
    (no per-row autocommits).
C5. A constraint violation mid-file rolls back the entire file: zero rows
    in files/symbols/refs/imports/attributes for that path.
C6. Memoized Pass 2 resolves the same set: `resolved_count` on the frozen
    tree equals baseline (1,847) and dump rows are identical; within a file,
    refs sharing a name get identical outcomes.
C7. Batched updates commit once per resolve run and Pass 3/call-edges still
    observe them (ordering preserved).
C8. New `build_file_crate_map` equals the old implementation's output on a
    workspace with nested crates (overlapping name prefixes) and orphan
    files in subdirs and at root.
C9. Re-index without rebuild over an unchanged tree yields a dump identical
    to a fresh rebuild (refs-deletion fix keeps holding inside the new tx).
C10. Self-index median-of-5 ≥2.0× faster than 15.740s baseline.
C11. No criterion case (indexing or queries suite) >10% slower than the
     `pre-idxperf` baseline.

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| 1 | batch dump identical | frozen tree, both binaries, diff | probe-dump.py (external python/sqlite3) | 5m | **prototype slice PASSED** (19,770 rows, diff empty) | golden-content integration test `idxperf_golden::batch_content_frozen_fixture` (small fixture, exact expected canonical rows) |
| 2 | streaming dump identical | same via `idxperf_stream` example | probe-dump.py | 5m | pending | same golden test, streaming arm |
| 3 | C# dump identical | fixtures/csharp-ws, both binaries | probe-dump.py | 5m | pending | golden test, C# arm |
| 4 | 1 commit per file | rusqlite `commit_hook` counter during fixture index | SQLite engine hook (independent of tethys code) | 30m | pending | unit test `one_commit_per_file_write` — fails if any per-row autocommit returns (buggy impl: refs inserted via old `insert_reference`) |
| 5 | mid-file rollback | symbol with dangling `parent_symbol_id` → FK error → SELECT counts | sqlite3 row counts | 20m | pending | unit test `failed_file_write_leaves_no_rows` (buggy impl: refs/imports outside the tx survive) |
| 6 | memo preserves outcomes | frozen-tree resolved_count + dump; fixture with 3 same-name refs | log line + dump + per-test asserts | 15m | pending | integration test `memo_same_name_refs_same_outcome` (buggy impl: memo keyed across files → wrong target; negative outcome not cached → no failure but C4's count catches cascade re-runs) |
| 7 | batched updates ordered | dump equality + call_edges count match | probe-dump.py edge rows | 5m | pending | covered by golden test edge rows (buggy impl: commit after populate_call_edges → zero edges in dump) |
| 8 | crate map equal | unit fixture: nested `foo`/`foo-utils` + orphans at root and subdir | hand-computed expected map | 20m | pending | unit test `fast_crate_map_matches_slow_path` keeps OLD logic inline as the oracle (buggy impl: first-prefix match; orphan-root naming) |
| 9 | re-index = rebuild | index twice no-rebuild vs rebuild, diff dumps | probe-dump.py | 10m | pending | extend `reindex_does_not_accumulate_refs` to assert full-table counts, not just refs |
| 10 | ≥2.0× self-index | hyperfine 5 runs vs 15.740s | hyperfine (recorded baseline json) | 10m | pending | **manual** — needs user approval; mechanism fence is #4 (commit count), which deterministically blocks reintroduction of per-row autocommit |
| 11 | no bench regression | criterion vs `pre-idxperf` baseline | criterion compare | 30m | pending | **manual** — same approval; queries suite touches no changed code paths, indexing suite guarded by #4 |

Cheapest falsifier (C1 slice, prototype patch `.idxperf/prototype.patch`):
**RUN AND PASSED** — 15.74s → 2.508s (6.3×) with byte-identical dump on the
frozen tree. Prediction was ≥3×; survived.

## Negative space

This change deliberately does NOT:
1. Implement incremental `update()` — tracked at **tethys-q8qw** (verified).
2. Replace Pass 2 SELECTs with an in-memory name→symbols multimap (layer c)
   — tracked at **tethys-rex0** (filed at sign-off with requester approval).
3. Batch the ~250 `insert_file_dependency` autocommits in
   compute_dependencies/resolve_pending (~0.2s) — tracked at **tethys-8ya3**.
4. Touch resolution strategies, order, or outcomes — Pass 2 logic is
   restructured (memo + deferred apply) but every strategy call is verbatim.
5. Change SQLite pragmas (synchronous/journal_mode untouched).
6. Change the public CLI surface.

## Risks

- `prepare_cached` on a Transaction caches on the underlying Connection —
  statements survive across files (that's the point) but must not outlive
  schema changes; `Index::reset` reopens the connection, invalidating the
  cache naturally.
- The same-file resolution maps move into the DB layer; the OwnedSymbolData→
  SymbolData conversion and module_path computation stay in the callers —
  watch that batch mode's module_path behavior is preserved exactly
  (streaming leaves it empty; C1/C2 dumps verify).
- Memo key is the full `reference_name` string (qualified names included) —
  NOT the simple name; collapsing distinct qualified names would change
  outcomes (C6's fixture has both `foo` and `Bar::foo`).

## Approval

Requester approved the design, the manual regression fences for C10/C11,
and the filing of tethys-rex0 / tethys-8ya3, verbatim: "Yes, approved and
proceed" (2026-06-09). Cheapest falsifier status at approval: PASSED
(6.3×, dump-equal on frozen tree). Criterion `pre-idxperf` baselines: 34
saved across both suites. Hyperfine baseline: 15.740s ± 0.262s.
