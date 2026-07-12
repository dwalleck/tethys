# tethys-3i35 — falsifiable design: resolve bare-`crate` qualified paths to the crate-root file

## Purpose

A qualified reference through the crate root (`crate::helper()`,
`crate::Thing::make()`) and a crate-root import (`use crate::Foo;`) both die
in the same hole: `resolve_module_path(["crate"], …)` →
`resolve_crate_path(&[], crate_root)` returns the `src/` **directory**
(src/resolver.rs:88-91), which has no `files` row, so every consumer's
file-id lookup silently fails. The probe (`.tethys-3i35/findings.md`)
confirmed the mechanism and bounded the blast radius.

## The rule (one sentence)

A module path that is exactly `["crate"]` resolves to the referencing file's
own crate-root **file**, chosen compiler-faithfully; every other path shape
is untouched.

### Crate-root file choice (pinned by rustc, cheapest falsifier — ran, passed)

Given `crate = get_crate_for_file(current_file, workspace_crates)`:

| current_file situation                     | `crate` maps to                       |
|--------------------------------------------|---------------------------------------|
| is one of the crate's bin roots            | `current_file` itself (bin crate)     |
| under `src/bin/` but not a bin root        | decline (`None`) — bin-module ambiguity |
| any other file, crate has a lib target     | `lib_path` (joined, `.filter(exists)`) |
| any other file, no lib, exactly one bin    | that bin root (single-target crate)   |
| any other file, no lib, 0 or ≥2 bins       | decline (`None`)                      |
| `get_crate_for_file` → `None` (foreign file) | decline (`None`)                    |

rustc evidence (recorded 2026-07-12): in a bin+lib crate, `crate::x()` in
`main.rs` with `x` in `main.rs` **compiles**; `crate::y()` in `main.rs` with
`y` only in `lib.rs` fails with E0425 "cannot find function `y` in the crate
root". A lib-preferred-always rule would fabricate resolutions rustc
rejects; declining on ambiguity follows the error posture (suppressions,
not accusations — conservative non-resolution over wrong resolution).

### Fix location

`resolve_module_path`'s `"crate"` arm (src/resolver.rs:42) gains a
`path.len() == 1` early return, mirroring the single-segment
workspace-crate arm (src/resolver.rs:57-58). Both `current_file` and
`workspace_crates` are already parameters — no plumbing. All three
consumers inherit the fix through the one seam:

1. `qualified_splits` as-written arm → `qualified_module_fallback`
   (this ticket's symptom);
2. `resolve_import_segments` → unused-imports `classify_confidence`
   (tethys-xzdr's symptom — same root cause, fixed by the same line;
   xzdr's ACs become fences here and xzdr closes with this PR, pending
   approval at the design pause);
3. `resolve_import_segments` → dependency computation (the repro's
   "Unresolved dependencies: 1" stat drops to 0).

`resolve_crate_path`'s empty-path branch becomes unreachable from
production paths; it is removed (or made `unreachable`-documented) rather
than left as a trap — plan decides the exact shape against its unit tests.

## Input shapes

- **Path shapes**: `["crate"]` (the fix); `["crate", …rest]` (unchanged);
  `["self"]` / `["super", …]` (untouched arms); `[workspace-crate, …]`
  (untouched); external prefix (untouched).
- **Ref tails at crate root**: single-segment (`crate::x()`), two-segment
  method (`crate::Type::method()`), submodule tail (`crate::mod::x()` —
  must NOT bind to the root file), bare call + `use crate::x` (control).
- **CrateInfo shapes**: lib-only; single-bin-only; lib+bin (ref in
  `main.rs` / in a lib-owned module / under `src/bin/` non-root);
  no-entry-point (neither lib.rs nor main.rs); foreign file (no crate).
- **Import shapes**: `use crate::Foo;` (explicit), `use crate::*;` (glob),
  `use crate::{a, b}` (group → two explicit imports, same mechanism).
- **Languages**: Rust only. C# out of scope — `CSharpModuleResolver` has
  its own namespace mechanism and no `crate` token.

## Claims

1. **C1 (single-segment tail).** In the probe's two-crate repro,
   `crate::helper()` in `crate_a/src/b.rs` resolves to crate_a lib.rs's
   `helper` (strategy `qualified_module_fallback`, `reference_name` NULL);
   crate_b's same-named decoy gains no ref.
2. **C2 (method tail).** `crate::Thing::make()` resolves to crate_a's
   `Thing::make` method symbol (methods store `parent::name` qualified
   names, so the entry-point lookup finds two-segment tails).
3. **C3 (monotonicity — amended at plan time).** On the tethys self-index,
   post-fix: (a) every pre-fix resolved ref keeps its **symbol_id**; (b) no
   pre-fix resolved ref becomes unresolved; (c) strategy changes are
   permitted only as `same_crate`/`unique_workspace` → `explicit_import`
   for names imported via a bare-crate `use crate::X;` (band medium→high
   upgrade: the explicit-import path previously FAILED on source module
   `crate` and fell through to fallbacks — post-fix it succeeds earlier);
   (d) the 13 submodule-tail `crate::` refs remain unresolved (tethys-qtq5
   territory). Any symbol_id change = STOP and investigate (drift rule).
   *Amendment rationale (2026-07-12): the original "identical strategy"
   form was too strict — planning surfaced that `use crate::X;`-imported
   bare refs legitimately upgrade strategy when the import resolves.*
4. **C4 (CLI AC).** `tethys callers helper` on the repro lists the
   `crate::helper()` call site in b.rs.
5. **C5 (crate-root-choice matrix).** Each row of the table above holds:
   (a) `crate::x()` in `main.rs`, `x` in main.rs → resolves to main.rs's
   `x`; (b) `crate::y()` in `main.rs`, `y` only in lib.rs → stays
   unresolved; (c) `crate::y()` in a lib-owned module of a bin+lib crate →
   lib.rs's `y`; (d) `crate::x()` in `src/sub.rs` of a single-bin crate →
   main.rs's `x`; (e) `crate::x()` in `src/bin/tool/helper.rs` (non-root
   bin module) → stays unresolved.
6. **C6 (degenerate crates).** A crate with no entry point on disk, and a
   file belonging to no known crate, both decline: ref stays unresolved,
   indexing completes without error.
7. **C7 (explicit import side / xzdr).** `use crate::Foo;` for an unused
   crate-root `pub struct Foo` is reported **Definite** by unused-imports
   (was MaybeTrait), and the repro's unresolved-dependencies stat drops to 0.
8. **C8 (glob import side).** `use crate::*;` in a submodule lets a bare
   call to a crate-root fn resolve via the glob arm (FirstMatch now reaches
   a real file).
9. **C9 (refs_named tripwire).** tests/refs_named.rs flips exactly as its
   TRIPWIRE comment predicts: `name='helper'` call count 3→4,
   `name='crate::helper'` 1→0.
10. **C10 (idempotency).** Indexing the repro twice produces identical
    refs and file_deps content (no ref_count double-bump from the newly
    resolving import).
11. **C11 (self-index oracle).** unused-imports Definite-tier findings on
    tethys itself remain zero post-fix (tethys builds warning-free, so any
    Definite finding is a false positive by construction).
12. **C12 (goldens).** idxperf golden dumps are byte-identical pre/post
    (their fixtures contain no crate-root-qualified shapes — verified by
    grep during design).

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| 5-sem | crate-root-choice semantics | bin+lib fixture: `crate::x()` / `crate::y()` in main.rs; compile | **rustc** (E0425) | 2m | **passed** (pre-design, 2026-07-12) | C5 fixture test below |
| 1 | C1 | index repro; SQL: crate::helper ref row | rustc (repro compiles; `crate` cannot cross crates) + probe simulation | 5m post-impl | pending | new test `pass2_crate_root_paths.rs::single_segment_tail_resolves_with_decoy` |
| 2 | C2 | same run, distinct SQL row for Thing::make | rustc + probe simulation | 5m | pending | same file, `method_tail_resolves` |
| 3 | C3 | dump resolved refs (file,line,col,symbol,strategy) with pre-fix binary; re-dump post-fix; diff | pre-fix binary output (independent of new code) | 15m | pending (one-shot) | `root_decoy_does_not_shadow_submodule_tail`: fixture with `fn f` in BOTH lib.rs and inner.rs, `crate::inner::f()` must bind inner.rs's f (buggy short-split precedence would bind root) |
| 4 | C4 | run `tethys callers helper` on repro | source text (call visibly at b.rs:4) | 2m | pending | same test file, callers assert (via CLI or call_edges SQL) |
| 5 | C5 matrix | one fixture crate per row; SQL per row | rustc row-by-row (each fixture arranged to compile, or E0425-checked during design) | 20m | pending | `crate_root_choice_matrix` test, one assert per row (a)-(e) |
| 6 | C6 | no-entry-point fixture + foreign file | process exit + SQL NULL | 10m | pending | `degenerate_crates_decline` |
| 7 | C7 | xzdr repro; `unused-imports --json`; stats output | **cargo check** emits unused-import warning (compiler = ground truth Definite); xzdr's independent root-cause analysis | 10m | pending | unused-imports integration test per xzdr AC |
| 8 | C8 | glob fixture; SQL for the bare call ref | rustc (glob import makes the call compile) | 10m | pending | `glob_from_crate_root_resolves` |
| 9 | C9 | run refs_named tests post-fix: OLD asserts must fail, updated asserts pass | the test's own fixture text (3 bare + 1 qualified call, countable by eye) | 5m | pending | updated `refs_named.rs` asserts |
| 10 | C10 | index repro twice; diff SQL dumps | mechanical diff | 5m | pending | extend `file_deps_idempotency.rs` fixture with a `use crate::X;` shape if absent |
| 11 | C11 | run `tethys unused-imports` on tethys post-fix | rustc/clippy warning-free CI build | 5m | pending | existing self-index CI fence (verify name at plan time; add if absent) |
| 12 | C12 | `cargo nextest run idxperf` | golden dumps (pre-recorded, independent) | 2m | pending | existing `idxperf_golden.rs` |

Non-vacuity (buggy implementation each fence catches): C1 — workspace-first
crate mapping binds the crate_b decoy; C2 — entry lookup tries only
single-segment tails; C3-fence — `["crate"]` split ordered before longer
splits, root `f` shadows `inner::f`; C4 — resolution lands but call_edges
skips fallback-strategy refs; C5a/b — lib-preferred-always fabricates
`crate::y`→lib.rs from main.rs; C5e — treating every non-root file as
lib-owned; C6 — `.unwrap()` on `entry_point_file()`; C7 — fix scoped to
`qualified_splits` only, import side still dies; C8 — fix scoped to
explicit imports, glob path still gets the dir; C9 — refs_named view keys
regressing; C10 — import-derived file_dep UPSERT bumping ref_count on
reindex; C11 — confidence upgrade misfiring on *used* crate imports;
C12 — any accidental behavior drift in shapes goldens do cover.

Distinctness: every claim has its own SQL row / test assert / CLI output;
no two claims share a single yes-no oracle.

## Negative space (what this fix does NOT do)

1. Does NOT chase re-exports when the tail misses in the claimed file —
   the 13 self-index `crate::db::*` / `crate::types::*` refs stay
   unresolved; that is **tethys-qtq5** (filed from this probe).
2. Does NOT touch `self`/`super` semantics (tethys-nkjd unchanged).
3. Does NOT surface value-position scoped identifiers (tethys-i09d) or
   macro-interior refs (tethys-0nar, tethys-9l27) — those refs never reach
   the resolver.
4. Does NOT change method-call receiver resolution (tethys-53iv).
5. Does NOT touch C# resolution (no `crate` concept; separate resolver).
6. Does NOT add a resolution strategy or confidence band — resolutions land
   under the existing `qualified_module_fallback` strategy, so the
   tethys-9z7i provenance surface is unchanged.
7. Does NOT model full per-target module-tree membership. The
   crate-root-choice table is a compiler-faithful approximation that
   *declines* where it cannot know (src/bin submodules, multi-bin-no-lib);
   files reachable from BOTH lib and bin trees anchor to lib, matching the
   dominant layout.

## Decisions (design pause, 2026-07-12 — user-approved)

1. **Close tethys-xzdr in this PR: YES.** C7's fences satisfy xzdr's ACs;
   xzdr closes at close-out citing this PR.
2. **Crate-root-choice decline rows: ACCEPTED** (conservative posture).
3. **`resolve_crate_path` empty-path branch: REMOVE** (unreachable after
   the fix; a dir-returning branch is a trap for future callers).
