# tethys-1dcc — budgeted plan: rstest consolidation

Design: `design.md` (approved 2026-07-18, test_topology group OUT, manual
diff-scoped fences approved). Six slices, one file each, one commit each.

Plan-time verifications already run:

- Claim 5 grep (all target fn names, repo-wide): only historical audit
  artifacts (`.tethys-ygjx/`, `.tethys-53iv/`, `.tethys-2mjj/`) and two
  CLOSED issues (tethys-lwsc, tethys-9iwc) reference target names. No CI
  filter, script, or golden. No action; past audit trails are not edited.
- Class H: zero `#[should_panic]`/`#[ignore]` in the six target files.
- Class-D confirmations: `extracts_single_segment_glob_use` IN (each row
  parses its own code); `collect_local_bindings_covers_all_binding_forms`
  OUT (class E: one parse, loop asserts membership facets);
  `memo_preserves_per_name_outcomes` OUT (class F: tempdir workspace per
  row). Remaining loop hits triaged E/F/G per `probe-loops-output.txt`;
  none meet the core rule.

Uniform per-slice mechanics:

- Oracle (all slices): `cargo nextest list` captured pre/post; the ID-set
  diff must remove EXACTLY the old fn IDs and add EXACTLY
  `<fn>::case_i_<name>` IDs, count per design claims 1-2. Independent of
  the edit (runner enumeration vs git diff).
- Gates (real exit codes, per ship conventions): `cargo nextest run`,
  clippy pedantic `-D warnings`, `cargo fmt --check`, doctests.
- Mutation check (claim 4): apply the slice's named SUT mutation, run the
  converted fn, EXACTLY the predicted case(s) fail; revert; re-green.
- Review (claim 6): case args are data only; families stay separate fns.
- Docs: shared story hoisted to the consolidated fn's `///`; identifiers
  backticked (doc_markdown).

Suite-count ledger (claim 2): baseline captured before slice 1
(`nextest-baseline.txt`, 1026 tests); expected cumulative delta after
S1..S6: 0, 0, 0, 0, +4, +6.

**BUILD-OUTCOME RECONCILIATION (2026-07-18).** Probe clustering
undercounted two families; deviations were pre-registered before each
slice ran and oracles verified against the corrected predictions:

- S1 actual: −16 fns / +20 cases (three multi-assert fns split
  per-assert into cases), slice delta +4 — not the planned −13/+13/0.
- S2 actual: −15 / +15, delta 0 (`returns_option_false_for_none` and
  `reference_kind_parse_reexport`/`field_access` belong to their
  families; probe's 0.90 threshold missed them).
- S3: skipped (OUTCOME block below).
- S4: −4 / +4 as planned. S5: −1 / +5 as planned. S6: −1 / +3 as planned.
- Corrected cumulative ledger: +4, +4, +4 (skip), +4, +8, **+10**.
  Final audit: 1026 → 1036 (`nextest-final.txt`), ID-set diff exact.

No slice introduces a loop, a doc-comment precondition, or a write target;
per-slice sections therefore mark those budgets N/A-with-reason once here:
conversions are straight-line attribute lists replacing fns/loops; tests
produce no program output.

---

## Slice 1: is_excluded_dir ×13 → 1 rstest fn

**Claim:** design 1,2 (N=13, suite delta 0), 4, 6.
**Oracle:** nextest ID-set diff: −13 old fns, +13 `is_excluded_dir::case_i_*`.
**Stress fixture (mutation, expected outcome pre-written):** make
`Tethys::is_excluded_dir` stop excluding `"target"` → EXACTLY the
`excludes_target` case fails; 12 cases pass. Guards the porting bug class:
expected-bool transposition (an `allows_*` copied as excludes).
**Loop budget:** no new loops (13-attr straight line).
**Files:** `src/indexing.rs`.
**Code (advisory):** one fn, cases `(dir_name: &str, excluded: bool)`,
`assert_eq!(Tethys::is_excluded_dir(name, None), excluded)`; case names
keep old suffixes (`excludes_target` … `rejects_empty_string`).
**Verification:** uniform mechanics above.

## Slice 2: types.rs three groups → 3 rstest fns

**Claim:** design 1,2 (13 fns → 3 fns ×13 cases, delta 0), 4, 6 (result vs
option stay SEPARATE fns — the design's rule-4 hazard case).
**Oracle:** nextest ID-set diff: −13, +13 across three fns.
**Stress fixture:** mutate `ReferenceKind::parse("call")` to return
`Import` → exactly the `call` case of `reference_kind_parse` fails; then
mutate `is_result_type` to reject `Result<…>` → only result-family cases
fail, option fn untouched (proves family split localizes).
**Loop budget:** no new loops.
**Files:** `src/types.rs`.
**Verification:** uniform.

## Slice 3: format_uri ×6 → 1 rstest fn

**Claim:** design 1,2 (delta 0), 4, 6.
**Oracle:** nextest ID-set diff: −6, +6.
**Stress fixture:** remove space percent-encoding in `format_uri` → exactly
the two `percent_encodes_spaces*` cases fail. Guards: expected-string
transposition across similar cases.
**Loop budget:** no new loops.
**Files:** `src/lsp/transport.rs`.
**Note:** if any of the six turn out `#[cfg(windows)]`-split, keep one
rstest fn per cfg block (do not merge across cfg); oracle counts adjust to
the visible-platform subset. STOP if the split changes suite counts.
**Verification:** uniform.

**OUTCOME (build-time, 2026-07-18): SLICE SKIPPED.** The group is
cfg-split: `not(windows)` subset is 2 tests (< 3, class G by the approved
design's core rule); `windows` subset is 4 tests that cannot execute on
the build machine, so the claim-4 mutation fixture cannot run — the slice
cannot meet its own verification contract, and each test carries a
distinct load-bearing doc rationale a merge would flatten. Probe
limitation discovered: clustering ignores `cfg` attributes. `format_uri`
tests stay as-is; exclusion is settled rationale (same category as class
B/E/F), not deferred work. Ledger unchanged (slice predicted delta 0).

## Slice 4: csharp_declines ×4 → 1 rstest fn

**Claim:** design 1,2 (delta 0), 4, 6.
**Oracle:** nextest ID-set diff: −4, +4.
**Stress fixture:** mutate the C# module resolver to ACCEPT a dotted
namespace → exactly the `dotted_namespace` case fails. Guards: decline
tests passing vacuously after a port (asserting Some instead of None).
**Loop budget:** no new loops.
**Files:** `src/languages/module_resolver.rs`.
**Verification:** uniform.

## Slice 5: ready_classifier_malformed_params loop ×5 → 1 rstest fn (+4)

**Claim:** design 1,2 (1 fn → 5 cases, delta +4), 4, 6; class-D upgrade
(loop short-circuited at first failing row; cases all run and name the
datum).
**Oracle:** nextest ID-set diff: −1, +5.
**Stress fixture:** mutate `classify_server_status` to return `Ready` for
`Value::Null` → exactly the `null` case fails. Claim-1 count (=5) guards
the row-drop porting bug (a lost table row is invisible in a loop port).
**Loop budget:** REMOVES a loop; adds none.
**Files:** `src/lsp/status.rs`.
**Code (advisory):** cases `(params: Value)` named `health_only`,
`quiescent_string`, `quiescent_int`, `null`, `array`; expected
`ReadyState::NotReady` stays in the fn body (same for all — not a case arg).
**Verification:** uniform.

## Slice 6: extracts_single_segment_glob_use loop ×3 → 1 rstest fn (+2)

**Claim:** design 1,2 (1 fn → 3 cases, delta +2), 4, 6.
**Oracle:** nextest ID-set diff: −1, +3.
**Stress fixture:** mutate the Rust extractor to drop the glob flag (or
source path) for `use crate::*` → exactly the `crate` case fails. This
re-introduces the exact historical bug tethys-lwsc (closed) fixed — the
fixture embeds the bug class per the design's fence rule.
**Loop budget:** REMOVES a loop; adds none.
**Files:** `src/languages/rust.rs`.
**Verification:** uniform.

---

## Plan Self-Review

1. **Loops:** none introduced anywhere; slices 5-6 remove two. No budget
   entries needed beyond the N/A statement above. No gaps.
2. **Fixtures:** every slice has a mutation fixture with a named porting
   bug class (bool transposition S1, family merge S2, string transposition
   S3, vacuous-decline S4, row drop S5, historical-bug reintroduction S6)
   and a pre-written expected outcome (exactly-these-cases-fail). No gaps.
3. **Doc-comment preconditions:** none added; hoisted docs carry no "must"
   contracts. No gaps.
4. **Write targets:** none; test-only changes produce no program output.
   No gaps.
5. **Tracker references:** tethys-zoi3 (open, verified — negative space
   item 1); tethys-lwsc, tethys-9iwc (closed, historical name mentions —
   verified, no action). Class B/E/F/G exclusions are settled rationale
   per the approved design, not deferrals. No gaps.

Claim coverage vs design: claims 1,2,4,6 exercised per slice; claim 3
passed at design time (fence: existing CI); claim 5 pre-run at plan time
(results above). Complete.
