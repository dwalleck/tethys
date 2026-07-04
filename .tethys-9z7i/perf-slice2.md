# tethys-9z7i slice 2: C10/C13 audits (2026-07-04)

Old binary = origin/main (157bbce, post-ADR merge) built in a worktree;
new = this branch at B8. Both release profile.

## C10 — analysis outputs byte-identical (PASSED)

Fixture: two-file crate with a `#[deprecated]` fn + call, a `pub use`
re-export, and an unused pub fn (exercises deprecated-callers and
visibility-tightening down every render path). Each binary indexed with
its own `--rebuild`, then all four outputs diffed:

| output | result |
|---|---|
| deprecated-callers (human) | IDENTICAL |
| deprecated-callers --json | IDENTICAL |
| visibility-tightening (human) | IDENTICAL |
| visibility-tightening --json | IDENTICAL |

The strategy column is invisible to every existing consumer, as designed
(slice 3 exposes it). Deterministic floor: the existing CLI fences that
pin these outputs exactly, which ran green throughout B1–B7.

## C13 — indexing wall-clock delta ≤5% (PASSED, manual fence approved)

hyperfine, self-index `--rebuild`, 10 runs + 2 warmup each:

- main:   553.6 ms ± 33.0 ms
- branch: 564.2 ms ± 31.9 ms
- delta: +1.9% mean (1.02× ± 0.08 — inside measurement noise; σ ranges
  overlap)

Budget ≤5% holds. Cost sources: one TEXT bind per resolved ref on both
write shapes + one PRAGMA per open. `benches/indexing.rs` remains the
harness for any deeper regression hunting.
