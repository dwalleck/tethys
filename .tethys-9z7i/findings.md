# tethys-9z7i slice 2 probe findings (prove-it-prototype, 2026-07-04)

Substrate: the two refs write shapes, schema evolution, and Pass-2
mechanics — probed against the real repo and self-index. Probes:
`probe1.sh` (schema experiments), trace-vs-DB reconciliation (transcript;
counts below).

## Oracle

Two independent mechanisms per question:
- Schema behavior: raw `sqlite3` (PRAGMA table_info, error text) vs the
  tethys binary's own open/rebuild path — agreement on all three
  sub-experiments (probe1.sh output).
- Write-path completeness: per-event trace logs (probe) vs SQL count
  arithmetic on the resulting DB (oracle). Pass-level reconciliation is
  EXACT on the self-index: total resolved 3650 = Pass 1 1564 (== the DB's
  same-file-resolved count, to the digit) + Pass 2 aggregate 2086. No
  unknown write path exists.

## What I learned (one sentence)

Pass 2 resolves once per unique (file, name) through a per-file memo that
caches only `Option<SymbolId>` and fans the outcome out to every duplicate
ref — so the strategy label must ride the memo value and the resolutions
tuple, or duplicates would need re-resolution to learn their provenance.

## Facts for the design

1. **Write shapes = 2 in production** (+1 test-only fixture helper):
   Pass 1 INSERT (`src/db/files.rs:297`) and the unified UPDATE seam
   `RESOLVE_REFERENCE_SQL` (`src/db/references.rs:20`; batch
   `apply_resolutions` + single LSP `resolve_reference`). The `#[cfg(test)]`
   `insert_reference` fixture helper must gain the column too.
2. **No migration machinery exists** (no user_version, no ALTER anywhere).
   Adding `refs.strategy` + an index on it breaks EXISTING DBs exactly as
   tethys-xvlw describes ('no such column' at index creation; SELECTs
   fail). Verified live in probe1.sh(a).
3. **`--rebuild` is already schema-safe**: `Index::reset()` deletes the
   file + sidecars; a canary column vanished across --rebuild. tethys-xvlw
   is fixed-but-not-closed for the rebuild path; its residual (friendly
   feedback on non-rebuild opens of old DBs) is updated on the issue.
4. **Idempotent ALTER works**: `PRAGMA table_info` check + `ALTER TABLE
   refs ADD COLUMN strategy TEXT` on a live DB succeeds; the index then
   creates; existing rows read NULL. BUT: post-migration, OLD RESOLVED
   rows have `symbol_id NOT NULL` + `strategy NULL` — colliding with
   ADR-0003's "NULL means unresolved". The design must pick: backfill
   sentinel (e.g. 'legacy'), define NULL+resolved as "unknown provenance",
   or require rebuild. This is a flagged design decision.
5. **Memoization shape** (`src/resolve.rs:170`): per-FILE
   `HashMap<String, Option<SymbolId>>` keyed by full reference name;
   macro-kind refs bypass it (namespace cross-contamination). Threading:
   memo value widens to carry the strategy; `resolutions` becomes
   `(ref_id, symbol_id, strategy)`.
6. **Arm distribution on the self-index** (unique (file,name) resolutions,
   trace counts): fallback search 415, explicit import 242, qualified
   module fallback 25, glob import 15, LSP 0 (not enabled). The fallback
   arm — which the ADR splits into qualified_exact / same_crate /
   unique_workspace — dominates Pass 2; the speculative band will be a
   minority of edges, which is what its consumers want.
7. **Nullable-column precedent**: refs already carries never-written
   nullable columns (`end_line`, `end_column`) absent from INSERT
   column-lists — adding `strategy` follows an existing pattern.

## Hard-gate checklist

- [x] Probes run against the real codebase (probe1.sh; trace/DB runs)
- [x] Independent oracles produce output (sqlite3 vs binary; SQL vs logs)
- [x] Agreement on a non-trivial slice (exact pass-level reconciliation;
      the 697-vs-2086 apparent drift resolved as the memo model — model
      updated, not papered over)
- [x] Learned something new (memo shape; NULL-semantics collision; xvlw
      staleness split)
