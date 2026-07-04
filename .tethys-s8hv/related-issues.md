# Related issues — tethys-y3bx (untested-code analysis)

Prior-art search (rivets, 2026-07-04). `rivets list --search` is a no-op flag;
used `rivets list -n 500 | grep`.

- **tethys-3gey** (CLOSED) — Reachability analysis (forward/backward). The
  existing machinery y3bx reuses: `Tethys::bfs_reachable` /
  `get_forward_reachable` / `get_backward_reachable` (src/lib.rs:544+),
  backed by `get_callees`/`get_callers` over the **call_edges** table.
- **tethys-o6x7** (CLOSED) — Test topology mapping. The feature that
  INTRODUCED `is_test`. Its description claims "✅ Index all symbols including
  test functions" — the intent the probe falsifies (see findings.md).
- **tethys-dvsw** (open, P3) — Dead-code finder; the roadmap stage AFTER
  untested-code. Same reverse-traversal machinery.
- **tethys-7p54** (open, P3) — Hotspots (connectivity ranking).
- **tethys-ygjx** (open) — fn-as-value refs gap. y3bx documents this as a
  known, non-blocking limitation.
- **tethys-l6nt** (PRD, parent) — Act 1 code-health analyzer roadmap.

## New finding (unfiled at probe time)
call_edges is a lossy subset of refs (schema.rs:122: "both in_symbol_id and
symbol_id resolved"). SEPARATELY: inline module bodies are not indexed at all
(root cause below) — the dominant blocker, not the call_edges/refs distinction.
