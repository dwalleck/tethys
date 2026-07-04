# tethys-ygjx — prior art (tracker scan, 2026-07-04)

Bounded search of rivets for `ref`/`macro`/`identifier`/`value`. Findings:

## Direct siblings (same subsystem, established the pattern)
- **tethys-zp2j** (CLOSED 2026-06-30): "refs table omits bare free-function
  CALL references." The sibling fix — added `call` refs for bare `foo()`.
  ygjx is its complement: the *value* use `foo` (no call) and the *macro-token*
  use. AC #3 requires zp2j's behavior stays unchanged (regression fence).
- **tethys-6rlu** (CLOSED 2026-06-30): "Resolution NULLs refs.reference_name."
  Marked fixed, but **still observable**: resolved refs (strategy
  `unique_workspace`) carry empty `reference_name` (verified: `row_to_symbol`'s
  3 call-refs have NULL name; querying by `reference_name` misses them, querying
  by `symbol_id` finds them). **Consequence for ygjx's ACs:** the AC "yields >= 1
  refs row with reference_name='foo'" holds only for the *unresolved* state;
  once Pass-2 binds the value ref, assert by `symbol_id`, not name. Tests must
  account for this.

## Consumers this bug blocks (all open)
- **tethys-dvsw** (blocks): dead-code finder. A fn used ONLY as a value has zero
  inbound call-refs → false-positive dead code. ygjx closes that gap.
- **tethys-7p54** (blocks): hotspots. Value-uses are uncounted → undercount.
  Measured: `row_to_symbol` has ~13 value-uses invisible to hotspot ranking.
- **tethys-y3bx** (blocks): untested-code analysis. Recent commits
  (dccce03, fe61d62) parked y3bx's probe specifically on the **macro-token gap**
  ("blocked by ygjx macro-token gap"). NB: y3bx needs *category 2*, which this
  probe shows is high-noise / out-of-scope-candidate — surface at design pause.

## Related (not gating)
- **tethys-xoxq** (related): visibility-tightening.
- **tethys-l6nt** (related): PRD roadmap (sequencing).
- **tethys-jdly**: design fixture that found the 3rd concrete instance
  (unit-struct constructor `crate::OldStruct` in value position emits no ref).
  Cited in ygjx Notes; same class as category 1 but via `scoped_identifier`.

## Context tickets (closed, explain observed state)
- **tethys-s8hv** (CLOSED): inline module bodies (incl. `#[cfg(test)]`) now
  indexed → test code IS in the index; the `.map(parse)` example lives in a test
  *string literal* (not parsed code) — not a real value-use site.
- **tethys-v1w8** (CLOSED): `pub use` targets carry inbound `reexport` refs —
  explains why `row_to_symbol` shows 1 reexport ref (`use super::{…}`).

**No unfiled prior art discovered.** ygjx is the correct home for this work.
