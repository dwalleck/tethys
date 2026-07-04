# tethys-9z7i slice 2 prior art (tracker search, 2026-07-04)

- **tethys-xvlw** (open, P2) — "--rebuild fails on schema changes".
  Adjudicated by probe1.sh: the rebuild path is FIXED (reset() deletes the
  file; canary test), the residual is friendly feedback on non-rebuild
  opens of old-schema DBs. Issue description updated with the evidence.
  Slice 2's migration decision references it.
- **tethys-6rlu** (closed) — resolution NULLs reference_name; the
  refs_named view. Slice 3's band view is its sibling; the subtractive
  sweep from 6rlu is why the epic routes this slice through the loop.
- **tethys-q8qw** (open) — demote-and-rerun incremental is lossy
  (reference_name unrecoverable). The strategy column is new state with
  the same lifecycle: a future demote must null strategy alongside
  symbol_id (design note, not this slice's fix).
- **tethys-53iv / tethys-msn0 / tethys-3i35** (open) — the phantom class
  the strategy labels; explicitly NOT fixed by this epic.
- **tethys-8ya3** (open) — batch file_deps inserts; unrelated write-path
  perf, no overlap with the refs column.
