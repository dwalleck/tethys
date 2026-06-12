# Design: C# `using static` static-method-call disambiguation (usgf)

Spec: `.usgf/spec.md` (rev 2, signed). Probe: `.usgf/probe-findings.md`.
Extends the jwf9 C# glob arm; contradicts nothing the probe established.

## Purpose

A bare method call whose name collides across types resolves to the
static-imported type's method. Adds a **static-member arm** alongside the
shipped types-only arm inside `GlobPolicy::UniqueAcrossAll`; candidates from
both arms union into one unique-or-decline (spec decision #3). No schema
change — the static using is recognized by type-detection.

## Architecture

**Type-detection (resolver, DB-free).** A `using static Ns.Type;` stores as
glob `source_module = "Ns.Type"`, which misses the namespace map (keyed
`Ns`). New trait method:
```rust
struct StaticMemberImport { type_name: String, files: Vec<PathBuf> }
fn static_member_import(&self, source_module: &str, ctx: &ModuleContext) -> Option<StaticMemberImport>;
```
Default (Rust): `None`. C#: split `source_module` on the last `.`; if the
prefix is a key in `ctx.namespaces`, emit `{ type_name: last_segment,
files: namespaces[prefix] }`. Pure string + map lookup — no DB, C10 holds.

**Member lookup (driver owns DB).** New helper:
```rust
fn search_symbols_by_name_in_files(&self, name, kinds, files, limit) -> Vec<Symbol>;  // un-collapsed
fn search_type_members_by_name(&self, name, type_name, files, member_kinds, limit) -> Vec<Symbol>;
```
The second matches `name = ? AND qualified_name LIKE 'Type::%' AND kind IN
(member_kinds) AND file_id IN (files)` — the `Type::` prefix scopes to the
static-imported type (handle is qualified_name, NOT parent_symbol_id, which
is None for functions; probe Q2). The existing
`search_unique_symbol_by_name_in_files` is refactored to delegate to the
un-collapsed version (behavior-preserving; dump oracle confirms).

**GlobResolution** gains `member_kinds: Option<&'static [SymbolKind]>` —
Rust `None`, C# `Some([Function, Method])`.

**Driver (`resolve.rs`, UniqueAcrossAll, !is_qualified):**
```
candidates: Vec<Symbol> = []
for su in glob_imports:
    candidates += db.search_symbols_by_name_in_files(name, glob.kinds, resolver.resolve_import_files(su, ctx))   // types arm
    if let Some(smi) = resolver.static_member_import(su, ctx):
        candidates += db.search_type_members_by_name(name, smi.type_name, smi.files, glob.member_kinds)            // static arm
dedupe candidates by symbol id
if candidates.len() == 1 { resolve } else { decline }
```
FirstMatch branch (Rust) is untouched → Rust byte-identical structurally.

## Input shapes

- **source_module**: plain namespace (in map → types arm) | `Ns.Type` with
  `Ns` in map (→ static arm) | `Ns.Sub` where both `Ns` and `Ns.Sub` are
  namespaces (both arms fire; static arm's `Sub::%` finds no methods → no
  harm) | external (prefix not in map → neither) | single-segment, no `.`
  (no static arm) | empty.
- **ref name**: workspace-unique (resolves via fallback today — monotone) |
  colliding across types only (jwf9 case) | colliding across a static-method
  and another method (THE new win) | colliding type-vs-method across arms
  (cross-arm → decline) | a method of a DIFFERENT type in the same namespace
  files (prefix-scoping must exclude).
- **static usings per file**: 0 (arm contributes nothing) | 1 | N | duplicate.
- **language**: C# (enters the arm) | Rust (FirstMatch — never enters).

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|---|---|---|---|---|---|
| C1 | Type-detection: `Ns.Type` with `Ns` in the map yields `(Type, Ns-files)`; plain `Ns` and external prefixes yield None | probe DB: `My.Models` is a module symbol, `Helper::Assist` exists in its file, source_module=`My.Models.Helper` | sqlite3 vs probe data | 5m | **PASSED** (probe + falsifier run) | `module_resolver` unit test on `static_member_import` |
| C2 | A colliding bare method name with `using static Ns.Type;` resolves to Type's method | fixture: two `Assist` (Helper + Other), `using static Ns.Helper` → resolves to Helper::Assist (baseline: UNRESOLVED) | SQL on refs vs ground truth | 15m | pending | `tests/csharp_using_static.rs::disambiguates` |
| C3 | Cross-arm collision (type `Foo` via namespace using + method `Foo` via static using) declines | fixture with both → ref stays UNRESOLVED | SQL on refs | 10m | pending | same test, distinct assert |
| C4 | Prefix-scoping: a method of a DIFFERENT type in the same namespace files is NOT matched | fixture: `Ns.Helper` + `Ns.Other`, both have `Zap`; `using static Ns.Helper` + bare `Zap` → Helper::Zap, never Other::Zap | SQL on refs | 10m | pending | same test, distinct assert |
| C5 | External static using (prefix not a namespace) contributes no candidate | `using static System.Math;` + bare `Sqrt` → no static-arm resolution | SQL on refs | 5m | pending | same test, distinct assert |
| C6 | Existing C# resolutions monotone-stable (csharp-gt incl. Assist-via-fallback→same symbol, xdir) byte-identical | pre/post dumps on csharp-gt + xdir | dump.sh + diff | 10m | pending | jwf9 fixtures + new dump checks |
| C7 | Rust byte-identical (FirstMatch branch untouched, types-arm refactor behavior-preserving) | frozen self-index + c6trap dumps pre/post | dump.sh + diff | 15m | pending | existing pass2/resolver/trap suites |
| C8 | Seam stays DB-free: type-detection is string+map, no DB in resolver | `rg 'use crate::db|&Index' module_resolver.rs` = 0 | rg / existing seam_lint | 1m | pending | existing `tests/seam_lint.rs` (unchanged) |
| C9 | Indexing wall-time ≤ baseline +10% | fresh-built binaries both sides, frozen input, median ≥5 | wall clock | 20m | pending | **manual** (criterion bench; separator-fix C9 / csharp-ns C12 precedent) — needs user approval |

Named buggy implementations (non-vacuity): C1 — split on FIRST `.` instead
of last (`My`/`Models.Helper`); C2 — static arm never added to candidates;
C3 — separate Option-returning calls per arm picking one instead of unioning
(misses cross-arm collision); C4 — member lookup omits the `qualified_name
LIKE 'Type::%'` clause (matches Other::Zap too); C5 — type-detection emits a
candidate even when the prefix isn't in the map (external over-fires); C6 —
types-arm refactor drops a candidate or changes ordering (csharp-gt Assist
flips target or unresolves); C7 — member_kinds leak into the Rust path; C8 —
ctx grows a DB handle; C9 — per-ref full-table scan in the member lookup.

### B3/B6 reconciliation (recorded, not a contradiction)

B6's success criterion is "measured by pre/post dump join **on the existing
fixtures**" — csharp-gt and xdir contain no type-vs-method cross-arm
collision, so they stay byte-identical (C6). B3's cross-arm decline is a NEW
behavior on a NEW fixture (C3). A name that resolved via the types arm alone
today AND gains a colliding static-method candidate WOULD newly decline —
but that is a genuine type-vs-method ambiguity (C# would disambiguate by
call-vs-type syntax, which tethys does not track), correct to decline under
decision #3's union rule, and absent from the existing corpus. No signed
criterion is violated; the union semantics are honored.

## Negative space

This design deliberately does NOT:
1. Resolve non-method members — const/static-field/enum (tethys-cfme: not indexed).
2. Add a schema column or `is_static` storage flag (type-detection suffices).
3. Resolve alias usings (tethys-alus) or global usings (tethys-glus).
4. Enforce C#'s static-only method rule (instance methods may match).
5. Touch the Rust resolution path (FirstMatch branch unchanged).
6. Handle nested-block-namespaced static-imported types (tethys-nnst).

## Approval

Design approved by requester 2026-06-07. C9's manual regression fence
explicitly approved 2026-06-07 (consistent with separator-fix C9 /
csharp-ns C12 precedent). The B3/B6 cross-arm-decline reconciliation
stands as documented.

## Tracker references

tethys-cfme, tethys-alus, tethys-glus, tethys-nnst (all verified open),
tethys-jwf9 (closed — the arm this extends). No new deferrals.
