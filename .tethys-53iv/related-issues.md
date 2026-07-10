# tethys-53iv — prior art (tracker sweep, 2026-07-09)

- **tethys-0aqj** (open, P4) — kind-blind binding, both facets; filed from the
  xebx loop. Its data-member slice shipped as xebx D10 (PR #20): data members
  have their own Pass-1 map and `ref_binds_to_symbol_kind` refuses
  Call/Construct→data-member binds. 53iv is the RECEIVER facet of the same
  precision family — the gate seam and the Pass-1 map-split precedent are the
  natural extension points.
- **tethys-k543** (open, P3) — Pass 3 LSP re-verification of speculative-band
  binds; explicitly names the 53iv phantom-edge class and calls LSP "the one
  component with receiver-type information". Boundary: 53iv is the non-LSP
  conservative tier (extraction/resolution time); k543 re-verifies whatever
  remains speculative. 53iv must not attempt receiver-TYPE inference — that
  is k543's LSP territory.
- **tethys-6rlu** (closed) — `refs_named` view; discovered-from parent.
  Corollary that matters here: resolution NULLs `reference_name`, which is
  exactly why panic-points (raw-column filter, `db/panic_points.rs:51`) goes
  blind when an unwrap call binds.
- **tethys-z9mr** (open, P3) — import-decline → `unique_workspace` fallback
  interplay for re-exports; adjacent resolver-precision work, not gating.
- **tethys-jdly / tethys-9z7i** (closed) — deprecated-callers + provenance
  bands; `refs_banded` is where a demote-instead-of-decline design would land.

- **tethys-9l27** (open, P3) — filed FROM this probe: refs inside macro
  invocations are invisible (728 grep unwrap/expect sites vs 661 stored on
  tethys src); panic-points recall gap orthogonal to 53iv's precision gap.
  Related to tethys-ygjx's value-ref token-tree work.

No open issue duplicates 53iv's scope (receiver-aware conservatism for
method-call refs).
