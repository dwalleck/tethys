# Plan — tethys-s8hv (Scope A): index inline module bodies

Two slices. Slice 1 is the atomic indexing fix + its forced fence updates.
Slice 2 closes the visibility-tightening regression (INV-1), red-first.

---

## Slice 1: Recurse `MOD_ITEM` into inline module bodies

**Claim:** Symbols (esp. `#[cfg(test)] mod tests` unit tests) declared inside an
inline `mod { … }` are indexed; their refs re-attach to them; file-module
declarations (`mod foo;`) and nested/empty inline mods still index correctly.
(Design claims 1–5, 7.)

**Oracle:** `.tethys-s8hv/probe.sh` — is_test src/ count > 0, the three named
known unit tests present with is_test=1, independent of the extractor (SQL +
grep). Plus full `cargo nextest`.

**Stress fixture:** `tests/inline_module_indexing.rs` builds a fixture workspace with:
- `#[cfg(test)] mod tests { #[test] fn t() { product_fn(); } }` → `t` is a symbol
  with is_test=1 AND a resolved ref edge `t → product_fn` exists (edge attach).
- nested `mod a { mod b { fn c() {} } }` → `c` is indexed (recursion depth).
- `mod decl_only;` sibling file-module → module symbol present, no panic, not duplicated.
- empty `mod e {}` → module symbol, zero body symbols, no panic.
Expected outputs written in the test as assertions BEFORE implementing.

**Loop budget:** No new loop. The recursion reuses the existing single tree walk
(`extract_symbols_recursive`), now descending into `mod` bodies via the same
`node.children()` iteration the `_` arm already uses. Cost O(nodes-per-file),
unchanged; each node visited once (MOD_ITEM is a disjoint match arm — no double
visit). Production scale: total AST nodes across files; no threshold concern.

**Wall budget:** n/a (indexing is on-demand, not always-on). Spike measured
self-index 1.1s at +626 symbols — no regression concern.

**Files:**
- `src/languages/rust.rs` (the ~5-line recursion in the `MOD_ITEM` arm, ~line 954)
- `tests/inline_module_indexing.rs` (new fence)
- `tests/deprecated_callers.rs` (FORCED golden update: `resolved_sites_cross_file_and_top_level` — the tests-mod call now attaches to a unit-test symbol; update the expected caller. Not new logic — a golden-expectation change the indexing behavior forces. Justifies the 3rd file.)

**Code (advisory):**
```rust
MOD_ITEM => {
    if let Some(sym) = extract_simple_definition(node, content, SymbolKind::Module) {
        symbols.push(sym);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_symbols_recursive(&child, content, symbols, parent_name);
    }
}
```

**Verification:**
- [ ] Unit tests pass (`tests/inline_module_indexing.rs`)
- [ ] Stress fixture produces expected outcome (edge attach + nested + no-body + empty)
- [ ] `probe.sh` flips to ✅ (unit tests indexed, edges attach)
- [ ] Full `cargo nextest` green (deprecated-callers fence updated)
- [ ] Loop budget holds (no new loop; self-index time unchanged)

---

## Slice 2: Exclude same-crate unit-test usage from visibility-tightening (INV-1)

**Claim:** A `pub` item used ONLY by a same-crate `#[cfg(test)]` unit test is
STILL reported as a tightening candidate; an item used by product code OR by an
integration test (cross-crate) is NOT. (Design claim 6.)

**Oracle:** Fixture-built index + `tethys visibility-tightening --json`. Independent
of the SUT internals: grep the fixture confirms the only non-test caller is absent.

**Stress fixture:** `tests/visibility.rs` (extend) — fixture crate with:
- `pub fn only_unit_tested()` called only from `#[cfg(test)] mod tests` → MUST be a
  candidate (the regression case).
- `pub fn used_by_product()` called from a non-test fn → MUST NOT be a candidate
  (real usage still suppresses — guards against over-broadly dropping refs).
- `pub fn used_by_integration()` referenced from a `tests/` integration file →
  MUST NOT be a candidate (cross-crate test usage = real public API need).
Expected candidate set written BEFORE implementation.

**Red-first note:** Write the fixture and RUN it before touching `visibility.rs`.
If `only_unit_tested` already stays a candidate (existing root-reachable/same-pkg
logic absorbs it), the code change is unnecessary — ship the fence only. If it
regresses (predicted), add the exclusion.

**Smallest code change (advisory):** In the usage-evidence query that suppresses a
candidate (`src/db/visibility.rs`), exclude refs whose caller symbol is `is_test=1`
AND in the SAME package as the item. Keep cross-package refs (integration tests,
real consumers). Exact predicate determined in build against the fixture.

**Loop budget:** No new loop — adds a predicate to an existing per-candidate
evidence query. Cost O(refs) as before; production scale refs ≈ 10^4–10^5, single
indexed SQL pass. Within budget.

**Files:**
- `src/db/visibility.rs` (evidence-query predicate; only if fixture goes red)
- `tests/visibility.rs` (fixture fence)

**Verification:**
- [ ] Unit tests pass (all three fixture cases)
- [ ] Stress fixture: only_unit_tested IS a candidate; used_by_product and
      used_by_integration are NOT
- [ ] prove-it-prototype oracle (probe.sh) still ✅ from slice 1
- [ ] Full `cargo nextest` green
- [ ] Loop budget holds (no new loop)

---

## Plan Self-Review

1. **Loops:** Slice 1 — no new loop (reuses tree walk, O(nodes), within budget).
   Slice 2 — no new loop (SQL predicate on existing O(refs) query). No gaps.
2. **Fixtures:** Slice 1 fixture designed to fail on: missing recursion (edge
   won't attach), shallow recursion (nested `c` missing), no-body panic
   (`mod foo;`), empty-mod panic. Slice 2 fixture designed to fail on: over-broad
   ref-drop (used_by_product wrongly becomes candidate) and cross-crate confusion
   (used_by_integration wrongly becomes candidate). Both beyond happy-path. No gaps.
3. **Doc-comment preconditions:** None introduced (no new `callers must X` contracts).
   The MOD_ITEM recursion has no precondition; the visibility predicate is internal.
   No gaps.
4. **Write targets:** Slice 1 — test assertions only (no new println!). Slice 2 —
   candidates already flow to stdout as data via existing `--json`; no new writes.
   No gaps.
5. **Tracker references:** Deferrals cite tethys-0nar (proptest fns) and tethys-m7zm
   (unused-imports/deprecated-callers policy), both filed and verified. Slice 1's
   deprecated-callers fence update is the ACCEPTED policy for that analysis (test
   call sites count); the broader question is tethys-m7zm. No gaps.
