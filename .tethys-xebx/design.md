# tethys-xebx falsifiable design — C# member declarations + member-read refs

2026-07-05. Stands on `.tethys-xebx/findings.md` (probe/oracle agreement:
42/42 properties, 2/2 fields, 4/4 `Data` reads; grammar ground truth for all
five node kinds on the pinned `tree-sitter-c-sharp` 0.23.1).

## Purpose

C# properties, fields, events, and delegates produce no symbols rows, and
standalone member reads (`result.Data`) produce no refs rows — so `[Obsolete]`
on the most common real-world carrier (a property) is invisible end to end
(probe2: 0 attribute rows, all-zero `deprecated-callers` on a corpus whose only
`[Obsolete]` sits on a property). Extract member declarations as symbols and
member-access reads as refs; everything downstream is already kind-agnostic.

## Core rule

**Extraction-only change.** `src/languages/csharp.rs` (+ enum plumbing in
`src/types.rs`, `src/languages/common.rs`, and compiler-forced match arms in
`src/db/helpers.rs`, `src/cli/stats.rs`, `src/cli/search.rs`). Zero changes to
the resolver arms, `deprecated-callers`, the schema, or the generic insert
path — verified empirically by `falsifier-c9.sh` (synthetic rows through the
real binary produced the exact predicted output).

## Decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | Kinds: new `SymbolKind::Property` / `Event` / `Delegate`; C# fields reuse existing `StructField` | Property/event/delegate have no honest existing analog; a class field is the same domain concept as `StructField` (precedent: C# record → `Class`, static method → `Function`) |
| D2 | Reads reuse declared-but-unused `ReferenceKind::FieldAccess` (wire `field_access`) via a new `ExtractedReferenceKind::FieldAccess` | The kind + its DB parse already exist (`types.rs:416,463`); CONTEXT.md's Reference definition already includes "field access" |
| D3 | Member reads are **excluded from `call_edges`** (`kind <> 'value'` → `kind NOT IN ('value','field_access')`) | A read is not a call; keeps `callers`/`impact`/coupling call-only (the `value`-refs precedent). deprecated-callers is unaffected — it reads `refs` directly |
| D4 | Unresolved `field_access` refs are **kept** (no `drop_unresolved_value_refs` analog) | They are always receiver-qualified (`result::Data`) and are exactly what deprecated-callers Path B consumes for Maybe sites; dropping them re-hides variable-receiver readers |
| D5 | Chained access emits **one ref per access level** (`response.Data.Name` → reads of `Data` and `Name`) | Fold-to-outermost hides `Data` in `result.Data.Length` — the probe's oracle disagreement caught precisely this |
| D6 | Member access consumed as an invocation **callee** emits no read (whole callee subtree, receivers of calls included: `a.B.M()` folds into the call ref as today); receivers that are themselves invocations (`Get().Data`) do emit their read | No double representation of one source expression |
| D7 | `reference_name` folds receiver segments exactly like invocations: `parse_member_access` → `result::Data` | Consistency with existing call refs; lands in the same Pass-1/Pass-2/Path-B machinery unchanged |
| D8 | `property_declaration` and `event_declaration` (accessor bodies) join the containing-span arm, so refs inside member bodies get `in_symbol_id` = the member symbol | `Data => Value` should attribute its `Value` read to `Data`; today accessor-body refs have `in_symbol_id` NULL |
| D9 | Reads inside attribute arguments and `nameof(...)` are emitted | They are references; `nameof` over-report (compiler suppresses CS0618 there) accepted and documented in tethys-5uqz |

## Input shapes (step 2)

Declarations — all in scope: accessor-block property, auto-property,
expression-bodied property (`=>`), property in class / struct / interface /
record / nested type, static and instance members (one kind each — no
static/instance kind split, unlike methods), field with one declarator, field
with multiple declarators (symbol per declarator), `const` / `static readonly`
fields (grammar makes no distinction — see cfme boundary below),
`event_field_declaration` (declarator-named, possibly multiple),
`event_declaration` (accessor form), `delegate_declaration` at namespace and
class level, attributes present/absent/multiple on any of the above.

Reads — in scope: receiver kinds measured in the corpus (`identifier` 851,
`member_access_expression` 26, `element_access_expression` 3,
`parenthesized_expression` 1), plus `this_expression` / `base_expression` /
invocation-result receivers (synthetic fixtures; absent from corpus), chained
access per level, reads on assignment LHS (`obj.Prop = x` — a use; emitted),
reads inside property/event accessor bodies, reads at top level of a file
(`in_symbol_id` NULL survives deprecated-callers by design).

Out of scope, each with a one-sentence reason and a verified tracker ID:
implicit-this bare-identifier reads, object-initializer assignments,
null-conditional `?.` (different node family), indexers (no simple name) — all
tethys-5uqz; `using static` bare-name reads and `enum_member_declaration`
symbols — tethys-cfme (whose field/member-access half this PR delivers; cfme
narrows to enum members + using-static disambiguation at close-out);
field/event-initializer containing-spans stay NULL (rare, no reader-site
consequence — sites still listed with `caller: null`).

## Removed-invariant sweep (step 2b)

The change is additive in code but subtractive in one respect: **it removes
the invariant that C# `name_to_id` / Pass-2 candidate sets contain only
type/method/namespace symbols.** New member symbols widen every name-keyed
candidate set:

- Same-file last-wins collisions (`files.rs:278`): a member symbol later in
  the file could steal call binds. Measured: zero same-file collisions in the
  corpus (probe1 members × index symbols join). Claim C11 audits pre/post;
  kind-aware binding tracked at tethys-0aqj.
- Cross-file unique-or-decline arms: a member named like a method elsewhere
  makes the union/unique arms decline calls that resolved before. C11's diff
  catches any corpus instance; tethys-0aqj tracks the kind-aware fix.
- `unused_imports` / used-name corroboration consumes ref names: new read refs
  can only ADD used-names (an import can flip unused→used, never the reverse)
  — strictly fewer false positives; safe, no claim.
- Rust extraction untouched (C#-only extractor change); full suite is the
  fence (C12).

## Claims

1. **C1** Every `property_declaration` (accessor-block, auto,
   expression-bodied; class/struct/interface/record incl. nested) produces
   exactly one symbols row, kind `property`, named by its `name` field —
   corpus count: exactly 42.
2. **C2** Every `field_declaration` produces one symbols row per
   `variable_declarator`, kind `struct_field` (const and static readonly
   included) — corpus count: exactly 2; a two-declarator fixture yields 2 rows.
3. **C3** `event_field_declaration` (per declarator) and `event_declaration`
   produce kind `event`; `delegate_declaration` produces kind `delegate` at
   both namespace and class level — synthetic fixture yields exactly
   {Changed, Renamed} events + {Transform, Nested} delegates.
4. **C4** `attribute_list`s on member declarations land as attributes rows on
   the member's symbol (multi-declarator field: one row per declarator symbol)
   — corpus: exactly 1 `Obsolete` row, attached to the kind=`property` `Data`
   symbol in `GenericResult.cs`.
5. **C5** Member symbols carry `qualified_name` = `EnclosingType::Member`
   matching the method convention — `Data` → `Result::Data`.
6. **C6** Every `member_access_expression` read outside invocation callees
   produces exactly one refs row, kind `field_access`, per access level, with
   folded `reference_name` — corpus count: exactly 881.
7. **C7** Reads whose bare member name is declared in the same file bind
   Pass-1 `same_file` — corpus: `FunctionalMethodsTests.cs:867` and
   `docs/TDD-EXAMPLE-MATCH-TESTS.cs:205` bind to their local
   `ApiResponse.Data`, and neither appears as a deprecated reader site.
8. **C8** Cross-file variable-receiver reads stay unresolved with qualified
   `reference_name` (strategy NULL) — corpus: `result::Data` at
   `BasicTests.cs:77`, `dataResult::Data` at `test-package.cs:23`.
9. **C9** `deprecated-callers --json` on the corpus reports exactly 1
   deprecated symbol (`Data`, kind `property`, note parsed, `error` null) with
   exactly those 2 Maybe sites via `unresolved-qualified` — and no others.
10. **C10** `call_edges` rows are identical pre/post on the corpus
    (`field_access` excluded; D8's `in_symbol_id` changes don't add edges
    because affected refs resolve to BCL/unresolved targets there).
11. **C11** Existing call/construct behavior is unchanged: pre/post diff of
    (file, line, kind, symbol_id, strategy) for kinds call/construct on the
    corpus is empty, except `in_symbol_id` gains on accessor-body refs (D8),
    which are enumerated and each inside a member body.
12. **C12** Rust behavior is untouched: the full existing test suite passes
    unmodified.
13. **C13** Reindexing the unchanged corpus is idempotent for member rows:
    second run produces identical symbols/refs/attributes counts and content
    for the new kinds.
14. **C14** Kind plumbing is total: `parse_symbol_kind` round-trips
    `property`/`event`/`delegate`, `search --kind property` returns the
    corpus properties, `stats` renders without panic.

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| C9 | end-to-end deprecated property | inject the exact rows the feature would produce into a real index; run real CLI; any deviation from predicted JSON falsifies | hand-derived site list from grep + C# semantics (`falsifier-c9.sh`) | 5m | **passed** (2026-07-05, output matched prediction exactly) | integration test `csharp_obsolete_property_reader_sites` (fixture embeds the decoy: local same-named non-deprecated property must NOT appear) |
| C5 | qualified_name convention | query method convention in real index (`Result::Combine`), assert property matches | existing DB rows, independent of new code | 2m | **passed** (convention verified 2026-07-05) | unit test `property_qualified_name_matches_method_convention` |
| C1 | 42 properties | index corpus; `COUNT(kind='property')` ≠ 42 falsifies | probe1 (independent tree-sitter walk) + grep-diagnosed list | 10m | pending (build S-audit) | unit tests `extracts_property_{auto,accessor,expression_bodied,nested,interface}` |
| C2 | fields per declarator | corpus count ≠ 2, or two-declarator fixture ≠ 2 rows falsifies | probe1 + grep (`_value`,`_error`) | 10m | pending | unit test `extracts_field_declarators_each` |
| C3 | events/delegates | synthetic fixture symbol set ≠ {Changed,Renamed,Transform,Nested} falsifies | probe1 run on `synthetic-members.cs` | 10m | pending | unit tests `extracts_event_{field,accessor}`, `extracts_delegate_{namespace,class}_level` |
| C4 | member attributes | corpus Obsolete rows ≠ 1 or wrong symbol falsifies; multi-declarator fixture missing a row falsifies | grep (exactly 1 `[Obsolete]` in corpus) | 10m | pending | integration `tests/attributes.rs::csharp_member_attribute_rows` (+ reindex survival, C13) |
| C6 | 881 reads per-level | corpus `field_access` count ≠ 881 falsifies; chain fixture emitting 1 ref for `a.b.C` falsifies | probe1 READ list (item-by-item joinable) | 10m | pending | unit tests `member_read_{simple,chained_per_level,skips_invocation_callee,assignment_lhs}` |
| C7 | same-file bind + exclusion | either decoy file's read resolving elsewhere, or appearing in deprecated output, falsifies | grep + C# name scoping (decoys declared same-file) | 15m | pending | integration `csharp_obsolete_property_reader_sites` (same fixture, distinct asserts) |
| C8 | cross-file reads unresolved-qualified | strategy ≠ NULL or reference_name ≠ `result::Data` falsifies | probe2 §J/K (invocation analog measured) | 15m | pending | integration `member_read_cross_file_stays_qualified` |
| C10 | call_edges identical | pre/post table diff non-empty falsifies | pre-feature snapshot (one-shot) | 20m | pending | integration `member_reads_produce_no_call_edges` (fixture: reader fn + property; assert edge count unchanged by adding the read) |
| C11 | call/construct unchanged | pre/post refs diff (beyond enumerated D8 `in_symbol_id` gains) non-empty falsifies | pre-feature snapshot (one-shot audit md) | 20m | pending | integration `call_resolution_unaffected_by_member_symbols` (fixture: cross-file call + same-named property; documents chosen semantics) |
| C12 | Rust untouched | any existing test failing falsifies | existing suite (written pre-feature) | 5m | pending | the suite itself |
| C13 | reindex idempotent | second-index diff non-empty falsifies | SQL snapshot diff | 10m | pending | extend `tests/attributes.rs` reindex fence to member rows |
| C14 | kind plumbing total | round-trip/search/stats failure falsifies | CLI on corpus + unit round-trip | 10m | pending | unit `symbol_kind_roundtrip_member_kinds` + CLI smoke in integration |

Cheapest falsifier (C9) **ran before this design was presented and passed**;
C5's convention check also ran and passed. One-shot audit results (C1, C6,
C10, C11) get recorded in `.tethys-xebx/audit.md` during the build; their
permanent forms are the named CI fences.

## Negative space

1. **No resolver changes.** Member reads ride the existing arms verbatim; no
   receiver-type inference, no loosening for variable receivers (tethys-53iv
   posture: conservative/narrow is correct), no kind-aware candidate filtering
   (tethys-0aqj).
2. **No call-graph participation.** Reads never create `call_edges`;
   `callers`/`impact`/coupling remain call-only. A future "readers" analysis
   would query `refs.kind='field_access'` directly.
3. **No read shapes beyond plain `member_access_expression`** — implicit-this,
   object initializers, `?.`, indexers: tethys-5uqz; `using static` bare
   reads + enum members: tethys-cfme.
4. **No Rust extractor changes** and no new analysis/output surface —
   `deprecated-callers` JSON schema is byte-identical in shape (tethys-zwaz
   unaffected).
5. **No static/instance kind split for members** (unlike method→Function/
   Method) — one `property` kind; `signature` carries the declaration text.

## Deferral index (tracker-verified)

- tethys-5uqz — remaining read shapes (filed from this design).
- tethys-0aqj — kind-blind binding, both facets (filed from this design).
- tethys-cfme — enum members + `using static` disambiguation (existing;
  narrows after this PR — update its description at close-out).
- tethys-53iv — receiver-type resolution precision (existing; posture adopted).
- tethys-9181 — obsolete constructors read Clean (existing; untouched here).
