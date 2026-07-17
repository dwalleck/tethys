# Design — tethys-epmj: discriminated `PackageNotFound` variant on `tethys::Error`

## Purpose

Make "package not found" structurally distinguishable from other not-found
conditions so future consumers (rivets-o4re's `tethys_coupling` MCP tool)
can react per-kind without parsing string payloads. Per the probe
(`findings.md`), the issue text's two mechanical claims were wrong; this
design follows the code, not the ticket prose.

## Architecture

One new variant, one converted construction site, one compiler-forced
categorization arm. No library-API signature changes.

- `src/error.rs`: add `PackageNotFound(String)` where the payload is the
  **bare package name** (data, not prose). Display:
  `#[error("not found: package '{0}'")]` — byte-identical rendering to
  today's `NotFound("package 'name'")` payload, so CLI output does not
  change. Add the variant to the `IoError` arm of
  `From<&Error> for IndexErrorKind` (same category as `NotFound`).
- `src/cli/coupling.rs` (`run_detail_to`): construct
  `Error::PackageNotFound(name.to_string())` instead of
  `Error::NotFound(format!("package '{name}'"))`.
- `NotFound(String)` stays as the catch-all for the other five payload
  flavors (28 → 27 remaining sites).

## Input shapes

Requested package name (`--package <name>`), against a real index:
1. Name of an existing package → success path, untouched (claim 7).
2. Missing name, no substring match → error path (claims 2, 3).
3. Missing name that substring-matches a real package → suggestion block +
   error (claim 3).
4. Empty string → behaves as any missing name; payload carries `""`
   verbatim (folded into claim 2's payload-verbatim assertion). Suggestion
   explosion (`contains("")` matches all) is pre-existing behavior, out of
   scope for this change.
5. Unicode / spaces in name → payload verbatim (claim 2); no formatting at
   construction.

Mode flag: `json` ∈ {true, false} — both construct the same error (claim 2).
Consumer-side shape: every `match`/`matches!` on `Error` (claim 5, 6).

## Removed-invariant sweep (step 2b)

The change is subtractive in one respect: it removes the invariant "every
not-found condition surfaces as `Error::NotFound(_)`". Enumerated consumers
of that invariant (probe slice C, grep over src/ + tests/):

- `src/error.rs` `From<&Error>` — exhaustive; compiler forces the new arm
  (cheapest falsifier proved it is the ONLY forced site). Covered: claim 4.
- `tests/indexing.rs:222,882,941` `matches!(err, NotFound(_))` — all
  file/symbol flavored paths, which still construct `NotFound`. Covered:
  claim 5 (their sites are untouched) — safe.
- No production code matches `NotFound(_)` to detect the package case
  (grep evidence). Covered: claim 6.

## Claims and Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| 1 | `PackageNotFound(name)` renders exactly `not found: package '{name}'` (byte-identical to today's output) | `to_string()` equality vs the literal captured in probe slice A | probe.sh pre-change stderr capture (runtime ground truth, recorded in findings.md) | 1m | pending (build S1) | unit test `error::tests::package_not_found_display_exact` |
| 2 | `run_detail_to` returns `Err(PackageNotFound(n))` with n == the bare requested name, in text AND json modes | unit tests calling `run_detail_to` with a missing name; `matches!` guard `n == "no-such-pkg"` | string equality vs the function argument itself (independent of Display/format machinery) | 5m | pending (build S2) | updated `run_detail_*_returns_not_found_err` tests (renamed to `*_package_not_found_*`) |
| 3 | CLI end-to-end behavior is unchanged: exit 1, stdout empty (text) / `null` (json), stderr `error: not found: package '<name>'` + suggestion block when applicable | re-run probe.sh post-build; diff slices A/A2/A3/control against the committed pre-change output | probe.sh committed pre-change capture | 2m | pending (build oracle re-run) | claim 1's Display test + existing `run_detail_json_mode_writes_null_*` test pin the components |
| 4 | `IndexErrorKind::from(&PackageNotFound(_))` = `IoError` | extend `error_to_index_error_kind_mapping` | the categorization doc rule (4xx/5xx philosophy, error.rs module docs): user-fixable input → IoError bucket like NotFound | 2m | pending (build S1) | same test |
| 5 | The other 27 `NotFound(format!` sites are untouched (no over-reach) | `grep -c "Error::NotFound(format!" src/` == 27 post-change (28 pre, one converted) | grep (text-level, independent of compiler and of tethys index — required: tethys-staf) | 1m | pending (build gate) | `tests/indexing.rs` `matches!(NotFound(_))` ×3 fail if file/symbol flavors change variant |
| 6 | The ONLY compile-forced consumer of the new variant is `From<&Error>` in error.rs | scratch-patch variant, `cargo check --all-targets`, count E0004 sites | rustc exhaustiveness checker | 3m | **passed** (1 error, exactly at the predicted match) | the match stays wildcard-free; any future variant re-runs the same forcing |
| 7 | Success path (`--package tethys`) output unchanged | probe.sh control case diff post-build | probe.sh pre-change capture | 1m | pending (build oracle re-run) | existing `run_detail_*_succeeds_when_package_exists` tests |

Claim-distinctness: each row has its own named test or probe slice; a
failure localizes to the row by test name / slice label.

Non-vacuity (buggy implementation each fence would catch):
1. Display attr written as `package not found: {0}` → claim 1 test fails.
2. Payload constructed as `format!("package '{name}'")` (decorated) →
   claim 2's `n == "no-such-pkg"` guard fails.
3. `run_detail_to` converted only in text mode (json arm forgotten) →
   claim 2 json test fails; probe A2 diff fails.
4. New variant added to the `DatabaseError` arm → claim 4 test fails.
5. Over-eager conversion of `file:`/`symbol:` sites → claim 5 grep count
   drops below 27 and `tests/indexing.rs` matches! tests fail.
6. Wildcard `_ =>` arm added to the match → claim 6's forcing property
   dies; caught at review + the arm-count check in S1.
7. Suggestion block accidentally dropped → probe A3 diff (claim 3) fails.

## Negative space (deliberately not doing)

1. **No discrimination of the other five flavors** (`file:`, `symbol:`,
   `file id:`, `symbol id:`, `type:`) — settled scope per tethys-epmj's own
   text: `NotFound` stays the catch-all; a future consumer needing another
   flavor files its own issue.
2. **No change to `get_package_coupling`'s `Result<Option<_>>` contract** —
   the probe proved the lib never constructs this error; the None→error
   mapping stays a CLI (later: MCP, tethys-o4re) concern.
3. **No `#[non_exhaustive]` on `Error`** — posture decision tracked at
   tethys-2uv6 (verified, filed this session), sequenced before rivets-o4re.
4. **No user-visible output change** — Display chosen byte-identical;
   changelog fragment documents the library-level addition only.
5. **No structured payload for suggestions** — suggestions remain a CLI
   stderr side effect; an MCP consumer recomputes its own (tethys-o4re's
   design space).

## Open decisions for approval

1. **Display string** — recommended (a): `not found: package '{0}'`,
   byte-identical to today. Alternative (b): `package not found: '{0}'`
   reads more directly but changes CLI stderr output.
2. **Payload = bare name** (recommended) vs pre-formatted string. Bare name
   is the entire point of the issue (structured data for MCP).
3. **Scope confirmation**: package variant only, per issue text.
