# tethys-aay4 — prove-it-prototype findings

**Feature:** populate `symbols.parent_symbol_id` (currently 0/2555 non-NULL)
from the `parent_name` the extractors already capture but
`parse_file_static` drops (src/indexing.rs:726, src/db/files.rs insert).

## Probes

`probe.py` — independent tree-sitter walk (Python) over src/ + tests/ +
benches/ computing proposed (child, parent) pairs for methods (impl blocks,
`type` field), struct fields, and enum variants, resolved against same-file
container symbols. ORACLE: the DB's `qualified_name` column — built by a
different mechanism (the Rust conversion) — prefix-joined to same-file
symbols in SQL.

## The disagreement — and what it decomposed into

First run: oracle resolved 1 pair (my bug: shared sqlite cursor clobbered
the outer iteration — fixed, separate cursors). Second run: stale index
(line drift) + missing benches/ in the probe universe — rebuilt, widened.
Final decomposition of the remaining diff (shared=657):

1. **probe-only, fn-local structs** (JsonOutput/JsonSummary in cli render
   fns, BrokenPipeWriter): the extractor does not index fn-local items AT
   ALL (verified: zero symbol rows). Outside the feature's universe —
   parent linkage only applies to indexed symbols.
2. **oracle-only + probe-parent-mismatch, trait-impl methods**: REAL
   SUBSTRATE BUG, filed as **tethys-dl7l**. `find_impl_type` takes the
   first type_identifier of an impl — for `impl Trait for Type` that is
   the TRAIT. Stored: `ModuleResolver::file_anchor`; correct:
   `RustModuleResolver::file_anchor`. The refs side (53iv
   `impl_type_base_name`) uses the `type` FIELD and disagrees with the
   symbol side on every one of the 28 trait impls in this workspace —
   meaning receiver-typed refs to trait-impl methods can never match
   `qualified_exact` against the trait-prefixed symbol row.

## Measurements (self-index scale, design-driving)

- 859 symbols carry a parent prefix today (methods 318, struct_fields 412,
  enum_variants 129) — exactly the kinds the extractors thread parent_name
  for. Rust probe side: 828 pairs, **719 resolve same-file, 109 orphans, 0
  ambiguous**. Orphans = cross-file impls + fn-local containers; NULL is
  the correct value for them (document, don't fabricate).
- Trait methods declared in `trait` BODIES are not indexed at all
  (TRAIT_ITEM arm pushes only the trait symbol) — parent linkage cannot
  cover them; j2r1's suppression must key off impl-side methods.
- Both index paths (batch + streaming) share `parse_file_static` — one
  conversion point to fix, plus the insert loop needs a within-file
  name→id map (parents insert before children in extraction order for
  same-file containers; cross-file parents stay NULL).

## What I learned that I did not know before running the probe

> **The symbol side and the reference side of the indexer disagree about
> what an `impl Trait for Type` block belongs to — symbols say the trait,
> refs say the type. aay4 cannot ship parent linkage on top of that without
> baking the wrong parent into 28 impls' worth of methods, so tethys-dl7l
> (filed) is now in-scope as this feature's first slice.**

## prove-it-prototype hard gate
- [x] Probe runs against the real codebase (all .rs in src/tests/benches)
- [x] Oracle defined (DB qualified_name prefix join) and produces output
- [x] Agreement on a non-trivial slice: 657 shared pairs; every
      disagreement decomposed to a named class (probe bug → fixed; stale
      index → rebuilt; fn-local → out of universe; trait-impl → filed dl7l)
- [x] Non-obvious learning recorded (symbol/ref side disagreement)
