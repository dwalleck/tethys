# tethys-y3bx — budgeted plan

Approved design: `.tethys-y3bx/design.md` (D-A report-all path-sorted, D-B
indeterminate zero-roots, D-C `untested-code` fn/method scope, D-D AC #2
rewording, D-E audit-only C8 / manual C9 — all user-approved). Cheapest
falsifier (C1 via the resumed probe) passed.

Global oracle (prove-it-prototype): the resumed probe — untested(refs)=235
on the self-index at the probe commit, refs-vs-call_edges gap=30, item
checks crate_glob_covers/scalar/is_excluded_dir TESTED,
print_reachability_result UNTESTED. The binary must reproduce these in the
slice-4 audit (fresh numbers re-derived by re-running the probe at audit
time — the branch's own new code shifts counts; probe and binary must move
in lockstep, the 8ym0 pattern).

## Slice 1: db layer — refs-closure BFS + facade

**Claim:** C1 (core rule), C3 (cycles/transitivity at unit level).
**Oracle:** unit expectations hand-enumerated; full-rule oracle is the probe
(checked at slice 4; spot SQL check after this slice: facade count on
self-index == probe count re-run at the same commit).
**Stress fixture (unit, expected outputs written first):**
pure BFS helper over (roots, edges): multi-root {t1,t2}, chain t1→a→b,
cycle b↔c, self-loop d→d reachable from t2, disconnected e — expect
closure = {t1,t2,a,b,c,d}; e outside. Zero-roots → closure = ∅. Duplicate
edges → no double-visit (S18 analog at id level).
**Loop budget:** edge load O(refs) one query (~20k rows self-index); BFS
O(V+E) ≈ 2.5k + 20k ops — far under 10^6. No always-on phase.
**Wall budget:** n/a (C9 audits <1s end-to-end).
**Files:** `src/db/untested.rs` (new: `UntestedFinding`, query + BFS +
module docs carrying the known-limitations section — 9l27/0nar/j2r1/
top-level-refs per D-D), `src/lib.rs` (facade `get_untested_code` +
re-export) + 1-line `src/db/mod.rs` wiring (mechanical, stays in slice).

**Verification:**
- [ ] Unit tests pass (BFS shapes above)
- [ ] Stress fixture exact-matches
- [ ] Oracle: facade self-index count == probe re-run at this commit
- [ ] Budgets hold

## Slice 2: CLI — `tethys untested-code [--json]`

**Claim:** C4 (zero-roots indeterminate posture), C7 (envelope + sort +
stream discipline) — production side.
**Oracle:** e2e fences in slice 3 drive the binary; this slice's check is
the human-render + JSON-render on the self-index by hand inspection plus
full suite green.
**Stress fixture:** covered e2e in slice 3 (F-U4/F-U7); render fns kept
data-only (no exit-code games: indeterminate is exit 0 + stderr note +
flagged summary, per D-B).
**Loop budget:** render O(findings) — trivial.
**Wall budget:** n/a.
**Files:** `src/cli/untested_code.rs` (new, `visibility_tightening.rs`
template: `{summary, findings}` via `to_json_pretty`, `write_report`
BrokenPipe-safe, human table grouped by file), `src/main.rs` +
`src/cli/mod.rs` wiring (mechanical).
**Output streams:** findings table/JSON → stdout (data); the zero-roots
indeterminate note + tracing → stderr (diagnostic). Classified per rule 6.

**Verification:**
- [ ] Unit/full suite passes
- [ ] Manual smoke: table + `--json | python -m json.tool` on self-index
- [ ] Oracle: n/a at this slice (e2e lands next)
- [ ] Budgets hold

## Slice 3: integration fences (`tests/untested_code.rs`)

**Claim:** C1, C2, C3, C4, C5, C6, C7 end-to-end on fixture-built indexes.
**Oracle:** hand-enumerated per fence; for F-U2 the second assert is
independent SQL against call_edges (the divergence proof).
**Stress fixtures (expected outputs pre-written):**
- F-U1: tested fn (direct `#[test]` call) absent; untested fn present with
  (name, kind, file, line); untested a→b pair BOTH present (S10); two
  same-named untested fns in different files both present (S18); struct +
  const + the test fn itself never present (C6).
- F-U2: the 8ym0-F1 workspace (`assert_eq!(helper(), 1)`) — `helper`
  ABSENT from the report AND SQL shows zero call_edges rows to `helper`
  (fails if the traversal ever switches substrate).
- F-U3: test→a→b chain + b↔c cycle → a, b, c all covered; self-loop d
  unreached → reported once.
- F-U4: no-test workspace → findings empty, JSON summary
  `{test_roots: 0, indeterminate: true}`, exit 0; all-test workspace →
  empty findings, `indeterminate: false`.
- F-U5: C# `[Fact]` test calling `Tested()`; `Untested()` sibling —
  Tested absent, Untested present (is_test parity).
- F-U7: drive the BINARY (`env!("CARGO_BIN_EXE_tethys")`) with `--json`
  on the F-U1 fixture: envelope fields present, findings sorted by
  (file, line), stdout parses as JSON with nothing non-JSON mixed in.
**Loop budget:** test-only.
**Wall budget:** n/a.
**Files:** `tests/untested_code.rs` (new).

**Verification:**
- [ ] All fences pass; F-U1's untested arm is red-first against a stub
      (by construction: command doesn't exist before slices 1-2)
- [ ] Stress fixtures produce expected outcomes
- [ ] Oracle: hand-enumerated + independent SQL (F-U2)
- [ ] Budgets hold

## Slice 4: audit + oracle closure (no production code)

**Claim:** C8 (audit-only, approved D-E), C9 (manual perf, approved D-E).
**Oracle:** (a) re-run the probe at the audit commit; binary
`untested-code --json` summary.untested_count must equal the probe's fresh
number, and the four item checks must agree; (b) `time` the command on the
self-index — < 1s wall.
**Stress fixture:** n/a (audit; the "fixture" is the self-index).
**Loop budget:** n/a.
**Wall budget:** < 1s for the analysis command at self-index scale
(2.5k symbols / 20k refs).
**Files:** `.tethys-y3bx/audit.md` (new).

**Verification:**
- [ ] Probe == binary (count + 4 items) at the audit commit
- [ ] Perf number recorded, < 1s
- [ ] Audit committed

## Plan self-review

1. **Loops:** slice-1 BFS O(V+E) ≈ 22.5k ops self-index; edge load one
   query; render O(findings). No always-on phases. ✓
2. **Fixtures:** every logic slice attacks named bug classes — cycle/
   self-loop (infinite-traversal), multi-root, S10 untested-pair (closure
   root-set confusion), S18 same-name collision, substrate-switch (F-U2's
   SQL divergence assert), zero-roots dump (F-U4), C# parity (F-U5),
   envelope drift + sort tie-break (F-U7 uses two files so the file sort
   key actually fires). ✓
3. **Doc-comment preconditions:** none load-bearing added — zero-roots is
   handled as data (indeterminate), not a precondition; facade opens the
   index via the existing `Tethys::new` error path. ✓
4. **Write targets:** stdout = findings table/JSON (data); stderr =
   indeterminate note + tracing (diagnostic). ✓
5. **Tracker references:** tethys-9l27, tethys-0nar, tethys-j2r1,
   tethys-o4re, tethys-w0qw, tethys-3yxn, tethys-zwaz, tethys-09wx,
   tethys-m7zm — all verified this session. ✓
