# tethys-j2r1 — falsifiable design: type-hierarchy edges + hierarchy query

## Purpose

Extract inheritance/implementation edges and expose a hierarchy walk —
promoted by the PRD from speculative feature to **dead-code suppression
infrastructure** (tethys-dvsw consumes it: trait/interface impl methods with
zero direct call sites are suppressed, not flagged). Deliverables per the
issue: extractor edges (Rust + C#), `get_type_hierarchy`, `tethys
hierarchy`, fixtures both languages; MCP defers to tethys-o4re.

## Core design

**Reuse `ReferenceKind::Inherit`** (in the enum since day one, never
emitted) at TWO granularities from one extraction walk:

1. **Type-level edge** — at `impl Trait for Type` (Rust) and base-list
   entries (C# `class X : Base, IIface`; Rust `trait A: B + C`): one
   Inherit ref, `name` = the SUPERTYPE, `containing_symbol_span` = the
   DECLARING construct's span so `in_symbol_id` anchors the SUBTYPE:
   - C# classes / Rust traits: the declaration span IS the subtype symbol's
     span — anchoring is automatic (probe: trait decls are symbols).
   - Rust `impl` blocks are not symbols: the extractor sets the ref's
     containing span to the impl block, and the INSERT-time same-file
     container map (aay4 machinery) can't help — so for impls the ref
     instead carries `path = [Type]` and containing span = None, and a
     dedicated insert step anchors `in_symbol_id` to the same-file Type
     symbol (24/27 on self-index; cross-file → NULL, documented).
2. **Method-level marker** — every method inside a Rust `impl Trait for
   Type` block ALSO emits one Inherit ref (`name` = Trait,
   `containing_symbol_span` = the METHOD's span → `in_symbol_id` = the
   method). This is the suppression channel: *this method implements a
   trait member* — precise even when the same type has inherent impls,
   and independent of whether the trait resolves.

**Retention posture (the load-bearing inversion):** unresolved Inherit refs
are KEPT — 21/27 supertypes on self-index are external (`Display`, `From`),
and "implements something external" is exactly the suppression signal.
`reference_name` stays queryable for them (`refs_named`).

**call_edges exclusion:** `inherit` joins the `NOT IN
('value','field_access','macro_call')` list — an edge to a trait is not a
call (same lesson as 8ym0).

**Query surface:** `Tethys::get_type_hierarchy(&str, HierarchyDirection)`
→ up = inherit refs whose `in_symbol_id` ∈ the named type's ids (resolved
supertypes as symbols, external ones as names); down = inherit refs whose
`symbol_id` = the named type (subtypes via `in_symbol_id`). Transitive walk
with cycle guard. CLI `tethys hierarchy <SYMBOL> [--direction up|down|both]
[--json]`, house envelope.

**C# scope:** type-level base-list edges only. C# has no impl blocks, so
method-level "implements interface member" needs name-matching against
interface members — the 0aqj kind-blind class; deliberately out
(`.tethys-j2r1/to-file.md`, files at close-out or lands with dvsw's C#
design).

## Input shapes

| # | shape | handling |
|---|---|---|
| S1 | `impl Trait for Type`, both same-file | type edge (in_symbol=Type, symbol=Trait) + method markers |
| S2 | trait external (`impl Display for X`) | edges kept unresolved (name='Display'); method markers likewise |
| S3 | subtype cross-file (`impl Display for X` in another file) | type edge in_symbol NULL (documented degrade); method markers still anchor to the METHODS |
| S4 | inherent `impl Type` | NO edges, NO markers (fence: inherent methods unsuppressed) |
| S5 | `trait A: B + C` supertraits | type edges from A to each bound (probe: 10/10 external — kept unresolved) |
| S6 | generic `impl<T> From<T> for Type` | base-name via the shared `type_base_name` (dl7l rule) |
| S7 | non-nominal impl target (`impl T for (i32,i32)`) | no anchor — edge with in_symbol NULL, name kept |
| S8 | C# `class X : Base, IFace1, IFace2` | one type edge per base-list entry (no extends/implements split — see D-B) |
| S9 | C# nested class with base list | edge anchors to the innermost class (xov3 spans) |
| S10 | `struct X<T: Bound>` generic bounds | OUT — a bound constrains the parameter, not the declaring type (issue listed it; rationale recorded, D-D) |
| S11 | marker impls (`impl Marker for X {}`, no methods) | type edge only, zero method markers |
| S12 | hierarchy cycles (A: B, B: A — illegal in Rust, possible in indexed-but-broken code) | walk guard, no infinite loop |
| S13 | duplicate impls of same trait for same type (cfg'd) | duplicate edges tolerated; walk dedupes |

Subtractive sweep: additive (new refs of an already-parsed kind; one new
exclusion). The retention posture ADDS unresolved rows of kind `inherit` —
consumers filtering `symbol_id IS NULL` for deprecated-callers Path B use
`LIKE '%::%'` qualified names, which bare trait names don't match; fenced.

## Falsification

Fences in `src/languages/rust.rs`/`csharp.rs` (unit) + new
`tests/type_hierarchy.rs`.

| # | Claim | Falsifier / Oracle | Cost | Status | Regression fence |
|---|---|---|---|---|---|
| C1 | Rust impls emit type edge + method markers; inherent impls emit nothing | probe edge-set (27 impls, by-trait distribution) vs post-build DB; unit fixtures | done+20m | **probe half passed** | unit arms + F-H1/F-H4 |
| C2 | supertrait bounds emit (`trait A: B + C`) | 10/10 grep-exact probe | 10m | passed (probe) | F-H2 |
| C3 | unresolved inherit refs RETAINED, name-queryable | fixture w/ external trait; count post-index | 10m | pending | F-H3 |
| C4 | inherit never enters call_edges | fixture + SQL | 5m | pending | F-H5 |
| C5 | C# base-list edges incl. nested class | fixture | 15m | pending | F-H6 |
| C6 | get_type_hierarchy up/down/transitive + cycle guard | chain fixture A→B→C + down-walk | 20m | pending | F-H7 |
| C7 | CLI table+JSON envelope, binary seam | run binary | 15m | pending | F-H8 |
| C8 | self-index audit: 27 type edges, method markers ≥ trait-impl method count, analyses unchanged (8ym0 isolation) | rebuild + diff | 15m | pending | audit |
| C9 | suppression join delivers: SQL "methods WITH inherit marker" exactly = trait-impl methods on a mixed fixture | the dvsw-consumer preview | in F-H4 | pending | F-H4 |

Cheapest falsifier: **run** — the probe computed the exact type-level edge
set with grep-oracle agreement (27 + 10, decomposed).

## Negative space

1. **Method-override resolution** (M in X → M's declaration in Trait/Base)
   — the aay4-dependency half of the issue; scoped out per the PRD's
   "scoped to that need". The method MARKER (implements-something) meets
   the suppression need without member mapping. Revisit inside dvsw if its
   design demands member-level mapping (tethys-dvsw is the tracked home).
2. **C# method-level markers** — out (0aqj class); `.tethys-j2r1/to-file.md`.
3. **Extends/Implements enum split** — single `Inherit` kind; the display
   distinction has no consumer yet, the schema stays untouched (D-B).
4. **Generic bounds as edges** (S10) — settled rationale.
5. **MCP tool** — tethys-o4re (Act 2), as with every analysis this cycle.
6. **No new tables** — refs only, per the issue's own constraint.

## Open decisions flagged for approval

- **D-A (dual granularity)**: type-level edges + method-level markers from
  one walk (recommended) vs type-level only (breaks method-precision:
  inherent methods of trait-implementing types would need suppressing by
  join, over-suppressing).
- **D-B (single Inherit kind)**: reuse the existing enum variant for
  extends AND implements (recommended; no schema/enum growth, suppression
  doesn't care) vs adding Extends/Implements variants now.
- **D-C (retention posture)**: keep unresolved inherit refs (recommended —
  external traits are the majority and the whole point) vs dropping like
  value/macro_call (would erase the Display/From suppression signal).
- **D-D (issue AC trim)**: `struct X<T: Bound>` bounds excluded with
  rationale; method-override walk deferred to dvsw's design. Both are
  departures from the issue's literal text.
