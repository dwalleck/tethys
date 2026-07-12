# tethys-3i35 — budgeted plan

Design: `.tethys-3i35/design.md` (approved 2026-07-12, decisions recorded).
Claim coverage: C1→S2, C2→S2, C3→S2(fence)+S7(audit), C4→S2, C5→S3, C6→S3,
C7→S4, C8→S4, C9→S5, C10→S6, C11→S7, C12→S6.

Pre-fix baseline for the S7 audit was captured at plan time (pre-fix binary):
`.tethys-3i35/prefix-resolved-refs.tsv` (4,295 resolved refs).

---

## Slice 1: resolver fix — bare `crate` resolves to the crate-root file

**Claim:** The `["crate"]` path maps per the design's crate-root-choice
table; `["crate", rest…]`, `self`, `super`, workspace-crate arms unchanged;
`resolve_crate_path`'s empty-path branch removed.
**Oracle:** rustc semantics table (pinned by the design's cheapest
falsifier, E0425 run recorded); unit tests assert against on-disk temp
trees, not resolver internals.
**Stress fixture:** unit fixtures designed against: lib-preferred-always
fabrication (bin-root file must map to itself, NOT lib.rs); missing
`.filter(exists)` (CrateInfo declares lib.rs, file deleted → None);
unwrap-on-no-entry panic (no lib.rs, no main.rs → None, no panic);
src/bin/tool/helper.rs non-root (None); foreign file with non-empty crate
list (None); single-bin-no-lib submodule (→ main.rs). Expected outputs
written in the test names before implementation.
**Loop budget:** new helper `crate_root_file`: `get_crate_for_file` =
O(|crates|) scan + 1 canonicalize syscall; + O(|bins|) bin-root comparison;
+ 1 `.exists()` syscall. Called once per bare-crate import or `crate::`
split enumeration: tethys scale ≈ 40 bare-crate imports + ≤13 qualified
refs ≈ 60 calls × ~3 syscalls ≈ 200 syscalls, in the one-shot Pass-2 phase
(not always-on). Generous ceiling: a 50k-file workspace with 200 crates and
5k bare-crate imports ⇒ 5k × O(200) = 10^6 comparisons worst case —
at the budget edge, acceptable for a one-shot phase; no new always-on loop.
**Wall budget:** n/a (no always-on phase).
**Files:** `src/resolver.rs` (arm + helper + unit tests; import
`crate::cargo::get_crate_for_file`).

**Code (advisory):**
```rust
"crate" => {
    if path.len() == 1 {
        return crate_root_file(current_file, workspace_crates);
    }
    resolve_crate_path(&path[1..], crate_root)
}
// resolve_crate_path: empty-path branch removed; add
// debug_assert!(!path.is_empty()) (sanity hint — both remaining callers
// pass a non-empty slice by construction).
```
`crate_root_file`: per design table; bin-root equality via the same
canonicalized-path comparison family `get_crate_for_file` uses.

**Verification:**
- [ ] Unit tests pass (incl. existing `resolves_crate_path_to_file` /
      `_to_mod_rs` — multi-segment arm unchanged)
- [ ] Stress fixtures produce expected outcome (each row a distinct test)
- [ ] prove-it-prototype oracle still agrees (probe rerun deferred to S2 —
      no binary-observable change until integration)
- [ ] Budgets hold (no new always-on loop)

---

## Slice 2: integration fences — decoy repro, method tail, callers, shadow

**Claim:** C1 (crate::helper → crate_a's helper, strategy
`qualified_module_fallback`, reference_name NULL, decoy untouched),
C2 (crate::Thing::make → method symbol), C4 (get_callers("helper") includes
the b.rs call site), C3-fence (root decoy does not shadow submodule tail).
**Oracle:** rustc — every fixture shape was compile-checked during
probe/design (cargo check on the repro; `crate` cannot cross crates);
asserts read refs/symbols/files via SQL joins, independent of resolver
internals.
**Stress fixture:** (i) same-named `helper` in a sibling crate — fails
under workspace-first crate mapping; (ii) `fn f` in BOTH lib.rs and
inner.rs with `crate::inner::f()` — fails if the `["crate"]` split
outranks the longer `["crate","inner"]` split (must bind inner.rs's f).
Expected: helper→crate_a/src/lib.rs, Thing::make→method row,
callers⊇{use_it}, f→inner.rs.
**Loop budget:** tests only — no production loops.
**Wall budget:** n/a.
**Files:** `tests/pass2_crate_root_paths.rs` (new).

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixtures produce expected outcome
- [ ] Probe rerun: `python .tethys-3i35/probe.py <repro>` now shows both
      refs RESOLVED to the simulation's exact targets (oracle agreement)
- [ ] Budgets hold

---

## Slice 3: crate-root-choice matrix + degenerate crates

**Claim:** C5 rows (a)–(e) and C6 (no-entry crate + foreign file decline;
indexing completes without error).
**Oracle:** rustc semantics table (row (b) was E0425-verified during
design; rows arranged so every fixture compiles or the non-compiling shape
is deliberately absent from the fixture and asserted unresolved).
**Stress fixture:** row (b) IS the adversarial case — `crate::y()` in
main.rs with y only in lib.rs must stay UNRESOLVED (fabrication detector);
row (e) src/bin/tool/helper.rs must stay UNRESOLVED (bin-module
ambiguity). Expected outputs: (a) main.rs::x, (b) NULL, (c) lib.rs::y,
(d) main.rs::x, (e) NULL, C6 NULL + exit success.
**Loop budget:** tests only.
**Wall budget:** n/a.
**Files:** `tests/pass2_crate_root_paths.rs`.

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixtures produce expected outcome
- [ ] Oracle agreement (rustc table)
- [ ] Budgets hold

---

## Slice 4: import side — xzdr Definite upgrade, stats drop, glob arm

**Claim:** C7 (`use crate::Foo;`, unused root struct → **Definite**;
unresolved-dependencies stat 0 on the repro shape), C8 (`use crate::*;`
lets a bare call to a root fn resolve via the glob arm).
**Oracle:** cargo check emits `unused_imports` warning for the fixture =
compiler ground truth that the import is unused and (being a struct) not a
trait-method channel; glob fixture compiles only because the glob import
brings the fn in — rustc again.
**Stress fixture:** a USED bare-crate import (`use crate::Foo;` + `Foo` in
a type position) must produce NO finding — fails if the confidence upgrade
misfires on used imports (C11's bug class at fixture scale). A
`use crate::NotThere;` (name absent from lib.rs) must stay MaybeTrait —
fails if resolution success is assumed instead of checked.
**Loop budget:** tests only.
**Wall budget:** n/a.
**Files:** `tests/indexing.rs` (extend where unused-imports integration
asserts live) — or a focused new `tests/unused_imports_crate_root.rs` if
indexing.rs has no natural home; ≤2 files either way.

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixtures produce expected outcome
- [ ] Oracle agreement (cargo check warning on the fixture, run once and
      recorded in the test comment)
- [ ] Budgets hold

---

## Slice 5: tripwire flips — refs_named + deprecated_callers

**Claim:** C9 — tests/refs_named.rs flips exactly as its TRIPWIRE predicts
(`helper` 3→4, `crate::helper` 1→0); deprecated_callers' `crate::old_q()`
"Path B recovery" expectations updated to post-fix truth.
**Oracle:** the fixtures' own source text (call sites countable by eye:
3 bare + 1 qualified `helper`); rustc semantics for which calls must
resolve.
**Stress fixture:** the flip protocol itself: run BOTH files' existing
asserts against the post-fix binary FIRST and record which fail — each
failure must be one the TRIPWIRE/Path-B comments predicted. An unpredicted
failure = drift → STOP (checkpointed-build rule). Then update asserts.
**Loop budget:** tests only.
**Wall budget:** n/a.
**Files:** `tests/refs_named.rs`, `tests/deprecated_callers.rs`.

**Verification:**
- [ ] Unit tests pass (post-update)
- [ ] Pre-update failures were exactly the predicted set
- [ ] Oracle agreement (counts match source text)
- [ ] Budgets hold

---

## Slice 6: idempotency + goldens

**Claim:** C10 — double-index of a bare-crate-import fixture yields
identical refs and file_deps content (incl. `ref_count` — no UPSERT
double-bump from the newly-resolving import); C12 — idxperf goldens
byte-identical (their fixtures contain no crate-root shapes; verified by
grep at design time).
**Oracle:** mechanical SQL dump diff (independent of resolver);
pre-recorded golden dumps.
**Stress fixture:** fixture with `use crate::helper;` + a bare call + a
qualified `crate::helper()` call, indexed twice via rebuild AND via
re-index — fails if import-derived file_deps rows accumulate ref_count
across runs.
**Loop budget:** tests only.
**Wall budget:** n/a.
**Files:** `tests/file_deps_idempotency.rs` (extend fixture if the shape
is absent; verify existing snapshot helpers cover ref_count).

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixture produces expected outcome (identical snapshots)
- [ ] `cargo nextest run idxperf` green with NO golden churn (any golden
      diff = drift → STOP)
- [ ] Budgets hold

---

## Slice 7: one-shot audits — monotonicity (C3) + self-index oracle (C11)

**Claim:** C3 as amended (symbol_id stable; unresolved→resolved only;
strategy shifts only same_crate/unique_workspace→explicit_import on
bare-crate-imported names; the 13 submodule-tail refs stay unresolved);
C11 (zero Definite unused-imports findings on tethys itself post-fix).
**Oracle:** the pre-fix binary's own output
(`.tethys-3i35/prefix-resolved-refs.tsv`, captured at plan time) — fully
independent of the new code; rustc/clippy warning-free CI build for C11.
**Stress fixture:** n/a (audit slice — the "fixture" is the production
codebase itself; per-class classification of every diff line replaces it).
Expected outputs written down NOW: diff contains (1) zero removed lines,
(2) zero symbol_id changes, (3) added lines only for newly-resolved refs,
(4) strategy-column changes only same_crate/unique_workspace→
explicit_import on names with a bare-crate import in that file, (5) the 13
findings.md refs absent from both dumps' resolved sets, (6) unused-imports
Definite count = 0.
**Loop budget:** audit script O(refs) ≈ 20k rows, one-shot, local.
**Wall budget:** n/a.
**Files:** `.tethys-3i35/audit.md` (+ the dump/diff script alongside the
probe artifacts). No src changes. Any deviation from the expected outputs
= STOP and surface (drift rule).

**Verification:**
- [ ] Audit diff classified 100% into the expected classes
- [ ] unused-imports on tethys: zero Definite findings
- [ ] prove-it-prototype oracle agreement recorded in audit.md
- [ ] Budgets hold

---

## Plan Self-Review

1. **Loops:** one new production code path (S1 `crate_root_file`):
   O(|crates|) + O(|bins|) + ≤3 syscalls per bare-crate resolution; ~200
   syscalls at tethys scale, 10^6 comparisons at a hostile 50k-file/200-
   crate/5k-import scale — one-shot index phase, within budget. All other
   slices are tests/audits. No gaps.
2. **Fixtures:** every logic slice has an adversarial fixture naming its
   bug class (workspace-first mapping, short-split shadowing, lib-preferred
   fabrication, exists-filter omission, upgrade-misfire on used imports,
   ref_count accumulation, unpredicted tripwire failures). No happy-path-
   only fixtures. No gaps.
3. **Doc-comment preconditions:** `entry_point_file()`'s documented
   "callers must `.filter(exists)`" → complied (load-bearing, enforced at
   call site). `resolve_crate_path` non-empty precondition after branch
   removal → `debug_assert!` (sanity hint: both remaining callers pass
   non-empty by construction; release-mode violation yields a nonexistent
   path that `resolve_as_module` maps to None — no silent wrong output).
   No gaps.
4. **Write targets:** no new production writes; audit artifacts are repo
   markdown (data). No gaps.
5. **Tracker references:** tethys-qtq5 (re-export tail chasing — filed
   from this probe, verified), tethys-oojq (self-index CI fence — filed at
   plan time, verified), tethys-xzdr (closing with this PR per design
   decision), tethys-nkjd / tethys-i09d / tethys-0nar / tethys-9l27 /
   tethys-53iv (negative space, all verified open), tethys-k543
   (speculative-band LSP re-verify — verified, informational). No gaps.
