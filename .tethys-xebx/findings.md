# tethys-xebx probe findings (prove-it-prototype, 2026-07-05)

Probes ran against **real production data**: `Tethys.Results` (dwalleck's
published C# library, 40 `.cs` files incl. docs/test-package trees), copied to
scratchpad and indexed with the actual tethys binary. Events/delegates do not
occur in that corpus, so their grammar shapes were verified on a committed
synthetic snippet (`synthetic-members.cs`) parsed by the same probe.

## The probe

- `probe1/` — standalone Rust binary (raw `tree-sitter` 0.24.7 +
  `tree-sitter-c-sharp` **0.23.1**, the exact versions in tethys's
  `Cargo.lock`; none of tethys's extraction code). Dumps every
  `property_declaration` / `field_declaration` / `event_field_declaration` /
  `event_declaration` / `delegate_declaration` (name, line, arrow-bodied flag,
  raw `attribute_list` text) and every `member_access_expression` READ that is
  not an invocation callee (name, full text, receiver node kind). Output:
  `probe1-output.txt`.
- `probe2.sh` — system side: index the corpus with the real binary, then raw
  SQL + CLI: symbol/ref kind distributions, `Data`/`Value` symbol rows,
  attribute rows, `module_path` shape, imports rows, resolution strategy
  distribution, and `deprecated-callers --json`. Output: `probe2-output.txt`.

## Oracle (independent)

Raw-text `grep -rn` over the corpus source — no tree-sitter, no SQLite.
Item-by-item agreement achieved on three slices:

1. **Properties (42/42)**: probe list joined against a `get`-keyword grep.
   Three discrepancies, all diagnosed to the oracle, none to the probe:
   the arrow-bodied `Data` property has no `get` for grep to see (verified by
   reading the source); two multi-line accessor blocks anchor two lines apart
   (same properties); one grep hit was the string literal
   `"Failed to get data"` (oracle noise).
2. **Fields (2/2)**: `_value`, `_error` in `TypedResult.cs` — exact match with
   a field-pattern grep.
3. **`Data` reads (4/4)**: probe READ sites equal grep `\.Data\b` sites after
   the grep's fifth line is diagnosed as a `namespace Tethys.Test.Data;`
   declaration, not a read.

## What I learned that I did not know before running the probe

> **Chained member access would silently defeat the feature's purpose under a
> fold-to-outermost design: the first probe cut reported only the outermost
> access, and the grep oracle caught it missing the `Data` read inside
> `response.Data.Name` — reads must be emitted per access level, or
> `result.Data.Length` never surfaces the `[Obsolete]` property.**

Also non-obvious:

- **Pass-1 same-file resolution binds by BARE name first**
  (`src/db/files.rs:308-311` tries `name_to_id[name]` before the qualified
  form). Consequence on the real corpus: `apiResponse.Data` and
  `response.Data` bind same-file to their local, non-deprecated
  `ApiResponse.Data` properties and are correctly EXCLUDED from the
  `[Obsolete]` reader list — and the same mechanism is the tethys-53iv-class
  misattribution vector for a file that declares one `Data` and reads another.
- **Variable-receiver member access stays unresolved by existing machinery and
  that is what surfaces it**: today's analogous invocation refs
  (`result::GetValueOrDefault` ×16, strategy NULL, probe2 §J/K) are exactly
  the deprecated-callers Path-B input (`reference_name LIKE '%::%'`,
  last-segment match, Maybe tier). Member reads emitted the same way need
  zero resolver changes to satisfy AC3.
- **`tree-sitter-c-sharp` 0.23.5 is ABI-incompatible with tree-sitter 0.24**
  (`LanguageError { version: 15 }`); the probe had to pin 0.23.1. Any future
  grammar bump is a real compatibility event, not a routine update.
- **`const` and `static readonly` members are plain `field_declaration`s** in
  the grammar — the tethys-cfme (const/static-field/enum-member) boundary is a
  policy line, not a grammar line.

## Measurements (design-driving)

| Fact | Value |
|---|---|
| Member declarations in corpus | 44 (42 properties, 2 fields, 0 events, 0 delegates) |
| Member-access reads (non-callee, per-level) | 881 |
| Read receiver kinds | 851 `identifier`, 26 `member_access_expression`, 3 `element_access_expression`, 1 `parenthesized_expression` |
| `Data` property declarations | 3 (1 `[Obsolete]` in src, 2 non-deprecated `ApiResponse.Data` in test/docs) |
| `Data` reads | 4 (2 same-file-bindable to local decls, 2 cross-file) |
| C# symbol kinds today | method 537, function 59, class 48, module 31, struct 2, interface 1, enum 1 — **no member kinds** |
| C# ref kinds today | call 3862, construct 254 — **no read kind** |
| C# resolution strategies today | unresolved 3703, `qualified_exact` 284, `same_file` 124, `unique_workspace` 3, `import_union` 2 |
| Attribute rows in index | **0** (the corpus's only `[Obsolete]` sits on the unextracted property — gap is end-to-end) |
| `deprecated-callers --json` today | all zeros |

**Predicted end-state after the feature (the build's oracle):** deprecated-callers
on this corpus reports the `Data` property as its 1 deprecated symbol with
exactly **2 Maybe reader sites** (`test/Tethys.Test/BasicTests.cs:77` via
`result::Data`, `test-package/test-package.cs:23` via `dataResult::Data`);
`FunctionalMethodsTests.cs:867` and `docs/TDD-EXAMPLE-MATCH-TESTS.cs:205` are
absent (same-file binds to local non-deprecated `ApiResponse.Data`).

## Grammar ground truth (tree-sitter-c-sharp 0.23.1, verified by parsing)

- `property_declaration` — `name` field; expression-bodied form carries an
  `arrow_expression_clause` child; `attribute_list` children hold attributes;
  node start = attribute line when attributes precede.
- `field_declaration` — names live in `variable_declaration` →
  `variable_declarator` children (multiple declarators possible); covers
  `const` and `static readonly`.
- `event_field_declaration` (`public event EventHandler Changed;`) — names via
  declarators, like fields.
- `event_declaration` (accessor form `{ add {} remove {} }`) — `name` field.
- `delegate_declaration` — `name` field; occurs at BOTH namespace level and
  class level (dispatch must handle both).
- `member_access_expression` — `expression` (receiver) + `name` fields;
  receiver is `identifier` for both variables and type names (disambiguation
  is resolution's job, not the grammar's).

## prove-it-prototype hard gate

- [x] Probe written and runs against the real codebase (probe1 on
  Tethys.Results source; probe2 through the real binary + SQLite index)
- [x] Oracle defined and produces output (raw grep scans, item-by-item joins)
- [x] Probe and oracle agree on non-trivial slices (42/42 properties, 2/2
  fields, 4/4 `Data` reads after diagnosed oracle noise)
- [x] Learned something new (per-level chained-access emission requirement —
  caught by the oracle disagreement, plus the bare-name-first Pass-1 bind)
