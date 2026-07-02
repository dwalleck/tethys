# Plan: re-export references (tethys-v1w8)

Design: `.tethys-v1w8/design.md` (APPROVED 2026-07-01 — user proceeded to budgeted-plan).
Oracle of record: `.tethys-v1w8/probe.py` against a fresh self-index copy.
Baseline (pre-change self-index): refs kinds = call 6007, construct 227, macro 1669,
type 2648, reexport 0; 80 re-exported names / 18 sites; 9 zero-ref re-exported symbols.
Post-build expected: probe Q1b reports refs at all sites (80 names), Q2 list = [].

Branch: `feat/tethys-v1w8-reexport-refs`.

---

## Slice 1: `ReferenceKind::Reexport` variant + string round-trip

**Claim:**          C1 (prerequisite: the kind exists and round-trips as `'reexport'`)
**Oracle:**         exhaustive variant round-trip vs the hand-written string table (types.rs unit test)
**Stress fixture:** `from_str("reexport") == Some(Reexport)`; every variant `as_str→from_str` identity; `from_str("bogus")` still `None` (unknown-kind behavior unchanged for old DBs)
**Loop budget:**    none (no loops)
**Wall budget:**    n/a (not an always-on phase)
**Files:**          `src/types.rs`

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixture produces expected outcome
- [ ] Probe oracle unaffected (no behavior change yet — self-index refs identical)
- [ ] Budgets hold (vacuous)

## Slice 2: extractor emits reexport refs for named leaves

**Claim:**          C1 (one ref per non-glob leaf name, kind=reexport, at site), C3 (original name, not alias), C6-partial (glob/module leaves emit nothing)
**Oracle:**         hand-enumerated expected `(name, line, kind)` set for a fixed source snippet, written below *before* implementation
**Stress fixture:** source containing: `pub use a::B;` · `pub(crate) use c::D;` · `pub use e::{F, G as H, i::J};` · `pub use k::*;` · `use l::M;` · `pub use n::Trait as _;` · `fn f() { use o::P; }` · a `pub use` inside a string literal.
Expected refs: exactly `{B, D, F, G, J, Trait}` (kinds all `reexport`, correct lines) — targets: per-statement-instead-of-per-name, alias-name recording, glob leak, non-pub leak, fn-scope leak, `as _` handling, string-literal safety. Nested `i::J` asserts **parity with `parse_use_declaration`'s name list** (inherits tethys-pdea, doesn't fix it).
**Loop budget:**    O(use_declarations × leaf_names) per file ≈ O(import rows); tethys self ≈ 10² rows/file max, workspace total ≈ 10³ — far under 10⁶
**Wall budget:**    self-index stays ≤ 500 ms (443 ms baseline + noise; measured in slice 7)
**Files:**          `src/languages/rust.rs` (emission + inline unit tests)

**Code (advisory):** reuse `parse_use_declaration`'s `UseStatement` (already computes `is_reexport` + names); when `is_reexport`, emit one `ExtractedReference { kind: Reexport, name: <original leaf>, .. }` per non-glob name. Doc-comment precondition "call only for use_declaration nodes" = sanity hint → `debug_assert!`.

**Verification:**
- [ ] Unit tests pass (expected set above, exact)
- [ ] Stress fixture produces expected outcome
- [ ] Probe oracle: Q1a still 80/80 (inventory unchanged)
- [ ] Loop budget holds at fixture scale

## Slice 3: storage + Pass-2 resolution parity (integration fences)

**Claim:**          C2 (resolves to same symbol_id as bare body-usage), C5 (external target stored unresolved: symbol_id NULL + reference_name set), C7 (self::/crate:: path-prefix parity with plain imports)
**Oracle:**         equality of two independently produced rows (reexport ref vs body-call ref for the same symbol) — neither side knows about the other
**Stress fixture:** workspace with `inner.rs: pub fn helper()`, **plus a same-named `other.rs: pub fn helper()`** (name-collision class, tethys-53iv family): the reexport must resolve via its own import path to *inner*'s helper, or decline — never other's. External: `pub use serde::Serialize;` → NULL + name. Path-prefix: `pub use self::inner::helper as H2;` and `pub use crate::inner::helper as H3;` each asserted equal-outcome to a plain-import control in the same fixture (documents 3i35/xzdr-family behavior by parity, not by absolute expectation).
**Loop budget:**    test-only; index of ≤6-file fixture — trivial
**Wall budget:**    n/a
**Files:**          `tests/reexport_refs.rs` (new; fixture built fresh per test — never ambient DB)

**Verification:**
- [ ] Unit tests pass
- [ ] Collision stress resolves to the import-path symbol (or declines), never the wrong file's
- [ ] Probe oracle still agrees post-slice
- [ ] Budgets hold (vacuous)

## Slice 4: headline, glob/module zero-emission, idempotency

**Claim:**          C12 (re-export-only symbol has exactly 1 inbound ref), C6 (glob + module re-exports emit zero refs — deferred work is tethys-pv7w), C13 (double index → identical refs/file_deps/call_edges counts)
**Oracle:**         SQL counts against the fixture DB; expected values written here: reexport-only symbol refs = 1; refs at glob/module sites = 0; second-index deltas = 0 across all three tables
**Stress fixture:** symbol `OnlyViaReexport` with zero body uses anywhere; `pub use m::*;` and `pub use crate::db_mod;` sites; **index the same workspace twice without --rebuild** (the UPSERT/stale-row class, tethys-wsix's sibling family) and diff counts. Plus a re-exported `macro_rules!` name — resolves to the macro symbol via its import path.
**Loop budget:**    test-only
**Wall budget:**    n/a
**Files:**          `tests/reexport_refs.rs`

**Verification:**
- [ ] Unit tests pass; expected counts exact
- [ ] Idempotency: zero delta on re-index
- [ ] Probe oracle still agrees
- [ ] Budgets hold (vacuous)

## Slice 5: consumer no-op fences (call_edges, panic-points)

**Claim:**          C8 (zero call_edges from reexport refs), C9 (zero panic-points even when the re-exported name is `expect`)
**Oracle:**         SQL counts with expected values fixed in advance: call_edges where callee = re-exported symbol == number of body CALLS only; panic-points == 0 for the reexport, and the *body call* control in the same fixture keeps its expected count — so the fence distinguishes "reexport leaked in" from "panic-points broke"
**Stress fixture:** external `pub use ext::expect;` (unresolved → reference_name='expect', the exact panic-points predicate) + an in-crate fn body that also calls a real `expect`-named fn. Buggy implementation this fails under: emission attributing a file-level pseudo-symbol to `in_symbol_id`.
**Loop budget:**    test-only
**Wall budget:**    n/a
**Files:**          `tests/reexport_refs.rs`

**Verification:**
- [ ] Unit tests pass
- [ ] Stress: reexport of `expect` yields 0 panic rows while control body-call rows unchanged
- [ ] Probe oracle still agrees
- [ ] Budgets hold (vacuous)

## Slice 6: file_deps gains the re-export-only edge

**Claim:**          C10 (previously-missing edge appears; no duplicates on re-run)
**Oracle:**         F4's design-time result inverted: fixture `a.rs: pub use b::OnlyReexported;` (name unused in a's body) must now yield file_deps a→b; pre-change code demonstrably lacks it (probe finding 2: lib.rs→unused_imports.rs absent)
**Stress fixture:** third file `c.rs: use b::OnlyReexported;` (plain, unused) must NOT gain c→b (no spill into the plain-unused-import case — the tethys-msn0 corroboration family); re-run indexing → edge count stable at 1
**Loop budget:**    `compute_dependencies` refs_set grows by #reexport refs (≈80 on self-index, ≈10¹ on fixture) — no new loop, existing O(refs) pass absorbs it; ≪ 10⁶
**Wall budget:**    covered by slice-7 self-index measurement
**Files:**          `tests/reexport_refs.rs`, `src/indexing.rs` (only if refs_set turns out to filter by kind; expectation from code read: no change needed)

**Verification:**
- [ ] Unit tests pass; a→b edge exists; c→b absent; count stable on re-run
- [ ] Probe oracle still agrees
- [ ] Loop budget holds

## Slice 7: self-index oracle re-run, golden update, kind-histogram parity

**Claim:**          C11 (unused-imports self-index output unchanged), C14 (non-reexport kind counts unchanged: call 6007, construct 227, macro 1669, type 2648 on the frozen probe workspace)
**Oracle:**         `.tethys-v1w8/probe.py` re-run on a fresh post-build self-index — expected: Q1a 0 missing, Q1b refs present at pub-use sites for all 80 names, Q2 zero-ref list EMPTY; plus `tethys unused-imports` output diffed byte-for-byte pre/post (rustc-adjacent self-oracle: warning-free crate ⇒ any new Definite finding is a bug here)
**Stress fixture:** the idxperf golden fixture — regenerate expected dump ONCE, and review the diff: it must contain ONLY added kind='reexport' rows (any other delta fails C14). Golden test then re-pins byte-identical content permanently.
**Loop budget:**    no new loops
**Wall budget:**    self-index wall time ≤ 500 ms (baseline 443 ms); recorded in audit trail — the golden test is the CI fence, the timing is a one-shot measurement
**Files:**          `tests/idxperf_golden.rs` (+ its regenerated golden data file)

**Verification:**
- [ ] Probe Q2 = [] (the 9 symbols each gained ≥1 ref) — headline
- [ ] unused-imports diff empty
- [ ] Golden diff = reexport rows only; non-reexport histogram identical
- [ ] Wall budget ≤ 500 ms recorded

---

## Claim coverage

C1→S1+S2 · C2→S3 · C3→S2 · C4→S2 (parity assert) · C5→S3 · C6→S2+S4 ·
C7→S3 · C8→S5 · C9→S5 · C10→S6 · C11→S7 · C12→S4 · C13→S4 · C14→S7.
All 14 design claims covered; no slice exceeds 2 files or ~30 minutes.

## Plan Self-Review

1. **Loops:** one new loop (S2 emission, O(use_decls × leaf_names) ≈ O(imports) ≈ 10³ workspace-wide); one existing loop absorbs growth (S6 refs_set, +80 items). Both ≪ 10⁶. No gaps.
2. **Fixtures:** every slice names its bug class — per-name emission (S2), name collision / wrong-symbol binding (S3), UPSERT idempotency + glob leak (S4), pseudo-symbol in_symbol_id (S5), corroboration spill (S6), golden drift beyond reexport rows (S7). No happy-path-only fixtures. No gaps.
3. **Doc-comment preconditions:** one — S2's "call only for use_declaration nodes" = sanity hint → `debug_assert!` (release violation produces no output rather than wrong output). No load-bearing preconditions introduced. No gaps.
4. **Write targets:** none new — indexing diagnostics remain on tracing/stderr; no CLI surface changes in this issue. No gaps.
5. **Tracker references:** tethys-pv7w (glob/module deferral — verified, blocks-linked), tethys-pdea (nested-group parity), tethys-53iv (collision family context), tethys-msn0 (corroboration family), tethys-3i35 / tethys-xzdr / tethys-nkjd (path-prefix parity), tethys-wsix (stale-row family). All verified to exist this session. No unfiled deferrals.
