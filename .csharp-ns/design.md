# Design: C# namespace resolution + post-pass absorption (csharp-ns)

Spec: `.csharp-ns/spec.md` (rev 2, signed). Probe basis:
`.csharp-ns/probe-findings.md` + two design-time falsifier runs (below).
This design extends the probe and contradicts none of it.

## Purpose

One mechanism — a namespace→files map — serves two consumers: (1) Pass-2's
glob-import arm disambiguates colliding C# type names through the file's
using-directives; (2) the file-dep pipeline replaces the deleted
`resolve_csharp_dependencies` post-pass under L2 used-only semantics, with
K-hybrid corroboration extended so cross-directory C# deps survive.

## Design-time falsifier results (already run)

- **Parent-chain reconstruction for nested namespaces: FAILED.** No module
  symbol has `parent_symbol_id` (SQL on probe DB). Nested blocks are
  descoped — the post-pass had the identical gap (map keyed on un-dotted
  symbol names), so behavior is unchanged. Tracked: **tethys-nnst**.
- **Cross-directory baseline: ESTABLISHED.** `services/Svc.cs →
  models/Widget.cs (1)` exists today; ref_count arithmetic (single-source
  count 1 vs dual-source count 2 elsewhere) proves it is post-pass-sourced
  and that K-hybrid drops the call-edge candidate across `orphan:` buckets.
  Deletion without corroboration work loses these edges — claim C9.

## Architecture

**Namespace map.** `Tethys::build_namespace_map` (existing, kept) lifted to
produce `HashMap<String, Vec<PathBuf>>` (workspace-relative paths) from
Module-kind symbols in C# files — flat keys only (`My.Models`, `My.Scoped`);
nested blocks excluded (tethys-nnst). Built once per resolve/index run by
the driver, never by the resolver (DB-free fence C10 unchanged).

**ModuleContext** gains `namespaces: Option<&'a HashMap<String, Vec<PathBuf>>>`
(`None` for Rust contexts — the Rust resolver never reads it).

**Trait additions** (both provided-default, Rust untouched):
```rust
/// All files defining the imported module/namespace. Default: 0-or-1 via
/// resolve_import. C# overrides via ctx.namespaces (flat-key lookup).
fn resolve_import_files(&self, source_module: &str, ctx: &ModuleContext<'_>) -> Vec<PathBuf>;

/// How the driver's glob-import arm consumes candidates.
/// Default (Rust): FirstMatch, kinds: None — the driver's existing loop,
/// byte-identical. C#: UniqueAcrossAll, kinds: Some([Class, Struct,
/// Interface, Enum]) (no Record SymbolKind exists; records out of scope).
fn glob_resolution(&self) -> GlobResolution;
```

**resolve.rs glob arm** branches on policy: `FirstMatch` = the EXACT current
loop (early return per glob, no kind filter); `UniqueAcrossAll` = collect
candidate symbols across ALL the file's glob imports (per namespace:
`resolve_import_files` → per file: name + kind-filtered symbol lookup),
dedupe by symbol id, resolve iff exactly one candidate (decisions #3/#4).

**File deps.** `resolve_csharp_dependencies` + its call site deleted;
`build_namespace_map` retained for the map. `compute_dependencies`' glob
skip stays (C# namespace deps are NOT minted at Pass 1). Instead, the
existing call-edge→file_deps phase carries C# (probe: construct refs make
call edges — no kind filter in populate_call_edges), with K-hybrid
corroboration extended: a cross-bucket C#→C# edge is corroborated when the
caller file has a stored import for a namespace declared in the callee file
(namespace-map lookup) — the using IS the import corroboration, exactly
parallel to Rust's crate-prefix check.

## Input shapes

- **Language**: Rust (defaults, byte-identical) | C# (new semantics).
- **Collision count for a simple name**: 0 candidates (declines) | 1
  (resolves; must equal today's fallback target — arm-order trap) | ≥2 in
  one used namespace (invalid C#, unique rule declines) | ≥2 across used
  namespaces (declines) | ≥2 workspace-wide but 1 in used namespaces (THE
  disambiguation win).
- **Ref/symbol kinds**: type kinds (Class/Struct/Interface/Enum — match) |
  member kinds (Method/Function/Const — filtered out of the using-arm).
- **Usings per file**: 0 | 1 | N | duplicates (idempotent) | forms: plain
  (in scope) | static/alias/global (decline; type-level dotted keys miss
  the map naturally — probed) | `using` of an external namespace (map miss).
- **Namespace declarations**: dotted single (`namespace A.B`) | file-scoped
  (`namespace X;`) | spread over N files | nested blocks (descoped,
  tethys-nnst) | namespace with zero types.
- **Directory layout**: same `orphan:` bucket | cross-bucket (C9).
- **ctx.namespaces**: `Some(map)` (C# driver paths) | `None` (Rust paths,
  and any future caller that skips map construction — resolver must treat
  as empty, not panic).

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|---|---|---|---|---|---|
| C1 | The namespace map from Module symbols keys dotted-declaration and file-scoped namespaces correctly, flat-only | SQL map enumeration vs hand-listed namespaces on the probe WS | sqlite3 vs fixture source read | 5m | **PASSED** (probe Q2 + parent-chain SQL) | unit test on the lifted map builder |
| C2 | Nested-block reconstruction via parent chain is impossible today | SQL: any module symbol with module parent? | sqlite3 | 2m | **RAN — FAILED (by design intent); descoped to tethys-nnst** | spec edge row + tethys-nnst |
| C3 | (B1) A type name colliding across namespaces resolves to the used namespace's symbol | collision fixture (probe slice 2 + using arm): Widget must resolve to My.Models' | SQL on refs vs hand ground truth | 15m | pending | integration test `csharp_using_disambiguation` |
| C4 | Arm-order safety: names the fallback already resolves get the SAME target or a decline from the using-arm, never a different symbol | fixture: unique Widget inside a used namespace — pre/post target identical | pre/post dump join | 10m | pending | same integration test, distinct assert + C6 dump |
| C5 | (B2) Colliding bare MEMBER names are not disambiguated (kind filter) | two `Assist` methods in two namespaces, one used → stays UNRESOLVED | SQL on refs | 10m | pending | integration test, distinct assert |
| C6 | (B8) Probe-WS C# refs are monotone-stable (no target changes/losses) | pre/post natural-key dump join on the probe workspace | dump.sh + join | 10m | pending | fixture test asserting each known resolution |
| C7 | Rust resolution byte-identical (FirstMatch branch IS the current loop) | frozen-worktree self-index + c6trap dumps pre/post | dump.sh + diff | 15m | pending | existing pass2/resolver/trap suites + per-slice oracle |
| C8 | (B5) L2 delta on the ground-truth fixture is exactly the enumerated set: unused-using edge (App→Other) gone, used edges persist, call-edge rows separated | pre/post file_deps diff vs hand enumeration | sqlite3 + hand table | 20m | pending | integration test `csharp_l2_file_deps` |
| C9 | Cross-directory C# deps survive deletion via namespace-corroborated K-hybrid (baseline: edge exists today, post-pass-sourced) | multi-dir fixture: services→models edge must exist post-change | sqlite3 (baseline RAN: 1 edge today) | 15m | baseline **PASSED** | integration test `csharp_cross_dir_deps` |
| C10 | The post-pass is gone | `rg 'resolve_csharp_dependencies' src/` = 0 | rg | 1m | pending | seam_lint-style source test |
| C11 | Resolver impls stay DB-free/stateless; the map arrives via ctx | existing seam_lint C10 + no new `use crate::db` | rg / existing lint | 1m | pending | existing `tests/seam_lint.rs` (unchanged) |
| C12 | Indexing wall-time ≤ baseline +10% (improvement expected: C# re-parse deleted) | fresh-built binaries both sides, frozen input, median ≥5 | wall clock | 20m | pending | **manual** (criterion bench exists; precedent: separator-fix C9 approval) — requires user approval |

Named buggy implementations (non-vacuity): C1 — map keyed on qualified_name
(`Outer1::Inner1` forms) instead of name; C3 — using-arm searching only the
first using; C4 — per-glob early-return under UniqueAcrossAll picking the
first namespace's same-named symbol (target flip); C5 — missing kind filter
(member resolves); C6/C7 — policy branch misrouted (Rust through
UniqueAcrossAll → glob collision behavior changes → dump diff); C8 —
compute_dependencies glob-skip removed (L1 edges reappear); C9 — deletion
without corroboration extension (edge vanishes — proven by baseline
arithmetic); C10 — partial deletion; C11 — ctx carrying a DB handle.

## Negative space

This design deliberately does NOT:
1. Resolve `using static` / alias / `global using` forms (tethys-usgf).
2. Handle nested-block namespace declarations (tethys-nnst — pre-existing gap).
3. Change C# symbol/ref storage or the canonical `::` format.
4. Mint file deps at Pass-1 for namespace imports (deps derive from
   resolved refs + corroboration — that IS the L2 semantics).
5. Touch Rust resolution paths (FirstMatch default = existing code).
6. Add project discovery (.csproj/.sln) or cross-language resolution.

## Approval

Design approved by requester 2026-06-06. C12's manual regression fence
explicitly approved 2026-06-06 (same shape as separator-fix C9 precedent).

## Tracker references

tethys-jwf9 (closes), tethys-nmsp (closes), tethys-usgf (verified, open),
tethys-nnst (filed this design), tethys-dsp1/8mze (context, verified).
