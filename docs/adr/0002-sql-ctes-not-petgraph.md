---
status: accepted
---

# Graph queries use SQL recursive CTEs, not petgraph

Symbol- and file-level graph queries — transitive callers/dependents, shortest
path, and cycle detection — are implemented as SQLite recursive CTEs on
`db::Index` (`src/db/graph.rs`), behind the `SymbolGraphOps` / `FileGraphOps`
traits. The 2026-01-22 storage spike (`docs/spikes/2026-01-22-tethys-sqlite-petgraph.md`)
had designed a SQLite **+ petgraph hybrid** — load a subgraph into an in-memory
`DiGraph`, run petgraph algorithms, map results back — and we did **not** adopt
it. petgraph is not a dependency.

## Why

Keeping traversal in SQL avoids the SQL↔petgraph bridge (query nodes → build
`DiGraph` → run algorithm → map back), keeps the database as the single source of
truth with no in-memory graph to hold in sync, and lets `max_depth` bounding live
inside the query. The spike's premise — that impact, cycles, and paths *require* a
graph library — did not hold: recursive CTEs express all of them.

## Consequences

This is recorded chiefly as a **deliberate deviation from the spike**, so the spike
doc is not mistaken for the shipped design and nobody "adds the missing petgraph."
Algorithms that don't express cleanly as CTEs (e.g. Tarjan SCC for rich cycle
grouping, weighted shortest path) could justify swapping petgraph in for those
*specific* operations; the trait boundary (`*GraphOps` on `Index`) is where such a
swap would land.
