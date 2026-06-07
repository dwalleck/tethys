# Plan: ModuleResolver seam (separator-fix)

Design: `.separator-fix/design.md` (approved; C1 passed, C6 baseline verified).
Every slice's standing oracle: **the dump oracle** — `.separator-fix/dump.sh` output
on (a) tethys self-index, (b) C# probe workspace, (c) C6 trap fixture must be
byte-identical to the Slice-0 baselines after EVERY slice (all slices are
behavior-neutral). `cargo test` green after every slice (nextest not installed on
this machine; cargo test is the equivalent gate).

**Oracle revision (Slice 1 halt):** the self-index oracle originally indexed the
live repo — but each slice adds the feature's own source to that repo, so the
input mutated and the dump legitimately drifted (84/158 diff lines were rows for
the new file; the rest, +1 line shifts in mod.rs). Corrected procedure: the input
workspace is a git worktree frozen at the Slice-0 commit (`/tmp/sf-frozen` @
57a506d); the baseline (`baselines/self-frozen.dump`) was captured with the
pre-seam binary built FROM that frozen tree and verified deterministic; every
slice's gate runs the current binary against the frozen tree. Slice-8 timing
comparison likewise: both binaries timed against the frozen tree, median of ≥5.

**Design refinement (budget-driven):** design.md said the Rust impl recomputes
src_root per resolve call. Budget math: `O(calls × crates)`; at 50k files ×
~20 calls/file × 200 crates ≈ 2×10⁸ path comparisons — over the 10⁶ budget.
Refinement: the trait gains `fn file_anchor(&self, file, ctx) -> Option<PathBuf>`,
called ONCE per file by the driver and carried in `ModuleContext.anchor`
(Rust: src_root via `cargo::get_crate_for_file`; C#: None). Restores today's exact
per-file cost. No design claim is invalidated (none asserted per-call recompute).

---

## Slice 0: Capture baselines (no code)

**Claim:** C1 (operationalize) — baselines exist and are deterministic.
**Oracle:** `.separator-fix/dump.sh` ×2 from-scratch runs per workspace, diff = empty (re-confirms C1).
**Stress fixture:** the three workspaces themselves: tethys self-index (161 files, multi-shape), C# probe WS (`probe.sh` recreates), C6 trap WS (recipe in design; recreate under `.separator-fix/fixtures/c6-trap/` recipe script).
**Loop budget:** n/a (no code).
**Wall budget:** n/a (batch, one-shot).
**Files:** none (artifacts: `.separator-fix/baselines/{self,csharp,c6trap}.dump`, hyperfine baseline JSON).

Commands: rebuild current binary from clean main; index each WS from scratch; dump; store. `hyperfine -r 3 'rm -f .rivets/index/tethys.db*; ./target/debug/tethys index'` → save median.

**Verification:**
- [ ] Three baseline dumps captured and re-run-deterministic
- [ ] Hyperfine baseline recorded
- [ ] Working tree still clean (no code touched)

---

## Slice 1: Trait, ModuleContext, C# stub, registry

**Claim:** C7 (stub declines), C10 (impls DB-free by construction).
**Oracle:** dump oracle (trivially — dead code); C7 unit tests; `rg 'use crate::db|&Index' src/languages/module_resolver.rs` = 0.
**Stress fixture (unit tests, expected outputs written now):**
  - `CSharpModuleResolver.resolve_import("System", ctx)` → `None`
  - `resolve_import("MyApp.Models", ctx)` → `None`
  - `resolve_import("", ctx)` → `None` (empty-input path exists)
  - `resolve_import("A::B", ctx)` → `None` (string containing the OTHER language's separator — name-collision bug class)
  - `qualified_splits("Foo::Bar", ctx)` → `[]`
  - `import_separator()` → `"."`
**Loop budget:** provided `resolve_import`: O(len(source_module)) split — trivial.
**Wall budget:** n/a (batch).
**Files:** `src/languages/module_resolver.rs` (new), `src/languages/mod.rs` (register + extend the "adding a language" doc checklist with "implement ModuleResolver" — covers spec B5).

**Code (advisory):** trait per design.md §Architecture plus the `file_anchor` refinement above; `get_module_resolver(lang) -> &'static dyn ModuleResolver`.

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixtures produce expected outcomes
- [ ] Dump oracle unchanged (3 workspaces)
- [ ] Budgets hold

---

## Slice 2: RustModuleResolver

**Claim:** enabler for C2/C6 — Rust impl reproduces today's candidate generation exactly.
**Oracle:** unit tests against on-disk tempdir fixtures (resolve_module_path itself is moved-by-delegation, already covered by existing `src/resolver.rs` tests + `tests/resolver_routing.rs`).
**Stress fixture (expected outputs written now, against a tempdir replica of the C6 trap):**
  - `qualified_splits("helper::do_thing", ctx@app/src/lib.rs)` → exactly one split with `files = [app/src/helper.rs, helper/src/lib.rs]` **in that order**, `tail = "do_thing"` (both candidates present — impl must NOT pre-filter; driver owns abandonment)
  - `qualified_splits("crate::db::open", ctx)` → splits whose files contain NO implicit-crate duplicate (prefix[0] == "crate" suppresses retry); tie-break/ordering bug class
  - `qualified_splits("a::b::c", ...)` → splits ordered longest prefix first: `[a::b | c]`, then `[a | b::c]`
  - hyphenated crate `my-lib` referenced as `my_lib::x` → resolves via existing normalization (moved code, existing tests)
  - `file_anchor` for orphan file (no crate) → parent-dir sentinel (matches `src_root_for_file` fallback)
**Loop budget:** `qualified_splits`: O(segments²) string ops per ref (segments ≤ ~8; same as today) + ≤2 `resolve_module_path` filesystem probes per split. Eager-B adds probes only where A succeeded on disk (today B was skipped then). Bound: fallback runs only for refs surviving all import arms (~10²–10³ per large workspace) × ~4 splits × ~12 stats ≈ ≤10⁵ syscalls, batch-phase, justified; C9 fence empirically guards. Mitigation if C9 busts: make B lazy via per-split closure.
**Wall budget:** n/a (batch; C9 fence at Slice 8).
**Files:** `src/languages/module_resolver.rs`, `src/cargo.rs` (new free fn `src_root_for(file, crates, workspace_root)` hoisted from `Tethys::src_root_for_file`, which becomes a thin delegate — its doc contract "sentinel for orphans" is a **sanity-hint** precondition, enforcement = existing behavior preserved + unit test, no runtime check needed since decline-to-sentinel is the documented refusal value).

**Verification:**
- [ ] Unit tests pass (including all stress shapes above)
- [ ] Dump oracle unchanged (dead code still)
- [ ] Budgets hold at fixture scale

---

## Slice 3: Rewire resolve.rs import arms

**Claim:** C2 (partial) — resolve_module_to_file_id + resolve_refs_for_file go through the seam.
**Oracle:** dump oracle ×3 vs baselines (FIRST live-wire slice — this is the moment of truth for the import arms).
**Stress fixture:** self-index dump diff (10⁴ refs across crate::/self::/super::/external shapes — production-shape, not hand-built); C# probe dump diff (stub now live on real C# arms: `using System`/`using MyApp.Models` must still decline identically).
**Loop budget:** no new loops — ctx construction once per file: O(1) + `file_anchor` O(crates) per file (= today's src_root_for_file cost, unchanged).
**Wall budget:** n/a (batch).
**Files:** `src/resolve.rs` (ResolveContext gains `resolver: &dyn ModuleResolver` + `module_ctx`; `resolve_module_to_file_id` body becomes `resolver.resolve_import(...)` + relative_path + get_file_id; `src_root` local removed).

**Doc-comment contracts touched:** `resolve_refs_for_file`'s "src_root anchor" doc rewritten to describe `file_anchor`; classification: sanity-hint (decline path documented), no runtime check added — wrong-anchor produces decline-not-wrong-output (resolve_module_path returns None for nonexistent paths).

**Verification:**
- [ ] Unit tests + existing pass2 integration tests pass
- [ ] Dump oracle byte-identical ×3
- [ ] Budgets hold

---

## Slice 4: qualified_module_fallback → neutral driver

**Claim:** C6 (within-split abandonment preserved), C2 (partial).
**Oracle:** dump oracle ×3; the C6 trap SQL row specifically.
**Stress fixture:** the C6 trap fixture — expected output (written before code, verified on current binary): `helper::do_thing` at `app/src/lib.rs:3` → **UNRESOLVED**. A flat-candidate driver resolves it via `helper/src/lib.rs` → slice FAILS. Committed as integration test `tests/qualified_split_trap.rs` (the C6 regression fence; fixture layout embeds the bug class per design).
**Loop budget:** driver: O(splits × files-per-split) = O(segments × 2) per surviving ref — strictly ≤ today's work + the eager-B delta already budgeted in Slice 2.
**Wall budget:** n/a (batch).
**Files:** `src/resolve.rs` (replace lines ~286–351 with the neutral driver: per split → first indexed file claims → tail lookup → miss abandons split), `tests/qualified_split_trap.rs` (new).

**Code (advisory):**
```rust
for split in ctx.resolver.qualified_splits(ref_name, &ctx.module_ctx) {
    let Some(file_id) = split.files.iter()
        .find_map(|f| self.db.get_file_id(&self.relative_path(f)).transpose())
        .transpose()? else { continue };
    if let Some(sym) = self.db.search_symbol_by_qualified_name_in_file(&split.tail, file_id)? {
        return Ok(Some(sym));
    }
    // tail miss: abandon split — do NOT try remaining files (C6)
}
```

**Verification:**
- [ ] `qualified_split_trap` passes (UNRESOLVED preserved)
- [ ] Dump oracle byte-identical ×3
- [ ] Existing pass2_qualified_paths tests pass
- [ ] Budgets hold

---

## Slice 5: Rewire indexing.rs import-dep call sites

**Claim:** C5 (indexing routed through the seam).
**Oracle:** dump oracle ×3 — `## file_deps` and `## imports` sections specifically; `rg 'resolve_module_path' src/indexing.rs` = 0.
**Stress fixture (script-level now; committed test in Slice 7):** mixed-language workspace — Rust workspace containing a crate named `System` + a C# file with `using System;` and a used import. Expected (written now): NO file_dep edge from the C# file to `System/src/lib.rs` (per-file-language dispatch sends C# imports to the stub). A dispatch-by-workspace-language bug creates the edge → fixture fails.
**Loop budget:** no new loops; per-import resolve unchanged in count; `file_anchor` once per file (as today).
**Wall budget:** n/a (batch).
**Files:** `src/indexing.rs` (sites ~:950 and ~:1112 → `resolver.resolve_import_segments(&import_stmt.path, &ctx)`; language available from the per-file record/extension at both sites), `src/types.rs` (line ~237 doc comment re-pointed from `resolver::resolve_module_path` to the trait — comment-only).

**Verification:**
- [ ] Mixed-WS stress script: no phantom System edge
- [ ] Dump oracle byte-identical ×3 (file_deps section scrutinized)
- [ ] `rg resolve_module_path src/indexing.rs` = 0
- [ ] Budgets hold

---

## Slice 6: Storage-side separator matches → import_separator()

**Claim:** C8.
**Oracle:** dump oracle ×3 — `## imports` section (a swapped constant stores `MyApp::Models` for C# → caught); existing indexing tests.
**Stress fixture:** C# probe WS imports section must read exactly `System` / `MyApp.Models` (dotted); self-index imports must keep `::` modules. Both already in baselines — byte-comparison IS the adversarial check (bug class: constant swap / wrong-language lookup).
**Loop budget:** none (constant lookup replaces match).
**Wall budget:** n/a.
**Files:** `src/batch_writer.rs` (:378), `src/indexing.rs` (:852, :1047).

**Verification:**
- [ ] Dump oracle byte-identical ×3 (imports section)
- [ ] nextest green
- [ ] Budgets hold (vacuously)

---

## Slice 7: Lint fences + mixed-dispatch fence

**Claim:** C4, C5 (grep half), C10 — as permanent CI fences; C5's stress fixture committed.
**Oracle:** the lint tests themselves ARE greps: read `src/resolve.rs`, assert no `resolve_module_path|CrateInfo|"crate"|"super"` token; read `src/indexing.rs`, assert no `resolve_module_path`; read `src/languages/module_resolver.rs` + impl files, assert no `use crate::db`/`&Index`.
**Stress fixture:** the lint test must FAIL when run against the pre-slice-3 tree (verify by temporarily reverting one call site — TDD-inversion check that the fence can fire); the mixed-WS System-trap from Slice 5 lands as committed `tests/mixed_language_dispatch.rs`.
**Loop budget:** test-only file reads, O(file size).
**Wall budget:** n/a (test time).
**Files:** `tests/seam_lint.rs` (new), `tests/mixed_language_dispatch.rs` (new).

**Verification:**
- [ ] Lint tests pass on post-change tree
- [ ] TDD-inversion: lint test demonstrably fails on a reverted call site
- [ ] Mixed-dispatch test passes
- [ ] Dump oracle byte-identical ×3

---

## Slice 8: Acceptance ceremony (no code)

**Claim:** C2, C3 (final), C9.
**Oracle:** full dump diff ×3 vs Slice-0 baselines; `cargo nextest run`; `cargo clippy`; `hyperfine -r 3` vs baseline median (±10%; manual fence, approved in design).
**Stress fixture:** n/a — this slice is the oracle run itself; its adversarial content lives in slices 1–7.
**Loop budget:** n/a.
**Wall budget:** n/a.
**Files:** none (artifact: `.separator-fix/acceptance.md` recording diffs=0, test counts, clippy=0, hyperfine numbers).

**Verification:**
- [ ] Dump diffs empty ×3
- [ ] nextest green, clippy zero new warnings
- [ ] Hyperfine within ±10% (manual fence recorded)
- [ ] acceptance.md written

---

## Plan Self-Review

1. **Loops:** Slice 2 `qualified_splits` O(segments²) strings + ≤2 fs-probes/split, ≤10⁵ batch syscalls justified in-slice with C9 backstop; Slice 4 driver O(segments×2)/ref; `file_anchor` O(crates) once per file (= status quo, the per-call design wart was refined out above); Slices 1/6 trivial; no always-on loops. **No gaps.**
2. **Fixtures:** S1 cross-separator + empty-string inputs (collision/empty bug classes); S2 trap-ordering + crate-prefix suppression + hyphen + orphan (pre-filter, suppression, normalization, sentinel bugs); S3 production-shape self-index + live-stub C# (not hand-built happy path); S4 C6 trap (flat-candidate bug, expected output pre-written and binary-verified); S5 System-name collision (dispatch bug); S6 byte-comparison vs baseline (constant-swap bug); S7 TDD-inversion proves fences can fire. **No happy-path-only fixtures; no gaps.**
3. **Doc-comment preconditions:** `src_root_for`/`file_anchor` orphan-sentinel — sanity-hint, documented refusal value (decline), unit-tested (S2); `qualified_splits` "driver abandons on tail miss" — behavioral contract enforced by C6 fence test, not assertable at runtime without false positives; existing `segments.len() < 2` safe-decline runtime guard preserved (load-bearing, already a runtime check). **No unenforced contracts; no gaps.**
4. **Write targets:** no new stdout/stderr writes anywhere; all new diagnostics via existing `tracing` (stderr) — data/diagnostic rule vacuously satisfied; dump.sh writes data to stdout (correct, pipeable). **No gaps.**
5. **Tracker references:** tethys-jwf9 (stub doc cite — verified open), tethys-8mze (checklist doc cite — verified open), tethys-dsp1 (filed during design, verified present in `.rivets/issues.jsonl`). No other deferral language in this plan. **No gaps.**

Claim coverage vs design: C1→S0, C2→S3/S4/S8, C3→S3/S8, C4→S7, C5→S5/S7, C6→S4, C7→S1, C8→S6, C9→S8, C10→S1/S7. **All 10 design claims covered.**
