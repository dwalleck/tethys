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
