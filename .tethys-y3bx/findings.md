# prove-it-prototype findings — tethys-y3bx (untested-code)

## Smallest question
For symbols with a known answer, does multi-root forward BFS from `is_test`
roots (over the reference graph) classify tested vs untested correctly?

## Probe (`.tethys-y3bx/probe.py`, against merged-s8hv self-index)
Multi-root forward BFS from the 814 is_test roots over `refs`
(in_symbol_id → symbol_id). Untested = product fns/methods (is_test=0, kind in
function/method) not in the reachable closure.
- roots=814, product fns=647, reachable=1325, **untested=251**.

## Oracle (grep-trace — independent of the refs graph)
- `is_excluded_dir` → probe: TESTED. grep: called via `assert!(Tethys::
  is_excluded_dir(...))` inside `#[test]` fns (indexing.rs:1447+). ✓ AGREE.
- `print_reachability_result` → probe: UNTESTED. grep: sole caller is
  `cli/reachable.rs::run()` (non-test). ✓ AGREE.

## Key findings (design-shaping)
1. **`refs` and `call_edges` yield IDENTICAL untested sets** — untested(refs)=251,
   untested(call_edges)=251, set difference = 0. The AC's premise ("consume refs,
   not only call_edges — call_edges skips top-level refs") does NOT manifest on
   real data: no product fn is covered only via a top-level (in_symbol_id NULL)
   reference from a test path. **Design implication:** reuse the existing
   `bfs_reachable`/`get_forward_reachable` machinery (call_edges) — a bespoke
   refs traversal buys nothing here. To honor the AC's letter, either traverse
   refs anyway (same result, no downside) OR document the measured equivalence
   and add a CONSTRUCTED fixture that pins the top-level-ref case.
2. **~7 of 251 untested are known-limitation false positives**: `arb_*` proptest
   generators (called only from `proptest!` macros → tethys-0nar → no edges) and
   `benches/` helpers. Composition limitation to document, like ygjx.

## What I learned that I didn't know before probing
The `refs`-vs-`call_edges` distinction the AC is architected around produces zero
difference on this codebase — untested-code is an assembly of existing
reachability infra (multi-root forward BFS + set complement), not a new
traversal. The real accuracy risks are the pre-existing refs gaps (ygjx, 0nar),
not the call_edges/refs choice.

## Oracle agreement: YES (2/2 slices). Gate passed.

## BLOCKER found by the cheapest falsifier (falsifiable-design step 6)
A fixture where a `#[test]` calls `helper()` inside `assert_eq!(helper(), 1)` —
the dominant Rust unit-test pattern — reports helper AND target UNTESTED. The
test's only captured ref is `assert_eq` (kind=macro); the `helper`/`target`
identifiers INSIDE the macro token tree produce NO ref at all (verified: a plain
`let _ = helper();` control DOES create the edge and covers helper).

Root cause: **tethys-ygjx category 2** — "identifiers (including calls) inside a
macro invocation's token tree are not parsed into call_expression nodes." The
y3bx issue listed ygjx as a related, "acceptable" limitation; the probe shows
it is NOT acceptable for untested-code — assert-macro calls are how most unit
tests exercise product code. Real-data evidence: `as_str` (13 test-context grep
hits), `as_i64` (11), `args` (7) are flagged untested despite heavy assert use.
Pure fns with production callers (normalize_path) survive; fns tested ONLY via
asserts do not.

Also confirmed here (spec correction): `call_edges` is `SELECT in_symbol_id,
symbol_id FROM refs WHERE both NOT NULL` (call_edges.rs:53) — NOT kind-filtered.
So refs≡call_edges for forward-from-test reachability; AC #2's "must use refs,
a fixture pins a case call_edges misses" is unsatisfiable (no such case exists).

## DECISION (2026-07-04): PIVOT — fix tethys-ygjx (cat 2) first, then resume y3bx.
y3bx is blocked-by ygjx. This probe (refs≡call_edges, 251 untested, the textual
-guard posture option) is preserved for the resumed y3bx pipeline.

---

# RESUMED probe (2026-07-15, post tethys-8ym0, fresh index on merged main)

Substrate changed since the 2026-07-04 probe: tethys-8ym0 shipped macro-token
call refs (kind `macro_call`, EXCLUDED from call_edges), tethys-53iv shipped
receiver-typed method resolution. Re-measured everything; the old probe's
central finding is now FALSE by design.

## Q1 — refs vs call_edges (the AC #2 premise)

- untested(refs) = **235**; untested(call_edges) = **266**; gap = **30**.
- The 2026-07-04 finding "refs ≡ call_edges, AC #2 unsatisfiable" is
  OVERTURNED: macro_call refs exist only in `refs`, so the analysis MUST
  traverse refs (or a view) — call_edges misses every assert-only-tested fn.
  AC #2's fixture case is now real (tests/macro_token_refs.rs F1 fixture).
- Cross-validation: 235 exactly matches the independent prediction from
  `.tethys-8ym0/probe2.py` (260 → 235 with bare-call edges).

## Q2 — item checks (grep-trace oracle, independent of the refs graph)

| symbol | probe | oracle (grep) | agree |
|---|---|---|---|
| crate_glob_covers | TESTED | 8 assert-context sites in visibility.rs | ✓ |
| scalar (tests/value_refs.rs) | TESTED | 11 assert sites (8ym0 oracle slice) | ✓ |
| is_excluded_dir | TESTED | assert!(Tethys::is_excluded_dir(...)) | ✓ |
| print_reachability_result | UNTESTED | sole caller cli/reachable.rs:31 run(), non-test | ✓ |

## Q3 — composition of the 235 (design-driving)

src core **152**, src/cli+main **43**, benches/ **20**, src/lsp **14**
(LSP tests are ignored-by-default), proptest `arb_*` **5** (tethys-0nar),
tests/ helper **1**. Known-FP classes visible in src core: method-shape
calls inside asserts (`as_str` ×6, `as_i64` ×2, `debug_assert_valid` ×6 —
tethys-9l27). The CLI layer (43) is genuinely untested-by-unit-tests but is
exercised by no test root by construction — a reporting/scoping decision
for the design, not a bug.

## What I learned that I did not know before re-running

> **The parked probe's conclusion inverted: refs-vs-call_edges now differ by
> 30 symbols on self-index, so the traversal-substrate choice is load-bearing
> — and the noise floor of the report is dominated not by resolver gaps but
> by SCOPING decisions (benches/CLI/LSP = 77 of 235; the 9l27 method-shape
> class is real but ~14 sites).**

Gate: probe runs against real codebase ✓; oracle (grep-trace) agrees 4/4 ✓;
non-obvious learning recorded ✓.
