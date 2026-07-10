# tethys-53iv falsifiable design — receiver-gated method-call resolution (Rust)

2026-07-09. Stands on `.tethys-53iv/findings.md`: the ticket reproduces
exactly; extraction discards receivers (`rust.rs:550-556`); the phantom
channel is concentrated in Pass-1 same-file bare-name binds (7/8 `is_empty`
binds phantom on tethys itself); cross-file `unique_workspace` binds sample
4/4 true; 24% of receivers are locally type-derivable without LSP.

## Purpose

`x.unwrap()` on an external type binds by bare name to any same-named
in-crate method (fabricated call edges, caller over-attribution) and the
bind NULLs `reference_name`, blinding panic-points (AC2's false negative).
Fix: make Rust method-call refs receiver-aware at extraction, and stop
name-only Pass-1 binding for them — without breaking legitimate in-crate
resolution (AC3).

## Core rule

Rust method calls (`call_expression` whose callee is a `field_expression`)
get a new in-memory extraction kind, `ExtractedReferenceKind::Method`,
which stores to the DB as the existing `'call'` string (zero DB surface
change). Two behaviors hang off it:

1. **Receiver derivation (extraction).** When the receiver's type is
   locally derivable, the ref carries `path = [TypeLastSegment]` →
   `reference_name "T::m"` → binds ONLY via the existing `qualified_exact`
   arm (verified: Rust methods carry `qualified_name = "Type::method"`;
   the arm is workspace-wide including same-file). Derivable receivers:
   - `self` / `&self` → the enclosing `impl` block's type (inherent and
     trait impls; generics stripped to the base name);
   - identifier with a type annotation in the same fn — `let x: T` or a
     typed parameter `x: T` (`&T`/`&mut T` stripped, path types by last
     segment, generic `T<U>` by base) — derived ONLY when the identifier
     is bound exactly once in the fn (any shadowing → unknown; deriving
     from the wrong branch would CREATE phantom qualified binds, so
     ambiguity degrades to unknown, never to a guess).
2. **Pass-1 skip (binding).** `Method` refs never consult the bare-name
   `name_to_id` map (the D10/macro-map precedent, third instance). They
   resolve in Pass 2: qualified ones via `qualified_exact`; bare
   (unknown-receiver) ones via the existing unique-or-decline name arms
   (`same_crate`, `unique_workspace`).

Consequences on the repro: `x.unwrap()` (annotated `Option<i32>`) →
`Option::unwrap` → no in-crate match → stays unresolved with its
qualified name (AC1); panic-points gains last-segment matching
(`reference_name = 'unwrap' OR LIKE '%::unwrap'`) so the site reports
(AC2); `t.unwrap()` (underivable `let t = Thing;`) → bare → Pass-2
`unique_workspace` → still binds `Thing::unwrap` (AC3, resolution target
unchanged; provenance label changes — see Decisions).

## Decisions (flagged for the design pause)

| # | Decision | Rationale |
|---|---|---|
| D1 | AC3 is interpreted as **binding targets unchanged**; strategy labels and confidence bands MAY change (`same_file` method binds become `qualified_exact` for derivable receivers, `unique_workspace` for unknown ones, or decline for workspace-ambiguous names) | The label is provenance, not resolution; the phantom channel IS Pass-1's label. Corpus effect: ~419 same-file method binds redistribute; ~400 unknown-receiver binds land in the speculative band, which is exactly the population tethys-k543 wants LSP to re-verify. `--exclude-speculative` callers output shrinks accordingly |
| D2 | Derivation scope: `self`/`Self` + single-binding type annotations (lets, params) only; constructor lets (`let t = Thing;`, `T::new()`, struct literals) are NOT derived | AC3 stays safe for underived shapes via Pass-2 name arms; constructor heuristics guess (capitalization, `Self::new`, generics) and a wrong guess creates NEW phantom qualified binds — the failure direction derivation must never take |
| D3 | Unknown-receiver method calls keep resolving (Pass-2 unique-or-decline), NOT blanket-declined | Probe: `unique_workspace` sample 4/4 true, ~400 binds at stake; blanket decline violates AC3. The residual risk (workspace-unique name that is also a std method) is inherited, measured, and is k543's LSP tier |
| D4 | panic-points matches the last segment of qualified `reference_name`s | Declined known-external calls carry `T::unwrap`; the raw `= 'unwrap'` filter would re-hide them (the exact AC2 trap, one layer up) |
| D5 | Rust only; C# reference extraction untouched | C# already folds receivers into qualified names; its Pass-1 bare-first hole for same-file variable receivers is the C# facet of tethys-0aqj, not this ticket |
| D6 | `ExtractedReferenceKind::Method` maps `to_db_kind()` → `ReferenceKind::Call` | Downstream (call_edges, deprecated-callers, panic-points, views, xebx fences keyed on `kind='call'`) sees no new wire string; the distinction lives only where it's needed — insert-time Pass-1 routing |

## Input shapes

Receiver shapes: `self` (incl. `&self`/`&mut self` — same `self` node),
identifier+let-annotation, identifier+typed-param, identifier bound twice+
(shadowed → unknown), identifier with no local type (unknown), identifier
bound in a closure param (counts toward shadowing, never derived),
`field_expression` receiver (unknown), `call_expression` receiver
(unknown), literal receiver (unknown), chained calls (each link's receiver
classified independently). Type-annotation shapes: bare `T`, `&T`/`&mut T`,
`path::to::T` (last segment), `T<U>` (base), `Self` (impl type), tuple/
slice/`impl Trait`/`dyn Trait`/fn-pointer (opaque → unknown, one-sentence
reason: no single nominal type to anchor `T::m`). Impl contexts: inherent,
trait-for-type, generic impl, method outside any impl (free fn — `self`
impossible), nested impls in fn bodies (enclosing-impl walk takes the
NEAREST). Callee shapes staying unchanged: bare identifier `foo()`, scoped
`T::m()`/`crate::a::f()`, macro callee. Duplicate `qualified_name` across
crates: inherited first-match imprecision of `qualified_exact`
(tethys-bvgb, filed from this design; unchanged here).

## Removed-invariant sweep (subtractive: Pass-1 no longer binds every same-file name match for method calls)

- "Same-file method calls always carry `same_file` strategy" — GONE by
  design (D1); tests pinning that label for method calls will fail and are
  updated deliberately, enumerated in the build audit.
- "A resolved method call implies a same-file or import-corroborated
  target" — unchanged for other kinds; for Method refs the qualified arm
  tightens this (type-anchored), the name arms keep unique-or-decline.
- "`callers` includes same-file method callers" — retained where the bind
  survives (qualified or unique); LOST for workspace-ambiguous names with
  underivable receivers (the `is_empty` class — on the corpus those are
  7/8 phantom, and the 1/8 true bind survives via `self` derivation). C9
  enumerates every lost edge for adjudication.
- panic-points/deprecated-callers/unused-imports consume `reference_name`s:
  more refs stay unresolved (names preserved) → recall can only improve;
  the qualified-name shape change is covered by D4 and deprecated-callers'
  existing `%::%` Path-B matching.
- Rust `value`/`macro`/`type`/`construct` refs and ALL C# refs: extraction
  paths untouched (C10 freezes them with baselines).

## Claims

1. **C1 (AC1)** In the repro, `x.unwrap()` with `let x: Option<i32>` stays
   unresolved with `reference_name = "Option::unwrap"`; no edge
   `use_external → Thing::unwrap` exists.
2. **C2 (AC2)** `panic-points` on the repro reports exactly 1 production
   point at `src/lib.rs:7`.
3. **C3 (AC3)** `t.unwrap()` with `let t = Thing;` still binds
   `Thing::unwrap` (strategy `unique_workspace`) and `use_internal →
   unwrap` remains the only call edge into it.
4. **C4** `self.m()` inside `impl T` (inherent and trait impls) binds
   `T::m` via `qualified_exact` — the corpus's one true `is_empty` bind
   (`types.rs:1224`) survives as a qualified bind.
5. **C5** Annotated receivers derive: `let w: Widget` / param `w: &Widget`
   / `w: lib::Widget` / `w: Widget<T>` all bind `Widget::m` cross- and
   same-file; a SHADOWED identifier (two bindings) is treated unknown.
6. **C6** Known-external annotated receivers decline: `let v: Vec<i32>;
   v.contains(..)` with an in-crate `contains` method produces NO bind.
7. **C7** Unknown-receiver method calls never bind Pass-1: a same-file
   method + unknown-receiver call resolves via Pass-2 (`unique_workspace`
   when unique, declined when ambiguous — the `is_empty` 2-candidate class
   declines everywhere).
8. **C8** Plain fn calls are untouched: `foo()` with same-file `fn foo`
   still binds `same_file`.
9. **C9** Corpus audit (tethys self-index): zero `is_empty`/`as_str`
   phantom binds remain; every call/construct bind change vs the
   pre-feature baseline is enumerated and adjudicated (phantom-removed /
   label-shifted-same-target / declined-ambiguous).
10. **C10** Non-method refs are frozen: construct/type/value/macro/import
    refs and the ENTIRE C# corpus (Tethys.Results baselines from xebx)
    are bit-identical pre/post.
11. **C11** DB surface unchanged: no new `refs.kind` string, no schema or
    view change; `Method` exists only in `ExtractedReferenceKind`.
12. **C12** Reindex idempotency: second index run over the repro and the
    self-corpus produces identical refs/edges for the new shapes.
13. **C13** deprecated-callers still surfaces method-call sites on both
    languages: existing Rust/C# fences green; a declined qualified
    `T::old()` call appears as a Path-B Maybe site via last-segment match.

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| premise | Rust methods carry `qualified_name = T::m`; `qualified_exact` is workspace-wide incl. same-file | query self-index; read the arm | SQLite rows (`SymbolId::as_i64`, `Index::connection`, `StalenessReport::is_empty`); `resolve.rs:592-596` | 5m | **passed** (2026-07-09; duplicate-name first-match edge found and filed as tethys-bvgb) | `self_receiver_binds_qualified_same_file` asserts the mechanism permanently |
| C1 | repro AC1 | rerun `probe1.sh` post-build; any bind of line-7 `unwrap` falsifies | rustc semantics (hand-derived, in ticket) | 5m | pending | integration `annotated_external_receiver_does_not_bind` |
| C2 | repro AC2 | `panic-points` on repro ≠ exactly `src/lib.rs:7` falsifies | grep (1 genuine `.unwrap()` outside the impl) | 5m | pending | integration `panic_points_sees_annotated_external_unwrap` |
| C3 | repro AC3 | `t.unwrap()` unbound or bound elsewhere falsifies; extra edges falsify | rustc semantics + ticket AC | 5m | pending | integration `underived_receiver_still_resolves_unique` |
| C4 | self derivation | fixture: two impls in one file, same method name; `self.m()` binding the OTHER type falsifies | hand-derived expected target | 15m | pending | unit `self_receiver_binds_qualified_same_file` + trait-impl variant |
| C5 | annotation derivation + shadowing | fixture matrix (`let`, param, `&T`, `path::T`, `T<U>`, shadowed) with adversarial same-named in-crate method; any wrong bind falsifies | hand-derived per-shape expected | 20m | pending | unit `annotated_receiver_matrix` (per-shape asserts) |
| C6 | external decline | fixture `Vec` annotation + in-crate `contains`; a bind falsifies | rustc semantics | 10m | pending | integration `annotated_external_receiver_does_not_bind` (second assert) |
| C7 | Pass-1 skip | fixture: same-file method + unknown receiver, ambiguous twin in another file; `same_file` strategy or ambiguous bind falsifies | SQL on fixture DB | 15m | pending | integration `unknown_receiver_skips_pass1_unique_or_decline` |
| C8 | fn calls untouched | fixture `fn foo` + `foo()`; strategy ≠ `same_file` falsifies | SQL | 5m | pending | existing suite (`tests/strategy.rs`, `tests/graph.rs` intra-file) + explicit assert in C7's fixture |
| C9 | corpus phantoms gone | pre/post diff of call-kind refs + call_edges vs baselines; unadjudicated diff falsifies | pre-feature baselines (captured at plan time) + probe3 phantom list | 30m | pending | `audit.md` enumeration; permanent form = C4/C7 fences (embed the phantom shapes) |
| C10 | non-method freeze | pre/post diff of non-call refs (self-corpus) + full C# corpus baselines | xebx baselines + new snapshots | 20m | pending | existing xebx fences + `tests/value_refs.rs` + C# suites |
| C11 | DB surface unchanged | `SELECT DISTINCT kind FROM refs` pre/post identical; schema_tests pass | SQL + existing schema fences | 5m | pending | `schema_tests` column/kind pins (existing) |
| C12 | reindex idempotent | double-index diff non-empty falsifies | SQL snapshot diff | 10m | pending | extend existing reindex fences to a method-call fixture |
| C13 | deprecated-callers intact | existing Rust+C# fences; new: obsolete method called via annotated receiver declined → Path B site | existing fences + hand-derived site | 15m | pending | existing `tests/deprecated_callers.rs` + `deprecated_method_declined_call_is_path_b_site` |

## Negative space

1. **No receiver-TYPE inference beyond single-binding local annotations** —
   no flow analysis, no field types, no return-type propagation, no
   constructor heuristics (D2); wrong-guess derivation creates phantoms in
   the qualified tier, the one tier that must stay precise.
2. **No LSP involvement** — re-verifying speculative binds with
   rust-analyzer is tethys-k543's scope.
3. **No C# changes** — the C# same-file bare-first hole is the C# facet of
   tethys-0aqj.
4. **No new DB kind/strategy/band** — provenance vocabulary is frozen
   (ADR-0003); the redistribution uses existing labels.
5. **No fix for duplicate-qualified-name first-match** (tethys-bvgb) or
   macro-context refs (tethys-9l27) — both filed, both orthogonal
   substrate issues.

## Deferral index (tracker-verified)

tethys-k543 (LSP re-verification tier), tethys-0aqj (kind-blind binding,
C# facet + general kind-aware work), tethys-bvgb (duplicate qualified
names, filed from this design), tethys-9l27 (macro-context refs, filed
from the probe), tethys-z9mr (import-decline interplay, adjacent only).
