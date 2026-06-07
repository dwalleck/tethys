# Plan: C# namespace resolution + post-pass absorption (csharp-ns)

Design: `.csharp-ns/design.md` (approved; C1 passed, C2 ran-failed→descoped
tethys-nnst, C9 baseline established). Standing per-slice oracle, reusing the
separator-fix infrastructure (`.separator-fix/dump.sh`, frozen-worktree
procedure): after EVERY slice — `cargo test` green; Rust dumps byte-identical
(frozen self-index worktree @ slice-0 commit + c6trap fixture); probe-WS C#
dump either byte-identical (slices 1–3) or matching the slice's enumerated
expectation table (slices 4–6, where behavior deliberately changes).

---

## Slice 0: Baselines + expectation tables (no code)

**Claim:** operationalizes C6/C7/C8 oracles; all baselines pre-date code.
**Oracle:** dump determinism re-check (×2 from-scratch per workspace).
**Stress fixture:** the workspaces themselves: probe WS (recipe → `.csharp-ns/fixtures.sh csharp-gt`), cross-dir WS (`xdir`), c6trap (existing recipe), frozen self-index worktree @ current main.
**Loop budget:** n/a (no code).
**Wall budget:** n/a.
**Files:** none (artifacts: `.csharp-ns/fixtures.sh`, `.csharp-ns/baselines/*.dump`, `.csharp-ns/expectations.md`, timing baseline).

`expectations.md` written NOW, before any code:
- New resolutions (post-S4): collision-fixture Widget → My.Models' Widget. Probe-WS refs: NO target changes (monotone table = current dump rows).
- L2 delta (post-S6) on probe WS: `App.cs→Other.cs` GONE (unused using); `GlobalUsings.cs→Globals.cs` GONE (no refs); `App.cs→Models.cs` ref_count 4→3 (post-pass row removed, 3 call-edge rows remain); `UseScoped.cs→Scoped.cs` 2→1; `UseScoped.cs→Nested.cs` 1→1 (call-edge only, unchanged).
- Cross-dir WS: `services/Svc.cs→models/Widget.cs` count 1→1 (source flips post-pass→corroborated call-edge).

**Verification:**
- [ ] Baselines deterministic ×2 each
- [ ] Expectation tables complete before slice 1 begins
- [ ] Working tree clean

---

## Slice 1: NamespaceMap type + lift build_namespace_map to paths

**Claim:** C1 — flat-key map (dotted declarations + file-scoped key correctly; nested blocks absent as dotted keys; Rust modules excluded).
**Oracle:** unit tests vs hand-enumerated namespace list; probe-WS dump byte-identical (post-pass adapts path→file_id at lookup, same files).
**Stress fixture (expected outputs pre-written):**
  - Same namespace declared in 2 files → `Vec<PathBuf>` **sorted by path** (determinism; insertion order from parallel discovery must not leak)
  - `"Outer1.Inner1"` is NOT a key; flat `"Outer1"`/`"Inner1"` keys exist but no dotted lookup can hit them (tethys-nnst)
  - Rust `mod` symbols excluded (mixed fixture: Rust module name absent from map)
  - Dotted declaration `My.Models` and file-scoped `My.Scoped` both key exactly
**Loop budget:** O(module-symbols) query (existing NAMESPACE_QUERY_LIMIT cap) + O(n log n) sort; n ≤ limit — within budget.
**Wall budget:** n/a (once per index/resolve run, batch).
**Files:** `src/languages/module_resolver.rs` (`pub(crate) type NamespaceMap = HashMap<String, Vec<PathBuf>>`), `src/indexing.rs` (build_namespace_map returns NamespaceMap; post-pass converts path→file_id per lookup — transitional, dies in slice 6).

**Verification:**
- [ ] Unit tests pass (4 stress shapes)
- [ ] Dump oracle byte-identical ×3
- [ ] Budgets hold

---

## Slice 2: ModuleContext.namespaces + GlobResolution + trait methods + C# overrides

**Claim:** trait surface for C3–C5 with Rust-identical defaults (C7 enabler); ctx-carried map preserves DB-free fence (C11).
**Oracle:** unit tests; dump oracle ×3 (defaults = dead code for Rust, stub-equivalent for C# until slice 4).
**Stress fixture (pre-written):**
  - `RustModuleResolver.glob_resolution()` == `{ FirstMatch, kinds: None }`; default `resolve_import_files("crate::db")` == vec![the single resolve_import file]
  - `CSharpModuleResolver.resolve_import_files("My.Models", ctx with map)` == sorted namespace files; with `ctx.namespaces = None` → **empty vec, no panic** (the None input shape; doc-comment contract: None is a documented refusal, enforced by Option handling — load-bearing, runtime by construction)
  - `CSharpModuleResolver.glob_resolution().kinds` == `[Class, Struct, Interface, Enum]` exactly (no Record variant exists)
  - Cross-separator input `"A::B"` → empty (map miss)
**Loop budget:** map lookup O(1) + clone of Vec<PathBuf> per call (≤ files-per-namespace ≈ 10²) — trivial.
**Wall budget:** n/a.
**Files:** `src/languages/module_resolver.rs`, plus the mechanical `namespaces: None` field-init at the 3 existing ModuleContext construction sites (resolve.rs ×1, indexing.rs ×2 — 1 line each; 3-file touch justified: field addition cannot compile partially).

**Verification:**
- [ ] Unit tests pass
- [ ] Dump oracle byte-identical ×3
- [ ] seam_lint suite (incl. DB-free C10) passes unchanged
- [ ] Budgets hold

---

## Slice 3: DB helper — unique kind-filtered symbol lookup across files

**Claim:** C3/C4/C5's query primitive: `search_unique_symbol_by_name_in_files(name, kinds, file_ids) -> Option<Symbol>` returning Some iff exactly one match.
**Oracle:** unit tests against a hand-built DB (rusqlite direct inserts, independent of the resolver).
**Stress fixture (pre-written):**
  - Same name, different kinds in one file (class `Widget` + method `Widget`): kinds=[Class,...] → the class only (kind-filter bug class)
  - Two classes same name in two listed files → None (unique rule; LIMIT 2 semantics)
  - Empty `file_ids` → None, no SQL error (empty-IN bug class; documented refusal, runtime-checked early return — load-bearing)
  - 1,200 file_ids → works (SQLite host-parameter limit bug class: chunk the IN-list at 500 and aggregate counts across chunks; expected: match found, uniqueness still global across chunks)
**Loop budget:** O(file_ids / 500) indexed queries per call; called once per candidate-bearing ref (slice 4 keeps total at one helper call per unresolved C# ref): refs ≈ 10⁴ × chunks ≈ 1–3 → ≤ 3×10⁴ indexed queries per index run, batch phase — within budget.
**Wall budget:** n/a (batch).
**Files:** `src/db/mod.rs` or `src/db/symbols.rs` (wherever search_symbol_in_file lives — implementer locates; 1 file + its test module).

**Verification:**
- [ ] Unit tests pass (4 stress shapes)
- [ ] Dump oracle byte-identical ×3 (helper unused yet)
- [ ] Budgets hold at 1,200-file fixture scale

---

## Slice 4: Glob-arm policy branch + map threading + disambiguation fence

**Claim:** C3 (collision disambiguates), C4 (arm-order safety: same target or decline), C5 (members not disambiguated), C7 (FirstMatch branch is the EXACT current loop).
**Oracle:** Rust dumps byte-identical; probe-WS C# dump byte-identical (no collisions there — monotone); collision fixture vs expectations.md.
**Stress fixture:** committed `tests/csharp_using_disambiguation.rs` with three DISTINCT asserts (per-claim localization):
  - C3: Widget in `My.Models` (used) + `Dupe.Ns` (unused) → resolves to My.Models' (baseline: UNRESOLVED, probed)
  - C4: unique `Gear` class inside the used namespace → resolves to the SAME symbol as the pre-change binary (anti-target-flip; per-glob-early-return bug class)
  - C5: `Assist` METHODS in two namespaces, one used → stays UNRESOLVED (kind-filter bug class)
**Loop budget:** UniqueAcrossAll per unresolved C# ref: u map lookups (u = usings ≈ 10, no DB) + ONE chunked helper call. Total ≈ refs(10⁴) × (10 + 1 call) ≈ 10⁵ ops + ≤3×10⁴ queries, batch — within budget (justified vs the naive per-file-per-using 2×10⁶ design, rejected).
**Wall budget:** n/a (batch; C12 fence at slice 8).
**Files:** `src/resolve.rs` (map built ONCE in resolve_cross_file_references when any C# file exists — O(1) namespace query per run — threaded via ctx; glob arm: `match resolver.glob_resolution().policy` where FirstMatch arm is the verbatim current loop), `tests/csharp_using_disambiguation.rs` (new).

**Verification:**
- [ ] Fence test passes (3 distinct asserts)
- [ ] Rust dumps byte-identical ×2; probe-WS C# dump byte-identical
- [ ] Existing pass2/trap/mixed-dispatch suites green
- [ ] Budgets hold

---

## Slice 5: K-hybrid namespace corroboration + cross-dir fence

**Claim:** C9 — cross-bucket C#→C# call-edge candidates are corroborated by "caller imports a namespace declared in the callee file."
**Oracle:** cross-dir fixture vs expectations.md (edge survives with count 1); Rust dumps byte-identical (Rust K-hybrid arm untouched).
**Stress fixture:** committed `tests/csharp_cross_dir_deps.rs`:
  - services/ + models/ split (the C9 baseline shape) → edge EXISTS post-change (deletion-without-corroboration bug class — proven live by baseline arithmetic)
  - Negative control: third dir `util/Helper.cs` whose class is referenced by Svc.cs WITHOUT a matching using (unique-fallback resolution, no namespace import) → cross-bucket edge ABSENT (corroboration-too-loose bug class)
  - Same-bucket pair unchanged (corroboration-too-tight bug class)
**Loop budget:** per cross-bucket C#→C# candidate edge: O(1) lookup against a prebuilt inverted map (file→namespaces, built once O(map size)) ∩ caller's imports (HashSet). Candidates ≈ 10⁴ — within budget.
**Wall budget:** n/a (batch phase).
**Files:** `src/db/call_edges.rs` (or the populate phase's Rust-side filter — implementer locates the seam-friendly spot), `tests/csharp_cross_dir_deps.rs` (new).

**Verification:**
- [ ] Fence test passes (3 distinct asserts)
- [ ] Rust dumps byte-identical; probe-WS dump byte-identical (same-bucket, unaffected)
- [ ] Budgets hold

---

## Slice 6: Delete the post-pass; L2 delta fence

**Claim:** C8 — file_deps on the ground-truth WS becomes exactly the enumerated L2 set; C10 (deletion).
**Oracle:** probe-WS file_deps section vs expectations.md's enumerated delta (App→Other GONE, GlobalUsings→Globals GONE, counts 4→3 / 2→1, Nested 1→1); `rg resolve_csharp_dependencies src/` = 0.
**Stress fixture:** committed `tests/csharp_l2_file_deps.rs` asserting each enumerated row INCLUDING the removals (absence asserts) and the count adjustments (glob-skip-removed/L1-reintroduction bug class would resurrect App→Other; partial-deletion bug class caught by the count rows).
**Loop budget:** deletion only — negative cost (removes a full re-parse of every C# file).
**Wall budget:** n/a.
**Files:** `src/indexing.rs` (delete resolve_csharp_dependencies + call site; build_namespace_map stays — now feeding resolve/K-hybrid only; transitional path→file_id shim from slice 1 dies here), `tests/csharp_l2_file_deps.rs` (new).

**Verification:**
- [ ] Fence test passes (every enumerated row, removals included)
- [ ] Rust dumps byte-identical; cross-dir fixture still matches slice-5 expectation
- [ ] Post-pass grep = 0
- [ ] Budgets hold (vacuous)

---

## Slice 7: Permanent lint fence + stale-reference sweep

**Claim:** C10's permanent form; docs truthful post-deletion.
**Oracle:** lint test; TDD-inversion (pre-slice-6 tree contains the symbol name — verified via git show).
**Stress fixture:** the lint must FAIL against the pre-slice-6 tree (`git show <slice5-commit>:src/indexing.rs` contains `resolve_csharp_dependencies`).
**Loop budget:** test-only file read.
**Wall budget:** n/a.
**Files:** `tests/seam_lint.rs` (add: indexing.rs contains no `resolve_csharp_dependencies`; CSharpModuleResolver doc no longer says "declining stub" — sweep), `src/languages/module_resolver.rs` (doc updates: stub language removed, jwf9 reference becomes "implemented"; mod.rs checklist already language-neutral).

**Verification:**
- [ ] Lint passes on post-change tree; demonstrably fails on pre-slice-6 content
- [ ] No stale "stub"/"jwf9 pending" docs remain (`rg -i 'declining stub|jwf9' src/` reviewed)
- [ ] Dump oracle byte-identical (docs/tests only)

---

## Slice 8: Acceptance ceremony (no code)

**Claim:** C6 (monotone-stable, final), C7 (Rust strict, final), C12 (wall-time, manual fence — approved).
**Oracle:** full dump runs ×4 workspaces vs baselines/expectations; pre/post natural-key join on probe-WS refs (zero target changes/losses); `cargo test` + clippy; timing: fresh cold-target builds BOTH sides, frozen input, interleaved, median of ≥5 (per tethys-self-index-oracle-freezing memory — the incremental-target artifact from last loop must not recur).
**Stress fixture:** n/a — ceremony; adversarial content lives in slices 1–7.
**Loop budget:** n/a.
**Wall budget:** C12: ≤ baseline +10% (improvement expected).
**Files:** none (artifact: `.csharp-ns/acceptance.md`; close tethys-jwf9 + tethys-nmsp in the tracker with commit references).

**Verification:**
- [ ] All dumps match baselines/expectations exactly
- [ ] Monotone join: 0 changed/lost C# targets
- [ ] Suite green, clippy --all-targets 0
- [ ] C12 recorded (manual fence, approved)
- [ ] acceptance.md written; jwf9 + nmsp closed

---

## Plan Self-Review

1. **Loops:** S1 map build O(n log n) under existing cap; S2 O(1) lookups; S3 chunked-IN queries ≤3×10⁴/run with the 500-chunk bound stated; S4 rejected the naive 2×10⁶-query shape in writing, adopted one-helper-call-per-ref ≈10⁵ ops; S5 O(1) per candidate with prebuilt inverted map; S6 negative; no always-on phases. **No gaps.**
2. **Fixtures:** every slice names its bug class — sort-determinism + nested-exclusion (S1), None-map no-panic + kind-list exactness (S2), kind filter + empty-IN + host-param limit (S3), target-flip + missing-kind-filter + per-glob-early-return (S4), too-loose + too-tight corroboration (S5), L1-reintroduction + partial-deletion via absence/count asserts (S6), TDD-inversion (S7). **No happy-path-only fixtures.**
3. **Doc-comment preconditions:** `ctx.namespaces = None` → documented refusal (empty vec), enforced by Option construction (load-bearing, runtime by shape); helper's empty `file_ids` → early-return None (load-bearing, runtime); chunking invariant (uniqueness aggregated ACROSS chunks) documented + unit-tested. **No unenforced contracts.**
4. **Write targets:** no new prints; tracing (stderr) only; dump scripts stdout=data. **No gaps.**
5. **Tracker references:** tethys-nnst (verified — filed at design), tethys-usgf (verified), tethys-jwf9/nmsp (closed at S8 with references), tethys-dsp1/8mze (context). **No phantom IDs.**

Claim coverage vs design: C1→S1, C2→design-stage (closed), C3/C4/C5→S4, C6→S8, C7→S4+every-slice oracle+S8, C8→S6, C9→S5, C10→S6/S7, C11→S2+standing lint, C12→S8. **All 12 covered.**
