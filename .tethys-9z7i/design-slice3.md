# tethys-9z7i slice 3 design: the band query surface (2026-07-04)

Per ADR-0003 (mapping verbatim) and the slice-3 probe addendum in
`findings.md`. Purely additive (one new view, one optional flag; no
constraint removed — one-sentence subtractive sweep).

## Architecture

- **View `refs_banded`** (sibling of `refs_named`, per the epic's own
  wording): all `refs_named` columns + `strategy` + `band`, where band is
  ONE `CASE` implementing ADR-0003's table verbatim; `band` is NULL when
  `strategy` is NULL (band is a property of resolved refs only —
  NULL⇔unbound symmetry). `refs_named` itself is untouched.
- **Callers exclusion**: `get_callers(..)` gains `exclude_speculative:
  bool`; when set, BOTH CTE arms require the edge to have at least one
  non-speculative supporting ref (`EXISTS` against refs on
  (caller,callee)) — an edge dies only when NO trustworthy ref supports
  it (suppression-honest: mixed-support edges survive). CLI:
  `tethys callers --exclude-speculative`.
- **Panic-points: deliberately skipped** — probe-proven vacuous (its refs
  are unresolved-name matches, strategy NULL by construction). Epic AC
  amended at close-out with the evidence; if a future workspace surfaces
  resolved-speculative panic candidates, that is the tethys-53iv fix's
  concern (bind-but-band changes what panic-points sees), not a flag here.

## Input shapes

Strategies ×9 + NULL through the band CASE; edges with all-speculative /
mixed / no-speculative support; depth-1 vs transitive exclusion; flag
on/off; empty result after exclusion; refs_named consumers untouched.

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| 1 | refs_banded exists; band ∈ {high,medium,speculative,NULL}; refs_named untouched | schema_tests | sqlite_schema + PRAGMA (raw) | 10m | prototype ran on self-index (probe) | `refs_banded_view_shape` + existing refs_named tests |
| 2 | Band mapping matches ADR-0003 verbatim for all 9 strategies + NULL | fixture rows per strategy via the typed helper; view readback | the ADR table (source text) vs readback, per-strategy asserts | 15m | pending | `band_mapping_matches_adr` (10 distinct asserts). Buggy: a strategy in the wrong CASE arm |
| 3 | Exclusion drops speculative-ONLY edges, keeps mixed-support edges | fixture: explicit-import call (kept) + cross-crate bare unique call (dropped); one callee with BOTH kinds of support (kept) | fixture source hand-read; band derivation is claim 2's | 25m | pending | `exclude_speculative_drops_only_unsupported`. Buggy: EXISTS inverted; filter dropping mixed edges |
| 4 | Exclusion applies transitively (recursive CTE arm filtered too) | chain A→B (good) → C (speculative): with flag, C absent from A's transitive callers-of view | fixture hand-read | 15m | pending | `exclusion_is_transitive`. Buggy: only the base arm filtered |
| 5 | Default (no flag) byte-identical to today | existing callers CLI/integration tests unmodified | the existing pinned outputs | 5m | pending | existing callers tests. Buggy: flag default flipped |
| 6 | CLI flag surfaces in the table output (callers has no JSON mode today) and composes with --transitive | run_cli | output text (mechanical) | 10m | pending | `cli_callers_exclude_speculative`. Buggy: flag parsed but not threaded |

Cheapest (view prototype + distribution) ran at probe time — passed.

## Negative space

1. No panic-points flag (probe-proven vacuous; recorded above).
2. No impact/reachable/affected-tests CLI exclusion — the epic names
   callers. NOTE (post-review): the library get_symbol_impact API gained
   the parameter because `callers --transitive` routes through it; the
   impact CLI pins false, so impact BEHAVIOR is unchanged.
3. No band stored, no index added (ADR; measure first — the EXISTS probes
   refs by (in_symbol_id, symbol_id), covered by existing idx_refs_symbol).
4. Does not fix 53iv/msn0/3i35 — the flag excludes their fabrications
   from callers; the bugs stay open.

## Open decisions flagged for approval

1. **Panic-points skip** (probe-evidence above) — the epic's AC wording
   changes at close-out.
2. View name `refs_banded`; flag `--exclude-speculative`.
3. Edge semantics: any-good-support keeps the edge (vs all-good) —
   suppression-honest default.
