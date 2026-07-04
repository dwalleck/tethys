# Design — tethys-s8hv: index inline module bodies

## Purpose
`extract_symbols_recursive`'s `MOD_ITEM` arm (src/languages/rust.rs:954) records
the module shell but does not recurse into the body, so every symbol inside an
inline `mod { … }` (dominantly `#[cfg(test)] mod tests`) is dropped. Fix: recurse
into the module body, mirroring `IMPL_ITEM`.

## What the spike proved (cheapest falsifier — RAN, passed)
Applied the 5-line recursion, rebuilt, re-indexed:
- `is_test` 330 → **809** (src/ 0 → 464); oracle ~842 source test fns.
- symbols 1700 → 2326 (+626).
- **refs re-attach automatically**: refs with `in_symbol_id` = a src/ unit-test
  symbol went ~0 → **3909**. The reference walk already recursed into mod bodies;
  those refs were unattached (NULL `in_symbol_id`) only because the enclosing unit
  test wasn't a symbol. So NO refs-walk change is needed — the symbol fix suffices.
- test-suite blast radius: **1 failure** (`deprecated_callers::
  resolved_sites_cross_file_and_top_level`) — a legitimate caller-attribution
  improvement, not a break.

## Input shapes
1. `#[cfg(test)] mod tests { #[test] fn … }` — dominant; unit tests → is_test roots.
2. Plain inline `mod foo { fn bar }` — non-test inline module (product code).
3. Nested inline `mod a { mod b { fn c }}` — recursion depth.
4. Mixed items in a mod (fn, struct, impl, const, nested mod).
5. Empty inline `mod foo {}` — no body items.
6. File-module decl `mod foo;` — NO `declaration_list`; must not panic/duplicate.
7. `#[test] fn` inside the mod — is_test must be set (extract_function path).
8. proptest!/macro-generated test fns — NOT function_item nodes (OUT of scope).

## Removed-invariant sweep (this change is subtractive)
Removed invariant: **"symbol/ref tables contain no `#[cfg(test)]` inline-module
(unit-test) code."** Product-health analyses relied on it. Readers that now see
test code (none filter test today except panic-points, which already does
`s.is_test = 0`):
- **INV-1 visibility-tightening (REGRESSION RISK):** a `pub` item used only by a
  same-crate unit test currently looks unused → correctly flagged tightenable.
  After the fix, that test usage suppresses the candidate. Must exclude test refs.
- **INV-2 unused-imports:** now sees test-module `use` statements (noise vs signal
  — decision needed).
- **INV-3 deprecated-callers:** now counts test call sites (the 1 broken fence).
- **INV-4 coupling / index counts:** +626 symbols shift metrics.

## Falsification
| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| 1 | Recursing MOD_ITEM indexes inline-mod symbols | index; `is_excluded_dir_allows_lib` is a symbol | grep confirms it's a `#[test] fn` | 2m | **passed** | fixture test: cfg(test) mod fn is indexed |
| 2 | Unit test fns get is_test=1 | is_test count ≈ source test-fn count (809 vs 842) | grep test attrs | 2m | **passed** | `.tethys-s8hv/probe.sh` floor + CI count test |
| 3 | Unit-test→product edges attach | refs with in_symbol_id in src is_test > 0 (3909) | SQL count (independent of symbol walk) | 2m | **passed** | fixture: unit test call → edge to product fn |
| 4 | `mod foo;` (no body) still indexes, no panic | index a file-module; module symbol present, no dup | existing file-module symbol counts unchanged | 2m | pending | existing module tests |
| 5 | Nested inline mods index innermost fn | fixture `mod a{mod b{fn c}}`; c is a symbol | grep fixture source | 5m | pending | fixture test |
| 6 | INV-1: pub item used only by unit test STILL flagged tightenable | fixture: `pub fn` called only from `#[cfg(test)]`; run visibility-tightening | manual/grep on fixture | 15m | **pending (LIKELY FAILS w/ minimal fix)** | fixture test in visibility suite |
| 7 | Blast radius bounded to known fences | full nextest | nextest | 20s | **passed** (1 expected) | updated deprecated-callers fence |

Claim 6 is the load-bearing one: it fails unless the fix also excludes test refs
from visibility-tightening.

## Negative space (what this fix deliberately does NOT do)
1. Does NOT index proptest!/macro-generated test functions (they're macro nodes,
   not function_item) — separate gap, filed as **tethys-0nar**.
2. Does NOT propagate is_test to non-attribute test *helper* fns inside cfg(test)
   modules (they keep is_test=0) — a "test-context" flag is out of scope (see
   tethys-m7zm for the shared-lever discussion).
3. Does NOT touch the C# extractor (namespace/class recursion already works).
4. Does NOT redesign unused-imports / deprecated-callers test handling beyond the
   minimal fence update — those semantics are filed as **tethys-m7zm**.

## Approved scope: A (2026-07-04)
Indexing fix + exclude test refs from visibility-tightening (close INV-1) +
update deprecated-callers fence + fences for claims 1–5. Follow-ups filed:
tethys-0nar (proptest fns), tethys-m7zm (unused-imports/deprecated-callers policy).

## Scope decision (for approval)
The bug fix is 5 lines. The DECISION is how much analysis-side test-handling
rides with it, given INV-1 is a real regression:
- **A (recommended):** indexing fix + exclude test refs from visibility-tightening
  (close INV-1) + update the deprecated-callers fence (INV-3) + fences for
  claims 1–5. File follow-ups for unused-imports test handling (INV-2) and
  proptest-macro-fn indexing.
- **B (minimal):** indexing fix + fence updates only; accept INV-1 regression and
  file it. Faster, but ships a known visibility-tightening regression — violates
  the PRD trust bar.
- **C (comprehensive):** A + a test-context flag + test filtering across all
  product-health analyses in one PR. Largest; likely over-scoped for one issue.
