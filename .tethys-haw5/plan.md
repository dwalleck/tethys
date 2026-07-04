# tethys-haw5 plan: C# parity for deprecated-callers (budgeted-plan, 2026-07-03)

Upstream: `design.md` (14 claims, cheapest falsifier passed 19/19),
`findings.md`, frozen `baseline-rand-deprecated.json`. No always-on phases are
introduced anywhere in this plan — every new loop runs inside one-shot `index`
or `deprecated-callers` invocations — so **wall budgets are n/a throughout**;
loop budgets are stated per slice.

Build order: S1 → S2 → S3 → S4 → S5 → S6 → S7 → S8. S5–S7 are integration
fences and depend on S1+S3 (S7's mixed test also on S4).

---

## Slice 1: Extract C# attributes into `ExtractedAttribute` rows

**Claim:** Design C1 — one row per attribute per extracted symbol; name as
written, args = raw inner text of `attribute_argument_list` minus outer parens
(None when absent), line = the attribute's own line.
**Oracle:** Hand-read of the fixture source text (grep-style, per probe1's
oracle); unit asserts compare extracted vecs against literally-written rows.
**Stress fixture:** One source string containing: stacked lists
(`[Obsolete]\n[Serializable]`), multi-attribute list `[Obsolete("m"), Fact]`,
target specifier `[method: Obsolete("t")]`, an attribute on a *nested* class,
and a verbatim arg `[Obsolete(@"a, ""b""")]`. Expected (pre-written):
outer method → rows `(Obsolete, Some("\"m\""), L1)`, `(Fact, None, L1)`;
stacked method → `(Obsolete, None, L2)`, `(Serializable, None, L3)`;
target-specified method → `(Obsolete, Some("\"t\""), L4)`; nested class →
its row attaches to the *nested* symbol, not the parent; verbatim args
preserved byte-for-byte including inner quotes. Bug classes: first-list-only
walk, parens included, symbol line stored as attr line, parent/child
misattachment, arg text mangled by quote handling.
**Loop budget:** New loop: O(attribute_lists × attributes) per declaration
node, amortized O(symbols × avg_attrs); production scale symbols ≈ 10^5,
avg_attrs ≤ 3 → ≈ 3×10^5 ops inside one-shot indexing. Within 10^6. No
syscalls.
**Wall budget:** n/a (one-shot indexing phase).
**Files:** `src/languages/csharp.rs` (fn `extract_attributes` + wire
`extract_type_declaration`, `extract_method`, `extract_constructor`;
`extract_namespace` keeps `Vec::new()` — C# forbids attributes on namespace
declarations, comment says so; unit tests in the file's tests module).

**Code (advisory):**
```rust
fn extract_attributes(node: &tree_sitter::Node, content: &[u8]) -> Vec<ExtractedAttribute> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != node_kinds::ATTRIBUTE_LIST { continue; }
        let mut inner = child.walk();
        for attr in child.children(&mut inner) {
            if attr.kind() != node_kinds::ATTRIBUTE { continue; }
            let Some(name_node) = attr.child_by_field_name("name") else { continue };
            let Some(name) = node_text(&name_node, content) else { continue };
            let args = attr.children(&mut attr.walk())
                .find(|c| c.kind() == "attribute_argument_list")
                .and_then(|al| node_text(&al, content))
                .map(|t| t.trim_start_matches('(').trim_end_matches(')').to_string());
            out.push(ExtractedAttribute {
                name, args,
                line: u32::try_from(attr.start_position().row + 1).unwrap_or(u32::MAX),
            });
        }
    }
    out
}
```

**Verification:**
- [ ] Unit tests pass (expected rows above, literal)
- [ ] Stress fixture produces expected outcome
- [ ] probe1.sh on Tethys.Results copy: sections A–D **still empty** (its only
      `[Obsolete]` is on a property, tethys-xebx) — the oracle-agreement check
      for this repo is "unchanged zeros," written down here on purpose
- [ ] Loop budget holds (index the 31-file fixture repo; no slowdown)

---

## Slice 2: `parse_obsolete_args` — total parser for C# Obsolete arguments

**Claim:** Design C3 + C4 (parse side of C2) — first string literal (positional
or `message:`) → note, unquoted/unescaped; bool literal (positional or
`error:`) → error flag; degrades to `(None, None)` on anything else.
**Oracle:** Hand-written expected-value table (same style as
`parses_all_deprecated_args_shapes`, which is the jdly precedent).
**Stress fixture:** Unit table rows (expected pre-written):
`None → (None, None)`; `""` → `(None, None)`; `"\"m\"" → (Some("m"), None)`;
`"\"m\", true" → (Some("m"), Some(true))`; `"\"m\", false" → (Some("m"),
Some(false))` (kills any-second-arg→true); `"message: \"m\", error: true"` →
`(Some("m"), Some(true))`; `"\"a, true, b\"" → (Some("a, true, b"), None)`
(comma+bool INSIDE string kills naive split); `"\"say \\\"hi\\\"\"" →
(Some("say \"hi\""), None)`; `"@\"C:\\path\"" → verbatim passthrough`;
`"true" → (None, Some(true))` (bool without message — legal parse, degrade
gracefully); Unicode `"\"déjà vu\""`. Bug classes: naive comma split,
positional-only or named-only parsing, false→null collapse, escape mangling.
**Loop budget:** O(len(args)) per attribute, args ≤ ~500 chars, obsolete
attrs ≈ 10^2 per workspace → ≈ 5×10^4 ops, one-shot query time. Within budget.
**Wall budget:** n/a.
**Files:** `src/db/deprecated.rs` (fn + unit table
`parses_all_obsolete_args_shapes`).

**Code (advisory):** signature
`fn parse_obsolete_args(args: Option<&str>) -> (Option<String>, Option<bool>)`;
reuse `split_top_level_commas` + `unquote`; per part: starts with `"`/`@"` →
note (first wins); equals `true`/`false` → flag; `message:`/`error:` prefixes
strip-then-recurse on the same two rules.
**Doc-comment contract:** the parser is documented **total** (never errors,
degrades to nulls) — no caller preconditions exist, so no runtime check or
debug_assert is owed. The name-dispatch contract ("only called for Obsolete
spellings") is a sanity hint: mis-dispatch degrades to nulls in both
directions (both parsers are total), never wrong attribution → no enforcement
beyond the match arm that does the dispatch.

**Verification:**
- [ ] Unit table passes, every row
- [ ] Stress rows (comma-in-string, explicit false) produce expected outcome
- [ ] probe oracle n/a for a pure function — covered by table oracle
- [ ] Loop budget holds trivially

---

## Slice 3: Detection widening, `error` field, display

**Claim:** Design C5 (four spellings detected, decoys never) + C10 key set
(serialization side) + C2/C4 report side.
**Oracle:** DB-level unit test inserting attribute rows *directly* (the
`db/files.rs` test-helper pattern) — independent of Slice 1's parser; expected
detected-set written literally.
**Stress fixture:** Insert symbols decorated (one each): `Obsolete`,
`ObsoleteAttribute`, `System.Obsolete`, `System.ObsoleteAttribute`,
`NotObsolete` (custom decoy — kills `LIKE '%Obsolete%'`), `Serializable`,
`deprecated` (Rust row — must still detect via old path). Expected: exactly 5
findings (4 Obsolete spellings + 1 deprecated); the bare-marker one reports
`note: null, error: null`; the `("m", true)` one reports both. JSON: symbol
key set is exactly `{name, kind, file, line, since, note, error}` for every
entry. Bug classes: substring matching, exact-'Obsolete'-only, serde skip on
error, dispatch corrupting Rust since/note.
**Loop budget:** No new Rust loops. SQL `a.name IN (…5)` = 5 probes of
`idx_attributes_name` instead of 1, one-shot. Within budget.
**Wall budget:** n/a.
**Files:** `src/db/deprecated.rs` (query widening, dispatch, `error:
Option<bool>` on `DeprecatedSymbol`), `src/cli/deprecated_callers.rs`
(`deprecation_meta` renders `error — ` prefix when `Some(true)`).
**Output stream:** no new writes; existing single guarded stdout write (data)
and `tracing` diagnostics (stderr) unchanged.

**Verification:**
- [ ] Unit tests pass (detected set = literal expectation)
- [ ] Stress fixture (NotObsolete decoy) produces expected outcome
- [ ] probe3.sh on rand-0.8.5: identical to `baseline-rand-deprecated.json`
      modulo `"error": null` (claim 11's first checkpoint)
- [ ] Loop budget holds

---

## Slice 4: Language guard for Path B and ambiguity tiering

**Claim:** Design C9 — Path B attachment and tier demotion are
same-language only.
**Oracle:** Fixture construction knowledge: site file extensions determine
language ground truth, independent of the query under test.
**Stress fixture:** Mixed workspace: C# obsolete instance method `Run` +
Rust file with unresolved `crate::Run` ref; Rust `#[deprecated] fn
legacy_shared` + C# unresolved `x::legacy_shared` ref; plus a Rust
non-deprecated `fn Run` (same name as the C# obsolete method — kills
cross-language tier demotion: C# `Run` must stay Definite). Expected
(pre-written): C# `Run` finding lists zero `.rs` sites and tier=Definite;
Rust `legacy_shared` lists zero `.cs` sites. Bug classes: guard-less code
(fails today — the non-vacuity witness), guard applied to resolved refs
(over-filtering same-language sites), name-only ambiguity demotion.
**Loop budget:** Path B loop unchanged O(u + d), u ≈ 10^5 unresolved refs at
production scale, one-shot. Ambiguity CTE returns (name, language) pairs —
same row count bound as today. New: `DeprecatedSymbol` carries
`#[serde(skip)] pub(crate) language: String` (from the files join already in
the query). Within budget.
**Wall budget:** n/a.
**Files:** `src/db/deprecated.rs`, `tests/deprecated_callers.rs` (test
`no_cross_language_attachment`).

**Verification:**
- [ ] Unit/integration tests pass
- [ ] Stress fixture produces expected outcome (all three bug classes)
- [ ] probe3.sh rand output unchanged (pure-Rust workspace unaffected by guard)
- [ ] Loop budget holds

---

## Slice 5: C# integration fence — resolved sites, construction, Clean (flips C11)

**Claim:** Design C6 + C7 — static-receiver invocation sites resolved with
correct tier; `new T()` sites listed; uncalled obsolete symbol lands Clean.
**Oracle:** grep of the fixture source (file:line list written in the test as
literals — same oracle mechanism as probe2's 12/12 Combine check).
**Stress fixture:** C# workspace (tmpdir, own index per AC6): `Legacy.cs` —
static `[Obsolete("use New")] Old()` called cross-file twice AND from inside
`Legacy.cs` itself (same-file site must appear too); obsolete class
`LegacyService` constructed once; obsolete uncalled `Dormant()`; a
*non*-obsolete `Old()` on another class (same-name decoy → the static one's
sites must tier **Maybe**, testing the iff's demotion direction); plus a
second workspace variant where the name is unique → **Definite** (both
directions of C6's iff). Expected file:line lists pre-written in the test.
Bug classes: call_edges join dropping top-level sites, kind='call' filter
dropping construct refs, tie-break unsorted output, tier always-Definite.
**Loop budget:** No new production loops (test-only slice).
**Wall budget:** n/a.
**Files:** `tests/deprecated_callers.rs` (replace C11 no-findings fence with
the positive fixture; keep a one-line comment pointing at design C6/C7).

**Verification:**
- [ ] Integration tests pass with literal site lists
- [ ] Stress fixture (decoy tier demotion + same-file site) as expected
- [ ] probe1.sh unchanged (property repo stays zeros — xebx boundary)
- [ ] Loop budget n/a (no new loops)

---

## Slice 6: Path B fence + JSON key-set parity fence

**Claim:** Design C8 (variable-receiver → Maybe via unresolved-qualified) +
C10 fence (identical key sets, both languages).
**Oracle:** For C8: fixture source grep (the design-time SQL sim already
passed 19/19 on real data; this is its deterministic CI form). For C10:
`serde_json` key iteration compared to a *literal* sorted key list —
independent of the serializer defaults changing.
**Stress fixture:** C# fixture: obsolete instance method `Fetch` called as
`client.Fetch()` (variable receiver) in another file → expected: site listed,
tier=Maybe, via=unresolved-qualified; a same-named method on an unrelated
class ensures Maybe attaches to BOTH candidates (multi-candidate fan-out is
expected behavior, written down). JSON test: run report on this fixture and on
the S5 Rust-style fixture; assert every symbol object's sorted keys ==
`["error","file","kind","line","name","note","since"]`. Bug classes: Path B
requiring resolution, single-candidate assumption, `skip_serializing_if` on
error.
**Loop budget:** No new production loops (test-only).
**Wall budget:** n/a.
**Files:** `tests/deprecated_callers.rs`.

**Verification:**
- [ ] Tests pass (tier/via literals, key-set literal)
- [ ] Stress fixture (multi-candidate fan-out) as expected
- [ ] Oracle lineage noted: CI form of the passed design-time falsifier
- [ ] Loop budget n/a

---

## Slice 7: Empty envelope, mixed merge, idempotent reindex fences

**Claim:** Design C12 + C13 + C14.
**Oracle:** Literal expected JSON summary values; SQL dump diff for attribute
rows (empty diff), per the probe1 oracle mechanism (sqlite3 against the
built index).
**Stress fixture:** (a) C# workspace whose only attributes are `[Fact]`,
`[Test]`, `[TestMethod]` → summary all zeros, `deprecated: []` (kills
detection matching test-framework attrs); (b) mixed workspace, 1 Rust
deprecated + 1 C# obsolete → `symbol_count == 2`, one entry per language,
site counts summing; (c) index the same workspace twice → attributes table
dump identical (count AND content — kills UPSERT duplication, the
tethys-wsix bug class) and `get_deprecated_callers` JSON byte-identical
(extends the existing `first==second` test to C#). Expected values
pre-written. Bug classes: test-attr false positives, per-language early
return, insert-without-delete duplication.
**Loop budget:** No new production loops (test-only).
**Wall budget:** n/a.
**Files:** `tests/deprecated_callers.rs`, `tests/attributes.rs` (row-dump
idempotency lives with the other attribute-row tests).

**Verification:**
- [ ] All three fences pass with literal expectations
- [ ] Stress fixtures as expected
- [ ] Oracle: sqlite3 dump diff empty on double index
- [ ] Loop budget n/a

---

## Slice 8: Rust regression measurement + help text

**Claim:** Design C11 — rand-0.8.5 output differs from the frozen baseline
only by `"error": null` insertions; plus `main.rs` help text drops "out of
scope pending tethys-haw5".
**Oracle:** `baseline-rand-deprecated.json` (captured from the pre-change
binary — cannot be regenerated after the change, which is why it was frozen
at design time) + `jq 'del(.deprecated[].symbol.error)'` normalize-and-diff.
**Stress fixture:** The measurement itself is one-shot; its deterministic CI
form is S6's key-set fence + the existing Rust fixture tests (which pin
since/note values — any parser-dispatch corruption fails those, not just the
diff). Additional stress: run the diff with `--json` AND human mode (human
output must be byte-identical to pre-change since no Rust entry has
error=Some(true)). Expected: empty normalized diff; empty human diff.
**Loop budget:** No new loops.
**Wall budget:** n/a.
**Files:** `src/main.rs` (help string), `.tethys-haw5/regression-run.md`
(measurement record, audit trail).
**Output stream:** measurement record is a repo doc, not program output;
program streams unchanged.
**Regression fence note:** measurement-based claim → fence = S6 key-set test
+ existing `detects_all_kinds` / golden field asserts (deterministic, CI).
No `manual` fences in this plan.

**Verification:**
- [ ] Normalized JSON diff empty; human diff empty
- [ ] Help text updated, `cargo run -- deprecated-callers --help` reflects it
- [ ] probe3.sh agreement recorded in regression-run.md
- [ ] Loop budget n/a

---

## Claim coverage map (design → slices)

C1→S1 · C2→S2+S3 · C3→S2 · C4→S2+S3 · C5→S3 · C6→S5 · C7→S5 · C8→S6 ·
C9→S4 · C10→S3+S6 · C11→S3(checkpoint)+S8 · C12→S7 · C13→S7 · C14→S7.
All 14 design claims covered; no slice implements an unclaimed behavior.

## Plan Self-Review

1. **Loops.** S1 attribute walk: O(symbols × avg_attrs) ≈ 3×10^5, one-shot —
   in budget. S2 parser: O(len) ≈ 5×10^4 — in budget. S3: SQL IN(5) index
   probes — in budget. S4: Path B unchanged O(u+d), u ≈ 10^5 one-shot — in
   budget. S5–S8: no new production loops. No always-on phases → no wall
   budgets owed. **No gaps.**
2. **Fixtures.** Every slice names its bug classes: S1 first-list-only/parens/
   line/parent-misattach; S2 comma-split/false-collapse/named-vs-positional;
   S3 substring-match decoy/serde-skip/dispatch-corruption; S4 guard-less
   (fails today)/over-filter/name-demotion; S5 call_edges-drop/kind-filter/
   tier-iff both directions; S6 resolution-required/single-candidate/key-skip;
   S7 test-attr false positive/early-return/UPSERT-dup; S8 dispatch corruption
   via existing golden asserts. None is happy-path-only. **No gaps.**
3. **Doc-comment preconditions.** Only one new documented contract:
   `parse_obsolete_args` totality (no precondition → nothing to enforce);
   name-dispatch is a sanity hint — both parsers total, mis-dispatch degrades
   to nulls, never wrong attribution (classified, S2). `extract_namespace`
   non-wiring documented with the language-rule reason (S1). **No gaps.**
4. **Write targets.** No new program writes; analysis output keeps the
   existing single guarded stdout write (data) and tracing-to-stderr
   (diagnostic). S8's regression-run.md is a repo document. **No gaps.**
5. **Tracker references.** tethys-xebx (verified via `rivets show`, scope
   updated to properties/fields/events/delegates), tethys-9181 (created this
   session, ctor Clean-verdict bug), tethys-wsix (verified in tracker sweep;
   cited as the bug class S7(c) fences against), tethys-53iv / tethys-zwaz
   (verified in sweep; context only, no work deferred to them). No deferral
   lacks a citation. **No gaps.**
