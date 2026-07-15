# tethys-8ym0 — slice 6 audit (oracle closure)

All numbers from the final branch state (post slice 5), binary
`target/release/tethys`, workspace = tethys itself (100 files — the branch's
own new test code is part of the indexed universe).

## C6 — analysis stability with the REAL kind (falsifier1 re-run, isolated)

Methodology correction vs the plan: the committed `base-*.txt` baselines
predate the branch's new test code, so a raw diff would confound code growth
with macro-row effects. Isolation used instead: same binary, same tree —
analyses run WITH the 193 macro_call rows, then after `DELETE FROM refs
WHERE kind='macro_call'`, everything else held constant.

| analysis | with vs without macro_call rows |
|---|---|
| unused-imports | IDENTICAL |
| visibility-tightening | IDENTICAL |
| deprecated-callers | IDENTICAL |
| panic-points | IDENTICAL |
| callers crate_glob_covers | IDENTICAL |

(Artifacts: `audit-with-*.txt` / `audit-without-*.txt`.) Note
deprecated-callers identity is expected — tethys itself carries no
`#[deprecated]`; the behavioral fence for that consumer is F11.

## Probe ⇄ binary agreement (prove-it-prototype oracle, final)

| checkpoint | probe | binary |
|---|---|---|
| slice 2 (pre-drop) | after-guard in-crate 182 | resolved 182 |
| slice 6 final tree | after-guard in-crate **193** | resolved **193** |
| unresolved surviving | — | **0** |
| item check: `scalar` sites in tests/value_refs.rs | 11 (grep + hand-read) | **11/11**, all bound same-file |

The +11 from slice 2 to slice 6 is the branch's own new fence code (fence
fixtures call `scalar(...)` etc. inside asserts) — probe and binary moved in
lockstep on the changed universe, which is itself evidence the emission rule
is stable.

## C12 — perf (manual fence per approved D-C; CI trip-wire = tethys-ng1v)

3× `tethys index --rebuild` on the same tree, main-baseline binary (built
from origin/main in a worktree) vs branch binary:

| | run1 | run2 | run3 | median |
|---|---|---|---|---|
| main | 497.87ms | 470.42ms | 392.76ms | 470ms |
| branch | 402.11ms | 435.26ms | 459.56ms | 435ms |

No regression (branch median below main's; deltas within run-to-run noise).

## y3bx payoff (recorded for the resumed untested-code pipeline)

probe2 (impact BFS, pre-implementation): untested 260 → 235 with bare-call
edges. The consumption note for tethys-y3bx: macro_call is EXCLUDED from
call_edges (posture D-A), so untested-code must traverse `refs` (or a view
over it), not call_edges — its own probe already blessed refs traversal, and
this finally gives its "refs beats call_edges" AC a real pinning case (F1).

## Fence inventory (permanent CI forms)

- `src/languages/rust.rs` unit: shape classification ×4 + reconciled ygjx
  fence (`macro_token_identifier_not_emitted_as_value` now also pins the
  macro_call channel).
- `src/types.rs` / `src/languages/common.rs` / `src/resolve.rs` unit:
  round-trip, db-kind mapping, binding gate.
- `tests/macro_token_refs.rs`: F1, F1b, F2, F3, F4 (TRIPWIRE 9l27/ewa7),
  F5 (TRIPWIRE 7dqj), F6, F7, F11, F9, F10 — 11 integration fences.
- Untouched-and-green: `idxperf_golden` ×3 (C11), full suite 944/944.
