# tethys-aay4 — slice 4 audit (oracle closure)

Final branch state; workspace = tethys itself.

## Probe ⇄ binary agreement (C3)

Self-index: **699 links, 0 cross-file** (invariant query); **168
parent-prefixed symbols remain NULL** — cross-file impl targets plus
compound prefixes (enum-variant struct fields like `E::Bad`), the probe's
orphan class (it measured 109 on the Rust subset; the delta is compound
prefixes and benches/test universe edges). Pair-set diff vs the probe's
independent AST walk, fully decomposed:

| class | count | verdict |
|---|---|---|
| shared pairs | 695 | agree |
| binary-only: tuple-struct `0` fields (`SymbolId(i64)` newtypes) | 4 | binary correct; probe model gap (no `ordered_field_declaration_list` walk) |
| probe-only: fn-local structs (`JsonOutput` in render fns) | 26 | outside universe — extractor indexes no fn-local items (findings S8) |

## C12 — analyses, main-baseline binary vs branch binary, same tree

unused-imports, visibility-tightening, deprecated-callers, panic-points:
**IDENTICAL**. untested-code: **improves by 2** — `SymbolId::from` /
`FileId::from` (src/types.rs) were falsely untested because their symbols
were stored as `From::from` (tethys-dl7l), so the heavy test-side
`FileId::from(...)` qualified calls could not bind; with type-qualified
names they resolve (explicit_import/same_file/qmf strategies visible on
self-index) and the methods read tested. The predicted C2 healing
direction; not a regression.

## C9-perf (rides C11)

3× `index --rebuild`: main median 518ms vs branch median 456ms — the
linkage phase costs nothing measurable.

## Fence inventory

- Unit: `trait_impl_methods_parent_is_the_implementing_type` (red-first
  dl7l), closure shapes in `src/db/untested.rs` unaffected.
- `tests/parent_symbols.rs`: F-P1…F-P7 (7 fences) — incl. F-P3, the
  qualified_exact healing proof, and F-P7 reindex/cascade safety.
- idxperf goldens: deliberate 2-row update (C# members gained parents);
  all other rows byte-identical (C11).
- Suite 972/972; clippy pedantic; fmt.
