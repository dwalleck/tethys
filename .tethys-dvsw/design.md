# tethys-dvsw — falsifiable design: `tethys dead-code`

Status: DRAFT (pending approval). Probe basis: `.tethys-dvsw/findings.md`.

## Purpose

Report symbols with zero inbound evidence — removal candidates — tiered
Definite/Maybe under the suppressions-not-accusations posture. Final Act 1
stage of the PRD (tethys-l6nt); Act 2's MCP surface is gated on this
shipping with a clean self-index oracle.

## Core rule (from probe evidence)

A symbol is reported iff it survives the whole evidence funnel; its tier
is decided by a textual scan:

1. **Candidate**: `visibility != 'public'` AND `is_test = 0` AND kind is
   analyzable for its file's language AND not an entry point.
2. **No resolved inbound ref** originating outside the symbol itself
   (`refs.symbol_id = s.id AND (in_symbol_id IS NULL OR in_symbol_id != s.id)`)
   — ANY band; a speculative-band bind suppresses (ADR-0003, transferred AC).
3. **No unresolved inbound name-match**: no refs row with `symbol_id IS
   NULL` and (`reference_name = s.name` OR `reference_name LIKE '%::'||s.name`),
   self-originated rows excluded as in 2.
4. **No method-level inherit marker**: no refs row `kind='inherit' AND
   in_symbol_id = s.id` (j2r1's dvsw suppression channel; works for
   external/unresolved traits because the marker row itself carries
   `in_symbol_id` regardless of `symbol_id`).
5. **No live descendant**: for container kinds, no transitive
   `parent_symbol_id` descendant with evidence 2-4 or `is_test = 1`.
6. **Tier**: zero word-boundary textual occurrences of `s.name` outside
   the symbol's own definition span (`line..=end_line` in its file),
   across ALL indexed files' on-disk content → **Definite**; otherwise
   **Maybe** (with the matching evidence kept as a demotion note).

Probe measured the funnel on the self-index: 820 candidates → 254
zero-evidence → 37 after kind exclusions → 37 Maybe / **0 Definite**,
agreeing with the rustc `dead_code` oracle in both directions (0 FP on
warning-free self-index; 3/3 seeded dead items found, decoy demoted).

### Kind analyzability (language-aware, via `files.language`)

- **Rust**: function, method, struct, enum, trait, type_alias, const,
  enum_variant. Excluded: `module` (path segments never emit refs),
  `struct_field` (Rust field reads emit no refs — `field_access` is 0 on
  a pure-Rust index; 427 guaranteed FPs otherwise).
- **C#**: class, interface, struct, method, property, event, delegate,
  function, struct_field, enum. Excluded: `module` (namespaces,
  implicitly public anyway). C# data members ARE candidates — unlike
  Rust, `member_access_expression` reads emit `field_access` refs
  (tethys-xebx); the shapes still invisible (implicit-this, initializers,
  `?.`, indexers — tethys-5uqz) are absorbed into Maybe by the textual
  channel.

### Entry points (explicit, not luck)

Probe finding #4: `main` survived only because 203 unrelated textual hits
exist; a single-bin workspace's `main` would be a Definite FP. Rule:
- Rust: `kind='function' AND name='main'` in `src/main.rs`, `src/bin/*.rs`,
  `examples/*.rs`, or `build.rs` (workspace-relative, per-crate).
- C#: `kind='method' AND name='Main'` anywhere (documented
  over-suppression; conservative direction is suppression).

### Output surface

- `src/db/dead_code.rs`: `DeadCodeReport { findings: Vec<DeadCodeFinding>,
  summary: DeadCodeSummary }`; finding = name, qualified_name, kind,
  visibility, file, line, tier; sorted (file, line). Summary = candidates,
  definite, maybe (full counts, never truncated).
- Facade: `Tethys::find_dead_code(limit: Option<usize>) -> Result<DeadCodeReport>`
  — the seam o4re's `tethys_dead_code` MCP tool wraps (verified: listed in
  o4re's v2 tool table).
- CLI: `tethys dead-code [--limit N] [--json]`, envelope mirroring
  untested-code's `{findings, summary}` JSON shape. `--limit` truncates
  findings after sorting; summary keeps full counts. Zero candidates =
  empty findings + zero summary, exit 0 — a legitimately clean report,
  NOT Indeterminate (unlike y3bx there is no root-set precondition; the
  candidate set itself is the population).
- Module docs list known FP sources by ID: tethys-9l27, tethys-0nar,
  tethys-7dqj, tethys-ewa7, tethys-wbrh, tethys-i09d, tethys-0aqj
  (all textual-absorbed to Maybe on the self-index; documented not fixed).

## Input shapes

- Symbol kinds: full Rust and C# kind lists above, incl. excluded ones
  (C1, C10). Unicode identifiers: out of scope — word-boundary class is
  `[A-Za-z0-9_]`; neither indexed language population uses unicode idents,
  and a boundary miss shifts tier toward Maybe (safe direction).
- Visibility: public / crate / module / private (C1); C# internal→crate
  (C10).
- is_test: 0 and 1 (C1).
- Inbound evidence: resolved high/medium/speculative (C2), resolved
  self-only (C2), unresolved bare and `::`-qualified (C3), inherit marker
  with resolved AND unresolved trait (C4), none (C6-C8).
- Containers: with live descendant (direct and transitive/nested), without
  (C5, C8, C10).
- Entry points: Rust main variants per path, C# Main (C9).
- Limit: absent, 0, N < total (C11).
- Workspace: rust-only, C#-containing, empty/zero-candidate (C7-C11).

## Subtractive sweep

Purely additive: a new read-only query module + facade method + CLI
subcommand; no lock, guard, ordering, or uniqueness property is removed,
and no existing code path changes semantics. The only mutation risk is
incidental refactoring of shared helpers — pinned by C13.

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| C1 | Only non-public, non-test, analyzable-kind, non-entry-point symbols can appear | Fixture: pub dead fn, `#[test]` fn, Rust struct_field, module decl, `src/main.rs` main — none reported; private dead fn control IS | Fixture is ground truth by construction | fixture | pending | `tests/dead_code.rs::candidacy_filters` |
| C2 | Any resolved non-self inbound ref suppresses, incl. speculative-band-only; a self-recursive ref does NOT | Fixture: name-unique unknown-receiver method call (speculative bind) → absent; recursive otherwise-dead fn → reported | `refs_banded` band values + rustc flags recursive fn | fixture | pending | `::speculative_suppresses_and_self_ref_does_not` |
| C3 | Unresolved name-match suppresses, bare AND `::`-suffix shapes | Fixture: ambiguous same-named methods (decline→bare) + qualified-decline shape → all absent | DB shows both shapes exist (14292 bare / 1920 qualified unresolved) | fixture | pending | `::unresolved_bare_and_qualified_suppress` |
| C4 | Trait-impl methods are marker-suppressed even for external traits | Fixture: `impl Display for T { fn fmt }` → fmt absent | Buggy impl joining `symbol_id` (NULL for external) instead of `in_symbol_id` fails | fixture | pending | `::trait_impl_marker_suppresses_external_trait` |
| C5 | Containers with a live (evidence or is_test) transitive descendant are suppressed | Fixture: struct whose only method is called → struct absent; dead struct control present | Hand-built fixture | fixture | pending | `::container_live_descendant` |
| C6 | Zero-ref candidate with any textual occurrence outside its own span tiers Maybe, never Definite | Fixture: fn mentioned only in ANOTHER file's macro arg → Maybe | Probe: 37/37 absorbed incl. format-string captures | fixture | pending | `::macro_only_mention_is_maybe` |
| C7 | Definite on the warning-free tethys self-index is EMPTY | Ran: probe2 funnel on fresh self-index | rustc `dead_code` (warning-free ⟹ ∅) | **ran** | **passed (0)** | `::self_index_zero_definite` (CI, the PRD fence) |
| C8 | Seeded unmentioned dead items (fn, struct, const, recursive fn) are Definite | Ran (probe, seeded copy): 3/3 exact, decoy → Maybe; recursive variant added to fixture | `cargo check` dead_code warnings on the seeded source | **ran** | **passed (3/3)** | `::seeded_dead_items_definite` |
| C9 | Entry points never reported even with zero textual hits | Fixture: bin-only crate, unmentioned main → absent | rustc doesn't flag main; probe finding #4 shows textual won't save it | fixture | pending | `::entry_points_excluded` |
| C10 | C# flows the same funnel: fields/properties candidates, internal→crate candidacy, nested-class recursion, Main excluded | Fixture: internal class w/ dead private method (Definite), used method (absent), nested class (suppressed via descendant), Main | Hand-built fixture; visibility mapping read from `extract_visibility` | fixture | pending | `tests/dead_code.rs::csharp_funnel` |
| C11 | CLI: sort (file,line); limit truncates findings only; JSON `{findings,summary}`; zero-candidate → empty + exit 0 | Fixture: assert order, `--limit 1`, parse JSON, empty workspace | serde round-trip + fixture | fixture | pending | `::cli_json_limit_sort_empty` |
| C12 | Byte-identical output across consecutive runs on same index | Run twice, diff | diff | 5m | pending | `::deterministic_output` |
| C13 | No existing analysis output changes | Run unused-imports, visibility-tightening, untested-code, deprecated-callers, panic-points on same index at branch base vs head; diff empty | Old binary's output | 15m | pending | existing suites (deterministic); diff recorded in audit trail |

Cheapest falsifiers (C7, C8) ran at probe stage and passed — recorded
above. Per-claim fences are distinct named tests; a failure localizes.

Non-vacuity spot-checks: C2's fence fails under "copy callers'
`--exclude-speculative` default"; C4's under the `symbol_id` join bug;
C7's under "drop the textual channel" (37 known FPs reappear); C8 kills
the report-nothing implementation that trivially passes C7 — C7+C8 are
the two-sided oracle pair.

## Negative space (deliberately not doing)

1. **Public symbols are never reported** — no `--workspace-closed` lift:
   composition with visibility-tightening (shrink pub first, then
   dead-code sees it) covers the closed-workspace deletion workflow.
   Settled rationale, not deferred work.
2. **Read-only**: reports candidates; never deletes or suggests edits
   (PRD: tethys never edits).
3. **No macro expansion / cfg evaluation**: macro-hidden uses are handled
   solely by textual demotion; residual gaps are the documented FP issues
   (tethys-9l27, tethys-0nar, tethys-7dqj, tethys-ewa7).
4. **No C# method-level interface-impl suppression** — requires override
   resolution, tracked at tethys-3b06; C# gets type-level inherit edges
   only, and interface-impl method names textual-match their interface
   declaration → Maybe, not Definite.
5. **No `--exclude-speculative` flag**: bands are suppressions here by
   the transferred AC — the opposite posture from callers' precision
   tier, on purpose.
6. **No test-only-liveness distinction**: a symbol referenced only by
   tests is alive; the policy question is tracked at tethys-m7zm.
7. **No MCP tool in this PR**: `tethys_dead_code` ships with the MCP
   server, tracked at tethys-o4re (verified listed in its v2 tool table);
   `find_dead_code` is the facade seam it will wrap.

## Open decisions (flagged for approval)

1. C# data members as candidates (rec: yes — refs channel exists;
   asymmetry with Rust struct_field is substrate-justified).
2. Entry-point rule as specified, incl. any-`Main` C# over-suppression
   (rec: accept).
3. Self-recursion exclusion — recursive otherwise-dead fn IS reported
   (rec: yes, matches rustc).
4. Default unlimited findings, `--limit` optional (rec: y3bx report-all
   precedent; issue's `LIMIT ?` sketch predates it).
5. Tier names `Definite`/`Maybe` (deprecated-callers precedent).
