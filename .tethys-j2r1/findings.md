# tethys-j2r1 — prove-it-prototype findings

**Feature:** type-hierarchy edges (Rust `impl Trait for Type`, supertrait
bounds; C# base lists) + `get_type_hierarchy` + `tethys hierarchy`. Nothing
exists today: `ReferenceKind::Inherit` is in the enum but NO extractor emits
it (0 rows on self-index, 0 code references).

## Probe / oracle

`probe.py` — AST walk over src/+tests/+benches/ computing the proposed edge
set. Oracle: grep `^impl .* for` + trait-bound declarations, hand-decomposed.

- **Implements: 27 edges.** Grep says 32 — the 5-line surplus is `impl …
  for` text inside test-fixture STRING literals (this branch's own dl7l
  fixtures among them); the AST side is correct. Item check:
  module_resolver.rs's 2 known trait impls found by both. AGREE.
- **Extends (supertraits): 10 edges — grep-exact** (5 declarations × 2
  bounds), every one `Send`/`Sync` (external marker traits).

## Design-driving measurements

- Subtype (the `impl … for TYPE` end) is same-file for **24/27** — an
  `in_symbol` anchor works same-file; 3 cross-file degrade to NULL.
- Supertype is EXTERNAL for **21/27** (`Display` ×8, `From` ×5, `Write`,
  `Error`, `Drop`…) and 10/10 of supertraits — so unresolved Inherit refs
  MUST be retained (contra `value`/`macro_call`): "implements *something*,
  even external" IS the dead-code suppression signal (`Display::fmt` with
  zero direct calls is the canonical false positive).
- Method-level precision gap: parent linkage says a method belongs to a
  TYPE, but not whether its impl block was a TRAIT impl — an inherent
  method of a type that also has trait impls must NOT be suppressed. The
  extraction site (the impl walk) knows; the schema needs it recorded at
  method granularity.
- Zero Inherit rows today → `populate_call_edges` would swallow resolved
  inherit refs as CALLS unless excluded (the 8ym0 exclusion-list lesson).

## What I learned that I did not know before running the probe

> **The suppression consumer needs edges at TWO granularities: type-level
> (hierarchy walks) AND method-level (an inherent method of a
> trait-implementing type must not inherit the suppression), and the
> dominant supertypes are EXTERNAL — retention of unresolved inherit refs
> is the load-bearing posture decision, opposite to value/macro_call.**

Gate: probe ran on real code ✓; oracle agrees after decomposition (string
-literal decoys named) ✓; non-obvious learning recorded ✓.
