# Falsifiable design — tethys-6rlu

**Make `refs` name-queryable for resolved symbols, without breaking the consumers
that rely on `reference_name` meaning "unresolved".**

Anchored by `prove-it.md` (probe + ripgrep oracle agree on 5 functions, counts
1→59) and the subtractive sweep below.

## Purpose

Today `reference_name` is populated **only for unresolved refs** (initial store
sets it iff `symbol_id IS NULL` — indexing.rs:779, batch_writer.rs:329; Pass-2
nulls it — references.rs:157). So `SELECT … FROM refs WHERE reference_name=X` is
silently empty for any in-crate symbol that resolved. Goal: a name → refs query
that returns the real references (the kiro `make_service` gate's actual need).

## Decision: an additive `refs_named` SQL view — NOT overloading `reference_name`

The issue floated two options; the subtractive sweep (below) is decisive evidence
**against** option 1 (overload `reference_name` to always carry the name) and
**for** option 2 (a view):

```sql
CREATE VIEW IF NOT EXISTS refs_named AS
  SELECT r.id, r.symbol_id, r.file_id, r.kind, r.line, r.column,
         r.in_symbol_id, r.reference_name,
         COALESCE(r.reference_name, s.name) AS name
  FROM refs r
  LEFT JOIN symbols s ON r.symbol_id = s.id;
```

- `reference_name` keeps its exact current semantics (unresolved marker) → every
  existing consumer is untouched.
- `name` is computed **live** at query time → no denormalized copy to go stale on
  rename; full re-index already keeps it consistent.
- `LEFT JOIN` (not INNER) so unresolved refs (`symbol_id IS NULL`) still appear.

Implementation: add the view to `src/db/schema.rs`; document `refs_named` as the
name-query surface for external/ad-hoc SQL. No change to store or resolve paths.

## Input shapes (every production-reachable shape gets a claim)

Ref **resolution state** (the governing sum type):
- **resolved-at-store, same-file** (`symbol_id` Some, `reference_name` NULL today) → C1 (these are same-file call sites; view surfaces via `s.name`)
- **unresolved→resolved in Pass 2, cross-file** (`reference_name` nulled by resolve) → C1 (cross-module callers of `node_text` etc.)
- **never resolved, external, BARE** (`symbol_id` NULL, `reference_name` = bare name — e.g. `unwrap`) → C2
- **unresolved, QUALIFIED** (`symbol_id` NULL, `reference_name` = qualified path — e.g. `crate::helper`, `PathBuf::from`) → keyed by the **qualified path**, NOT the bare tail. A bare-name query (`name='helper'`) does NOT match these. Documented limitation, pinned by C1's fixture. For external qualified refs (`PathBuf::from`) this is arguably correct; for in-crate ones (`crate::helper`) it only persists because they fail to resolve — resolving them removes the limitation (tracked **tethys-3i35**).
- **`symbol_id` NULL AND `reference_name` NULL** → asserted impossible by the store invariant (references.rs:46 `symbol_id.is_some() || reference_name.is_some()`); view `name` would be NULL. Out of scope (cannot occur); noted, no claim.

Ref **kind**: `call` (C1), `type` / struct-constructor / others — the view's
`COALESCE` is kind-independent, so name-queryability holds for every kind; C1
pins `call`, the rest are covered by construction (fence asserts one non-call kind).

Symbol **name uniqueness**:
- unique name (`node_text` → 1 symbol) → C1
- colliding name (`from`, `new`, or a method+function sharing a name) → C7 (view returns the union)

## Subtractive sweep (this change touches the meaning of `reference_name`)

**Removed invariant (only if option 1 were taken):** "`reference_name` populated ⟺
ref is unresolved / points at no in-crate symbol." The view design **preserves**
this invariant (it never writes `reference_name`); the sweep's value is (a) proving
the view is safe and (b) producing regression fences that fail if a future refactor
"simplifies" the view back into an overload. Consumers that assume the invariant:

| Consumer | Uses | Under option 1 (overload) | Under the view |
|---|---|---|---|
| `panic_points.rs:51,103` | `WHERE reference_name IN ('unwrap','expect')`, **no symbol_id guard** | in-crate symbol named `unwrap`/`expect` resolves → gains name → **false panic site** | unchanged (C4) |
| `indexing.rs:1003` → `compute_dependencies_from_stored` | `refs_set` = `reference_name`s of file's refs; decides `is_used` per import → `file_deps` | resolved same-file names leak into `refs_set` → import wrongly "used" → **phantom file_dep** | unchanged (C5) |
| `references.rs:119` (LSP unresolved select) | `WHERE symbol_id IS NULL AND reference_name IS NOT NULL` | safe — `symbol_id` guard already present | unchanged (C6) |

## Claims

1. **C1 — name-query soundness (resolved + bare-unresolved).** For an in-crate symbol X, `SELECT count(*) FROM refs_named WHERE name=X AND kind='call'` equals the count of X's call sites that either **resolved** (keyed by `symbols.name`) or are **bare unresolved** (keyed by `reference_name`). Unresolved **qualified** calls (`crate::X()`) are keyed by their qualified path and are NOT matched by a bare-name query — documented limitation; removing it depends on resolving such calls (**tethys-3i35**). [Narrowed at Slice 2, 2026-06-30, after the original claim overreached — the prove-it probe's 5 functions were all fully resolved, so the unresolved-qualified shape was never exercised.]
2. **C2 — no loss for unresolved.** For an unresolved/external name X, `refs_named WHERE name=X` returns exactly the rows `refs WHERE reference_name=X` returned (LEFT JOIN keeps `symbol_id IS NULL` rows).
3. **C3 — additive (refs untouched).** A full dump of `refs` (all rows/columns) is byte-identical before and after the change.
4. **C4 — panic-points unchanged.** `tethys panic-points` output is identical before/after (no new false positives).
5. **C5 — file_deps unchanged.** The `file_deps` table is identical before/after (no phantom dependencies).
6. **C6 — Pass-2/LSP select unchanged.** The unresolved set `WHERE symbol_id IS NULL AND reference_name IS NOT NULL` is identical before/after.
7. **C7 — name collision = union.** For a name X shared by N symbols, `refs_named WHERE name=X` returns the union of refs resolving to any symbol named X plus unresolved refs named X, with no double-count.

## Falsification

| # | Claim | Falsifier (input → falsifying result) | Oracle (independent) | Cost | Status | Regression fence |
|---|-------|----------------------------------------|----------------------|------|--------|------------------|
| C1 | name-query soundness (resolved + bare-unresolved) | Index a fixture where free fn `foo` has K resolved/bare-unresolved call sites; if `refs_named WHERE name='foo' AND kind='call'` ≠ K → false. The fixture ALSO pins that an unresolved `crate::foo()` is keyed by `crate::foo` (limitation, tethys-3i35), so the test documents the boundary instead of papering over it. | ripgrep textual call-site count (`prove-it.md`: 5 fns, all resolved, counts 1–59, exact) + the row-level dump | 5m | **passed** (prove-it 5/5; Slice-2 fixture green after narrowing) | CI test `refs_named::name_query_counts_all_callsites_including_cross_file` — builds its OWN index (never an ambient DB) |
| C2 | no loss for unresolved | Replace LEFT with INNER JOIN; if `refs_named WHERE name='unwrap'` drops to 0 (vs 54) → false | pre-change `refs WHERE reference_name='unwrap'` (=54, distinct query path) | 5m | **passed** (view=54 == ref_name=54) | CI test `refs_named::unresolved_names_preserved` |
| C3 | refs untouched | `.dump refs` before/after; non-empty diff → false | sqlite `.dump` checksum | 10m | pending | CI test `refs_named::refs_table_unchanged` (snapshot of refs dump) |
| C4 | panic-points unchanged | Fixture defines in-crate method `unwrap` that's called; if post-change `panic-points` lists it → false | pre-change `panic-points` output | 20m | pending | CI test `panic_points::no_fp_from_in_crate_unwrap` (fixture embeds the bug class) |
| C5 | file_deps unchanged | Fixture: file imports `Bar` (unused) AND has a same-file resolved symbol `Bar`; if a `file_dep` to Bar's module appears → false | pre-change `file_deps` dump | 20m | pending | CI test `file_deps::no_phantom_from_resolved_name` (fixture embeds the bug class) |
| C6 | Pass-2/LSP select unchanged | Count `symbol_id IS NULL AND reference_name IS NOT NULL` before/after; differ → false | pre-change count | 10m | pending | CI test `references::unresolved_set_stable` |
| C7 | collision = union | Name X on 2 symbols; if `refs_named WHERE name=X` ≠ (refs→symbols(name=X)) ∪ (reference_name=X) or double-counts → false | two-term SQL decomposition (separate query) | 15m | pending | CI test `refs_named::collision_is_union` |

The cheapest claim (C1) is **passed**, so the design may proceed to planning.

### Regression-fence note (from prove-it "What I learned")
Every measurement fence above **builds its own index from a fixture inside the
test** and asserts against it. The on-disk `.rivets/index/tethys.db` was stale
(line numbers offset; one call site since deleted), which silently turned an
agreement into an off-by-one. A fence that queries an ambient DB measures the
wrong snapshot. Fences C4/C5 additionally embed the bug class (an in-crate
`unwrap`; an unused import colliding with a resolved name) so pre-fix-style code
(an overload impl) fails them and the view passes.

## Negative space (what this deliberately does NOT do)

1. Does **not** modify `reference_name` storage or resolution — option 1 (overload) is rejected; the sweep shows it breaks panic-points and file_deps.
2. Does **not** denormalize a stored name, so there is **no** incremental/rename staleness to manage — the view computes `name` live. (Sidesteps the staleness family entirely; cf. tethys-q8qw / tethys-gkt2, not depended on.)
3. Does **not** improve reference *extraction* coverage — the view only re-exposes refs that already exist. Functions used as values and identifiers inside macro token-trees remain unrecorded (**tethys-ygjx**); this also explains the 54-vs-74 `unwrap` textual gap (≥10 of the extras are `.unwrap()` inside `assert!` macros).
4. Does **not** change `get_callers` / `call_edges` / dead-code / hotspots — they already join `symbol_id` and never read `reference_name`.
5. Does **not** auto-migrate external consumers — callers must query `refs_named` (documented) instead of `refs`.

## Self-review

- **Claim count:** 7 (in 3–15 band). ✓
- **Independence:** C1 oracle = ripgrep (lexical, no resolver); C2 = distinct column-only query; C3–C6 = pre-change dumps/CLI; C7 = decomposed SQL. ✓
- **Non-vacuity (named buggy impl per fence):** C1 ← view forgets `kind`/miscounts; C2 ← INNER JOIN drops unresolved (54→0); C3 ← impl also writes store; C4 ← overload without symbol_id guard adds FP; C5 ← overload leaks resolved names into refs_set; C6 ← overload + a consumer drops the symbol_id guard; C7 ← `OR` instead of `COALESCE` double-counts. ✓ (None are schema-mutually-exclusive predicates or redundant disjuncts.)
- **Per-claim distinct output:** each fence asserts a different table/CLI/count; a failure localizes to one claim. ✓
- **Cost distribution:** no claim requires production data or multi-day soak; all ≤20m. ✓
- **Tracker refs:** tethys-ygjx (verified, open — covers extraction gaps incl. macro unwrap); tethys-q8qw/tethys-gkt2 (verified — incremental/staleness, explicitly NOT depended on). No un-tracked deferrals. ✓
- **Removed-invariant coverage:** the `reference_name`-meaning invariant → C3/C4/C5/C6 each a "still holds" claim with a fence. ✓
