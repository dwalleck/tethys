# tethys-xebx budgeted plan — C# member declarations + member-read refs

2026-07-05. Implements the approved `.tethys-xebx/design.md` (D1: fields reuse
`struct_field`; D3: `field_access` excluded from `call_edges` — both confirmed
by dwalleck at the design pause). Baselines for C10/C11 captured pre-feature
at plan time: `baseline-call-edges.txt` (300 edges),
`baseline-call-construct-refs.txt` (4116 refs),
`baseline-refs-in-symbol.txt` (D8 enumeration).

Corpus = the Tethys.Results copy in scratchpad; audits re-index it with
`--rebuild`. Every slice gate: `cargo nextest run`, clippy pedantic
`-D warnings`, `cargo fmt --check`, doctests — real exit codes, no pipes.

## Slice 1: `SymbolKind` member variants + DB round-trip

**Claim:** C14 (kind plumbing total — enum half).
**Oracle:** unit round-trip through the same string the DB stores; falsifier-c9
already proved downstream tolerates the strings.
**Stress fixture:** round-trip test over ALL `SymbolKind` variants (not just
new ones) via `as_str` → `parse_symbol_kind`; bug class = a new variant whose
wire string parses back to an error/mismatched variant (the `Unknown`-style
silent fallback).
**Loop budget:** none (match arms).
**Files:** `src/types.rs`, `src/db/helpers.rs`

Add `Property`, `Event`, `Delegate` variants (wire: `property`, `event`,
`delegate`); C# fields map to existing `StructField` (D1) — document that on
the variant's doc comment. Compiler forces the `parse_symbol_kind` arm.

**Verification:**
- [ ] Unit tests pass (`symbol_kind_roundtrip_all_variants`)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees (round-trip == stored string set)
- [ ] Budgets hold (n/a)

## Slice 2: CLI kind wiring (stats display + search --kind)

**Claim:** C14 (CLI half).
**Oracle:** CLI smoke on a fixture workspace — output text, independent of the
enum internals.
**Stress fixture:** `search --kind property` on a fixture that also contains a
same-named method (bug class: kind filter not actually applied, returns both).
**Loop budget:** none.
**Files:** `src/cli/stats.rs`, `src/cli/search.rs`

Display names + `parse_kind` arm + `--kind` help text.

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees
- [ ] Budgets hold (n/a)

## Slice 3: property symbol extraction

**Claim:** C1 (property symbols), C5 (qualified_name), C4 (attribute
auto-wiring, property half).
**Oracle:** probe1's independent tree-sitter walk (42 corpus properties;
item-by-item list committed in `probe1-output.txt`); grep for the attribute.
**Stress fixture:** one file, two classes each declaring a property named
`Data` (name-collision class); plus expression-bodied with `[Obsolete]`,
auto-property, accessor-block, interface property, property in a NESTED class
(bug classes: expression-bodied arm missed; nested `declaration_list` not
re-entered; `qualified_name` built from outermost type instead of enclosing).
Expected written first: 6 symbols, each `parent::name` distinct, 1 attribute
row on the expression-bodied one.
**Loop budget:** O(AST nodes) per file — existing traversal, no new loop
class; corpus ≈ 31 files × ~3k nodes ≈ 10^5 ops.
**Files:** `src/languages/csharp.rs` (extractor + inline unit tests)

New `PROPERTY_DECLARATION` const; `extract_property` sibling of
`extract_method` (name field, `extract_attributes(node)`, signature = first
line of declaration text, kind `Property`); arms in BOTH
`extract_symbols_recursive` and `extract_class_members`.

**Verification:**
- [ ] Unit tests pass (`extracts_property_*` family)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees (corpus count 42 — checked again in Slice 11 audit)
- [ ] Budgets hold

## Slice 4: field / event / delegate symbol extraction

**Claim:** C2 (fields per declarator), C3 (events + delegates both levels),
C4 (multi-declarator attribute fan-out).
**Oracle:** probe1 on corpus (2 fields) and on `synthetic-members.cs`
(committed; expected set {Changed, Renamed} events, {Transform, Nested}
delegates, {Max, Tag} fields).
**Stress fixture:** `[Obsolete] public int A, B;` → exactly 2 `struct_field`
symbols, 2 attribute rows (bug class: one symbol per declaration instead of
per declarator; attribute attached to first declarator only); delegate at
namespace level AND class level (bug class: only `extract_class_members` arm
added, namespace-level delegate falls through `extract_symbols_recursive`).
**Loop budget:** O(declarators) nested in the node walk — bounded by node
count, same 10^5 class.
**Files:** `src/languages/csharp.rs`

Declarator expansion helper (shape exists in probe1's `collect_declarators`);
`event_field_declaration` shares it; `event_declaration` and
`delegate_declaration` use the `name` field.

**Verification:**
- [ ] Unit tests pass (`extracts_field_declarators_each`, `extracts_event_{field,accessor}`, `extracts_delegate_{namespace,class}_level`)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees (synthetic set exact)
- [ ] Budgets hold

## Slice 5: member-read reference extraction

**Claim:** C6 (per-level `field_access` refs, callee-skip, folded names).
**Oracle:** probe1 READ list (881 on corpus, item-by-item joinable on the
`Data` slice vs grep).
**Stress fixture:** written-first expected ref sets for: `a.b.C` chain → 2
refs (`a::b::C`, `a::b`); `a.B.M()` → 0 field_access (whole callee folds into
the call, as today); `Get().Data` → 1 read + 1 call; `x.P = 1` (assignment
LHS) → 1 read; file with zero member access → 0 (empty path). Bug classes:
fold-to-outermost (the probe's own caught bug), double-emit under invocation
callees, LHS skipped.
**Loop budget:** per-level emission is O(chain depth) summed over chain nodes
= O(AST nodes); corpus 881 emissions over ~10^5 nodes.
**Files:** `src/languages/csharp.rs`, `src/languages/common.rs`

`ExtractedReferenceKind::FieldAccess` → existing `ReferenceKind::FieldAccess`
in `to_db_kind` (types.rs parse arm already exists — verified). New
`MEMBER_ACCESS_EXPRESSION` arm mirrors probe1's traversal shape (probe1 is the
reference implementation of the walk, already oracle-validated).

**Verification:**
- [ ] Unit tests pass (`member_read_{simple,chained_per_level,skips_invocation_callee,assignment_lhs,none}`)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees (Data slice 4/4; full 881 in Slice 11)
- [ ] Budgets hold

## Slice 6: containing spans for accessor bodies (D8)

**Claim:** C11's enumerated `in_symbol_id` gains; reads/calls inside property
and event accessor bodies attribute to the member symbol.
**Oracle:** `baseline-refs-in-symbol.txt` diff — every changed row must be
inside a member body (hand-checkable list, expected small on corpus).
**Stress fixture:** `int X => Helper();` and a get-block with an
`object_creation` — both inner refs get `in_symbol_id` = `X` (bug class: span
captured for the declaration but children visited with the OLD span; or
expression-bodied arrow body not covered).
**Loop budget:** none new (span threading in existing recursion).
**Files:** `src/languages/csharp.rs`

`PROPERTY_DECLARATION | EVENT_DECLARATION` join the span-capture arm alongside
`METHOD_DECLARATION`. Field/event-initializer spans stay NULL (design: rare,
sites still listed with `caller: null`).

**Verification:**
- [ ] Unit tests pass (`accessor_body_refs_attribute_to_member`)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees (baseline diff = enumerable accessor-body rows only)
- [ ] Budgets hold (n/a)

## Slice 7: call_edges exclusion (D3)

**Claim:** C10 (reads never create call edges).
**Oracle:** `baseline-call-edges.txt` (300 edges) — post-feature diff empty.
**Stress fixture:** fixture where a member read RESOLVES same-file (resolved +
`in_symbol_id` set = exactly the shape `populate_call_edges` selects; bug
class: exclusion forgotten and the resolved read mints an edge) alongside a
real call (edge must still exist — proves the filter isn't over-broad).
**Loop budget:** none (SQL predicate change).
**Files:** `src/db/call_edges.rs`, `tests/graph.rs`

`kind <> 'value'` → `kind NOT IN ('value','field_access')` with the doc
comment updated to name both excluded kinds and why.

**Verification:**
- [ ] Unit tests pass (`member_reads_produce_no_call_edges`)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees (corpus edge diff empty — Slice 11)
- [ ] Budgets hold (n/a)

## Slice 8: deprecated-callers end-to-end fences

**Claim:** C7 (same-file decoy exclusion), C9 (property surfaces with reader
sites), plus a Definite-tier resolved read.
**Oracle:** hand-derived site lists written in the test BEFORE running
(fixture mirrors the corpus shape: `[Obsolete]` property `Data`, decoy class
with own `Data` + same-file read, cross-file variable-receiver read; second
`[Obsolete]` static property with a UNIQUE name + type-receiver read →
`qualified_exact` → Definite tier).
**Stress fixture:** the decoy IS the stress (bug class: kind-blind Path B
listing the decoy read; same-file bind failing and the decoy leaking into
Maybe sites). Tier expectations: `Data` sites Maybe (ambiguous name), unique
static property site Definite.
**Loop budget:** none (test only).
**Files:** `tests/deprecated_callers.rs`

**Verification:**
- [ ] Unit tests pass (`csharp_obsolete_property_reader_sites`, `csharp_obsolete_static_property_definite_site`)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees (site sets match hand-derived lists)
- [ ] Budgets hold (n/a)

## Slice 9: resolution-behavior fences

**Claim:** C8 (cross-file reads stay unresolved-qualified), C11 fence
(call resolution vs new member symbols — pins the conservative decline,
cites tethys-0aqj).
**Oracle:** direct SQL over the fixture DB (`open_db` harness), asserting
`strategy IS NULL` + `reference_name = 'result::Data'` (assert by column
values, not `refs_named` — resolved refs null their `reference_name`).
**Stress fixture:** (a) cross-file read of a property that IS unique in the
workspace — bug class: `unique_workspace` arm binding a variable-receiver
qualified name it shouldn't (qualified names skip the union/unique simple-name
arms; if it binds, conservatism was lost); (b) cross-file call `Work()` where
a method `Work` and a property `Work` both exist behind the same using — pins
union-arm decline (bug class: property candidate silently winning instead).
**Loop budget:** none (tests only).
**Files:** `tests/value_refs.rs`-style new file `tests/member_reads.rs`

**Verification:**
- [ ] Unit tests pass (`member_read_cross_file_stays_qualified`, `call_resolution_with_member_symbol_declines_ambiguous`)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees (SQL columns match)
- [ ] Budgets hold (n/a)

## Slice 10: attribute + reindex integration fences

**Claim:** C4 (integration), C13 (reindex idempotency for member rows).
**Oracle:** SQL row snapshots before/after second index (existing
`tests/attributes.rs` fence pattern, extended fixture).
**Stress fixture:** `[Obsolete] public int A, B;` + `[Obsolete]` property in
one fixture; reindex; bug classes: UPSERT duplicating member symbol rows on
reindex; attribute rows orphaned or doubled on the second pass.
**Loop budget:** none (tests only).
**Files:** `tests/attributes.rs`

**Verification:**
- [ ] Unit tests pass (`csharp_member_attribute_rows_match_source_and_survive_reindex`)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees
- [ ] Budgets hold (n/a)

## Slice 11: corpus audit (one-shot measurements → audit.md)

**Claim:** C1 (42), C2 (2), C6 (881), C9 (predicted JSON exactly), C10 (edge
diff empty), C11 (call/construct diff empty beyond enumerated D8 rows), C13
(reindex diff empty on corpus).
**Oracle:** the committed baselines + probe1 output + the findings.md
predicted JSON — all produced before any feature code existed.
**Stress fixture:** the corpus itself (real production shape; the decoys at
`FunctionalMethodsTests.cs:867` and `docs/TDD-EXAMPLE-MATCH-TESTS.cs:205` are
the built-in adversarial cases).
**Loop budget:** audit SQL O(refs) ≈ 4×10^3 rows + O(edges) 300 — trivial.
**Files:** `.tethys-xebx/audit.md` (+ `audit.sh` if scripted)

STOP-on-drift: ANY unexplained deviation from written expectations halts the
build and surfaces to dwalleck (per checkpointed-build rules).

**Verification:**
- [ ] Unit tests pass (full suite — C12)
- [ ] Stress fixture produces expected outcome (all seven numbers match)
- [ ] Oracle agrees (baseline diffs + prediction)
- [ ] Budgets hold

## Slice 12: docs + final gates

**Claim:** C12 (suite green, unmodified Rust behavior) + repo hygiene.
**Oracle:** the pre-existing test suite; grep for the stale gotcha text.
**Stress fixture:** n/a — docs + full-gate slice (fixture work lives in
slices 3-11; this slice's check is the gate run itself).
**Loop budget:** none.
**Files:** `AGENTS.md` (the lines naming xebx as an open gap — rewrite to
describe shipped member extraction and its remaining boundaries, citing
tethys-5uqz/tethys-cfme), `CONTEXT.md` only if vocabulary needs a touch
(expected: none; "field access" already in the Reference definition).

**Verification:**
- [ ] Unit tests pass (full suite, doctests)
- [ ] Stress fixture n/a (gate run is the check)
- [ ] Oracle agrees (no stale gotcha text remains)
- [ ] Budgets hold (n/a)

## Plan Self-Review

1. **Loops:** two new loop sites (declarator expansion S4, per-level chain
   emission S5), both O(AST nodes)-bounded, ~10^5 ops at corpus scale, no
   always-on phase — within budget. No `O(?)` anywhere.
2. **Fixtures:** every logic slice has an adversarial fixture naming its bug
   class (same-file name collision S3, per-declaration-vs-declarator S4,
   fold-to-outermost + callee double-emit S5, stale-span S6, resolved-read
   edge-minting S7, decoy leakage S8, over-eager binding + kind-blind decline
   S9, reindex duplication S10); S1/S2 are type/CLI plumbing with round-trip +
   filter-bypass fixtures; S11's fixture is the production corpus; S12 is
   docs-only (justified above).
3. **Doc-comment preconditions:** no new "callers must X" contracts are
   introduced; extractor helpers tolerate absent fields by returning no
   symbols/refs for that node (existing csharp.rs posture). The one contract
   touched — `populate_call_edges`'s excluded-kinds rationale (S7) — is
   enforced by the SQL predicate itself, with the doc comment updated beside
   it. Any new helper that would silently produce wrong output on a violated
   assumption gets a runtime guard, per rule; none is currently planned.
4. **Write targets:** no new output streams; audit numbers go to committed
   markdown (data-as-artifact), test output via the harness. No unexamined
   `println!`.
5. **Tracker references:** tethys-5uqz (read shapes), tethys-0aqj (kind-blind
   binding, cited in S9), tethys-cfme (enum members/using-static, cited in
   S12 docs), tethys-53iv (posture), tethys-9181 (untouched surface) — all
   verified to exist with covering descriptions during the design stage.

Claim coverage: C1(S3,S11) C2(S4,S11) C3(S4) C4(S3,S4,S10) C5(S3) C6(S5,S11)
C7(S8) C8(S9) C9(S8,S11) C10(S7,S11) C11(S6,S9,S11) C12(S11,S12) C13(S10,S11)
C14(S1,S2) — all 14 covered.
