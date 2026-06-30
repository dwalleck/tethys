# Budgeted plan — tethys-6rlu (`refs_named` view)

Implements `.tethys-6rlu/design.md`. Approved design, cheapest falsifier passed.

**Shape of the work:** the code change is ~8 lines (one `CREATE VIEW` in
`src/db/schema.rs`). Everything else is regression fences. The view auto-applies
to existing DBs — `execute_batch(SCHEMA)` runs on every `Index::open`
(src/db/mod.rs:113) and every statement is `IF NOT EXISTS`, so **no migration and
no `--rebuild` required**. Pattern mirrors the existing `arch_coupling` view
(schema.rs:163) and its tests in the `schema_tests` module.

Claim coverage: C1→S1,S2 · C2→S3 · C3→S7 · C4→S5 · C5→S6 · C6→S7 · C7→S4.

---

## Slice 1: Create the `refs_named` view (+ doc comment)

**Claim:** C1 (structural) — `refs_named` exists and `name = COALESCE(reference_name, symbols.name)` over a LEFT JOIN; a resolved ref's name is its symbol's name.
**Oracle:** `sqlite_schema` query (view object exists, like the `arch_coupling` test) + a hand-built in-memory row whose expected `name` is computed by hand.
**Stress fixture:** in-memory DB (via existing `open_test_conn()`); insert one *resolved* ref (`symbol_id`→a symbol named `alpha`, `reference_name` NULL). Expected: `SELECT name FROM refs_named` = `alpha`. **Bug targeted:** a view that reads only `reference_name` (would yield NULL for resolved refs) — caught because expected is `alpha`, not NULL.
**Loop budget:** No new always-on loop. The view query is `O(refs)` (full LEFT-JOIN scan; a view's computed `name` column is not indexable). Production scale: refs up to ~10^6 on a huge repo. This is an **ad-hoc / external query surface, not an always-on phase**, so the 10^6-op always-on ceiling does not apply; a one-off `WHERE name=…` scan of 10^6 rows is tens of ms in SQLite. Schema-apply adds one `CREATE VIEW` = O(1). Documented trade-off vs the rejected indexed denormalized column (which reintroduces the sweep breaks).
**Wall budget:** n/a — no always-on phase; schema-apply cost is one extra DDL statement (negligible).
**Files:** `src/db/schema.rs`

**Code (advisory):**
```sql
-- Name-queryable view over refs. reference_name is populated ONLY for
-- unresolved refs (resolved refs carry it as NULL by design); this view
-- restores name-queryability by falling back to the resolved symbol's name.
-- LEFT JOIN so unresolved refs (symbol_id NULL) still appear. Query THIS
-- for name lookups instead of refs.reference_name. (tethys-6rlu)
CREATE VIEW IF NOT EXISTS refs_named AS
  SELECT r.id, r.symbol_id, r.file_id, r.kind, r.line, r.column,
         r.in_symbol_id, r.reference_name,
         COALESCE(r.reference_name, s.name) AS name
  FROM refs r
  LEFT JOIN symbols s ON r.symbol_id = s.id;
```
Unit test in `schema_tests`: assert `count_object("refs_named","view")==1`; insert files/symbols/refs rows; assert resolved ref's `name`.

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixture (resolved ref → `name='alpha'`) produces expected
- [ ] prove-it oracle still agrees (re-run `probe.py` + `oracle.sh` after `cargo build` + reindex → 5/5 AGREE)
- [ ] Budgets hold (view is O(refs) ad-hoc; no always-on loop added)

---

## Slice 2: C1 soundness integration fence

**Claim:** C1 — `SELECT count(*) FROM refs_named WHERE name=X AND kind='call'` equals X's real call-site count, including cross-file callers.
**Oracle:** the hand-counted K of calls placed in the fixture (independent of the resolver), cross-checked against the `symbol_id` join.
**Stress fixture:** `workspace_with_files` with TWO crates/files: `a.rs` defines `pub fn helper() {}` and calls `helper()` once; `b.rs` imports and calls `helper()` 3×. Expected `name='helper' AND kind='call'` = **4**. **Bug targeted:** cross-file resolved calls not counted (the zp2j-style miscount) → would yield 1, not 4. **Empty-case companion:** assert a defined-but-never-called `fn lonely()` yields count **0**.
**Loop budget:** test indexes a 2-file fixture → `O(files)=O(1)`; SQL assert `O(refs_fixture)` ≈ tens of rows. No production loop.
**Wall budget:** n/a (test-only).
**Files:** `tests/refs_named.rs` (new; local `workspace_with_files` helper mirrors existing pattern — consolidation tracked at tethys-dzn8)

**Code (advisory):** build index via `workspace_with_files`, open `rusqlite::Connection` to `tethys.db_path()`, run the two `COUNT` queries, `assert_eq!(4, …)` and `assert_eq!(0, …)`. Must build its OWN index — never an ambient DB (prove-it learning: the on-disk DB was stale).

**Verification:**
- [ ] Unit/integration test passes (count==4; lonely==0)
- [ ] Stress fixture produces expected (cross-file calls counted)
- [ ] prove-it oracle still agrees with binary
- [ ] Budgets hold (O(1) fixture)

---

## Slice 3: C2 — LEFT JOIN preserves unresolved refs

**Claim:** C2 — an unresolved/external ref (`symbol_id` NULL, `reference_name='X'`) still appears in `refs_named` with `name='X'`.
**Oracle:** the pre-existing `refs WHERE reference_name='X'` query path (distinct from the join).
**Stress fixture:** in-memory DB; insert one *unresolved* ref (`symbol_id` NULL, `reference_name='extern_fn'`, `kind='call'`). Expected: `refs_named WHERE name='extern_fn'` returns 1 row. **Bug targeted:** INNER JOIN instead of LEFT (drops `symbol_id IS NULL` rows) → returns 0.
**Loop budget:** no loop; O(1) in-memory rows.
**Wall budget:** n/a.
**Files:** `src/db/schema.rs` (`schema_tests`)

**Verification:**
- [ ] Unit test passes (unresolved row surfaces by name)
- [ ] Stress fixture: INNER-JOIN mutation makes it fail (non-vacuity check)
- [ ] prove-it oracle still agrees
- [ ] Budgets hold

---

## Slice 4: C7 — name collision returns the union (no double-count)

**Claim:** C7 — for a name X on N symbols, `refs_named WHERE name=X` returns the union of refs resolving to any symbol named X (plus unresolved refs named X), each ref once.
**Oracle:** two-term SQL decomposition computed separately: `(refs→symbols WHERE symbols.name=X) ∪ (refs WHERE reference_name=X)`.
**Stress fixture:** in-memory DB; two symbols named `dup` (a `function` in file1, a `method` in file2); one resolved ref to each. Expected: `refs_named WHERE name='dup'` = **2**. **Bug targeted:** (a) a join that picks only one symbol (→1); (b) a non-COALESCE `OR` form that double-counts a row (→3). Also assert a single ref id appears exactly once (`COUNT(DISTINCT id)==COUNT(*)`).
**Loop budget:** no loop; O(1).
**Wall budget:** n/a.
**Files:** `src/db/schema.rs` (`schema_tests`)

**Verification:**
- [ ] Unit test passes (count==2, no dup ids)
- [ ] Stress fixture distinguishes union from pick-one and from double-count
- [ ] prove-it oracle still agrees
- [ ] Budgets hold

---

## Slice 5: C4 — panic-points invariance (sweep guard)

**Claim:** C4 — adding the view does not change `panic-points` output; an in-crate symbol named `unwrap`/`expect` is NOT reported as a panic site.
**Oracle:** the documented panic-points contract (only external std `.unwrap()/.expect()` are panic sites); equivalently, panic-points output is byte-identical to the pre-PR run on the same fixture.
**Stress fixture:** `workspace_with_files`: a struct with an inherent `fn unwrap(&self) {}`, a call `thing.unwrap()` on it (in-crate), AND a genuine `Option::unwrap()` call (external). Expected: `tethys panic-points` lists the `Option::unwrap` line, NOT the in-crate `thing.unwrap()` line. **Bug targeted:** a future overload-of-`reference_name` impl gives the in-crate `unwrap` a name → it shows up as a false panic site → fixture fails.
**Loop budget:** test indexes a small fixture → O(1). Panic-points query is the existing `WHERE reference_name IN ('unwrap','expect')` — unchanged, O(refs).
**Wall budget:** n/a.
**Files:** `tests/refs_named.rs`

**Verification:**
- [ ] Integration test passes (in-crate unwrap absent from panic-points)
- [ ] Stress fixture fails under a simulated overload impl (non-vacuity)
- [ ] prove-it oracle still agrees
- [ ] Budgets hold

---

## Slice 6: C5 — file_deps invariance (sweep guard)

**Claim:** C5 — adding the view does not introduce phantom file dependencies; an unused cross-file import whose name collides with a same-file resolved symbol does NOT create a dependency edge.
**Oracle:** the pre-PR `file_deps` for the fixture (the unused import yields no edge).
**Stress fixture:** `workspace_with_files`: `a.rs` defines `pub struct Bar;`; `b.rs` has `use crate::a::Bar;` but NEVER references the imported `Bar`, while `b.rs` defines and references its OWN same-file `struct Bar`/`fn` named so a resolved same-file ref named `Bar` exists. Expected: no `file_dep` from `b.rs` → `a.rs` (import is unused). **Bug targeted:** overload-`reference_name` leaks the same-file resolved `Bar` name into `refs_set` (indexing.rs:1003) → import judged "used" → phantom `b→a` edge appears → fixture fails.
**Loop budget:** O(1) fixture; dependency computation is the existing per-file path, unchanged.
**Wall budget:** n/a.
**Files:** `tests/refs_named.rs`

**Verification:**
- [ ] Integration test passes (no phantom b→a dep)
- [ ] Stress fixture fails under simulated overload (non-vacuity)
- [ ] prove-it oracle still agrees
- [ ] Budgets hold

---

## Slice 7: C3 + C6 — `reference_name` still means "unresolved" (root invariant)

**Claim:** C3 (refs untouched) & C6 (unresolved-set unchanged) — no resolved ref carries a `reference_name`; the unresolved set is exactly `symbol_id IS NULL AND reference_name IS NOT NULL`.
**Oracle:** direct SQL invariant over a freshly indexed fixture (distinct from the view).
**Stress fixture:** `workspace_with_files` with both resolved (in-crate `helper()` calls) and unresolved (`.unwrap()`) refs. Expected: `SELECT COUNT(*) FROM refs WHERE symbol_id IS NOT NULL AND reference_name IS NOT NULL` = **0** (no resolved ref is named), AND `SELECT COUNT(*) FROM refs WHERE symbol_id IS NULL AND reference_name IS NOT NULL` **> 0** (unresolved refs still named). **Bug targeted:** any impl that overloads `reference_name` onto resolved refs → first count > 0 → fail. This is the *root* guard that C4/C5 depend on; C4/C5 add per-consumer localization.
**Loop budget:** O(1) fixture; two SQL counts O(refs_fixture).
**Wall budget:** n/a.
**Files:** `tests/refs_named.rs`

**Verification:**
- [ ] Integration test passes (resolved-named count==0; unresolved-named>0)
- [ ] Stress fixture fails under overload impl (non-vacuity)
- [ ] prove-it oracle still agrees
- [ ] Budgets hold

---

## Plan Self-Review

**1. Every loop — complexity stated & within budget?**
- View query (S1): `O(refs)` LEFT-JOIN scan; ad-hoc/external surface, NOT always-on → 10^6 ceiling N/A; ~tens of ms at 10^6. ✓
- Schema-apply (S1): +1 `CREATE VIEW`, O(1). ✓
- S2–S7: only test-fixture indexing, `O(files)=O(1)`, and SQL counts over tens of fixture rows. ✓
- No `O(?)` / unbounded loops introduced. ✓

**2. Every fixture — bug class it's designed to fail under?**
- S1: view reads only `reference_name` (resolved→NULL name). ✓
- S2: cross-file resolved calls uncounted (zp2j-style) + empty (never-called→0). ✓
- S3: INNER vs LEFT JOIN drops unresolved. ✓
- S4: name collision — pick-one (under) and OR-double-count (over). ✓
- S5: overload makes in-crate `unwrap` a false panic site. ✓
- S6: overload leaks resolved name → phantom file_dep. ✓
- S7: overload writes name onto resolved refs. ✓
- None are happy-path-only. ✓

**3. Every doc-comment precondition — classified & enforced?**
- View comment "query refs_named instead of refs.reference_name" = **caller guidance for humans**, not a function precondition; no runtime enforcement applicable (it's documentation, not a code path). ✓
- The view's correctness relies on the store invariant `symbol_id.is_some() || reference_name.is_some()` (references.rs:46) so `name` is never NULL — that precondition is **load-bearing and already runtime-enforced** by the existing `InsertRefParams` assert; S7 additionally fences the resolved-side (`symbol_id NOT NULL ⟹ reference_name NULL`). No new unenforced precondition added. ✓

**4. Every write target — data or diagnostic?**
- The view is **data**, consumed via SQL (the user's `sqlite3`/`jq` pipeline use case). ✓
- Tests assert; **no new `println!`** to any stream. ✓
- No change to existing stdout/stderr split. ✓

**5. Every tracker reference resolves to a covering issue?**
- **tethys-ygjx** (open, verified) — extraction gaps incl. macro-token `.unwrap()` (the 54-vs-74 gap); referenced in design negative space, not a dependency. Covers it. ✓
- **tethys-dzn8** (open, verified) — `workspace_with_files` duplication; new test helper follows the existing pattern, consolidation tracked there. Covers it. ✓
- No un-tracked deferrals. ✓

No gaps in any of the five lists.
