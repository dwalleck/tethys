# Feature: C# namespace resolution through the ModuleResolver seam (jwf9 + nmsp)

> **Revision 2.** Revision 1 claimed unqualified C# type refs do not resolve
> today; the probe FALSIFIED that (`.csharp-ns/probe-findings.md`): the
> unscoped unique-name fallback already resolves any workspace-unique simple
> name, kind-blind. B1/B2 are re-pinned on the probed reality. All five
> interrogation decisions stand.

## What this is

`CSharpModuleResolver` stops declining: plain `using Namespace;` directives
resolve against a namespace→files map built from indexed workspace C# files.
The symbol-resolution gain is **collision disambiguation**: when a simple
type name exists in multiple namespaces (today: unique-fallback declines,
ref stays NULL — probed), the file's using-directives narrow the candidates
to one. Workspace-unique names already resolve today and must keep their
targets. In the same loop, the pre-existing `resolve_csharp_dependencies`
post-pass is deleted and C# file-level dependencies are produced by the
seam's normal path under L2 semantics (used imports only) — a deliberate,
enumerated behavior change from today's L1 per-using edges. Closes
tethys-jwf9 and tethys-nmsp together; both halves consume the one new
mechanism (the namespace map).

## Users

- **tethys maintainer (dwalleck)**: C# resolution consolidates into one
  mechanism behind the seam; the post-pass (re-parse of every C# file) dies.
- **future language implementor (tethys-8mze)**: first non-declining,
  non-Rust `ModuleResolver` — validates that the seam's contract actually
  stretches beyond Rust before Python/TS/Go consume it.
- **query consumers (CLI users, MCP/AI agents on C# codebases)**: callers /
  references / impact for C# types now work at symbol level for unqualified
  usages (`new Widget()`, type annotations), not just `Foo.Bar`-qualified
  ones; file_deps reflect used imports only.

## Behavior

### B1 — Using-directives disambiguate colliding type names
- **Given**: `class Widget` exists in BOTH `namespace My.Models` (B.cs) and
  `namespace Dupe.Ns` (C.cs); `A.cs` has `using My.Models;` and a
  `new Widget()` reference — which today stays UNRESOLVED (probed:
  unique-fallback declines on 2 candidates)
- **When**: `tethys index` runs Pass 2
- **Then**: the `Widget` ref row has `symbol_id` = B.cs's `Widget` (the
  used namespace's candidate); C.cs's is never chosen

### B2 — The using-arm matches types only; it does not disambiguate members
- **Given**: a bare call `Assist(...)` in `A.cs` with `using My.Models;`,
  where `Assist` methods exist in BOTH `My.Models` and another namespace
  (workspace-non-unique, so today's fallback declines)
- **When**: Pass 2 runs
- **Then**: the ref STAYS unresolved — the using-arm's kind filter
  excludes members (bare-member disambiguation needs `using static`,
  tethys-usgf). Workspace-UNIQUE bare members keep resolving via the
  pre-existing fallback exactly as today (B8 protects them).

### B3 — Unique-or-decline on collisions
- **Given**: `A.cs` uses two namespaces, both declaring a type `Widget`,
  and references bare `Widget`
- **When**: Pass 2 runs
- **Then**: the ref stays unresolved (NULL), with a trace-level decline log

### B4 — External namespaces decline
- **Given**: `using System;` and a `Console`-type reference, no workspace
  file declaring `namespace System`
- **When**: Pass 2 runs
- **Then**: no resolution, no panic — identical to today

### B5 — File deps are L2 (used imports only); unused usings lose their edges
- **Given**: `A.cs` with `using My.Models;` (a `Widget` ref present) and
  `using Other.Ns;` (nothing from it referenced); workspace files declare both
- **When**: indexing completes
- **Then**: file_deps contains A.cs→(My.Models files defining referenced
  types) and does NOT contain A.cs→(Other.Ns files) — the L1 post-pass
  would have produced both

### B6 — The post-pass is gone
- **Given**: the post-change source tree
- **When**: `rg 'resolve_csharp_dependencies' src/` runs
- **Then**: zero matches; C# file deps demonstrably originate from the
  seam path (B5's fixture proves edges still appear)

### B7 — Rust resolution byte-identical
- **Given**: Rust fixtures (frozen-worktree self-index per the established
  oracle procedure + C6 trap workspace) indexed pre/post
- **When**: natural-key dumps compared
- **Then**: zero diff lines

### B8 — Previously-resolved C# refs keep their exact targets
- **Given**: the PR-1 C# probe workspace (qualified `Hasher.Hash`,
  `Outer.Inner` constructor — resolved today via the qualified fallback)
- **When**: indexed post-change
- **Then**: each previously-resolved ref resolves to the SAME symbol
  (monotone-stable; new resolutions may appear, none may move or vanish)

## Success criteria

- **New-resolution correctness**: 100% of newly resolved C# refs on the
  ground-truth fixture match a hand-written expectation table (symbol +
  file + line), measured by dump + table comparison.
- **Monotone stability**: 0 previously-resolved C# refs change target or
  become unresolved, measured by joining pre/post dumps on natural keys.
- **L2 dep delta is exactly the prediction**: file_deps diff vs baseline =
  precisely the edges the ground-truth table marks as unused-using
  removals + used-import additions; no unexplained rows.
- **Rust strictness**: 0 diff lines on both Rust fixtures (dump oracle).
- **Mechanism deletion**: `rg resolve_csharp_dependencies src/` = 0 matches.
- **Suite**: `cargo test` green; `cargo clippy --all-targets` zero warnings;
  seam lints (incl. C10 DB-free) still pass.
- **Perf**: indexing wall-time ≤ baseline +10% (fresh-built binaries both
  sides, frozen input, median of ≥5). Note: deleting the post-pass removes
  a full re-parse of every C# file, so improvement is expected; the
  criterion bounds regression only.

## Edge cases and decisions

| Edge | Decision | Rationale |
|---|---|---|
| Namespace spread across N files (partial-class style) | All N files are candidates; unique-or-decline applies to the SYMBOL, not the file | Valid C# guarantees type-name uniqueness per namespace |
| Workspace-unique name declared in a namespace the file does NOT use | Keeps resolving via the pre-existing unique-fallback (kind-blind, using-blind) — unchanged | Probed today-behavior; B8 monotone-stability forbids regressing it; documented approximation |
| Nested block namespaces (`namespace Outer1 { namespace Inner1 {`) | OUT OF SCOPE (corrected at design time): module symbols carry no parent chain (design falsifier C2 ran and FAILED), so dotted reconstruction needs extractor work; the post-pass had the identical gap, so behavior is unchanged. Tracked: tethys-nnst | Design-stage falsifier; dotted declarations (`namespace A.B`) and file-scoped namespaces both work and remain in scope |
| `using static` / alias rows in imports table (`*\|My.Models.Helper`, `*\|My.Models.Widget\|W`) | Decline naturally — no namespace bears a type-level dotted name; `is_static` is dropped at extraction (csharp.rs:118), noted for tethys-usgf | Probe Q5 |
| file_deps rows from call edges (K-hybrid) vs import edges | B5's L1→L2 delta enumeration must separate the two sources; call-edge-derived rows are untouched by this change | Probe Q4: App→Models ref_count includes call-edge contributions |
| Same-namespace duplicate type (invalid C#) | Unique rule declines | Refuse-to-guess precedent |
| Empty namespace (declared, no types) | Resolves nothing; using produces no deps under L2 | Follows from decisions #2/#3 |
| File declares AND uses the same namespace | No self-dep edge (file→itself excluded) | Post-pass precedent preserved |
| File-scoped namespaces (`namespace X;`) | In scope IF the extractor already surfaces them identically to block-scoped (probe question); fixture includes one | Behavior follows extractor; no parser work this loop |
| Usings INSIDE a namespace block (scoped usings) | Treated as file-level (approximation); documented | Per-file resolution model; exact C# scoping out of scope |
| Nested/dotted namespace declarations (`namespace A.B`) | Probe verifies the stored namespace symbol name; map keys must match the stored using `source_module` format (dotted) | Storage consistency, probed not assumed |
| `using static` / alias usings / `global using` | Decline, unchanged | Decision #5; tethys-usgf |
| Duplicate using rows for one namespace | Idempotent — same candidate set | Map is keyed by namespace |
| Ref in a file with zero usings | Unchanged paths (same-file, qualified fallback) | No new arm fires |
| Qualified C# refs now also reachable via the glob arm | Permitted, but B8's monotone-stability criterion forbids target changes | Earlier-arm resolution must agree with today's fallback result |
| Mixed workspace (Rust crate named like a namespace) | Per-file dispatch unchanged; mixed_language_dispatch fence extends to assert the new C# resolution doesn't cross languages | Existing fence pattern |
| Concurrency / timezone / multi-tenancy | N/A — batch indexing | — |
| Max scale | Namespace map ~O(C# files); budget at plan stage; seam impls stay DB-free (map arrives via context — C10 lint unchanged) | Fence already exists |

## Out of scope

This change does NOT include:
- `using static`, alias usings (`using X = ...`), `global using` (tethys-usgf)
- .csproj/.sln project discovery — namespaces come from indexed .cs content only
- Cross-language resolution
- Architecture-phase/package support for C# (still Rust-crate-based)
- Bare member-name resolution (methods/consts/fields) through plain usings
- LSP changes; display spelling (tethys-dsp1); new languages (tethys-8mze)

## Constraints

| Dimension | Limit | How measured |
|---|---|---|
| Rust behavior | byte-identical dumps | frozen-worktree oracle ×2 fixtures |
| Existing C# resolutions | 0 target changes / losses | pre/post dump join |
| New C# resolutions | 100% ground-truth match | expectation table |
| file_deps delta | exactly the predicted L2 set | enumerated diff |
| Wall-time | ≤ baseline +10% | fresh builds, frozen input, median ≥5 |
| Seam integrity | seam_lint suite passes unchanged (incl. DB-free C10) | existing fences |

## Decisions log

| # | Question | Decision | Why |
|---|---|---|---|
| 1 | Scope: symbol resolution only, +absorb nmsp, or file-deps only? | Symbol resolution + absorb nmsp | Both halves consume the one namespace-map mechanism; building it twice across two loops is waste. Two-features-in-one tension accepted with doubled oracle surface |
| 2 | File-dep semantics under absorption: preserve L1 per-using edges or unify to L2 used-only? | Unify to L2 | Consistent cross-language semantics; better impact/affected-tests signal; removed edges enumerated and verified in acceptance |
| 3 | Which symbol kinds resolve through a plain using? | Types only (class/struct/interface/enum/record) | C# semantics: usings import types; bare members need using-static (out of scope) |
| 4 | Collision policy for simple-name matches? | Unique-or-decline across all of the file's usings | Refuse-to-guess precedent (search_unique_symbol_by_name); collisions indicate approximate parsing or invalid code |
| 5 | Boundary: using static / aliases / global usings / project discovery / cross-language? | All five out; plain block- and file-scoped `using Namespace;` only | Requester confirmed; deferred forms filed as tethys-usgf |
| 6 | (Post-probe revision) What does the using-arm actually add, given unique names already resolve? | Collision disambiguation only; B1/B2 re-pinned on probed baselines; decisions #1–#5 unchanged | Probe falsified rev-1's "was NULL before" premise; see probe-findings.md |

## Probe questions (for prove-it-prototype — flagged, not assumed)

1. Does csharp.rs:109-111 (`resolve_import` per jwf9's stale body) still
   exist post-seam, and what does it do?
2. How are namespace symbols stored for block-scoped, file-scoped, nested,
   and dotted declarations — do the names match stored using
   `source_module` strings exactly?
3. Are C# type references (constructors, type annotations) extracted with
   kinds that support the types-only filter?
4. What exactly does today's post-pass produce on the ground-truth fixture
   (the L1 baseline that B5's delta is measured against)?
5. Does the extractor surface `using static` / alias / global usings at
   all (informs tethys-usgf, confirms decline behavior)?

## Sign-off

Revision 1 sign-off (superseded by the probe revision): "This loop resolves
types from using statements. It does not resolve global or static using
statements" — 2026-06-06.

Revision 2 sign-off — the requester typed, verbatim: "It will handle
collision disambiguation, creating a namespace map as the shared mechanism
for resolution"

Matches B1 (disambiguation) and decision #1 (the map is the one mechanism
shared by symbol resolution and the absorbed file-dep path). Compresses
over decision #2 (L2 semantics), which the requester selected explicitly
and is logged above.

Date: 2026-06-06
