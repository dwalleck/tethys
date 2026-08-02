# tethys-n8pu prior art (tracker search, 2026-08-01)

Issues bearing on direct file dependency/dependent hydration:

- **tethys-mv36** (closed) — graph: collapse one-adapter traits into concrete
  Index queries. Blocker for n8pu; the graph cutover it performed is the
  substrate this slice hydrates on. Its "remove the legacy transitive file
  projection" work is why `get_transitive_dependents` is already a concrete
  `Index` JOIN (see `src/db/graph.rs`) — the direct queries are the
  remaining per-ID hydration stragglers.
- **tethys-8ya3** (open, P2) — batch file_deps inserts into transactions.
  Same subsystem, different direction (write path, not read/hydration).
- **tethys-zoi3** (open, P2) — expand file_deps test coverage (rename,
  target deletion, DB-unit, rebuild idempotency). "Target deletion" is the
  dangling-file case the LEFT JOIN missing-count path defends; zoi3 may
  later add tests that exercise that defensive branch.
- **tethys-u5o5** (open, P2) — cycles: batch and canonicalize file
  dependency cycles. Sibling epic slice; consumes `file_deps`, not the
  direct query hydration here.
- **tethys-71if** (open, P2) — graph: contract the legacy interface. Owns
  removal of legacy projections/DTOs; the `Index::get_file_dependencies` /
  `get_file_dependents` (ID-returning) methods may become its removal
  targets once n8pu's path-resolving variants land. n8pu keeps them (they
  are public surface) and does not pre-empt 71if.

No existing ticket for the direct-query N+1 itself — that is this ticket.
