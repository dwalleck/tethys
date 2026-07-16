# tethys-dvsw — build audit (final integration check, 2026-07-15)

## Oracles (prove-it-prototype lineage)

- **probe3 == binary, item-exact** (final run, post-S6): 36 findings on
  the fresh self-index, byte-for-byte agreement on (file, line, kind,
  name, tier) and summary — `0 Definite / 36 Maybe`. probe3 is the
  independent SQL+python implementation of the approved semantics,
  updated in lock-step at S6 (entry-point liveness; C# `function`-kind
  Main).
- **rustc dead_code, FP direction (C7)**: warning-free workspace ⟹
  Definite = ∅. Held at probe time (probe2: 0/37 absorbed), at S4
  (0/36), and finally as the permanent CI fence
  `tests/dead_code.rs::self_index_zero_definite` (indexes a copy of the
  repo source, 0.59s).
- **rustc dead_code, FN direction (C8)**: seeded-dead fixture — 4/4
  Definite (fn, struct, const, recursive fn), decoy demoted to Maybe —
  `::seeded_dead_items_definite`.

## C13 additivity audit

Branch-point binary (41ec37b, built in an isolated worktree) vs head
binary, SAME fresh self-index, all five existing analyses `--json`:

| analysis | verdict |
|---|---|
| unused-imports | IDENTICAL (88 B) |
| visibility-tightening | IDENTICAL (28,884 B) |
| untested-code | IDENTICAL (40,562 B) |
| deprecated-callers | IDENTICAL (125 B) |
| panic-points | IDENTICAL (64,222 B) |

(The step-0 `baselines/` snapshots predate this branch's new source
files; the worktree comparison controls for the corpus change and is
the binding result.)

## Budgets

- `tethys dead-code` end-to-end on the self-index: **17-18ms** (budget
  500ms).
- Funnel query + liveness walk: indexed anti-joins, O(u) name-set,
  O(s·depth) upward walk — no per-candidate LIKE scans.

## Drift stops and discoveries during the build

1. **S2 self-deadlock (caught by the gate)**: the liveness helper
   re-locked the non-reentrant connection mutex from inside the query
   path; all 14 tests hung 595s. Fix: pass the held connection.
   Documented on the helper.
2. **S6 scratch-probe discovery**: the C# extractor classifies
   `static void Main()` as kind `function`, not `method` — the design's
   C9 rule as written missed it, and its `Program` container leaked as
   a Definite finding. Fix (in design spirit, approved posture:
   over-suppression is the conservative direction): C# entry-point rule
   accepts both callable kinds, AND entry points confer liveness upward
   so entry-point containers are scaffolding, not dead. Fenced by
   `::csharp_funnel`.
3. **S6 C# verifications** (scratch probe, pinned in the fence):
   implicit-this and `this.`-qualified invocations bind; construct refs
   keep nested classes alive; same-file container linkage reaches
   depth 2; properties are candidates; cross-language textual scan
   demotes a C# name mentioned in Rust text.

## Fence inventory (all passing, `cargo nextest run`)

- `src/db/dead_code.rs` unit fences (seeded rows): candidacy,
  speculative-suppresses, self-ref-not-evidence (×2 channels),
  bare+qualified unresolved, segment-boundary trap, language-aware
  kinds, marker-suppresses-external-trait, container transitive
  liveness, is_test-descendant, function-not-container scope guard,
  entry-point path/language table, ordering.
- `src/dead_code.rs` scan fences: macro-mention Maybe, own-span
  recursion Definite, substring boundary, NULL end_line degrade,
  comment mention, twins mutual demotion, limit-vs-summary,
  unreadable-file skip.
- `tests/dead_code.rs` integration fences: candidacy end-to-end, seeded
  Definite + decoy, entry points, CLI envelope/sort/limit,
  zero-candidate clean-empty, determinism, tier serialization contract,
  C# funnel, self-index zero-Definite (C7, CI).
