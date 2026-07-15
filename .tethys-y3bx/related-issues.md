# tethys-y3bx — tracker prior art (refreshed 2026-07-15 at resume)

- **tethys-8ym0** (CLOSED, PR #26) — macro-token call refs; the blocker this
  issue was parked on. BINDING: macro_call is excluded from call_edges, so
  this analysis must traverse `refs` (consumption note on this issue).
- **tethys-ygjx / tethys-s8hv / tethys-53iv** (all CLOSED) — fn-as-value
  refs, unit-test indexing (is_test 330→813), receiver-typed method
  resolution. All previously-gating substrate work done.
- **tethys-9l27** (open, P3) — method-shape calls inside macros invisible
  (`x.as_str()` in asserts): known FP class here, ~14 sites on self-index
  (as_str/as_i64/debug_assert_valid). Document as limitation, don't fix here.
- **tethys-0nar** (open, P3) — proptest-defined fns not indexed: arb_* ×5
  read untested despite proptest coverage. Document as limitation.
- **tethys-7dqj / tethys-ewa7** (open, P4, filed during 8ym0) — nested
  macro-name refs and path-shaped macro calls; marginal recall for this
  analysis, tripwired in tests/macro_token_refs.rs.
- **tethys-m7zm** (open, P3) — policy for analyses over newly-indexed test
  code; this analysis EXCLUDES is_test symbols from the report by
  construction (a test is not "untested"), which is one input to m7zm.
- **tethys-zwaz** (open, P3) — analysis-command CLI output convergence
  (envelope fences, BrokenPipe-safe writes, shared display helpers): the
  new subcommand should follow whatever pattern existing analyses use and
  not add divergence zwaz would have to clean up.
- **tethys-oojq** (open, P3) — self-index CI oracle pattern for analyses
  (unused-imports); no compiler ground truth exists for "untested", so the
  self-index fence shape differs here (count-stability, not zero-findings).
