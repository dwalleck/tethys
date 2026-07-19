# tethys-1dcc — falsifiable design: rstest consolidation

## Purpose

Consolidate hand-rolled parameterization in the test suite into the repo's
existing rstest convention (`#[rstest]` + named `#[case::snake_name(...)]`),
improving failure localization and cutting duplication — without changing
what any test proves. Probe basis: `findings.md`, `probe-output.txt`,
`probe-loops-output.txt`.

## Core rule

Convert a test group ONLY when all hold:

1. ≥ 3 data points (fns in a duplicate group, or rows in a loop table).
2. Each datum's variation fits a one-line `#[case::name(...)]` attribute
   (scalars: strings, bools, ints, enum variants, short exprs).
3. All data share one assertion shape, applied per-datum to the SUT
   (independent cases — never facets of one shared result).
4. Case args are data (inputs/expected) or fixture factories (the house
   batch/streaming pattern); NEVER the function-under-test.

## Input shapes (candidate classes)

| Class | Shape | Verdict |
|---|---|---|
| A | Duplicate fns, scalar data (`is_excluded_dir` ×13) | IN — core target |
| B | Duplicate fns, workspace-fixture arrays vary (`unused_imports` ×6) | OUT — cases would embed multi-line fixture blobs; readability loss. Settled rationale, not deferred work. |
| C | Fuzzy-merged families targeting different SUT methods (`returns_result`/`returns_option`) | IN — as SEPARATE rstest fns per method (rule 4) |
| D | Loop tables, independent data, pure/cheap SUT (`ready_classifier_malformed_params` ×5) | IN — strict upgrade: all rows run (loops short-circuit at first failure), failures named |
| E | Loop tables sweeping facets of one result (`delete_files_cascades` ×9 table counts after ONE delete) | OUT — one scenario with N assertions; conversion is semantically wrong |
| F | Loop tables, independent data, expensive shared fixture (`panic_points_matches_qualified_last_segment`) | OUT — per-case fixture rebuild buys isolation nobody asked for; `#[fixture]`/`#[once]` machinery is negative space |
| G | Groups/tables of size 2 | OUT — churn exceeds benefit |
| H | Tests with `#[should_panic]`/`#[ignore]` | OUT of target set; plan verifies none of the shortlist has them |
| I | Group members carrying doc comments | IN — shared story hoists to the consolidated fn's doc comment (doc_markdown-clean); case names keep the discriminating suffix |

## Subtractive sweep

The change removes **per-datum top-level fn names**. What those names held up:

- External name-keyed references (CI filters, scripts, docs, goldens) →
  claim 5.
- nextest per-test process isolation → SAFE: rstest expands each case to its
  own `#[test]` item (verified: 3-case scratch listed as 3 tests), so
  processes-per-test is unchanged.
- Loop conversions also remove first-failure short-circuit → strictly
  better (all cases report), noted, no claim needed.
- tethys's own `is_test` detection of the suite → SAFE: already fenced by
  `test_topology.rs::detects_rstest_attribute` on main.

## Target set

Duplicate-fn groups (36 fns → 6 rstest fns):

- `src/indexing.rs` `is_excluded_dir_*` ×13
- `src/types.rs` `returns_result_*` ×4 and `returns_option_*` ×3 (two fns)
- `src/types.rs` `reference_kind_parse_*` ×6
- `src/lsp/transport.rs` `format_uri_*` ×6
- `src/languages/module_resolver.rs` `csharp_declines_*` ×4

Loop tables (class D, from `probe-loops-output.txt`; plan verifies each
against the core rule before converting):

- `src/lsp/status.rs` `ready_classifier_malformed_params` (5 rows, pure)
- `src/languages/rust.rs` `collect_local_bindings_covers_all_binding_forms`
  (7 rows) and `extracts_single_segment_glob_use` (4 rows) — pending
  cheapness check at plan time
- remaining 29 loop hits: plan walks the list with the rule; expected
  verdicts are mostly E/F/G

RESOLVED (user, 2026-07-18): `tests/test_topology.rs` `detects_*` ×7 is
OUT — templating fixture source per case would make the rewrite
non-mechanical; the group stays as plain tests. Settled rationale, not
deferred work. Design approved as written, including the manual
diff-scoped fences for claims 1/2/5/6.

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| 1 | Converting N data yields exactly N nextest tests `<fn>::case_i_<name>` | `cargo nextest list -E 'test(<fn>)'` count ≠ N | nextest enumeration vs datum count from git diff (independent accounting) | 2m/slice | pending | manual (diff-scoped; property only meaningful for this refactor) |
| 2 | Per slice, total nextest count changes by exactly (cases added − fns removed): 0 for class A/C, +(N−1) for class D | pre/post `cargo nextest list \| wc -l` vs predicted delta | prediction computed from git diff, count from nextest — different layers | 2m/slice | pending | manual (diff-scoped) |
| 3 | A failing case is reported with its case name, pinpointing the datum | scratch 3-case rstest fn with one deliberately failing case; failure output lacking case name falsifies | nextest run output on a purpose-built failing case | 5m | **passed** — output named `scratch_naming_probe::case_3_gamma`; format `::case_N_name` confirmed against merged tests | existing CI (24 merged `#[rstest]` fns exercise naming every run) |
| 4 | Converted tests still catch the bugs the originals caught | per slice: temporarily re-introduce one bug class the group guards (e.g. `is_excluded_dir("src") == true`); converted test passing falsifies | test runner against mutated SUT — mutation chosen from the ORIGINAL tests' assertions, not the new code | 5-10m/slice | pending | the converted tests themselves, permanently |
| 5 | No repo surface references a converted fn name as a filter/key | `git grep <old-fn-name>` outside the defining file; a functional hit left unfixed falsifies | git grep (text layer, independent of cargo/nextest) | 2m/slice | pending | manual (diff-scoped) |
| 6 | No `#[case]` arg selects the SUT; families split per SUT method | per-slice diff inspection; a case arg naming the assertion target falsifies | human review of diff against rule 4 | 5m/slice | pending | manual (house convention, review-enforced) |

Non-vacuity (named buggy implementations): (1) a conversion that collapses
13 asserts into one case; (2) a slice that deletes a test instead of
converting it; (4) a conversion that flips an expected bool while porting
(`allows_src` copied as `excludes`); (5) converting a fn named in a CI
filter; (6) `#[case(PackageSource::is_result)]`-style selector args.

Claims 1/2/5/6 are diff-scoped refactor properties — their falsifiers run
per slice and land in the audit trail; the permanent runtime fence for the
suite's correctness is claim 4's (the converted tests). The `manual` fence
entries therefore need explicit user approval at the design pause.

## Negative space

1. No new test coverage — consolidation only. Coverage expansion is
   separate, existing work (tethys-zoi3, open P2, verified 2026-07-18).
2. No rstest machinery beyond `#[case]`: no `#[fixture]`, `#[once]`,
   `#[values]` matrices, async mode (no real async tests exist), no
   `rstest_reuse`.
3. Class B/E/F/G groups stay untouched, as does the shared-fixture-builder
   pattern (`deprecated_callers.rs`) — a different good pattern.
4. No file/module restructuring, no renames beyond the consolidated fns,
   no changes to `tests/common`, no touching product code (except claim-4
   mutations, which are reverted before commit).
5. Not chasing the probe's 142-fn ceiling; scope is the named target set.

## Budget posture (for the plan)

≤ 8 files touched, ~45 fns/tables → ~10 rstest fns, one slice per file (or
per tight file pair), full gates per slice.
