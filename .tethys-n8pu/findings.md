# tethys-n8pu probe findings — 2026-08-01

## Probe

`src/db/file_deps.rs` → `#[cfg(test)] mod n8pu_probe::probe_direct_dep_hydration`.
Builds a real temp workspace (lib.rs → a.rs, b.rs, c.rs; a.rs → c.rs),
indexes through `Tethys::index()`, then:

- calls `Tethys::get_dependencies` / `get_dependents` / missing-root,
- counts SQL statements on the LIVE index connection via a rusqlite
  `trace` hook (dev-dependency already enables the `trace` feature),
  separating per-ID lookup statements (`... FROM files WHERE id = ...`)
  from the total.

## Oracle

Direct JOIN SQL against the same db (independent mechanism):
`SELECT path FROM file_deps JOIN files ... WHERE from_file_id = (SELECT id ...)`.

## Agreement

Probe and oracle agree on every slice (pre-fix run, output in
`probe1-output.txt`):

| slice | probe | oracle |
|---|---|---|
| deps(src/lib.rs) | [a.rs, b.rs, c.rs] | [a.rs, b.rs, c.rs] |
| dependents(src/c.rs) | [a.rs, lib.rs] | [a.rs, lib.rs] |
| missing root | NotFound("file: src/nope.rs") | (API contract) |
| paths workspace-relative | yes | — |

## The measured defect

`Tethys::get_dependencies(path)` issues **2 + N** SQL statements where N is
the number of returned dependencies:

```
PROBE stmts deps       = total 5, per-id-lookup 3   (N = 3)
PROBE stmts dependents = total 4, per-id-lookup 2   (N = 2)
PROBE stmts missing    = total 1, per-id-lookup 0   (short-circuits at root)
```

The per-ID statements are `get_file_by_id` calls from `file_ids_to_paths`
(one per returned ID). Post-fix target: total 2, per-id-lookup 0 — root
lookup + one set-oriented JOIN query, both directions.

## What I learned (not obvious before the probe)

1. **`mod X;` declarations alone create NO `file_deps` edge.** Only *used*
   imports do (L2 semantics, same rule the C# tests fence). `pub use
   crate::c::C;` counts as a use; an unused `use crate::c::C;` in a.rs
   produced no edge. First fixture iteration assumed `mod`/`use` always
   create edges — wrong, and the probe caught it because the oracle
   (raw `file_deps` table) disagreed with my expectation, not with the API.
2. **The N+1 is exactly "2 + N", and the hydration loop is the only
   offender** — the root lookup and the edge query are already minimal;
   the fix only replaces the per-ID loop, so NotFound behavior (root
   short-circuit at 1 statement) is untouched by construction.
3. **Dangling `file_deps` rows are impossible in real indexes** — Index::open
   enforces `PRAGMA foreign_keys = ON` and the table cascades on file
   delete. `file_ids_to_paths`'s missing-ID warn path is defensive dead
   code; a LEFT JOIN preserves its behavior at zero cost, so keep it.
4. `file_deps` PK is `(from_file_id, to_file_id)` — "no duplicate indexed
   file" in results is guaranteed by schema, not by the query.
