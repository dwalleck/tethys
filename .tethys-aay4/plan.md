# tethys-aay4 — budgeted plan

Approved design: `.tethys-aay4/design.md` (D-A absorb dl7l, D-B accept
`Type::method` qualified names, D-C container-kinds-only parents). Cheapest
falsifier (probe) passed pre-approval.

Global oracle: `probe.py` — 719 same-file links / 109 NULLs / 0 ambiguous
on the real workspace; post-implementation the binary DB must reproduce the
pair set (item + aggregate) with the dl7l rule (type field).

## Slice 1: dl7l — one impl-identity function for both sides

**Claim:** C1, C2 (production side).
**Oracle:** probe pair set (type-field rule) vs post-fix DB qualified
names; the 23 formerly-oracle-only trait pairs must flip to agreement.
**Stress fixture:** unit tests — `impl Trait for Type` → parent Type;
inherent `impl Type` unchanged; generic `impl<T> Foo<T> for Bar<T>` → Bar;
`impl &mut Foo`-style non-nominal → None (type_base_name contract).
Red-first: the trait-impl case FAILS before the fix (records the trait).
**Loop budget:** none (function delegation).
**Files:** `src/languages/rust.rs` (`find_impl_type` → delegate to
`impl_type_base_name`; reconcile any tests pinning `Trait::` names —
deliberate per approved D-B, noted per-test).

**Verification:** unit tests pass; full suite (reconciliations
documented); no clippy/fmt drift.

## Slice 2: parent_name plumbing + insert-time linkage

**Claim:** C3, C4, C5, C6 (production side), C10 invariant.
**Oracle:** self-index rebuild → SQL counts vs probe (≈719 linked / ≈109
NULL, exact at the audit commit); spot item checks.
**Stress fixture:** e2e fences land in slice 3; this slice's check is the
self-index SQL oracle + suite.
**Loop budget:** per-file container map O(symbols-in-file); UPDATE pass
O(children-with-parent_name) via one prepared statement inside the existing
transaction — self-index scale ≈2.5k symbols total, trivially within 10^6.
**Files:** `src/parallel.rs` (`OwnedSymbolData.parent_name` +
`as_symbol_data`), `src/indexing.rs` (conversion keeps parent_name),
`src/db/files.rs` (`SymbolData.parent_name`; two-phase link in
`index_parsed_file_atomic`: collect `name → id` for container kinds —
Struct/Class/Enum/Trait/Interface/TypeAlias — where the name is UNIQUE
among containers in the file; second pass UPDATEs children; miss or
collision → NULL, collision also `trace!`). Mechanical struct-literal
updates in tests forced by the new field ride along.

**Verification:** suite green; self-index oracle counts match probe;
budgets hold.

## Slice 3: fences (`tests/parent_symbols.rs`)

**Claim:** C1–C10 e2e.
**Stress fixtures (expected outputs pre-written):**
- F-P1: workspace with struct+fields, enum+variants, inherent impl,
  and an impl ABOVE the type declaration (S5) — exact (child→parent) id
  asserts via SQL joins.
- F-P2: `impl Anchor for Widget` — method qualified_name = `Widget::m`,
  parent = Widget's id (red before slice 1).
- F-P3: receiver-typed call to a trait-impl method resolves
  `qualified_exact` (dl7l healing; red today).
- F-P4: same-file name collision (two same-named structs in one file via
  modules? — construct with a struct and same-named TRAIT: both container
  kinds → collision → NULL) + cross-file impl target → NULL.
- F-P5: C# class members + NESTED class member → innermost class id.
- F-P6: batch ≡ streaming canonical dumps including parent column;
  double-rebuild determinism.
- F-P7: reindex file A (holding the type), file B (holding a cross-file
  impl) — B's rows survive, links stay same-file-consistent.
**Files:** `tests/parent_symbols.rs` (new).

**Verification:** all fences; suite; budgets n/a.

## Slice 4: audit + closure (no production code)

**Claim:** C3 counts, C11, C12.
**Oracle:** probe re-run at audit commit vs binary DB (pair-set agreement,
counts); analyses output diff with/without... C12 uses pre/post-branch
binaries on the same tree (8ym0 worktree pattern) since parent data can't
be deleted independently; goldens status; changelog fragment rides the
review stage.
**Files:** `.tethys-aay4/audit.md`.

**Verification:** agreement recorded; analyses diff clean (modulo D-B
qualified-name spelling in any output that prints qualified names —
enumerate them); audit committed.

## Plan self-review

1. **Loops:** container map + UPDATE pass, O(file symbols), prepared
   statements, inside existing transactions. ✓
2. **Fixtures:** each attacks a class — trait-vs-type identity (F-P2 red
   -first), resolution healing (F-P3), order independence (F-P1/S5),
   collision (F-P4), nesting (F-P5), path parity + determinism (F-P6),
   cascade/reindex (F-P7). ✓
3. **Doc preconditions:** "parents are same-file by construction" becomes
   a doc-comment on the linkage fn — enforced by construction (the map is
   per-file); no runtime check needed beyond that. ✓
4. **Write targets:** no new CLI output; trace! diagnostics only. ✓
5. **Tracker refs:** tethys-dl7l (filed this run), tethys-j2r1,
   tethys-53iv, tethys-0nar, tethys-mpth, tethys-o4re, tethys-9z7i
   (Option-C precedent) — all verified. ✓
