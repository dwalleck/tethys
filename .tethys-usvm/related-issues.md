# Prior art — tracker search for tethys-usvm

Searched `rivets list` for `cycle|johnson|scc|enumerat`, plus the
`discovered-from` and `related` edges on tethys-usvm itself.

| ID | State | Relevance |
|----|-------|-----------|
| tethys-u5o5 | closed | Parent work (PR #40). Shipped the current `enumerate_cycles` / `CycleSearch`, including the `blocked`/`B` bookkeeping added during review. tethys-usvm is the residual gap in that same function. |
| tethys-qqbi | open, P3 | **Overlaps this code directly.** `CycleSearch::visit` and `CycleSearch::unblock` both recurse without a depth bound; stack overflow at depth 250,000. Filed from the same PR #40 review. |
| tethys-vwrn | closed | Non-termination in `get_dependency_chain` on cyclic indexes (recursive CTE enumerating all walks). Different code path — SQL, not the in-memory search — but the same underlying hazard: cost driven by walk count rather than answer size. |
| tethys-e3j1 | open, P3 | `load_cycle_snapshot` dangling-edge posture. Touches the same function's caller but not the enumeration itself. |

## Consequences for this run

- **tethys-qqbi is a live constraint, not just a neighbour.** The SCC
  restriction needs a strongly-connected-components pass, and the textbook
  formulation of Tarjan's is recursive. Adding a *second* unbounded
  recursion site to this function would deepen qqbi rather than leave it
  where it is. The design must either use an iterative SCC or explicitly
  justify a recursive one.
- tethys-qqbi stays a separate issue: bounding `visit`'s own depth is its
  scope, not this one's. The SCC restriction does shrink the reachable
  depth (a search is confined to one SCC), but it does not bound it.
- No prior ticket claims the SCC restriction was attempted and rejected.
