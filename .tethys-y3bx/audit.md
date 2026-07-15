# tethys-y3bx — slice 4 audit (oracle closure)

Final branch state, binary `target/release/tethys`, workspace = tethys
itself (the branch's own new code is part of the indexed universe).

## C8 — probe ⇄ binary agreement (audit-only per approved D-E)

| checkpoint | probe (refs-BFS) | binary `untested-code --json` |
|---|---|---|
| slice 2 commit | roots 911 / prod 705 / untested 241 | 911 / 705 / 241 (**exact**) |
| slice 4 final | roots 917 / prod 706 / untested 241 | 917 / 706 / 241 (**exact**) |

Item checks at final state (grep-trace oracle recorded in findings.md),
4/4: `crate_glob_covers` covered ✓, `scalar` covered ✓,
`is_excluded_dir` covered ✓, `print_reachability_result` untested ✓.

refs-vs-call_edges divergence at final state: 241 vs 272 (gap 31) — the
substrate choice stays load-bearing; CI form is F-U2's SQL divergence
assert, not this drift-prone self-index number.

## C9 — perf (manual per approved D-E)

`tethys untested-code` on the self-index (2.5k symbols / 20k refs),
3 runs: **0.007s / 0.006s / 0.006s** — three orders of magnitude inside
the <1s budget. Wall budget honest: one edges query + O(V+E) BFS.

## Fence inventory (permanent CI forms)

- `src/db/untested.rs` unit: closure chain/cycle/self-loop/multi-root/
  zero-roots/duplicate-edge shapes.
- `tests/untested_code.rs`: F-U1 (core rule + S10 pair + S18 collision +
  kind/is_test scope), F-U2 (assert-only coverage + call_edges divergence
  SQL — the D-D pin), F-U3 (transitivity/cycles), F-U4 (indeterminate +
  all-test arms), F-U5 (C# [Fact] parity), F-U7 (binary seam: JSON
  envelope, sort, stream discipline, exit codes).
- Full suite 954/954; clippy pedantic; fmt; doctests.
