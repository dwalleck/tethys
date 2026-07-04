# tethys-haw5 design: C# parity for deprecated-callers (falsifiable-design, 2026-07-03)

Probe artifacts this design stands on: `probe1.sh`–`probe3.sh`, `findings.md`,
`baseline-rand-deprecated.json` (frozen pre-change Rust JSON). The design does not
contradict any probe result; one probe conclusion was *refined* at design time —
see Path B under Architecture.

## Purpose

Extend the deprecated-callers analysis to C#: extract attributes for C# symbols
during parsing (all attributes, matching Rust's store-everything posture), detect
`[Obsolete]`-marked symbols, and surface their call sites through the same CLI
subcommand with the same table/JSON shape. Scoped per AC1 to methods/types;
member declarations are tethys-xebx.

## Architecture (extension points, all existing)

1. **Extraction** — `src/languages/csharp.rs`: replace the four hardcoded
   `attributes: Vec::new()` sites with a walk of `attribute_list` → `attribute`
   children (the AST pattern `has_test_attribute` already uses in production),
   capturing `name` field text as written and `attribute_argument_list` inner
   text (parens stripped, whitespace preserved) into `ExtractedAttribute
   { name, args, line }`. Grammar precondition verified against the pinned
   tree-sitter-c-sharp 0.23.1: `attribute`, `attribute_argument_list`,
   `attribute_target_specifier` are named nodes; a target specifier is a
   *sibling* of the name inside `attribute`, so `[method: Obsolete]` extracts
   identically.
2. **Storage** — none. The write path is language-neutral
   (`indexing.rs:688` → `db/files.rs:237`); populating the vec suffices.
3. **Detection** — `src/db/deprecated.rs`: widen `WHERE a.name = 'deprecated'`
   (2 places) to also match the four Obsolete spellings (`Obsolete`,
   `ObsoleteAttribute`, `System.Obsolete`, `System.ObsoleteAttribute` — stored
   as written, matched at query time). Args parsing dispatches on attribute
   name: `deprecated` → existing `parse_deprecation_args`; Obsolete spellings →
   new `parse_obsolete_args` (first string literal → note; bool literal or
   `error:`-named bool → error flag; total function, degrades to nulls).
4. **Path B (probe refinement)** — `get_deprecated_callers` already attaches
   unresolved refs whose `reference_name` ends in `::<name>` as Maybe sites.
   C# refs use `::` (`result::GetValueOrDefault`), so variable-receiver
   instance calls surface automatically as Maybe — findings.md's "never
   listed" was true only of *resolved* sites. Design-time falsifier run
   confirmed the mechanism 19/19 against the grep oracle (claim 8).
5. **Language guard (invariant repair)** — Path B and the ambiguity CTE gain a
   same-language restriction (join `files.language` of the ref/symbol). See
   removed-invariant sweep.
6. **Output** — `DeprecatedSymbol` gains `error: Option<bool>` (serialized as
   null for Rust); human render appends the flag to `deprecation_meta` when
   true. `main.rs:157` help text drops "out of scope pending tethys-haw5".

## Input shapes (step 2)

**Attribute occurrence on an extracted symbol** — none; bare `[Obsolete]`;
`[Obsolete("msg")]`; `[Obsolete("msg", true)]` / `("msg", false)`;
named-arg forms (`message:`, `error:`); suffix/qualified spellings
(`ObsoleteAttribute`, `System.Obsolete`, `System.ObsoleteAttribute`);
multiple attributes per list `[Obsolete, Serializable]`; stacked lists;
decoy attributes (`Serializable`, `Test`, custom `NotObsolete`); target
specifier `[method: Obsolete]`; message content: escaped quotes, Unicode,
commas inside strings, verbatim `@"..."` (degrades to raw passthrough,
never wrong attribution — same posture as Rust `r#".."#`).

**Decorated symbol kind** — class, struct, interface, enum, record (detection
is kind-agnostic), static method (`function`), instance method (`method`),
nested type member, constructor. Constructor entries list but their sites bind
the type symbol → misleading Clean verdict, filed as **tethys-9181** (bug,
both languages, predates this slice). Properties/fields/events/delegates: not
extracted as symbols at all — out of scope here, tracked at **tethys-xebx**.
Local functions: not extracted, and deprecating non-public locals has no API
surface — out of scope, no ticket needed (settled rationale).

**Call-site shape for an obsolete symbol** — static-receiver invocation
(resolves); variable-receiver instance call (unresolved `recv::Name` → Path B
Maybe); generic receiver `Result<int>::Combine` (unresolved → Path B Maybe);
object creation `new T()` (construct ref, resolves conservatively);
same-named non-obsolete symbol in index (demotes to Maybe); same-named BCL
call (`HashCode::Combine` — Path B false-positive Maybe, accepted: honest
under Maybe = "verify by hand" semantics); zero callers (Clean bucket).

**Workspace shape** — pure C#; pure Rust (regression); mixed (merged report);
C# with zero `[Obsolete]` (empty envelope).

## Removed-invariant sweep (step 2b)

The core move is subtractive underneath: it deletes two "can't happen" facts.

1. **"The attributes table contains only Rust rows."** Consumers: the
   deprecated queries (`name = 'deprecated'` — untouched by C# rows except the
   deliberate widening) and the per-file delete cascade (`db/files.rs`,
   language-neutral). No other consumer exists (grepped). C# test-framework
   attributes (`Fact`, `Test`) now land as rows; nothing dispatches on them
   (`is_test` is computed at extraction, not from this table). **Safe.**
2. **"The deprecated symbol set contains only Rust symbols."** Consumers that
   silently relied on it:
   - **Path B** ran over *all* unresolved `::` refs but attached nothing
     non-Rust because the deprecated set was Rust-only. Widening the set makes
     cross-language attachment reachable both ways (a Rust `crate::Run` ref
     onto a C# obsolete `Run`, and vice versa) — phantom by construction,
     since neither language calls the other in-workspace. **Broken → claim 9.**
   - **Ambiguity CTE** counts same-named non-deprecated symbols with no
     language restriction; a C# `Combine` already demotes a Rust deprecated
     `Combine` today (latent jdly bleed), and widening makes it bidirectional.
     **Broken → claim 9** (the guard also fixes the latent direction).
   - **CLI display** consumes `kind` as display-only text (documented on
     `DeprecatedSymbol.kind`); C# kinds render as-is. **Safe.**
   - **Test C11** (`tests/deprecated_callers.rs:382`) asserts C# yields no
     findings — a gap fence, not a behavior contract; it flips into the
     positive fixture. **Intended.**

## Claims

1. Indexing a C# file stores one `attributes` row per attribute on each
   extracted symbol — name as written, args = raw inner text of the argument
   list (NULL when absent), line = the attribute's own line — matching the Rust
   row shape.
2. Bare `[Obsolete]` stores args NULL and reports `note: null, error: null`.
3. `[Obsolete("msg")]` and `[Obsolete(message: "msg")]` report note = the
   unquoted, unescaped message; commas inside the string never split parsing.
4. A bool argument (positional or `error:`-named) reports `error` = that bool;
   absent bool reports `error: null`; `false` is reported as `false`, not null.
5. Exactly the four Obsolete spellings are detected; all other attributes
   (including a custom `NotObsolete`) are stored but never reported.
6. An obsolete static method invoked as `Type.Method()` cross-file lists those
   sites as resolved, tier Definite iff no same-language same-named
   non-obsolete symbol exists; an uncalled obsolete symbol lands in Clean.
7. An obsolete class's `new T()` construction sites are listed as resolved
   sites of the class entry.
8. Variable-receiver instance calls (stored unresolved as `recv::Name`)
   attach as tier=Maybe, via=unresolved-qualified sites.
9. Path B attachment and ambiguity demotion are same-language only.
10. Symbol JSON objects carry the identical key set {name, kind, file, line,
    since, note, error} in both languages (since null for C#, error null for
    Rust).
11. On rand-0.8.5, post-change `--json` differs from
    `baseline-rand-deprecated.json` only by the added `"error": null` keys.
12. A C# workspace with zero `[Obsolete]` yields the empty envelope (summary
    zeros, `deprecated: []`, exit 0).
13. A mixed workspace reports both languages' findings in one envelope,
    summary counts summing both.
14. Re-indexing an unchanged workspace leaves attribute rows identical
    (count and content — no duplicates, no loss).

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| 1 | C# attribute rows match Rust shape | grammar: attr nodes exist in pinned 0.23.1 (ran); fixture with stacked/multi/target-specifier attrs → SQL dump | node-types.json grep (ran); hand-read of fixture source text | 5m + 20m | grammar passed; fixture pending | new integration test `csharp_attribute_rows_match_source` (tests/attributes.rs). Buggy impl caught: first-list-only walk; args stored with parens; symbol line stored as attr line |
| 2 | bare `[Obsolete]` → nulls | fixture symbol with bare attr | JSON fields `note`/`error` both null | 10m | pending | assert in unit table `parses_all_obsolete_args_shapes` (db/deprecated.rs). Buggy: empty-string args stored → `Some("")` |
| 3 | message parsing | unit table: positional, named, comma-in-string, escapes, Unicode, verbatim | hand-written expected values (same style as `parses_all_deprecated_args_shapes`) | 10m | pending | same unit table. Buggy: naive comma-split (killed by `"a, b"`); `message:` treated as unknown key |
| 4 | error flag | unit table rows + fixture `("m", false)` | hand-written expected; JSON field | 10m | pending | same unit table. Buggy: any-second-arg→true (killed by explicit `false`) |
| 5 | four spellings, decoys never | fixture: 4 variants + `[Serializable]` + custom `[NotObsolete]` | report lists exactly 4 symbols by name | 15m | pending | integration test `obsolete_spelling_variants`. Buggy: `LIKE '%Obsolete%'` (killed by NotObsolete decoy); exact-'Obsolete'-only (killed by variants) |
| 6 | resolved static sites + Clean bucket | AC1 fixture (own index, tmpdir): static method w/ cross-file callers + uncalled obsolete method | grep of fixture source, item-by-item file:line | 20m | pending | flipped C11 test (tests/deprecated_callers.rs). Buggy: joining call_edges (drops top-level sites); over-wide language guard (kills same-language sites) |
| 7 | construction sites listed | fixture: obsolete class + `new T()` elsewhere | grep `new LegacyService` | 15m | pending | same integration file, distinct test. Buggy: filtering refs to kind='call' only |
| 8 | Path B Maybe recovery | SQL sim on real Tethys.Results index: unresolved `%::GetValueOrDefault` | probe2 grep oracle (19 sites) | 5m | **passed** (19/19, design time) | fixture instance-method test asserting tier=Maybe via=unresolved-qualified. Buggy: Path B gated to Rust; requiring resolution |
| 9 | language guard | mixed fixture: Rust unresolved `crate::Run` ref + C# obsolete `Run` (and mirrored) | fixture construction (site file extensions) | 30m | pending — **fails today by design** (guard-less code is the non-vacuity witness) | integration test `no_cross_language_attachment`. Buggy: today's code verbatim |
| 10 | identical JSON key set | run `--json` on C# and Rust fixtures; compare sorted key sets of symbol objects | `jq keys` equality — external tool | 10m | pending | binary-level fence extension (AC1/AC2 fence file). Buggy: `skip_serializing_if` on error |
| 11 | Rust regression modulo `error: null` | index rand-0.8.5 post-change, normalized diff vs frozen baseline | `baseline-rand-deprecated.json` (pre-change binary output) + `jq del(...error)` diff | 10m | pending (baseline frozen) | existing idempotency + field asserts keep passing; add key-set assert. Buggy: Obsolete parser applied to 'deprecated' args (corrupts note/since) |
| 12 | empty envelope on clean C# | fixture with methods, no attributes | JSON: summary zeros, `deprecated: []`; exit 0 | 10m | pending | integration test `csharp_no_obsolete_empty_report`. Buggy: detection matching test-framework attrs |
| 13 | mixed workspace merge | fixture: 1 Rust deprecated + 1 C# obsolete | symbol_count == 2, one entry per language | 20m | pending | integration test `mixed_workspace_merged_report`. Buggy: per-language early return / UNION drop |
| 14 | idempotent reindex | index twice, dump attributes both times, diff | SQL dump diff (empty) | 10m | pending | extend existing `first==second` JSON idempotency test with attribute-row count. Buggy: INSERT without per-file delete cycle |

Cheapest falsifier (claim 8, 5m): **run and passed** before this design was
written — 19/19 sites, item-for-item against the probe2 grep oracle. Claim 1's
grammar precondition also ran and passed against the pinned 0.23.1 sources.

Per-claim distinctness: each falsifier is a distinct test/fixture or a distinct
assert row in a named unit table; no two claims share a single yes/no output.

## Negative space (what this slice deliberately does not do)

1. **No property/field/event/delegate support** — not extracted as symbols;
   tracked at **tethys-xebx** (verified, updated to cover all four).
2. **No resolution loosening** — variable/generic-receiver calls stay
   unresolved and surface only as Maybe via Path B; widening resolution is
   forbidden by the ticket's error posture.
3. **No constructor-site fix** — obsolete constructors list but read Clean;
   filed as **tethys-9181** (bug, discovered during shape enumeration).
4. **No DiagnosticId/UrlFormat surfacing** — the raw `args` column preserves
   them verbatim; surfacing is display-only and appears in no AC (settled
   rationale, re-parseable from stored args without reindexing).
5. **No cross-assembly awareness** — `[Obsolete]` in NuGet/BCL dependencies is
   invisible; the indexer only parses workspace source (settled scope of the
   tool, same as Rust's posture toward crates.io deps).

## Open decisions resolved (flagged for approval)

- **`error` field added to all symbol JSON** (null for Rust): keeps the key
  set identical across languages (AC4) at the cost of one always-null key in
  Rust output. Existing fences assert individual fields, not exact key sets —
  verified; blast radius is the intended fence updates only.
- **Language guard (claim 9) also alters mixed-workspace Rust output** — it
  removes phantom cross-language Maybe sites that jdly could emit today.
  Pure-Rust workspaces are unaffected (AC5 regression holds on rand-0.8.5,
  claim 11). Folded in because shipping without it makes the new C# rows
  actively degrade Rust findings in mixed workspaces.
