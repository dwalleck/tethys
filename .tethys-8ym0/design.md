# tethys-8ym0 — falsifiable design: macro-token call refs

## Purpose

Emit references for **bare call-shaped identifiers inside macro token trees**
(`assert_eq!(helper(), 1)` → a ref binding `helper`), closing the gap that
parked untested-code (tethys-y3bx): assert-macros are the dominant unit-test
idiom, so functions tested *only* through asserts currently look unreachable
from every test root.

## Probe evidence this design stands on (`findings.md`)

- Token trees are opaque today (`MACRO_INVOCATION` returns early,
  src/languages/rust.rs:226; fence `macro_token_identifier_not_emitted_as_value`).
- Self-index: 477 raw bare call-shapes → 180 in-crate fn matches → 177 after
  the ygjx scope guard; **every sampled survivor genuine**. The issue's feared
  591-noise number conflated method/scoped shapes (9l27/ewa7 territory).
- Oracle agreement 11/11 item-by-item (grep + hand-read, tests/value_refs.rs).
- Impact: y3bx untested count 260 → 235 from bare-call edges alone.

## Core design

One new reference kind, mirroring the shipped `value` (ygjx) posture exactly:

1. **Extraction** (src/languages/rust.rs, MACRO_INVOCATION arm): after the
   existing macro-name ref, walk the invocation's `token_tree` descendants.
   An `identifier` token emits `ExtractedReferenceKind::MacroCall` iff:
   - next sibling is a `(`-delimited `token_tree` (call shape), AND
   - previous sibling is not `.` (method shape → 9l27) and not `::`
     (path tail → ewa7), AND next sibling is not `!` (nested macro name →
     7dqj) — n.b. the `(`-tree condition already excludes `::`-followed heads, AND
   - the text is not in the enclosing function's local-binding set (reuses
     `collect_local_bindings`, already threaded to the macro arm).
2. **Resolution**: `macro_call` rows flow through Pass 1/Pass 2 unchanged,
   kind-blind like `call` (`ref_binds_to_symbol_kind` keeps gating only
   `Macro`), stamping `strategy` normally; they participate in the Pass-2
   memo (only kind=`Macro` bypasses it).
3. **Unresolved drop**: widen `drop_unresolved_value_refs` to
   `kind IN ('value','macro_call')` — a token matching no in-crate symbol is
   noise (≈290 of 477 on self-index: `Some(`, std calls, DSL tokens).
4. **call_edges exclusion**: add `macro_call` to the `NOT IN ('value',
   'field_access')` list. Precision consumers (`callers`, `impact`,
   reachability over call_edges) never see token-soup edges — excluded by
   default, which is *stronger* than the issue's suggested "band as
   speculative, opt-out". Suppression consumers (dead-code, untested-code,
   unused-imports, deprecated-callers) read `refs` and see them.
5. **Banding**: no special case — `refs_banded.band` stays strategy-derived.
   The KIND is the provenance marker; any consumer can filter
   `kind='macro_call'`.

Error-posture check: on DSL-heavy foreign codebases (`quote!`, `html!`) a
token like `div(...)` matching an in-crate fn fabricates a *same-package
suppression* ref. Every consumer degrades in the safe direction: dead-code /
untested suppress a finding (never accuse), unused-imports marks an import
used (finding removed), visibility-tightening loses a candidate (fewer
accusations), callers/impact see nothing (excluded). No consumer becomes
*more* accusatory — consistent with "suppressions, not accusations".

## Input shapes (identifier tokens inside a macro invocation's token tree)

| # | shape | handling |
|---|-------|----------|
| S1 | bare ident + `(`-tree, resolves in-crate | **emit**, resolve (C1,C2) |
| S2 | bare ident + `(`-tree, matches local binding (`let f = …; assert!(f(1))`) | suppress at extraction (C1/F2) |
| S3 | bare ident + `(`-tree, no in-crate match (`Some(1)`, std) | emit → unresolved → dropped (C3) |
| S4 | ident preceded by `.` (method shape) | not emitted — tethys-9l27 (F4 tripwire) |
| S5 | ident adjacent to `::` (path segment) | not emitted — tethys-ewa7 (F4 tripwire) |
| S6 | ident followed by `!` (nested macro name) | not emitted — tethys-7dqj (F5 tripwire) |
| S7 | bare ident, not call-shaped (4550 raw) | not emitted — settled: noise ≫ signal; ygjx covers real-AST value uses |
| S8 | tokens in `macro_rules!` definition bodies | not emitted — definitions aren't invocations (C8/F6) |
| S9 | names inside string literals (`"fn higher…"` fixtures) | lexer excludes — probe-verified |
| S10 | macro at module top level (proptest!) | emit; `in_symbol_id` NULL; guard vacuous; the 11 self-index sites unattachable until tethys-0nar |
| S11 | nested trees `assert!(f(g(1)))` | both emitted (C1/F5's `g`) |
| S12 | `vec![…]` / `foo!{…}` delimiters | descended; call shape still requires `(`-tree |
| S13 | empty token tree | no-op |
| S14 | tuple-ctor `Foo(…)` matching in-crate struct/variant | emits; kind-blind resolution binds (0aqj); acceptable usage-suppression semantics |
| S15 | raw idents `r#type(…)` | emit → no in-crate match → dropped (C3 class) |
| S16 | attribute/derive token trees (`#[derive(X)]`, `cfg_attr`) | never visited — walk starts at MACRO_INVOCATION only; attributes are metadata (nm98 governs cfg_attr) |

C# is untouched by construction (no `macro_invocation` nodes).

Subtractive sweep (step 2b): the change is **additive** (new rows of a new
kind; no constraint, ordering, or guard is removed). The one invariant it
retires is intentional and fenced: "macro tokens never produce refs" — its
fence `macro_token_identifier_not_emitted_as_value` flips deliberately
(spec change, C11). The `refs ≡ call_edges` equivalence y3bx *measured* was
never a guarantee — this change makes refs a strict superset by design (C4,
C5); y3bx must consume refs, not call_edges (recorded on y3bx at close-out).

## Falsification

Fences live in a new `tests/macro_token_refs.rs` (F-numbers) unless noted.
Every fixture builds its own index from a fixture workspace.

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| C6 | Adding the ~177 self-index macro-token refs leaves unused-imports / visibility-tightening / deprecated-callers / panic-points / callers output unchanged | inject probe survivors as resolved stand-in rows into the live self-index; run the real binary's 5 analyses; diff vs baseline; verify rows survived (non-vacuity) | real binary output diff (`falsifier1.py`; 253-vs-76 row check) | 10m | **passed** (5/5 identical) | post-build audit re-run with real kind + each analysis's own fixture suite (kind-blind evidence is what they fence) |
| C1 | Exactly the S1 shapes emit — one `macro_call` ref per qualifying token, none for S2/S4/S5/S6/S8 | fixture with all shapes; assert per-shape row counts | SQL counts on fixture-built index vs hand-enumerated expectations | 30m | pending | F1 (emit), F2 (local suppressed), F4 (method/path absent), F5 (nested-name absent, nested-tree `g` present) |
| C2 | `macro_call` resolves through existing arms: same-file fn binds `strategy='same_file'`; unique cross-file fn binds `unique_workspace` | fixture w/ same-file + cross-file targets; assert symbol_id + strategy | SQL on fixture index; strategies per ADR-0003 | in F1 | pending | F1 asserts (symbol_id, strategy) — a kind-gate bug leaves rows unresolved and F1 fails |
| C3 | No unresolved `macro_call` rows survive indexing | fixture `assert!(nonexistent_fn())`; count rows | `SELECT COUNT(*) … kind='macro_call' AND symbol_id IS NULL` = 0 | 10m | pending | F3 (fails if drop sweep not widened) |
| C4 | `macro_call` never reaches call_edges; `callers` of a macro-only-called fn is empty | F1 fixture; query call_edges + run `get_callers` | call_edges SQL + facade output | 15m | pending | F7 (fails if `NOT IN` list not extended) |
| C5 | The y3bx blocker edge exists: `#[test] t` + `assert_eq!(helper(),1)` → refs row (in_symbol=t, symbol=helper) | the exact parked y3bx fixture | SQL row assert (red today by ygjx fence) | in F1 | pending | F1; BFS consumption is y3bx's own fence (y3bx verified open, blocked on 8ym0). Audit records self-index 260→235 |
| C7 | deprecated-callers lists a macro-context call site of a `#[deprecated]` fn | fixture: deprecated fn called only in `assert!` cross-file | CLI/facade output (rustc `--force-warn deprecated` warns on the same site — jdly oracle) | 30m | pending | F11 |
| C8 | `macro_rules!` bodies emit nothing | macro-definition fixture with `foo()` in expansion template | SQL count = 0 | 10m | pending | F6 (fails if walk hooks token_tree generically) |
| C9 | Deterministic emission: two indexes → identical `macro_call` multisets | index twice, compare row dumps | canonical dump diff (value_ref_determinism precedent) | 10m | pending | F9 |
| C10 | Batch ≡ streaming for macro refs AND a cross-file import used only inside a macro corroborates its file_dep in both | two-file fixture; index both paths; diff refs + file_deps | canonical dumps + file_deps SQL | 45m | pending | F10 |
| C11 | Existing extraction unchanged: idxperf goldens byte-identical (fixture macro-free); full suite green except the deliberate flip of `macro_token_identifier_not_emitted_as_value` | run goldens + full nextest post-build | existing golden rows + suite | 15m | pending | existing `idxperf_golden` tests + flipped fence (TRIPWIRE→positive) |
| C12 | Self-index wall time regresses <10% (single O(tokens) walk) | time `tethys index --rebuild` pre/post (3 runs) | hyperfine/`time` measurement | 10m | pending | **manual** (audit-trail number); CI perf fencing is tracked at tethys-ng1v — needs approval |

Cheapest falsifier (C6) ran before this document was presented: **passed**,
non-vacuous (row-presence check), artifacts `base-*.txt`/`post-*.txt`.

## Negative space (deliberately not doing)

1. **Method-shape tokens** (`x.unwrap()` in asserts) — tethys-9l27. Probe:
   name-matching is 60% ambiguous (`is_empty` ×192); needs receiver typing.
   Would add −35 more untested FPs; not worth phantom risk here.
2. **Path-shaped calls** (`m::f(...)` in macros) — tethys-ewa7 (3 sites, all
   proptest-gated).
3. **Nested macro-name refs** (`matches!` inside `assert!`) — tethys-7dqj.
4. **Non-call bare idents in trees** (callbacks passed inside macros) —
   settled rationale: 4550 raw candidates, noise ≫ signal under the
   suppression posture; ygjx cat-1 handles real-AST value positions.
5. **proptest-body attachment** — refs inside `proptest!` stay top-level
   until tethys-0nar indexes macro-defined fns.
6. **No banding special-case, no LSP change, no C# change** — kind is the
   provenance marker; Pass 3 and C# paths untouched.

## Open decisions flagged for approval

- **D-A (posture)**: new kind `macro_call` **excluded from call_edges** (like
  `value`) instead of the issue's literal "include + band speculative".
  Consequence: y3bx must traverse refs (its findings already blessed this);
  callers/impact stay pristine by default. Alternative rejected: banding-only
  lets token-soup edges into `impact` by default on DSL-heavy codebases.
- **D-B (naming)**: `macro_call` vs reusing `value`. Recommend `macro_call` —
  distinct provenance for future consumers (hotspots weighting, dead-code
  reporting "kept alive only by macro call"), cleaner fences.
- **D-C**: C12's regression fence is `manual` (audit number; CI perf fence =
  tethys-ng1v). Needs explicit approval per design rules.
- **D-D**: deliberate TRIPWIRE flip of ygjx's
  `macro_token_identifier_not_emitted_as_value` fence (spec change).
