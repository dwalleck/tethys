# tethys-aay4 — falsifiable design: populate parent_symbol_id (+ dl7l fix)

## Purpose

Persist the parent linkage the extractors already capture: methods → their
impl's type, struct fields → their struct, enum variants → their enum, C#
members → their class/interface. Today `parent_symbol_id` is 0/2555 —
`parse_file_static` drops `parent_name` at conversion. This unblocks
tethys-j2r1 (type hierarchy → dead-code suppression infra).

**Absorbed substrate fix (tethys-dl7l, filed by this probe):** the symbol
side records the TRAIT as an impl's identity (`find_impl_type` takes the
first `type_identifier`), while the refs side (53iv `impl_type_base_name`)
uses the grammar's `type` FIELD. `impl Trait for Type` methods are stored
as `Trait::method`. aay4 cannot ship on that substrate — parent linkage
would bake the wrong parent into every trait impl (28 in this workspace).

## Probe evidence (`findings.md`)

- 859 symbols carry a parent prefix (methods 318, fields 412, variants 129).
- Proposed same-file rule on real data: 719 link, 109 orphans (cross-file
  impls — correctly NULL), **0 ambiguous**.
- Probe⇄oracle agreement 657 pairs after decomposing every disagreement
  into named classes (probe cursor bug, stale index, fn-local items not
  indexed at all, dl7l).

## Core design

1. **dl7l fix**: `find_impl_type` delegates to the existing
   `impl_type_base_name` (the `type` field + `type_base_name`) — the symbol
   and ref sides share one impl-identity function and can never disagree
   again. Consequence: trait-impl methods' `qualified_name` becomes
   `Type::method` (was `Trait::method`), healing `qualified_exact` for
   receiver-typed refs to trait-impl methods.
2. **Conversion**: `OwnedSymbolData` gains `parent_name: Option<String>`;
   `parse_file_static` passes it through (both index paths share this fn).
3. **Insert-time linkage** in `index_parsed_file_atomic` (the single insert
   point, already returning inserted ids): two phases inside the existing
   per-file transaction — insert all symbols collecting
   `name → (id, kind)` for CONTAINER kinds (Struct, Class, Enum, Trait,
   Interface, TypeAlias, Union-if-present), then one UPDATE pass setting
   `parent_symbol_id` for rows whose `parent_name` matches exactly one
   same-file container. Two phases because impls legally precede their
   type's declaration in file order (S5).
4. **Miss/ambiguity posture**: no same-file container → NULL (cross-file
   impls; suppression, don't fabricate); >1 same-named container → NULL +
   `trace!` (probe measured 0 in practice; fence constructs one).
5. **Invariant**: parent links are same-file by construction, so the
   schema's `ON DELETE CASCADE` on `parent_symbol_id` can never cascade
   across files (per-file reindex deletes the whole file's rows together).
6. **No backfill**: the index is a disposable cache — `--rebuild` populates
   (9z7i Option-C precedent; schema already has the column, so no guard
   change).

## Input shapes

| # | shape | handling |
|---|-------|----------|
| S1 | method in inherent impl, same-file type | linked (C3) |
| S2 | method in `impl Trait for Type`, same-file Type | linked to TYPE, qualified `Type::method` (C1; red-first vs today's trait-parent) |
| S3 | method in impl for a cross-file type | parent NULL (C6) |
| S4 | struct field / enum variant | linked to struct/enum (C3) |
| S5 | impl block BEFORE the type declaration in file order | still linked (C4, two-phase) |
| S6 | two same-named same-file containers | NULL + trace (C5, constructed) |
| S7 | C# method/field/ctor in class; member of a NESTED class | linked to the innermost class (C7) |
| S8 | fn-local items | not indexed at all — outside universe (probe-verified; settled) |
| S9 | trait-BODY method declarations | not indexed (TRAIT_ITEM arm) — documented; j2r1 keys suppression off impl-side methods |
| S10 | generic impl `impl<T> Foo<T>` | base name Foo via `type_base_name` (C1 fixture arm) |
| S11 | module members | NOT parented — settled: `qualified_name` deliberately excludes modules (CONTEXT.md); `module_path` column already answers "what's in module Y" |
| S12 | union / type-alias impl targets | linked when same-file (container-kind set) |
| S13 | file with no containers | all NULL, no error |

Subtractive sweep: additive (a NULL column becomes populated; one identity
function corrected). The dl7l half REMOVES an invariant consumers might
have leaned on — "trait-impl methods are qualified by trait name" — grep
shows no test or query depends on trait-prefixed qualified names except as
today's-behavior baselines; the healing direction (refs side already used
the type) is fenced by C2.

## Falsification

Fences in `src/languages/rust.rs` (unit) + new `tests/parent_symbols.rs`.

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| C1 | `impl Trait for Type` methods record parent/qualified by TYPE; inherent impls unchanged; symbol side ≡ refs side | probe already demonstrated the disagreement on real data (oracle-only pairs = exactly the trait impls); unit fixture pins post-fix | probe pair sets + hand-written fixture expectations | done/15m | **probe half passed** | unit `find_impl_type_uses_type_field_for_trait_impls` + F-P2 e2e |
| C2 | dl7l heals qualified_exact: receiver-typed ref to a trait-impl method resolves with strategy `qualified_exact` | fixture: `let r = RustLike; r.anchor()` + `impl Anchorable for RustLike` cross-checked pre/post | SQL strategy assert (red today) | 20m | pending | F-P3 |
| C3 | Every same-file (child,parent) pair links: methods/fields/variants, Rust + counts on self-index ≈719/109 | fixture asserts exact (child_id → parent_id) rows; audit re-runs probe vs binary DB | probe (python AST) vs DB join — independent mechanisms | 20m | pending (probe rule passed pre-impl) | F-P1; audit for self-index counts |
| C4 | impl-before-type file order still links | S5 fixture | SQL | in F-P1 | pending | F-P1 arm |
| C5 | same-file name collision → NULL + trace | S6 constructed fixture | SQL (parent IS NULL) | 10m | pending | F-P4 |
| C6 | cross-file impl target → NULL | S3 fixture | SQL | in F-P4 | pending | F-P4 arm |
| C7 | C# parity incl. nested class member → innermost class | C# fixture | SQL | 20m | pending | F-P5 |
| C8 | batch ≡ streaming for parent columns | parity fixture, canonical dumps incl. parent | dump diff | 15m | pending | F-P6 |
| C9 | determinism across rebuilds | double index, dump diff | dump diff | in F-P6 | pending | F-P6 arm |
| C10 | reindex safety: re-indexing one file preserves other files' rows (same-file cascade invariant) | fixture: touch+reindex file A; file B symbols/links intact | SQL before/after | 15m | pending | F-P7 |
| C11 | suites/goldens green; golden updates (if fixture has parented syms) are deliberate | run suites | existing goldens | 10m | pending | existing tests |
| C12 | analyses unchanged on self-index except dl7l's qualified-name display | pre/post analysis output diff (8ym0 isolation pattern) | binary output diff | 15m | pending (post-build audit) | audit + each analysis's fixtures |

Cheapest falsifier: **run** — the probe demonstrated C1's disagreement on
real data and validated C3's rule (719/109/0) before this document.

## Negative space

1. **No module-as-parent linkage** — settled: `qualified_name` excludes
   modules by design; `module_path` answers module membership.
2. **No trait-body method indexing** — existing gap, documented; j2r1's
   suppression works from impl-side methods (its probe re-evaluates).
3. **No override resolution** (method → trait method) — that IS tethys-j2r1.
4. **No backfill migration** — disposable cache, rebuild populates.
5. **No new query surface** — `parent_symbol_id` becomes trustworthy data;
   queries over it land with j2r1 (`get_type_hierarchy`) and tethys-o4re
   (MCP), not here.
6. **fn-local items stay unindexed** — settled; not a parent-linkage gap.

## Open decisions flagged for approval

- **D-A (dl7l absorption)**: fix tethys-dl7l inside this PR (recommended —
  aay4 is unshippable without it; one function, shared with the refs side)
  vs shipping dl7l separately first (extra PR cycle, same code).
- **D-B (qualified-name change)**: dl7l changes stored qualified names for
  trait-impl methods (`Trait::m` → `Type::m`). Recommended: accept —
  it heals resolution and matches the refs side; the old spelling was the
  bug. Flag because it's a user-visible query-surface change.
- **D-C (container-kind set)**: parents restricted to type-container kinds
  (Struct/Class/Enum/Trait/Interface/TypeAlias/Union) — a same-named FUNCTION
  can never become a parent (recommended; prevents kind-blind mislinks).
