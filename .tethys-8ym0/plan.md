# tethys-8ym0 — budgeted plan

Approved design: `.tethys-8ym0/design.md` (D-A kind-exclusion, D-B new
`macro_call` kind, D-C manual perf fence — all user-approved). Cheapest
falsifier (C6) passed pre-approval.

Global oracle (prove-it-prototype): `probe.py` numbers on the self-index —
477 raw bare call-shapes, ~180 resolving in-crate, and the 11 `scalar` sites
in `tests/value_refs.rs` item-checked. The binary must reproduce these after
the pipeline slices land.

## Slice 1: `macro_call` kind plumbing

**Claim:** C2 (support) — the kind exists end-to-end: extraction enum,
storage string, parse round-trip; kind-blind symbol binding (only `Macro`
stays gated in `ref_binds_to_symbol_kind`).
**Oracle:** `sqlite3` accepts and returns the literal `'macro_call'` through
a full index round-trip in slice 4; at this slice, unit round-trip only.
**Stress fixture:** round-trip test `MacroCall` ↔ `"macro_call"` PLUS the
ygjx regression class: `ReferenceKind::parse("macro_call")` succeeds via the
single shared parser (no resurrected duplicate parser); unknown kind still
errors. `ref_binds_to_symbol_kind(MacroCall, Function|Struct)` = true,
`(Macro, Function)` = false (gate preserved).
**Loop budget:** none (enum arms).
**Wall budget:** n/a (not always-on).
**Files:** `src/languages/common.rs`, `src/types.rs` (+ compiler-forced
exhaustive-match arms; if a third file needs an arm, it is mechanical and
stays in this slice).

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixture produces expected outcome
- [ ] Oracle: n/a at this slice (no emission yet); suite green
- [ ] Budgets hold (no loops)

## Slice 2: token-walk emission in the Rust extractor

**Claim:** C1 — exactly the S1 shapes emit one `MacroCall` ref each; S2
(local-bound), S4 (method), S5 (path), S6 (nested macro name), S8
(`macro_rules!` bodies) emit nothing; S11 (nested trees) and S12 (`[]`/`{}`
delimiters) are covered. Also C8 at unit level.
**Oracle:** probe raw count — extractor on the self-index must emit
bare-call `MacroCall` refs ≈ 474 (probe: 477 raw − 3 guard hits); checked in
slice 6's audit; at this slice, the unit fixture's hand-enumerated set.
**Stress fixture (expected output written first):**
```rust
fn target() {}
fn cross() {}
macro_rules! deffy { () => { inner_call() }; }   // S8: no ref for inner_call
fn user() {
    let clos = |x: i32| x;
    assert!(target() == 1);                  // ref: target
    assert_eq!(clos(1), m::path_fn(2));      // clos suppressed (S2); path_fn NOT (S5)
    assert!(matches!(cross(), 0));           // matches NOT (S6); cross IS (S11 nested tree)
    assert!(recv.meth(3));                   // meth NOT (S4)
    let v = vec![target(), clos(2)];         // target IS (S12); clos suppressed
}
// expected MacroCall emissions: target ×2, cross ×1 — nothing else.
```
Bug classes attacked: shape misclassification (method/path/nested-name
leak), nested-tree miss, local-shadow leak where the local name shadows a
real in-crate fn, macro_rules template leak.
**Loop budget:** token walk is O(nodes under macro token trees) per file;
self-index scale ≈ 7,078 identifier tokens (~30k nodes total across 99
files) — far under 10^6; the walk runs once per parse inside the existing
per-file extraction phase.
**Wall budget:** n/a (indexing is not always-on; C12 audits wall time).
**Files:** `src/languages/rust.rs` (walk fn + MACRO_INVOCATION arm + unit
tests; reconcile the ygjx unit fence
`macro_token_identifier_not_emitted_as_value` here — it asserts macro tokens
don't become **value** refs, which stays TRUE; extend it to assert they now
become `macro_call` refs instead, per approved D-D).

**Verification:**
- [ ] Unit tests pass (new shape tests + reconciled ygjx fence)
- [ ] Stress fixture emits exactly the pre-written set
- [ ] Oracle: emission-count spot check vs probe raw on one real file
- [ ] Loop budget holds (single pass, no nested rescans)

## Slice 3: pipeline posture — unresolved drop + call_edges exclusion

**Claim:** C3 (no unresolved `macro_call` rows survive) and C4 (call_edges
never contains `macro_call`) — production side.
**Oracle:** self-index SQL after `tethys index --rebuild`:
`COUNT(*) WHERE kind='macro_call' AND symbol_id IS NULL` = 0; call_edges
count unchanged from pre-8ym0 baseline (macro rows excluded).
**Stress fixture:** covered e2e in slice 4 (F3/F7); this slice's check is
the self-index SQL oracle above plus the full existing suite (no behavioral
drift elsewhere).
**Loop budget:** widened `DELETE` scans refs by kind (O(refs), ~20k rows,
one statement, index-assisted) — within budget; `NOT IN` list extension adds
no complexity.
**Wall budget:** n/a.
**Files:** `src/db/references.rs` (widen `drop_unresolved_value_refs` →
covers `('value','macro_call')`, rename/doc accordingly),
`src/db/call_edges.rs` (exclusion list + comment: macro_call = token-soup
provenance, consumers read refs — per approved D-A).

**Verification:**
- [ ] Unit tests pass
- [ ] Self-index SQL oracle: 0 unresolved macro_call rows; call_edges free of them
- [ ] Probe-vs-binary: resolved macro_call count ≈ probe's 180 (±10%)
- [ ] Budgets hold

## Slice 4: integration fences F1–F7, F11 (`tests/macro_token_refs.rs`)

**Claim:** C1–C5, C7 end-to-end on fixture-built indexes (never ambient DB).
**Oracle:** hand-enumerated SQL expectations per fence; for F11, rustc
ground truth (`--force-warn deprecated` warns on the same macro-context
site — the jdly oracle, cited not re-run).
**Stress fixture (all expected outputs pre-written):**
- F1 the exact y3bx blocker: `#[test] fn t() { assert_eq!(helper(), 1); }`
  → one refs row kind=`macro_call`, symbol=helper, in_symbol=t,
  strategy=`same_file`.
- F1b name-collision: same-named `helper` in TWO files; the test file's ref
  binds the SAME-FILE helper (attacks unique-name-only resolution).
- F2 `let f = |x| x; assert!(f(1))` → no row for `f` even though a fn `f`
  exists in ANOTHER file (shadow + collision combined).
- F3 `assert!(nonexistent_fn())` → zero macro_call rows post-index.
- F4 method + path shapes in macros → no macro_call rows (TRIPWIREs:
  tethys-9l27, tethys-ewa7).
- F5 `assert!(matches!(g(x), Ok(_)))` → `g` row present; no `matches`
  macro_call row (TRIPWIRE: tethys-7dqj).
- F6 `macro_rules!` expansion template calling `foo()` → zero rows.
- F7 F1's workspace: call_edges (t→helper) ABSENT; `get_callers(helper)`
  empty (D-A posture pinned).
- F11 cross-file `#[deprecated] fn old()` called only inside `assert!` →
  deprecated-callers lists the site (tier per existing rules).
**Loop budget:** test-only.
**Wall budget:** n/a.
**Files:** `tests/macro_token_refs.rs` (new).

**Verification:**
- [ ] All fences pass; F1 red-first sanity: F1 must FAIL when slice-2 walk
      is stubbed out (checked by construction — it was red before slice 2)
- [ ] Stress fixtures (collision/shadow classes) produce expected outcomes
- [ ] Oracle: fence expectations are hand-enumerated, independent of extractor
- [ ] Budgets hold

## Slice 5: determinism, batch/streaming parity, goldens (F9, F10, C11)

**Claim:** C9 (two indexes → identical macro_call multisets), C10 (batch ≡
streaming refs AND macro-only import corroborates file_dep in both), C11
(idxperf goldens byte-identical; suite green with only the deliberate
slice-2 fence reconciliation).
**Oracle:** canonical row dumps (idxperf pattern) diffed between runs/paths;
goldens predate this change.
**Stress fixture:** two-file workspace — `a.rs`: `use crate::b::helper;`
consumed ONLY inside `assert!(helper())`; `b.rs` defines `helper` (+ a
DUPLICATE identical `assert!(helper())` line to attack row-collapse/count
bugs). Expect: identical refs dumps batch vs streaming at batch sizes 1 and
default; file_dep a→b present in both; double-index dump identical.
**Loop budget:** test-only.
**Wall budget:** n/a.
**Files:** `tests/macro_token_refs.rs` (extend).

**Verification:**
- [ ] F9/F10 pass; `idxperf_golden` tests pass UNMODIFIED
- [ ] Stress fixture (duplicate-line, macro-only corroboration) as expected
- [ ] Oracle: canonical-dump diff empty
- [ ] Budgets hold

## Slice 6: audit + oracle closure (no production code)

**Claim:** C6 re-check with the real kind, C12 perf, and final
probe-vs-binary oracle agreement.
**Oracle:** (a) five-analysis output diff on self-index pre/post rebuild
with macro_call rows present — expect byte-identical (falsifier1 predicted);
(b) probe item-check: the 11 `scalar` sites in tests/value_refs.rs appear as
macro_call rows bound to that file's `scalar`; resolved total ≈ 180 ±10%;
(c) `tethys index --rebuild` wall time ×3 runs pre/post — regression <10%
(manual fence per approved D-C; CI perf trip-wire remains tethys-ng1v).
**Stress fixture:** n/a (audit); the "fixture" is the real self-index.
**Loop budget:** n/a.
**Wall budget:** n/a.
**Files:** `.tethys-8ym0/audit.md` (new).

**Verification:**
- [ ] Analyses diff empty (C6 closed with real kind)
- [ ] Probe/oracle item + aggregate agreement recorded
- [ ] Perf numbers recorded, <10% regression
- [ ] Audit committed

## Plan self-review

1. **Loops:** slice 2 token walk O(nodes under macro trees) ≈ 30k @ self-index
   scale, single pass — within 10^6. Slice 3 DELETE O(refs) one statement.
   No other new loops. No always-on phases. ✓ no gaps.
2. **Fixtures:** every logic slice attacks a named bug class — shape
   misclassification + shadow/collision (S2 fixture), same-name-two-files
   binding (F1b), unresolved lingering (F3), posture leak (F7), template
   leak (F6), duplicate-row collapse + path divergence (slice 5). ✓
3. **Doc-comment preconditions:** the token-walk helper's "only called from
   the MACRO_INVOCATION arm" is enforced structurally (it takes the macro
   node and locates its own token_tree — no caller can misuse it silently);
   the widened drop's "must run after all resolution passes" is the existing
   load-bearing ordering already enforced by pipeline sequence and now
   fenced by F3. No new unenforced contracts. ✓
4. **Write targets:** no new CLI/stdout writes; new `tracing::trace!`
   diagnostics only (stderr via subscriber). ✓
5. **Tracker references:** tethys-9l27, tethys-ewa7, tethys-7dqj,
   tethys-0nar, tethys-ng1v, tethys-y3bx — all verified open/existing this
   session; ewa7/7dqj filed during design. ✓
