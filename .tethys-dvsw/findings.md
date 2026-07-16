# tethys-dvsw — prove-it-prototype findings (2026-07-15)

## Question probed

Which non-public, non-test symbols on a fresh tethys self-index have zero
inbound evidence, what does each suppression channel absorb, and does the
result agree with an independent ground truth?

## Probes

- `probe.py` — layered funnel over the fresh self-index (2669 symbols,
  21669 refs, 109 files):

  | layer | absorbs | note |
  |---|---|---|
  | L1 candidates (non-public, non-test) | 820 of 2669 | visibility: public 890, crate 77, module 1, private 1701 |
  | L2 resolved inbound ref exists | 489 alive | of which 34 alive ONLY via speculative band (the transferred ADR-0003 AC is load-bearing) |
  | L3 unresolved ref textually matches name (`reference_name` = name or `%::name`) | 74 alive | post-53iv ambiguous method calls decline → land here |
  | L4 method-level `inherit` marker (`kind='inherit'`, `in_symbol_id`=method) | 2 alive | j2r1 suppression channel works |
  | L5 container with live descendant (parent_symbol_id, recursive) | 1 alive | same-file linkage only |
  | survivors | **254** | 124 module, 94 struct_field, 18 function, 17 const, 1 method |

- `probe2_textual.py` — kind exclusions (module, struct_field) + textual
  word-boundary scan over all indexed files, definition line excluded:
  37 survivors → **37 Maybe, 0 Definite**.

## Oracle

**rustc's `dead_code` lint** — a completely independent mechanism (compiler
liveness analysis vs SQL over refs + text scan).

- FP direction: tethys compiles warning-free ⟹ true-dead set ≈ ∅ ⟹ probe
  Definite must be 0. Probe: **0**. Agrees.
- FN direction: seeded 4 dead items into a workspace copy
  (`dvsw_dead_fn`, `DvswDeadStruct`, `DVSW_DEAD_CONST` unmentioned;
  `dvsw_macro_only` mentioned only inside `stringify!`). `cargo check`
  flags all 4. Probe on the copy's fresh index: the 3 unmentioned →
  **Definite 3/3 exact**; `dvsw_macro_only` → **Maybe** (textual hits) —
  the designed suppression trade, documented not fixed.

## What I learned (that I didn't know before)

Without the textual channel, Definite would carry **37 false accusations
on the self-index alone** — the naive zero-refs query from the issue
description is unshippable; textual word-boundary suppression is
load-bearing, not defense-in-depth.

Supporting discoveries:

1. **Rust `struct_field` (427) and `module` (148) kinds are structurally
   invisible to refs** — Rust field reads emit no refs (`field_access` is
   C#-only, count 0 on a pure-Rust index) and module path segments never
   emit refs. Both kinds must be excluded from candidacy for Rust.
2. **Same-file same-name collisions starve twins**: 3× `seeded_index` in
   `src/db/architecture.rs` — the same-file last-wins map binds all 7
   calls to ONE symbol; the other two show zero refs (tethys-0aqj class,
   same-kind manifestation). Absorbed by textual.
3. **FP classes among the 37**, all textually absorbed: format-string
   captures (`{CONST}` is string content — invisible to ANY token
   walker); bare value-shape identifiers in macro token trees
   (`criterion_group!(benches, bench_fn)` — 8ym0 covered call-shaped
   only); `Type::assoc_fn` as value (tethys-i09d, `row_to_panic_point`).
4. **Entry points are absorbed by luck, not design**: `main` had 203
   unrelated textual hits here; a single-bin workspace's `main` would be
   Definite-flagged. Design needs an explicit entry-point exclusion.
5. **Speculative-band-only symbols: 34** — the transferred AC (band as
   suppression) has real weight on the self-index.

## Gate

- [x] Probe runs against the real codebase (self-index + seeded copy)
- [x] Independent oracle (rustc dead_code) produces output
- [x] Agreement on a non-trivial slice (0 FP self-index; 3/3 seeded exact)
- [x] Learned-something note above

Seeded fixture preserved in the session scratchpad (`dead-fixture/`) for
the design-phase falsifier re-runs.
