# tethys-ygjx — falsifiable design: fn-as-value reference extraction

**Grounded in** `.tethys-ygjx/probe.py` + `findings.md` (probe/oracle agreement:
`row_to_symbol` has 13 value-uses, 0 recorded).

## Purpose

Emit `refs` rows for **free-function identifiers used as values** (category 1:
`iter.map(foo)`, `let g = foo;`) so a function used only as a value stops being a
false-positive for dead-code (tethys-dvsw) and stops being undercounted by
hotspots (tethys-7p54). Category 2 (macro token-tree) is **out of scope** →
tethys-8ym0. Scoped-path value uses are **out of scope** → tethys-i09d.

## Core mechanism

1. Add `ExtractedReferenceKind::Value` (`src/languages/common.rs`) → `refs.kind = 'value'`.
2. On entering a `FUNCTION_ITEM`, collect the set of **local binding names** for
   that function body (params, `let` patterns, `for` patterns, closure params,
   `if/while let` and `match` pattern idents — a whole-function over-approximation).
   Thread that set through the recursion alongside the existing `containing_span`.
3. A bare `identifier` in **value position** — child of `arguments`, the `value`
   field of a `let_declaration`, or child of `return_expression` — that is NOT a
   callee / macro name / field / scoped segment, and whose text is NOT in the
   containing function's local-binding set → emit a `Value` ref.
4. Pass-2 resolves `Value` refs by name like other refs. **Unresolved `Value`
   refs are dropped** (an unresolved value-position identifier is overwhelmingly a
   local or external, not a useful ref) — keeps the table clean and
   `reference_name` queries honest. Resolved `Value` refs bind to their symbol_id.
5. `Value` refs land in `refs` but **never** in `call_edges` (a value-use is not a
   call), so call-based analyses (`callers`, `impact`, `deprecated-callers`) are
   unchanged; dead-code / hotspots read the broader `refs` set and gain them.

   **Build requirement (found by the subtractive sweep):** `populate_call_edges`
   (`src/db/call_edges.rs:52-61`) currently consumes **all** resolved refs —
   `WHERE in_symbol_id IS NOT NULL AND symbol_id IS NOT NULL`, *no kind filter* —
   so resolved Value refs would leak in as fake calls. The build MUST add
   `AND kind <> 'value'` (or an explicit call-kind allowlist) to keep claim 6.
   Pre-existing fact, **out of scope to change**: `type`/`macro`/`construct`/
   `reexport` refs already flow into `call_edges` today.

## Input shapes (step 2)

The feature's input is a value-position `identifier` node. Reachable shapes:

| Shape | Example | Decision |
|---|---|---|
| bare fn name as call arg, not shadowed | `.query_map(.., row_to_symbol)` | **IN** → Value ref (claim 1) |
| bare name as `let` value | `let g = foo;` | **IN** → Value ref (claim 2) |
| bare name as return/tail expr | `return foo;` | IN (same path; negligible volume: 1) |
| name = local binding (param/let/for/closure/match) | `foo(ctx)`, `for sym in ..` | **suppressed** (claim 3) |
| name matches no in-crate symbol | `foo(external_thing)` | **dropped** unresolved (claim 4) |
| name = const / enum-variant / static | `foo(MAX_DEPTH)` | IN, resolves to that symbol (natural broadening; noted) |
| identifier inside macro token_tree | `dbg!(foo)` | **OUT** → tethys-8ym0 (claim 8) |
| scoped path in value position | `crate::Foo`, `T::assoc_fn` | **OUT** → tethys-i09d (negative space) |
| method reference (no call) | `obj.method` as value | OUT — needs closures, no consumer asking (negative space) |

Volume on tethys src/ (92 files): 271 non-locally-bound value-position bare
identifiers total (1.5% of 17892 refs); ~24 resolve to functions, rest to
other symbols or nothing (dropped). Modest scale.

## Removed-invariant sweep (step 2b)

Core move looks additive (+1 ref kind) but is **subtractive underneath**: it
removes the invariant *"a value-position identifier never appears in `refs`"*,
which three consumers silently relied on.

| Removed "can't happen" | Reader that assumed it | Still-holds claim |
|---|---|---|
| value idents never in `refs` → `refs` ≈ calls/types/macros | `call_edges` builder (`src/db/call_edges.rs`) — if it consumes all refs, not just call-kind, Value refs leak in as fake calls | **claim 6**: `callers`/`call_edges` for any fn unchanged |
| fn-as-value produces no ref → import looks unused unless textual guard fires | `unused_imports` (`function_passed_as_value_is_suppressed` test asserts this) | **claim 7**: import still USED, findings empty |
| ref counts per kind are stable | snapshot/diff-based tests, determinism | **claim 5**: existing 5 kinds' counts byte-identical |

## Falsification

| # | Claim | Falsifier (input → expected; falsifying result) | Oracle (independent) | Buggy impl it catches (non-vacuity) | Cost | Status | Regression fence |
|---|-------|-------------------------------------------------|----------------------|-------------------------------------|------|--------|------------------|
| 1 | Bare in-crate free fn as call arg (unshadowed) → ≥1 ref to its symbol_id, kind=`value` | fixture `.map(target)`; **query by symbol_id** (not name — 6rlu nulls name on resolve) for kind=value → ≥1. Zero ⇒ false | SQL `SELECT count FROM refs WHERE symbol_id=? AND kind='value'`; grep confirms the call sites | `_ =>` arm still ignores `arguments`-child idents → 0 rows | 10m | pending | integ test `value_ref_for_fn_as_arg` |
| 2 | Bare name in `let` value → Value ref | fixture `let g = target;`; query refs (name pre-resolve / symbol_id post) → present. Absent ⇒ false | SQL by (file,line,kind) | handles only `arguments`, skips `let value` field → missing | 10m | pending | integ test `value_ref_for_let_binding` |
| 3 | Local binding shadowing a symbol name → NO Value ref | partition all tethys-src value-position fn-name matches by scope guard; any local in KEPT or genuine callback in SUPPR ⇒ false | `probe.py` (independent tree-sitter) vs index fn-name set | emit without local-set check → `ctx`,`workspace` in KEPT | 5m | **passed** — 24 KEPT all genuine (`row_to_*`,`saturating_depth_to_u32`,`ignore_broken_pipe`), 59 SUPPR all locals (`sym`,`workspace`,`ctx`) | unit test `local_shadow_not_emitted` |
| 4 | Ident matching no in-crate symbol → no retained ref | fixture `foo(nonexistent_xyz)`; query refs name=`nonexistent_xyz` kind=value → 0. Present ⇒ false | SQL by reference_name (retained iff unresolved) | keeps unresolved value refs → row present | 10m | pending | integ test `unresolved_value_ref_dropped` |
| 5 | call/macro/type/construct/reexport counts unchanged | index tethys pre/post patch; diff per-kind counts for those 5 → 0. Non-zero ⇒ false | SQL `GROUP BY kind`, before vs after | value emission perturbs/reclassifies existing extraction | 15m | pending | **CI**: integ test `existing_ref_kinds_unchanged` asserting per-kind floors |
| 6 | Value refs don't inflate call-based analyses | `tethys callers row_to_symbol` + `SELECT count FROM call_edges` for its symbol, pre/post → unchanged. Increase ⇒ false | CLI `callers` + SQL call_edges count | `call_edges` builder consumes all refs incl. kind=value → +13 fake callers | 15m | pending | **CI**: integ test `value_refs_not_in_call_edges` |
| 7 | `unused_imports` still marks fn-as-value import USED | run existing `function_passed_as_value_is_suppressed` post-patch → passes (findings empty) | cargo nextest (existing test) | value ref path breaks the used-import logic → findings non-empty | 5m | pending | existing test `unused_imports::tests::function_passed_as_value_is_suppressed` |
| 8 | Macro-token identifiers → no Value ref (out of scope) | fixture `dbg!(target)`; assert NO kind=value ref on the macro's line (line-based, not name — robust to resolve-nulling) | SQL by (file,line,kind) | recursion descends into `token_tree` → value ref on macro line | 10m | pending | integ test `macro_token_not_emitted` |
| 9 | Re-index deterministic for Value refs | index unchanged workspace twice; diff kind=value rows → empty. Non-empty ⇒ false | SQL diff of two indexes | nondeterministic dedup/order in value emission | 10m | pending | integ test `value_ref_determinism` |

Cheapest falsifier (claim 3) **run and passed** before approval.

## Negative space (what this deliberately does NOT do)

1. **Macro token-tree identifiers** (`dbg!(foo)`, `vec![Bar::new()]`) — 591–893
   noisy candidates on tethys src; out of scope → **tethys-8ym0** (which now
   blocks tethys-y3bx, re-pointed from ygjx).
2. **Scoped-path value uses** (`crate::Foo` return, `T::assoc_fn` pointer) —
   distinct node type + resolver, 305 candidates; out of scope → **tethys-i09d**.
3. **Method references as values** (`obj.method` sans call) — needs closures in
   practice; no consumer requests it; settled rationale, no ticket.
4. **Block-precise lexical scoping** — suppression is a whole-function
   over-approximation (a name bound anywhere in the fn suppresses all its
   value-uses there). Deliberately conservative ("suppressions, not
   accusations"); may rarely over-suppress. Settled rationale.
5. **New cross-crate resolution** — Value refs resolve through the existing
   two-pass / k-hybrid path; no new cross-crate machinery.

## Notes / decisions to surface at the design pause

- **cat2 macro-tokens deferred** to tethys-8ym0; **y3bx re-linked** to depend on
  8ym0 (it was parked on the macro-token gap). Confirm this roadmap edit.
- **Naming/output posture**: new ref kind literal is `value`; new
  `ExtractedReferenceKind::Value`. Value refs excluded from `call_edges`;
  included in the general `refs` set that dead-code/hotspots read.
- **Drop-unresolved-Value** is a deliberate precision choice (claim 4). Alternative
  (retain unresolved as speculative band) rejected: adds ~250 junk rows and
  pollutes `reference_name` queries for no consumer benefit.
- **DECIDED (2026-07-04, user): EXCLUDE Value refs from `call_edges`** — add
  `AND kind <> 'value'` to `populate_call_edges`. `callers`/`impact`/
  `deprecated-callers` byte-unchanged; claim 6 holds. Design approved as-is.
- **DECISION FOR THE USER — Value refs and `call_edges`.** The subtractive sweep
  found `populate_call_edges` has no kind filter, so the fork is real:
  - **(Recommended) Exclude Value from `call_edges`** — add `AND kind <> 'value'`.
    `callers`/`impact`/`deprecated-callers` output is byte-unchanged (claim 6);
    dvsw/hotspots read `refs` directly. Conservative, zero regression to
    user-facing call tools. Costs one WHERE clause.
  - **(Alternative) Let Value flow into `call_edges`** like type/macro already do —
    zero call_edges change, and a value-use of a deprecated fn would then show in
    `deprecated-callers`. But it changes `callers` counts for ~24 functions and
    conflates "used as a value" with "called." Claim 6 would be dropped.
  Recommendation: exclude. Surface at pause; the answer flips claim 6.
