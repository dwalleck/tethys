# Plan: C# `using static` static-method-call disambiguation (usgf)

Design: `.usgf/design.md` (approved; C1 cheapest falsifier PASSED). Standing
per-slice oracle (reusing `.separator-fix/dump.sh`, `.csharp-ns/fixtures.sh`,
the frozen-worktree procedure — memory `tethys-self-index-oracle-freezing`):
after EVERY slice — `cargo test` green, `cargo clippy --all-targets` 0;
Rust dumps byte-identical (frozen self-index @ slice-0 + c6trap); C# dumps
byte-identical (csharp-gt incl. bare `Assist`→Helper::Assist, xdir) until
the union lands, then matching the slice's expectation. `cargo test`, not
nextest (not installed). Conventional-commit subjects (CI gate).

---

## Slice 0: Baselines (no code)

**Claim:** operationalizes C6/C7 oracles; baselines pre-date code.
**Oracle:** dump determinism ×2 from-scratch per workspace.
**Stress fixture:** the four workspaces — frozen self-index worktree @ the
slice-0 commit (baseline binary built FROM the frozen tree), csharp-gt
(`.csharp-ns/fixtures.sh csharp-gt`), xdir (`xdir`), c6trap
(`.separator-fix/fixtures.sh c6trap`).
**Loop budget:** n/a.
**Wall budget:** n/a.
**Files:** none (artifacts: `.usgf/baselines/*.dump`, timing baseline).

Note: csharp-gt's bare `Assist()` resolves to Helper::Assist via the
fallback TODAY (only one Assist in that workspace). After the union arm it
must resolve to the SAME symbol via the static arm — the baseline captures
that target so monotone stability is checked against it.

**Verification:**
- [ ] Four baselines deterministic ×2
- [ ] Timing baseline (median of ≥5) recorded
- [ ] Working tree clean

---

## Slice 1: StaticMemberImport + type-detection + member_kinds

**Claim:** C1 — type-detection splits `Ns.Type` → `(Type, Ns-files)` iff `Ns`
is a namespace in the map; plain/external/single-segment yield None.
**Oracle:** unit tests vs hand-constructed `NamespaceMap`; dump oracle ×4
(dead code — nothing calls it yet).
**Stress fixture (expected outputs pre-written):**
  - `static_member_import("My.Models.Helper", ctx{My.Models→[a.cs]})` →
    `Some{type_name:"Helper", files:[a.cs]}` (split on LAST `.`; a split-on-
    first bug yields type `Models.Helper` / prefix `My` → None)
  - `static_member_import("My.Models", ...)` → None (plain namespace, no type suffix... but `My` may be a namespace? see below)
  - `static_member_import("System.Math", ctx{no System})` → None (external prefix)
  - `static_member_import("Foo", ...)` → None (single segment, no `.`)
  - `static_member_import("My.Models.Sub", ctx{My.Models AND My.Models.Sub both namespaces})` → `Some{type_name:"Sub", files:[My.Models-files]}` (over-fires harmlessly — the later `Sub::%` member query finds nothing); documents the accepted over-fire
  - Rust resolver `static_member_import(...)` → None always
  - `glob_resolution().member_kinds`: Rust None, C# `Some([Function,Method])`
**Loop budget:** per import: one `rsplit_once('.')` + one map lookup = O(1);
called once per glob import per ref → folded into the existing glob loop, no
new asymptotic cost.
**Wall budget:** n/a (batch).
**Files:** `src/languages/module_resolver.rs` (struct + trait default + C#
impl + `member_kinds` field + the 3 existing `GlobResolution` literals get
the field — Rust/C#/any test literal).

**Doc-comment contract:** `static_member_import` "prefix must be a namespace
in ctx.namespaces" — sanity hint, enforced by the `ctx.namespaces.get()?`
early return (None is the documented refusal; no wrong output possible).

**Verification:**
- [ ] Unit tests pass (7 shapes)
- [ ] Dump oracle byte-identical ×4
- [ ] Budget holds

---

## Slice 2: DB helpers — un-collapsed search + type-member lookup

**Claim:** C4 primitive (prefix-scoped member lookup) + the behavior-
preserving refactor enabling the union.
**Oracle:** unit tests against a hand-built DB (rusqlite direct inserts),
independent of the resolver.
**Stress fixture (expected outputs pre-written):**
  - `search_type_members_by_name("Zap", "Helper", [a.cs], [Function,Method])`
    where a.cs has `Helper::Zap` (function) AND `Other::Zap` (function) →
    returns ONLY Helper::Zap (the `qualified_name LIKE 'Helper::%'` clause;
    omitting it returns both — the prefix-scoping bug class)
  - same with `Helper::Zap` a `method` kind → matched; a `class` named Zap → NOT matched (kind filter)
  - empty `files` → empty Vec, no SQL error (empty-IN refusal, runtime early return — load-bearing)
  - 1,200 files straddling the 500 chunk boundary with the one match in a late chunk → found; a second match in another chunk → both returned (cap 2, cross-chunk)
  - `search_symbols_by_name_in_files` (un-collapsed) returns all matches up to limit; the refactored `search_unique_symbol_by_name_in_files` returns Some iff exactly one (delegation behavior-identical)
**Loop budget:** `search_type_members_by_name`: O(files/500) indexed queries
per call; one call per (ref × static-using); refs ≈ 10⁴ × static-usings ≈
few → ≤ ~10⁴ indexed queries, batch — within budget. `LIKE 'Type::%'` is a
prefix match (index-friendly on qualified_name).
**Wall budget:** n/a (batch).
**Files:** `src/db/symbols.rs` (+ its test module).

**Doc-comment contract:** `search_type_members_by_name` "empty files → None"
— load-bearing (empty `IN ()` is a SQL syntax error); runtime early return,
unit-tested. `type_name` non-empty assumed — sanity hint (caller is
type-detection which never emits empty); a `LIKE '::%'` on empty type would
over-match, so guard with a debug_assert AND skip the query when empty
(runtime, since over-match is wrong output).

**Verification:**
- [ ] Unit tests pass (5 shapes)
- [ ] Dump oracle byte-identical ×4 (helpers unused yet)
- [ ] Budget holds at 1,200-file fixture scale

---

## Slice 3: resolve.rs glob-arm union + disambiguation fence

**Claim:** C2 (disambiguates), C3 (cross-arm declines), C4 (prefix-scoping),
C5 (external declines), C6 (existing C# monotone), C7 (Rust byte-identical).
**Oracle:** Rust dumps byte-identical; csharp-gt + xdir byte-identical
(monotone — Assist stays Helper::Assist, now via the static arm); new
fixture vs expectations.
**Stress fixture:** committed `tests/csharp_using_static.rs`, four DISTINCT
asserts (per-claim localization):
  - C2: two `Assist` (Helper + Other), `using static Ns.Helper` → resolves
    to Helper::Assist (baseline: UNRESOLVED — probed)
  - C3: type `Foo` via `using Ns;` + method `Foo` via `using static O.Util;`
    → bare `Foo` stays UNRESOLVED (cross-arm union collision)
  - C4: `Helper::Zap` + `Other::Zap` both in Ns, `using static Ns.Helper`,
    bare `Zap` → Helper::Zap, never Other::Zap (prefix-scoping)
  - C5: `using static System.Math;` + bare `Sqrt` → UNRESOLVED (external)
**Loop budget:** union: per ref, `g` glob imports × (1 types lookup + ≤1
static lookup) candidate queries, then dedup over ≤ a handful of candidates;
g ≈ 10 → ~10²–10³ ops per ref, batch — within budget. No change to the
naive-vs-smart tradeoff already accepted in jwf9.
**Wall budget:** n/a (batch; C9 at slice 4).
**Files:** `src/resolve.rs` (UniqueAcrossAll arm → union; FirstMatch branch
untouched), `tests/csharp_using_static.rs` (new).

**Symmetry audit:** the static arm parallels the types arm — both contribute
candidates to one set, both decline on ambiguity, neither logs differently;
the union's dedup-by-id is the new shared collapse point. Documented in the
arm comment.

**Verification:**
- [ ] Fence test passes (4 distinct asserts)
- [ ] Rust dumps byte-identical ×2; csharp-gt + xdir byte-identical
- [ ] Existing pass2/trap/mixed-dispatch/disambiguation suites green
- [ ] Budget holds

---

## Slice 4: Acceptance ceremony (no code)

**Claim:** C6/C7 (final), C8 (DB-free seam), C9 (wall-time).
**Oracle:** full dumps ×4 vs baselines; monotone join on csharp-gt+xdir
(0 target changes/losses); `rg 'use crate::db|&Index' module_resolver.rs`=0;
seam_lint suite; `cargo test` + clippy; timing fresh-built both sides,
frozen input, median ≥5 (per the freezing memory — fresh cold-target builds,
not the incremental dir).
**Stress fixture:** n/a — ceremony; adversarial content in slices 1–3.
**Loop budget:** n/a.
**Wall budget:** C9 ≤ baseline +10%.
**Files:** none (artifact: `.usgf/acceptance.md`; close tethys-usgf with
the static-method scope noted, leave tethys-cfme/alus/glus open).

**Verification:**
- [ ] All dumps match baselines/expectations
- [ ] Monotone join: 0 changed/lost C# targets
- [ ] Suite green, clippy 0, seam_lint unchanged
- [ ] C9 recorded (manual fence)
- [ ] acceptance.md written; usgf closed (scope note: static methods only)

---

## Plan Self-Review

1. **Loops:** S1 type-detection O(1)/import folded into the existing glob
   loop; S2 chunked member query ≤10⁴ indexed/run with the 500 bound stated,
   LIKE-prefix is index-friendly; S3 union ~10²–10³ ops/ref; no always-on
   phases. **No gaps.**
2. **Fixtures:** S1 split-on-last vs split-on-first, external-prefix-none,
   sub-namespace-over-fire (bug classes named); S2 prefix-scoping
   (Helper::Zap vs Other::Zap), kind filter, empty-IN, cross-chunk; S3 the
   four claim asserts incl. cross-arm collision + external. Every fixture
   names its bug class; none happy-path-only. **No gaps.**
3. **Doc-comment preconditions:** `static_member_import` prefix-in-map →
   sanity hint, `get()?` refusal; `search_type_members_by_name` empty files
   → load-bearing runtime early return; empty type_name → load-bearing
   (over-match) runtime skip + debug_assert. All enforced. **No gaps.**
4. **Write targets:** no new stdout/stderr; tracing (stderr) only; dump.sh
   stdout=data. **No gaps.**
5. **Tracker references:** tethys-cfme/alus/glus/nnst (verified open),
   tethys-jwf9 (closed — extended). No deferral lacks an ID. **No gaps.**

Claim coverage vs design: C1→S1, C2→S3, C3→S3, C4→S2+S3, C5→S3, C6→S3+S4,
C7→S3+S4, C8→S4 (standing lint), C9→S4. **All 9 covered.**
