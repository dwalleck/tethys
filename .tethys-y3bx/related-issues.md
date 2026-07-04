# Related issues — tethys-y3bx (untested-code)

- **tethys-3gey** (CLOSED) — Reachability analysis. The machinery to reuse:
  `Tethys::bfs_reachable` / `get_forward_reachable` / `get_backward_reachable`
  (src/lib.rs:544+), over `get_callees`/`get_callers` (call_edges). Single-root;
  untested-code needs multi-root (union of all is_test roots).
- **tethys-s8hv** (CLOSED, this session) — indexed inline-module unit tests +
  their edges. The prerequisite that unblocked y3bx: is_test roots + unit-test
  reference edges now exist (is_test 330→814).
- **tethys-dvsw** (open) — dead-code finder; roadmap stage AFTER untested-code,
  same reverse-traversal machinery, more dangerous failure mode.
- **tethys-ygjx** (open) — fn-as-value refs gap; callback-only-tested code can
  look untested. Documented non-blocking limitation per the issue.
- **tethys-0nar** (open, this session) — proptest!/macro test fns unindexed.
  CONSEQUENCE for y3bx: `arb_*` generators called only from `proptest!` macros
  look untested (their macro-generated call sites aren't edges). ~7 of 251.
- **tethys-l6nt** (PRD parent) — Act 1 code-health roadmap.

Prior-art search (rivets list | grep): no existing untested-code impl.
