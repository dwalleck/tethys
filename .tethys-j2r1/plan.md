# tethys-j2r1 — budgeted plan

Approved design: `.tethys-j2r1/design.md` (D-A dual granularity, D-B/C
single Inherit kind + retention, D-D both trims). Cheapest falsifier
(probe: 27+10 edges, grep-oracle agreement) passed.

Global oracle: probe edge set — 27 Implements (by-trait distribution:
Display 8, From 5, LspProvider 3…), 10 supertrait edges, 24/27 same-file
subtypes; post-build the binary DB must reproduce type-level counts and
anchoring rates, and method markers must equal the trait-impl method count.

## Slice 1: Rust extraction — edges + markers + supertraits

**Claims:** C1, C2 (extraction side).
**Fixture (pre-written expectations):** `impl Anchor for Widget {fn a(); fn
b();}` → 1 type edge (name=Anchor, path=[Widget]) + 2 method markers
(name=Anchor, containing=method spans); inherent `impl Widget {fn c()}` →
NOTHING; `trait A: B + Send` → 2 supertrait edges anchored to A's span;
generic + non-nominal impl targets per the dl7l contract arms.
**Loop budget:** one pass over impl/trait nodes already visited — O(nodes).
**Files:** `src/languages/rust.rs` (extract_references IMPL_ITEM/TRAIT_ITEM
arms + unit tests).

## Slice 2: anchoring + call_edges exclusion

**Claims:** C1 (in_symbol anchoring), C3 (retention), C4.
**Mechanics:** inherit refs carrying `path=[Type]` and no containing span
get `in_symbol_id` from the aay4 same-file container map during
`index_parsed_file_atomic` (cross-file → NULL). NO drop-sweep widening
(retention per D-C — the inversion is the point). `populate_call_edges`
NOT-IN list gains `'inherit'`.
**Fixture:** unit-level via slice-5 e2e; self-index SQL oracle after this
slice: 27 type edges, in_symbol NOT NULL = 24, unresolved retained ≥ 21.
**Files:** `src/db/files.rs`, `src/db/call_edges.rs`.

## Slice 3: C# base lists

**Claims:** C5.
**Fixture:** `class X : Base, IFace` → 2 type edges anchored to X; nested
class w/ base → innermost anchor; interface `interface I2 : I1` → edge.
**Files:** `src/languages/csharp.rs` (+ unit tests).

## Slice 4: facade + CLI

**Claims:** C6, C7 (production side).
**Mechanics:** `HierarchyDirection {Up, Down, Both}`;
`get_type_hierarchy(name)` — resolve the type symbol(s) by name, walk
inherit refs transitively w/ visited-set (up: refs whose in_symbol ∈ ids —
resolved targets as symbols, unresolved as bare names; down: refs whose
symbol_id ∈ ids). CLI `tethys hierarchy <SYMBOL> [--direction] [--json]`,
house envelope, BrokenPipe-safe.
**Loop budget:** walk O(inherit rows) ≈ dozens; per-level SQL by ids.
**Files:** `src/db/hierarchy.rs` (new) + `src/lib.rs` + `src/cli/
hierarchy.rs` (new) + wiring (mod.rs/main.rs mechanical).

## Slice 5: fences + audit

**Claims:** C1-C9 e2e; audit closes C8.
**Fences (`tests/type_hierarchy.rs`):** F-H1 mixed impl fixture (trait +
inherent: edges/markers exactly as pre-written; C9's suppression-join
preview: `SELECT methods WITH inherit marker` = exactly the trait-impl
methods); F-H2 supertraits; F-H3 external-trait retention + name-queryable
+ deprecated-callers Path B unpolluted (bare names lack '::'); F-H4
inherent-methods-unmarked; F-H5 call_edges exclusion; F-H6 C# e2e; F-H7
up/down/transitive/cycle-guard walk; F-H8 CLI binary seam.
**Audit:** probe vs binary (27/24/21 class counts), analyses isolation diff
(8ym0 pattern), perf spot check. Changelog fragment rides review stage.
**Files:** `tests/type_hierarchy.rs`, `.tethys-j2r1/audit.md`.

## Plan self-review

1. Loops: extraction one-pass; walk bounded by inherit rows + visited set;
   all ≪ 10^6. ✓
2. Fixtures attack: inherent-vs-trait confusion (F-H1/F-H4), external
   retention + Path-B pollution (F-H3), cycle guard (F-H7), nested C#
   anchoring (F-H6), call-graph contamination (F-H5). ✓
3. Doc preconditions: retention posture documented on the kind + the
   exclusion comment; no unenforced contracts. ✓
4. Write targets: CLI stdout data / stderr diagnostics per house pattern. ✓
5. Tracker refs: dvsw, o4re, 0aqj, xov3 verified; C# marker deferral
   queued in `.tethys-j2r1/to-file.md` (parallel-session jsonl discipline),
   files at close-out. ✓
