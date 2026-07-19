# tethys-1dcc — related issues (tracker prior art)

Searched: `rivets list | grep -iE "rstest|parameter|test"`, `rivets search rstest`,
`rivets search parameterized` (2026-07-18).

- **tethys-j9bu** (open epic, parent) — Codebase Intelligence Engine umbrella.
  Parent-child link only; does not gate.
- **tethys-zoi3** (open, P2) — expand `file_deps` test coverage (rename, target
  deletion, DB-unit, rebuild idempotency). Adjacent: adds NEW tests in an area
  this chore may reshape. Overlap risk is low (this chore consolidates existing
  tests, doesn't change coverage), but if `file_deps_idempotency.rs` gets
  restructured, note it there.
- **tethys-09wx** (open, P2) — affected-tests query standing. Not overlapping;
  touches analysis code, not the test suite's structure.
- No existing issue mentions rstest besides tethys-1dcc itself. No prior art on
  parameterization conventions beyond what's already merged in the suite
  (24 `#[rstest]` fns, 78 `#[case]`s across 8 files as of main @ 1403aa3).

Conclusion: no duplicate or blocking ticket; proceed.
