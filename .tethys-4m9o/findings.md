# tethys-4m9o probe findings — 2026-08-01

## Probe

`examples/probe_4m9o.rs` — 36-line facade-only harness: opens the workspace
via `Tethys::new`, calls `get_dependency_chain(from, to)`, prints chain /
NONE / ERR plus wall time. Run against tethys's OWN index
(`tethys index`: 114 files, 396 `file_deps` edges, 37 cycles per
`tethys cycles`) — production-shape data, not a fixture.

## Oracle

`.tethys-4m9o/oracle_bfs.py` — raw `sqlite3` edge dump + hand-rolled
Python BFS queue. No recursive CTE, no tethys code. Shortest paths are not
unique, so the agreement criterion is: same Some/None/Err class, same
length, every consecutive pair a real `file_deps` edge, both endpoints
included.

## Probe runs (probe1-output.txt)

| slice | oracle | probe (depth 50, production code) |
|---|---|---|
| equal endpoints `src/main.rs` | CHAIN len=1 | CHAIN len=1, 0.1 ms ✓ |
| missing from `src/nope.rs` | MISSING from | ERR NotFound, 0.0 ms ✓ |
| missing to `src/nope.rs` | MISSING to | ERR NotFound, 0.1 ms ✓ |
| disconnected `src/main.rs → src/db/schema.rs` | NONE | **TIMEOUT ≥ 60 s** ✗ |
| direct edge `src/db/graph.rs → src/types.rs` | CHAIN len=2 | **TIMEOUT ≥ 60 s** ✗ |
| deep `src/lsp/provider.rs → src/cli/mod.rs` | CHAIN len=8 | **TIMEOUT ≥ 60 s** ✗ |

Same production SQL rerun via `sqlite3` CLI with the depth bound lowered
50 → 8: deep pair answers in **21 ms** with a valid depth-7 path
(`provider.rs → types.rs → tests/orphan_files.rs → indexing.rs →
batch_writer.rs → lsp/transport.rs → cli/coupling.rs → cli/mod.rs`),
length-agreeing with the oracle's BFS path. Semantics are right;
termination is the defect.

## The measured defect

The recursive CTE in `Index::find_dependency_path` (src/db/graph.rs:248)
enumerates every distinct *walk* (path-string rows defeat `UNION` dedup,
no visited set) up to `DEFAULT_MAX_DEPTH = 50`. Walk-count model over the
real edge set, from `src/db/graph.rs` alone:

```
depth<=10: 1.3e6 walks   depth<=20: 2.1e12   depth<=50: 8.1e30
```

Any endpoint pair whose source can reach the 37-cycle region — including a
DIRECT EDGE query and every disconnected query — is non-terminating in
practice. Only equal-endpoint and missing-endpoint short-circuits work on
real data.

Secondary (the ticket's stated target): path hydration is a per-id
`get_file_by_id` loop (src/db/graph.rs:295-301) — the same 2+N shape
tethys-n8pu removed from direct deps; to be measured with the n8pu trace
fence during the build.

## Why no fixture ever caught it

`benches/queries.rs::bench_dependency_chain` uses
`generate_deep_call_chain` (linear, acyclic); every `tests/graph.rs` chain
fixture is acyclic, and the same-file/shortest tests are hedged
(`if let Some`, "either None or trivial is acceptable"). Walk enumeration
is linear on DAG-chains; the blowup needs cycles, which only real indexes
have.

## What I learned (not obvious before the probe)

`get_dependency_chain` is not slow-but-correct — it is effectively
non-terminating on ANY real cyclic index (8×10³⁰ CTE rows at depth 50 on
tethys's own 114-file graph), so the N+1 hydration the ticket names is the
*least* of the query's problems; the fix must replace walk enumeration
with a real visited-set BFS, not just batch the hydration.
