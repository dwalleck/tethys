# tethys-ygjx — budgeted plan: fn-as-value reference extraction

6 slices. Design: `.tethys-ygjx/design.md` (approved; Value refs EXCLUDED from
`call_edges`). Oracle throughout: `.tethys-ygjx/probe.py` (independent
tree-sitter walk) + `grep`/SQL. Build order is dependency order (1→6).

Gate per slice (checkpointed-build): `cargo nextest run`, clippy pedantic
`-D warnings`, `cargo fmt --check`, doctests, + the slice's stress fixture + the
probe oracle still agreeing + budget holds.

---

## Slice 1: Add the `Value` reference kind (types + string mapping)

**Claim:** A new ref kind `value` exists end-to-end (extraction enum → domain
enum → DB string → parse-back), so slices 3–5 can emit/query it. (Enables design
claims 1, 2.)
**Oracle:** Round-trip is independent of the DB: `ReferenceKind::Value.as_str()
== "value"` and `from_str("value") == Some(Value)`; the DB stores whatever
`as_str` returns.
**Stress fixture:** Unit test asserting `ReferenceKind::Value.as_str() == "value"`,
`ReferenceKind::from_str("value") == Some(Value)`, and
`ExtractedReferenceKind::Value.to_db_kind() == ReferenceKind::Value`. Bug class:
enum variant added in one place but not wired in `to_db_kind` / `from_str`
(silent mismatch → refs written as `value` but never parsed back).
**Loop budget:** No loop. Pure enum + match arms.
**Files:**
- `src/languages/common.rs` (add `ExtractedReferenceKind::Value` + `to_db_kind` arm)
- `src/types.rs` (add `ReferenceKind::Value`; `as_str` `Value => "value"`;
  `from_str` `"value" => Some(Self::Value)`; add `Value` to the
  `arb_known_reference_kind` proptest strategy at :2118 so property tests cover it)

**Code (advisory):**
```rust
// common.rs — enum + to_db_kind
/// Free-function / const / enum-variant identifier used as a value, not called.
Value,
// ...
Self::Value => crate::types::ReferenceKind::Value,

// types.rs — enum + as_str + from_str
Value,                         // in enum ReferenceKind
Self::Value => "value",        // in as_str
"value" => Some(Self::Value),  // in from_str
```

**Verification:**
- [ ] Unit tests pass (round-trip test above)
- [ ] Stress fixture produces expected outcome (all three round-trips hold)
- [ ] prove-it-prototype oracle unaffected (no extraction change yet; probe still agrees on current behavior)
- [ ] Budgets hold (no loop)
- [ ] `cargo build` + clippy pedantic clean (new variant may trigger non-exhaustive `match` warnings elsewhere — fix each, do not `_ =>` past them)

---

## Slice 2: `collect_local_bindings` helper (the suppression set)

**Claim:** For a `function_item` node, we can compute the set of identifier names
bound as locals (params, `let`, `for`, closure params, `if/while let`, `match`
patterns) — the guard that makes design claim 3 hold.
**Oracle:** `probe.py`'s `local_binding_names` (independent Python impl) computed
the same set; the Rust helper must produce the same names for the same fixture.
**Stress fixture:** A Rust function containing `fn f(param: T)`, `let (a, b) = x;`,
`for sym in items {}`, `.map(|closurearg| ..)`, `if let Some(y) = z {}`,
`match m { Named(inner) => .. }` → helper must return
`{param, a, b, sym, closurearg, y, inner}`. Bug class: for/closure/match bindings
missed — the probe proved `sym` (for-bound) leaks into false positives without
them (25 spurious refs on tethys src).
**Loop budget:** One walk of the function subtree per function.
`O(nodes_in_fn)`; summed over the file = `O(total AST nodes)` = `O(source bytes)`.
Production scale: source ≈ tens of MB → ~10^6–10^7 nodes total across a full
index, each visited once. Within the always-on indexing budget (the existing
extractor already walks every node once; this adds one more bounded walk per fn).
**Files:**
- `src/languages/rust.rs` (add `fn collect_local_bindings(fn_node, content) -> HashSet<String>` + a `pattern_idents` recursive helper)

**Code (advisory):**
```rust
/// Names bound as locals anywhere in `fn_node`'s body (whole-function
/// over-approximation — deliberately conservative; see design negative space #4).
fn collect_local_bindings(fn_node: &tree_sitter::Node, content: &[u8]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut cursor = fn_node.walk();
    // iterative pre-order; for parameter/let/for pull the `pattern` field,
    // for closure_parameters/match/let_condition pull identifier leaves.
    // (advisory — implementer may use a recursive helper)
    names
}
```

**Verification:**
- [ ] Unit test `collect_local_bindings_covers_all_binding_forms` passes
- [ ] Stress fixture produces `{param,a,b,sym,closurearg,y,inner}`
- [ ] Oracle: names match `probe.py` for the same fixture
- [ ] Loop budget holds (one bounded walk per fn)

---

## Slice 3: Emit value-position refs with local-binding suppression

**Claim:** A bare `identifier` in value position (call arg / `let` value / return)
that is not a callee/macro/field/scoped segment and not in the containing
function's local-binding set is emitted as a `Value` ref; a shadowed local is
not. (Design claims 1, 2, 3.)
**Oracle:** `probe.py` — over tethys src/, the extractor's `value`-kind refs must
match the probe's KEPT set (24: `row_to_*`, `saturating_depth_to_u32`,
`ignore_broken_pipe`) and never the SUPPR set (`sym`, `workspace`, `ctx`).
**Stress fixture:** three-part fixture crate:
(a) `v.into_iter().map(row_like)` where `fn row_like` is in-crate → **1 value ref**;
(b) `let g = target;` where `fn target` is in-crate → **1 value ref**;
(c) `helper(ctx)` where `ctx` is a **parameter** of the enclosing fn → **0 refs**
(name-collision guard). Expected outputs written here, pre-implementation. Bug
classes: name-collision (shadowed local emitted), `let`-value branch missing,
callee mis-emitted as value.
**Loop budget:** Suppression set computed once per `function_item` (Slice 2),
`O(1)` hash lookup per identifier. No new asymptotic cost beyond the existing
single recursion (`O(AST nodes)`); adds one `collect_local_bindings` walk per fn
(already budgeted in Slice 2).
**Doc-comment-as-contract:** the new `value_position_ref` helper's doc will state
"only call on `identifier` nodes." Classification: **sanity hint** — a non-identifier
node makes the parent-kind checks simply return `None` (no wrong output), so
`debug_assert!(node.kind()==IDENTIFIER)` is the correct enforcement, not a runtime
error.
**Output stream:** refs are data (persisted to DB), not stdout/stderr — N/A. Any
new `trace!` on a skipped identifier is diagnostic → stderr (EnvFilter), matching
existing `extract_call_reference` trace calls.
**Files:**
- `src/languages/rust.rs` (thread `local_bindings: Option<&HashSet<String>>`
  through `extract_references_recursive`; set it in the `FUNCTION_ITEM` and
  `IMPL_ITEM`-method arms alongside `containing_span`; add an `IDENTIFIER =>` arm
  calling a new `value_position_ref(node, content, local_bindings, containing_span)`)

**Code (advisory):**
```rust
IDENTIFIER => {
    if let Some(r) = value_position_ref(node, content, local_bindings, containing_span) {
        refs.push(r);
    }
}
// value_position_ref: return None if parent is call `function` field / macro
// `macro` field / let `pattern` / scoped_identifier / field_expression /
// field_identifier, or ancestor is token_tree, or name ∈ local_bindings.
// Return Some(Value ref) if parent is `arguments`, or let `value` field, or
// return_expression.
```

**Verification:**
- [ ] Unit tests: emit-arg, emit-let, suppress-shadow all pass
- [ ] Stress fixture: (a)=1, (b)=1, (c)=0 refs
- [ ] **Oracle: rebuild + run `probe.py`; extractor `value` refs == probe KEPT set, disjoint from SUPPR set** (the whole-branch check)
- [ ] Existing rust.rs ref tests unchanged (claim 5 locally)
- [ ] Loop/wall budgets hold

---

## Slice 4: Drop unresolved `Value` refs after Pass-2

**Claim:** A value-position identifier that resolves to no in-crate symbol leaves
no ref row. (Design claim 4 — "no spurious refs for non-symbol identifiers.")
**Oracle:** SQL `SELECT count FROM refs WHERE kind='value' AND reference_name=?`
— independent of the extractor; an unresolved value ref would carry its
`reference_name` (nulling only happens on resolve).
**Stress fixture:** index a crate with `helper(nonexistent_xyz)` where
`nonexistent_xyz` matches no symbol → after index, `count(kind='value',
reference_name='nonexistent_xyz') == 0`; AND a sibling `helper(real_fn)` →
`count(kind='value', symbol_id = real_fn) >= 1` (proves the DELETE is scoped to
unresolved, not all value refs). Bug class: DELETE too broad (drops resolved
value refs too) or absent (junk rows persist).
**Loop budget:** One SQL `DELETE ... WHERE kind='value' AND symbol_id IS NULL`.
No app-level loop; `O(refs)` inside SQLite over an indexed scan. Production:
≤ a few hundred value rows → negligible.
**Doc-comment-as-contract:** the new `Index` method doc states "call only after
all resolution passes complete." Classification: **load-bearing for correctness**
— calling it early would delete not-yet-resolved value refs (wrong output). But
the enforcement is *ordering*, not a value precondition: it is called from
exactly one site immediately before `populate_call_edges` (after resolution). No
runtime guard is meaningful (the method can't observe whether resolution ran); the
single call site + the doc + slice-6 determinism fence are the enforcement. Noted
explicitly so review doesn't read it as an unenforced contract.
**Output stream:** N/A (DB mutation); a `trace!(deleted=n)` is diagnostic → stderr.
**Files:**
- `src/db/references.rs` (add `pub fn drop_unresolved_value_refs(&self) -> Result<usize>`)
- `src/indexing.rs` (call it at line ~488, after resolution, before `clear_all_call_edges`/`populate_call_edges`)

**Code (advisory):**
```rust
// references.rs
pub fn drop_unresolved_value_refs(&self) -> Result<usize> {
    let conn = self.connection()?;
    Ok(conn.execute("DELETE FROM refs WHERE kind = 'value' AND symbol_id IS NULL", [])?)
}
// indexing.rs, before line 489:
let dropped = self.db.drop_unresolved_value_refs()?;
```

**Verification:**
- [ ] Unit/integration test `unresolved_value_ref_dropped` passes
- [ ] Stress fixture: nonexistent→0, real→≥1
- [ ] Oracle SQL agrees
- [ ] Budget holds (single DELETE)

---

## Slice 5: Exclude `Value` refs from `call_edges`

**Claim:** `callers`/`impact`/`deprecated-callers` are byte-unchanged by this
feature — resolved Value refs never become `call_edges`. (Design claim 6; the
approved decision.)
**Oracle:** `tethys callers <fn>` output + SQL `SELECT count FROM call_edges
WHERE callee_symbol_id=?` — both independent of the extractor.
**Stress fixture:** index a crate where `fn cb` is (i) called once `cb()` and
(ii) passed as a value twice `.map(cb)`. Expected: `call_edges` callee=`cb`
`call_count == 1` (only the real call), NOT 3. Bug class: the missing kind filter
lets the 2 value refs inflate the edge to count 3.
**Loop budget:** No new loop — one added `AND kind <> 'value'` predicate on the
existing aggregation SELECT. Same `O(refs)` scan.
**Files:**
- `src/db/call_edges.rs` (line 56: add `AND kind <> 'value'` to the
  `populate_call_edges` INSERT…SELECT WHERE clause; update the doc comment at
  :40-43 to state value refs are excluded)

**Code (advisory):**
```sql
FROM refs
WHERE in_symbol_id IS NOT NULL AND symbol_id IS NOT NULL AND kind <> 'value'
```

**Verification:**
- [ ] Integration test `value_refs_not_in_call_edges` passes
- [ ] Stress fixture: `cb` edge call_count == 1 (not 3)
- [ ] `tethys callers` on a known fn unchanged pre/post (spot check)
- [ ] Budget holds (one predicate)

---

## Slice 6: Cross-cutting fences (regression, out-of-scope, determinism, unused-imports)

**Claim:** The feature does not disturb existing behavior (claim 5), does not
extract macro tokens (claim 8, out of scope → tethys-8ym0), is deterministic
(claim 9), and keeps `unused_imports` correct (claim 7).
**Oracle:** SQL count-by-kind diff (independent of extractor); the existing
`unused_imports` test; line-based SQL for the macro fixture.
**Stress fixtures (each with pre-written expected output):**
- `existing_ref_kinds_unchanged`: index a fixture twice — once conceptually
  "pre" (assert per-kind floors for call/type/macro/construct/reexport are the
  same values the fixture had before value-refs existed) — implemented as an
  integration test asserting the 5 existing kinds' counts equal known constants
  for a fixed fixture crate. Bug class: value emission accidentally reclassifies
  or double-counts an existing kind.
- `macro_token_not_emitted`: fixture `dbg!(target)` where `fn target` is
  in-crate → assert **no** `kind='value'` ref on the macro invocation's line
  (line-based, robust to resolve-nulling). Bug class: recursion descends into
  `token_tree`.
- `value_ref_determinism`: index an unchanged fixture twice; the set of
  `(file,line,kind='value',symbol_id)` rows is identical. Bug class:
  nondeterministic ordering/dedup.
- claim 7: assert the existing
  `unused_imports::tests::function_passed_as_value_is_suppressed` still passes
  (findings empty) — now the import is used via the real ref path.
**Loop budget:** Tests only; no production loop.
**Files:**
- `tests/value_refs.rs` (new integration test file: the four fences above)
- (no source change; if claim 7 reveals `unused_imports` regressed, that fix
  becomes a new micro-slice — surfaced as drift per checkpointed-build)

**Verification:**
- [ ] `existing_ref_kinds_unchanged` passes
- [ ] `macro_token_not_emitted` passes (0 value refs on macro line)
- [ ] `value_ref_determinism` passes
- [ ] `function_passed_as_value_is_suppressed` still green
- [ ] Full suite `cargo nextest run` green; clippy pedantic `-D warnings`; fmt; doctests

---

## Plan Self-Review

**1. Every loop — complexity stated, within budget?**
- Slice 2 `collect_local_bindings`: `O(nodes_in_fn)` per fn, summed `O(AST nodes)` = `O(source bytes)`; one extra bounded walk per fn on top of the existing single extractor pass. Within always-on indexing budget. ✓
- Slice 3 emission: `O(1)` per identifier (hash lookup); no new asymptotic term. ✓
- Slices 4, 5: SQL scans `O(refs)` inside SQLite, no app loop. ✓
- No `O(?)` / unbounded loops. ✓

**2. Every fixture — what bug class, more than happy path?**
- S1: variant-wired-in-one-place-only (mismatch). S2: for/closure/match bindings missed (the proven `sym` leak). S3: name-collision + let-branch-missing + callee-mis-emit. S4: DELETE too broad / absent. S5: missing kind filter inflates edges. S6: reclassification, token_tree descent, nondeterminism, unused-imports regression. All adversarial, none happy-path-only. ✓

**3. Every doc-comment precondition — classified + enforced?**
- S3 `value_position_ref` "call on identifier only": **sanity hint** → `debug_assert!`. ✓
- S4 `drop_unresolved_value_refs` "after resolution only": **load-bearing ordering** → enforced by single call site + determinism fence (no meaningful runtime guard; noted, not a silent contract). ✓
- No unenforced correctness preconditions. ✓

**4. Every write target — data or diagnostic?**
- Refs → DB (data). New `trace!` calls → stderr (diagnostic), matching existing extractor traces. No stray `println!`. ✓

**5. Every tracker reference — resolves to a covering issue?**
- `tethys-8ym0` (macro-token out-of-scope) — exists, created 2026-07-04, covers cat2. ✓
- `tethys-i09d` (scoped-value) — exists, covers the jdly instance. ✓ (referenced in design negative space, not gating any slice)
- No un-filed deferrals in the plan. ✓

No gaps. Plan ready for checkpointed-build.
