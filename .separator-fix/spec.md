# Feature: Language-dispatched module resolution seam (ModuleResolver extraction)

> **Revision 2.** Revision 1 was premised on a bug ("C# qualified refs stored with
> `.` silently skip the `::` qualified-resolution gate") that the prove-it-prototype
> probe FALSIFIED — see `.separator-fix/probe-findings.md`. C# refs and qualified
> names are stored and resolved with `::` today and cross-file C# qualified refs
> resolve correctly. This revision re-pins the spec on the corrected facts.

## What this is

A behavior-neutral refactor. Pass-2 reference resolution (`resolve.rs`) directly
embeds Rust-only module semantics (`resolve_module_path` with `crate`/`self`/`super`
handling and Cargo crate knowledge). This change extracts a per-language
`ModuleResolver` seam that owns module-path→file translation (including parsing its
own language's import-path format), so Rust semantics are contained in the Rust
implementation and future languages (tethys-8mze) implement their own without
editing `resolve.rs`. The C# implementation is an explicit, documented stub that
declines (preserving today's behavior, where C# import arms dead-end — tethys-jwf9).
`::` is documented as tethys's internal canonical qualified-name separator for all
languages; it is a cache format, not a display format.

## Users

- **tethys maintainer (dwalleck)**: future languages are added by implementing one
  trait; `resolve.rs` is no longer edited per language.
- **future language implementor** (tethys-8mze epic; jwf9 implementor): consumes the
  seam; the trait contract documents what a language impl must provide.
- **query consumers (CLI users, MCP/AI agents)**: observe NO change. Same symbols,
  same resolutions, same output spellings (still `::`), same performance ±10%.

## Behavior

### B1 — Rust resolution is byte-identical
- **Given**: a Rust workspace fixture (tethys repo self-index + existing test
  fixtures) indexed with the pre-change binary, refs/symbols tables dumped in
  stable sorted order
- **When**: the same workspace is indexed with the post-change binary and dumped
- **Then**: dumps are identical (`diff` exit 0)

### B2 — C# resolution is byte-identical
- **Given**: the probe's C# workspace shape (namespaces, using directives,
  cross-file qualified call `Hasher.Hash(...)`, nested type `new Outer.Inner()`),
  indexed pre-change: `call → Hasher::Hash` and `construct → Outer::Inner` resolved,
  `Console::WriteLine` unresolved
- **When**: indexed post-change and dumped
- **Then**: dumps are identical — same refs resolved to same symbols, same
  unresolved set, same `::` spellings

### B3 — The seam is real: no Rust module semantics in resolve.rs
- **Given**: the post-change source tree
- **When**: `rg 'resolve_module_path|CrateInfo|"crate"|"super"' src/resolve.rs` runs
- **Then**: zero matches — module-path→file translation and Rust path-prefix
  keywords are reachable only through the `ModuleResolver` trait impls

### B4 — Stub C# module resolver declines explicitly
- **Given**: a C# file with `using Some.Namespace;` and a reference relying on
  import-based arms
- **When**: import-based resolution arms run
- **Then**: the C# `ModuleResolver` declines (returns no file) by explicit
  documented implementation — no panic, no behavior change vs. today (B2 covers
  the observable equivalence)

### B5 — A language addition does not touch resolve.rs
- **Given**: the post-change tree and the docs for the trait
- **When**: a reviewer follows the "adding a language" checklist
  (`src/languages/mod.rs` doc comment, updated by this change)
- **Then**: the checklist's resolution step is "implement `ModuleResolver`" —
  no step instructs editing `resolve.rs`

## Success criteria

- **Strict neutrality, both languages**: sorted dumps of refs + symbols tables
  before/after differ by 0 lines on (1) tethys self-index, (2) existing Rust test
  fixtures, (3) the C# probe workspace. Measured by dump script + `diff`.
- **Suite**: `cargo nextest run` green; `cargo clippy` zero new warnings.
- **Perf**: indexing wall-time on tethys self-index within ±10% of baseline
  (median of 3 runs each).
- **Seam grep oracle**: B3's grep returns zero matches.

## Edge cases and decisions

| Edge | Decision | Rationale |
|---|---|---|
| Ref name with no separator (simple name) | Unchanged path | Covered by B1/B2 byte-identical oracles |
| Empty/degenerate segments | Safe decline, mirroring existing guard (resolve.rs:303) | Existing pattern |
| C# `Console::WriteLine`-style external refs | Stay unresolved, unchanged | B2 oracle |
| C# imports.source_module stored dotted (`MyApp.Models`) | Unchanged; each ModuleResolver parses its own language's import format — Rust splits `::`, C# stub receives the dotted string and declines | Import-format knowledge moves INTO the seam, not unified |
| Rust ref containing `.` | Confirmed impossible by probe (0/11,657) | Probe Q1 |
| Mixed-language workspace | Per-file language selects the resolver impl; cross-language refs unresolved (same as today) | Out of scope |
| Existing tethys.db | No storage format changes in this loop — DBs stay valid | Decision #5: storage untouched |
| K-hybrid `first_path_segment` (call_edges.rs:250) | Untouched | No storage change |
| LSP Pass 3 | Untouched — position-based | No name parsing involved |
| Concurrency / retries / multi-tenancy / timezone | N/A | Single-process batch indexing |
| Max scale | ±10% perf criterion on tethys self-index | Trait dispatch adds at most one vtable call per resolution attempt |

## Out of scope

This change does NOT include:
- C# namespace→file resolution (tethys-jwf9 — the stub stays a stub)
- ANY storage format change (C# qualified names stay `::`; revision-1's
  unification plan is dead — killed by probe findings)
- Per-language display spelling at the query/output boundary (possible future loop)
- Package/crate discovery seam (`cargo.rs` stays Rust-only)
- Architecture-phase support for non-Rust languages
- New languages (Python/TS/Go — tethys-8mze consumes this seam later)
- Fixing tethys-xvlw; LSP provider changes; DB schema changes

## Constraints

| Dimension | Limit | How measured |
|---|---|---|
| Resolution behavior (both languages) | byte-identical refs+symbols dumps | sorted dump + diff on 3 fixtures |
| Indexing wall-time | within ±10% of baseline | median of 3 runs, tethys self-index |
| Test suite | green, zero new clippy warnings | cargo nextest, cargo clippy |
| Backward compatibility | none required (decision #4) — but neutrality is a correctness choice, not a compat one | — |

## Decisions log

| # | Question | Decision | Why |
|---|---|---|---|
| 1 | What does "fixed" mean — seam only, measurable C# gain, or full ModuleResolver seam? | Full ModuleResolver seam | Requester selected; sets up tethys-8mze |
| 2 | Is C# namespace→file resolution (jwf9) in scope? | No — stub C# impl; jwf9 is a follow-up loop behind the seam | Two features in one spec is a decomposition trigger |
| 3 | Neutrality semantics per language? | ~~Rust strict, C# monotone~~ **SUPERSEDED by #5: strict for both** | Monotone criterion existed only because of the false premise; with canonical `::` and stub resolver, full strictness is achievable and stronger |
| 4 | May stored C# qualified names change? | Requester verbatim: "We have no existing users. We don't need backwards compatability" — but the unification INTENT is superseded by #5 | The no-compat constraint stands; the storage change it authorized is dead |
| 5 | (Post-probe re-decision) Separator policy, given C# already works on `::`? | Canonical `::` internally for all languages; loop is a pure behavior-neutral seam extraction; display spelling deferred | Probe falsified the bug; flipping storage+resolution in lockstep adds risk for zero resolution gain |

## Sign-off

Revision 1 sign-off (now superseded): "we're creating a proper seam for module
resolution so that rust symantics are contained in it's implementation and other
languages can implement appropriate module resolution per language" — 2026-06-06.

Revision 2 sign-off — the requester typed, verbatim: "extracts a per-language
ModuleResolver seam where the trait owns the file to path to file resolution"
(read as: the trait owns module-path→file resolution; matches "What this is").
Decisions #3-superseded and #5 were selected by the requester directly during
the post-probe re-interrogation.

Date: 2026-06-06
