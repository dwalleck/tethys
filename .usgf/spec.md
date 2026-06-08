# Feature: C# `using static` static-method-call disambiguation

> **Revision 2.** The probe (`.usgf/probe-findings.md`) falsified B2's reach:
> the C# extractor indexes only callable members and only call-shaped refs,
> so consts / static fields / enum members are invisible end to end
> (filed tethys-cfme). Scope narrows to **static method calls**. The probe
> also showed `is_static` storage is unnecessary — the static using is
> distinguishable by type-detection — softening decision #4. Core premise
> intact: colliding method names stay unresolved (the gap); unique ones
> already resolve (the monotone baseline).

## What this is

`using static My.Models.Helper;` brings a type's static methods into scope.
Today the directive stores as a glob row whose `source_module` names a TYPE
(`My.Models.Helper`) — which misses the namespace map — so a bare method
call like `Assist()` resolves only when its name is workspace-unique. This
change adds a member-resolution arm to the C# glob path: a bare method name
that COLLIDES across types disambiguates to the static-imported type's
method. Like the jwf9 loop, the gain is collision disambiguation, not
first-time resolution — workspace-unique names already resolve via the
fallback and must keep their targets. Scoped to `using static` of method
members only; alias usings (tethys-alus), global usings (tethys-glus), and
non-method members (tethys-cfme) stay declining.

## Users

- **tethys maintainer (dwalleck)**: the third C# using-form behind the seam;
  `using static` stops being silently indistinct from a plain namespace using.
- **query consumers (CLI users, MCP/AI agents on C# codebases)**: callers /
  references / impact for static helper methods work at symbol level for
  unqualified calls where the method name collides across types.

## Behavior

### B1 — Static-using disambiguates a colliding member name
- **Given**: `class Helper` in `My.Models` (file B) has `static void Assist()`,
  AND another `Assist` member exists in a different type/namespace (so the
  name is workspace-non-unique and today's fallback declines); file A has
  `using static My.Models.Helper;` and a bare `Assist()` call
- **When**: `tethys index` runs Pass 2
- **Then**: the `Assist` ref resolves to Helper's `Assist` member (file B),
  not the other

### B2 — Method members only (the substrate's reach)
- **Given**: `using static My.Models.Constants;` where `Constants` declares a
  `const X` whose bare name `X` collides elsewhere
- **When**: Pass 2 runs
- **Then**: the bare `X` ref stays unresolved — consts / static fields /
  enum members are not extracted as symbols OR refs (tethys-cfme), so the
  static arm has nothing to resolve them against. Only static METHOD calls
  (kind function/method) are in reach (decision #2, revised by probe).

### B3 — Union with the types arm; cross-arm collision declines
- **Given**: file A has `using My.Models;` (a type `Foo` lives there) AND
  `using static Other.Util;` (a member `Foo` lives there); a bare `Foo` ref
- **When**: Pass 2 runs
- **Then**: the ref stays unresolved — candidates from both arms are unioned
  and the unique-or-decline rule sees two `Foo` symbols (decision #3)

### B4 — External static usings decline
- **Given**: `using static System.Math;` and a bare `Sqrt(...)` call; no
  workspace file declares type `Math` in namespace `System`
- **When**: Pass 2 runs
- **Then**: no resolution via the static arm (workspace-unique fallback may
  still resolve it if applicable); identical to today for this shape

### B5 — The static arm fires only for static usings
- **Given**: a file with `using static My.Models.Helper;` (source_module
  names type `Helper` in namespace `My.Models`) and another with plain
  `using My.Models;` (source_module names the namespace)
- **When**: indexed
- **Then**: the member-resolution arm fires for the static directive and not
  the plain one — distinguished by type-detection (source_module resolves to
  a type, not a namespace), no `is_static` storage flag required (probe:
  decision #4 revised)

### B6 — Existing C# resolutions are monotone-stable
- **Given**: the jwf9 probe workspace + the new static-using fixture; refs
  resolved before this change
- **When**: indexed post-change
- **Then**: every previously-resolved C# ref resolves to the SAME symbol;
  zero targets change, zero resolutions lost (new resolutions may appear)

### B7 — Rust resolution byte-identical
- **Given**: frozen self-index worktree + c6trap fixture
- **When**: dumped pre/post
- **Then**: zero diff lines

## Success criteria

- **New-resolution correctness**: 100% of newly resolved C# member refs on
  the ground-truth fixture match a hand-written expectation table (symbol +
  file + line), measured by dump + table.
- **Monotone stability**: 0 previously-resolved C# refs change target or
  become unresolved, measured by pre/post natural-key dump join.
- **Rust strictness**: 0 diff lines on frozen self-index + c6trap.
- **Suite**: `cargo test` green; `cargo clippy --all-targets` zero warnings;
  seam lints (incl. DB-free C10) still pass.
- **Perf**: indexing wall-time ≤ baseline +10% (fresh-built binaries both
  sides, frozen input, median ≥5).

## Edge cases and decisions

| Edge | Decision | Rationale |
|---|---|---|
| `using static A.B.C` vs plain `using A.B` distinguishability | Type-detection: `source_module` resolving to a TYPE (last segment is a class, prefix is a namespace) marks the static arm; no `is_static` storage flag needed (probe) | The two forms never share a shape — static names a type, plain names a namespace |
| Static-imported type spread over partial-class files | Methods in all declaring files are candidates | Valid C#; mirrors the namespace-spread handling |
| Within-type duplicate method name (overloads / invalid) | Unique rule declines | Refuse-to-guess precedent |
| Non-method member (const/field/enum member) via static using | Stays unresolved — not extracted as symbol or ref (tethys-cfme) | Substrate gap, not a choice; probe-confirmed |
| Instance method matched (extractor doesn't tag static) | Allowed (decision #2); over-match only bites on collision, which unique-or-decline guards | tethys doesn't track the static modifier (tethys-itez); honest approximation |
| `using static` of a workspace type with zero methods | Resolves nothing | Follows from the member lookup |
| Type-scoping handle for member lookup | `qualified_name` prefix (`Helper::`), NOT `parent_symbol_id` (None for functions) | Probe Q2 |
| Bare member name already workspace-unique | Keeps resolving via the pre-existing fallback, same target | Probed jwf9 behavior; B6 forbids regressing it |
| Nested-block-namespaced static-imported type | Declines (the type's namespace isn't in the flat map — tethys-nnst) | Pre-existing gap, unchanged |
| Extension methods / nested-type members via static using | Out of scope; decline | Boundary decision |
| Qualified `Helper.Assist()` ref in a static-using file | Unchanged — qualified refs keep their existing path | No new arm fires for qualified names |
| Member-name ref in a file with no `using static` | Unchanged paths | No new arm fires |
| Concurrency / timezone / multi-tenancy | N/A — batch indexing | — |
| Max scale | Static-arm candidate lookup bounded by members-per-type; budget at plan; seam stays DB-free (data via ctx) | Fence exists |

## Out of scope

This change does NOT include:
- Alias usings (`using W = ...`) — tethys-alus
- Global usings (`global using ...`) — tethys-glus
- Non-method members: const / static field / enum member (not indexed —
  tethys-cfme)
- Enforcing C#'s static-only method rule (instance methods may match)
- Extension methods / nested-type members through static usings
- Nested-block-namespaced static-imported types (tethys-nnst)
- A schema change (type-detection needs none); .csproj/.sln discovery;
  cross-language; new languages
- LSP changes; display spelling (tethys-dsp1)

## Constraints

| Dimension | Limit | How measured |
|---|---|---|
| Rust behavior | byte-identical dumps | frozen worktree + c6trap |
| Existing C# resolutions | 0 target changes / losses | pre/post dump join |
| New C# resolutions | 100% ground-truth match | expectation table |
| Wall-time | ≤ baseline +10% | fresh builds, frozen input, median ≥5 |
| Seam integrity | seam_lint suite passes unchanged (DB-free) | existing fences |

## Decisions log

| # | Question | Decision | Why |
|---|---|---|---|
| 1 | Which usgf form does this loop target? | `using static` only; alias + global stay declining, filed as tethys-alus / tethys-glus | Three forms, three mechanisms; interrogated-spec refuses bundling |
| 2 | Which member kinds may the static arm resolve to? | ~~Any member~~ **METHODS ONLY** (probe rev): consts/static-fields/enum-members aren't indexed (tethys-cfme); static-vs-instance not enforced | tethys extracts only callable members + call refs; over-match only bites on collision, which unique-or-decline guards |
| 3 | How does the member arm compose with the existing types arm? | Union the candidates, then one unique-or-decline across the union | Consistent with jwf9; one disambiguation rule, no arm-ordering subtlety |
| 4 | How is the static using distinguished from a plain namespace using? | ~~is_static storage propagation~~ **Type-detection** (probe rev): no schema change — source_module names a type vs a namespace | Probe showed the two forms never share a shape; storage change avoidable |
| 5 | (Post-probe) member-kind reach and distinguishing mechanism re-pinned | Methods-only + type-detection; tethys-cfme filed for the non-method gap | Probe falsified B2's breadth and decision #4's necessity |

## Probe questions (for prove-it-prototype)

1. Confirm `using static T;` and plain `using N;` are indistinguishable in
   stored data today (is_static dropped) — the load-bearing gap behind B5.
2. How are C# static members stored — name, kind, and the type-scoping
   handle (parent_symbol_id? qualified_name `Type::member`?) — does a bare
   member name + type scope suffice to find them?
3. Does a bare member ref (`Assist()`) get extracted as a ref with a usable
   name today, and does it stay unresolved when non-unique?
4. What does a `using static My.Models.Helper;` row's `source_module`
   actually contain (`My.Models.Helper`? just `Helper`?) — informs how to
   recover the type and its declaring files.
5. Are const / static-field / enum-member symbols extracted at all (informs
   B2's reach)?

## Sign-off

Revision 1 sign-off (superseded by the probe): "This loop disambiguates
colliding member names through using static, not unique names" — 2026-06-07.

Revision 2 sign-off — the requester typed, verbatim: "disambiguates
colliding static method calls, methods only, no schema change"

Matches B1 (colliding-method disambiguation), B2-rev (methods only — the
substrate's reach, tethys-cfme for the rest), and decision #4-rev
(type-detection, no schema change). Disambiguation-only framing carried
from the jwf9 lesson; union-with-types-arm (decision #3) unchanged.

Date: 2026-06-07
