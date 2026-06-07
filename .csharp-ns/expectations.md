# Expectation tables (written at Slice 0, BEFORE any code)

## E1 — csharp-gt dump stability through slices 1–5

The ground-truth workspace contains NO simple-name collisions, so the new
using-arm changes no targets there:

- `Widget` / `FileScopedThing` / `NestedThing` constructor refs: resolved
  today via the unique fallback; post-S4 they resolve via the using-arm to
  the SAME symbols (C4) → dump rows identical (the dump shows targets, not
  arms).
- Bare `Assist()` call: kind filter excludes methods from the using-arm →
  falls through to the unique fallback → same target as today.
- `Console::WriteLine` (qualified, external): qualified refs bypass the
  using-arm entirely (planned deviation, slice 4) → unresolved, unchanged.
- Static/alias using rows (`My.Models.Helper`, `My.Models.Widget`) are
  type-level dotted keys → namespace-map misses → no effect.

**Expectation: csharp-gt dump byte-identical after slices 1, 2, 3, 4, 5.**

## E2 — L2 file_deps delta on csharp-gt (after slice 6)

Baseline (slice-0 capture) vs post-deletion, with sources attributed:

| Edge | Baseline | Post-S6 | Why |
|---|---|---|---|
| App.cs → Models.cs | 4 (1 post-pass + 3 call-edge: Widget ctor, 2 Assist calls) | **3** | post-pass row gone; call edges remain (same bucket) |
| App.cs → Other.cs | 1 (post-pass only; UnusedThing never referenced) | **ABSENT** | THE unused-using L1 edge — decision #2's deliberate removal |
| GlobalUsings.cs → Globals.cs | 1 (post-pass; file has no refs at all) | **ABSENT** | no resolved refs → no L2 evidence |
| UseScoped.cs → Scoped.cs | 2 (1 post-pass + 1 call-edge) | **1** | post-pass row gone |
| UseScoped.cs → Nested.cs | 1 (call-edge only — nested namespace never matched the map, tethys-nnst) | **1** | unchanged; was never post-pass-sourced |

No other file_deps rows may appear or vanish.

## E3 — xdir (cross-directory) workspace

- Baseline: `services/Svc.cs → models/Widget.cs (1)` — post-pass-sourced
  (probed: K-hybrid drops the cross-bucket call-edge candidate; single-source
  arithmetic).
- After slice 5 (corroboration live, post-pass still alive): edge present;
  count may be 2 transiently (both sources) — recorded at S5 gate.
- After slice 6 (post-pass gone): **exactly `services/Svc.cs →
  models/Widget.cs (1)`** — corroborated call-edge source only.
- Widget ctor ref: resolved today (unique), resolved after (same target).

## E4 — Monotone join (slice 8, csharp-gt + xdir)

Joining pre/post dumps on (file, line, column, kind): every ref resolved at
baseline is resolved post-change to the IDENTICAL target string; zero rows
lose resolution; zero rows change target. (New resolutions: none expected
in these two fixtures — collisions live only in committed test tempdirs.)

## E5 — New-resolution ground truth (integration tests, not fixtures)

- csharp_using_disambiguation: Widget declared in `My.Models` (used) and
  `Dupe.Ns` (unused) → resolves to My.Models' Widget at its file/line;
  baseline for the same shape: UNRESOLVED (probed 2026-06-06).
- Unique `Gear` in used namespace: same target pre/post (C4).
- `Assist` methods in two namespaces, one used: UNRESOLVED pre AND post (C5).

## E6 — Rust fixtures

Frozen self-index worktree + c6trap: byte-identical after every slice, no
exceptions, all slices.
