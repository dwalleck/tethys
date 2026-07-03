# Design: deprecated-callers analysis (tethys-jdly)

**Status**: falsifiable-design complete, cheapest falsifiers run (2026-07-02).
**Upstream**: probe results in `findings.md` (prove-it-prototype); spec = rivets
`tethys-jdly` + probe-mandated amendment (tiering).

## Purpose

A CLI analysis listing every reference site of symbols marked `#[deprecated]`
(Rust), tiered by resolution trustworthiness, so call sites can be migrated before
the deprecated item is removed. Follows panic-points / unused-imports conventions.

## What the probe established (design may not contradict)

- `attributes(name='deprecated')` ⋈ `symbols` is exact vs grep oracle (12/12 on zbus).
- Raw refs-join has 26% precision on real data; every error is name-only resolution
  (`tethys-53iv`). Unique-name resolutions were 5/5 correct → tiering is mandatory.
- Must read `refs`, not `call_edges` (top-level refs have `in_symbol_id NULL`).
- `cargo rustc --profile check -- --force-warn deprecated` is the independent
  ground-truth oracle (overrides `#[allow(deprecated)]`).
- `crate::`/`super::`-qualified calls sit unresolved (`tethys-3i35`) but retain
  `reference_name` — recoverable at query time.
- On zbus, all 36 unresolved bare-name matches are false (rustc), and all
  qualified-unresolved matches are zero → Path B is qualified-only.

## Architecture

1. **Extractor slice** (`src/languages/rust.rs`): capture the RHS of name-value
   attributes (`#[deprecated = "msg"]`) into `attributes.args` (currently NULL —
   falsified by F2). Purely additive column data; `idxperf_golden` golden content
   re-reconciled via its documented probe-dump oracle if its fixture is affected.
2. **Query layer** (`src/db/deprecated.rs`): two-path query —
   - Path A (resolved): `refs.symbol_id = dep.id` → tier per C5.
   - Path B (unresolved-qualified): `refs.symbol_id IS NULL AND reference_name
     LIKE '%::' || dep.name` → always Maybe.
   - Clean: deprecated symbols with zero Path A + Path B rows.
   - since/note parsed from `args` (NULL → none; bare string → note; key-value
     list → since/note).
3. **Facade** (`src/lib.rs`): `Tethys::deprecated_callers()` returning typed report.
4. **CLI** (`src/cli/deprecated_callers.rs`, `Commands::DeprecatedCallers`):
   table + `--json`; deterministic ordering (symbol file/line, then site file/line);
   help text notes C# `[Obsolete]` out of scope pending `tethys-haw5`.
5. **Tests** (`tests/deprecated_callers.rs`): fixture workspaces built per-test
   (never ambient DB), fixture embeds the bug classes from the probe (same-file
   phantom pattern, ambiguous names, qualified calls, all three attr forms).

## Input shapes (step 2)

| Shape | Coverage |
|---|---|
| Attr form: bare `#[deprecated]` | C2 (args NULL → no note/since) |
| Attr form: `#[deprecated = "msg"]` | C2 (extractor slice; **currently lost** — F2 falsified) |
| Attr form: parens since/note/both, multi-line | C2 (probe-verified raw text) |
| Attr form: `#[cfg_attr(pred, deprecated)]` | Out of scope — **tethys-n7nf** (filed) |
| Symbol kind: fn / method / struct / enum variant | C1 (fixture-verified, kind-agnostic query) |
| Symbol kind: field, const, macro | C1 applies (detection is kind-agnostic); refs to them bounded by extractor coverage — `tethys-ygjx` noted |
| Deprecated `pub use` re-export | Out of scope — **tethys-tthy** (filed, 73% of zbus deprecations) |
| Caller: `use`-imported bare call, cross-file | C3 |
| Caller: `Type::method` qualified, import-resolvable | C3 (zbus-verified) |
| Caller: same-file call | C3 (zbus-verified; phantom risk → tier Maybe via C5) |
| Caller: `crate::`/`super::`-qualified (unresolved, `tethys-3i35`) | C4 (Path B) |
| Caller: bare ambiguous cross-file (resolver declines) | Out of scope: 100% noise on zbus (36/36 false per rustc); real instances need `tethys-53iv`/`tethys-9z7i` |
| Caller: alias-renamed import | Resolves via imports table (alias column); group-alias parser drop is `tethys-rylk` |
| Caller: top-level ref (`in_symbol_id NULL`) | C3 (caller = null in JSON, `<top-level>` in table) |
| Caller: inside test code | Included, undifferentiated (settled: test migration is still migration) |
| Cardinality: zero deprecated symbols | C10 (tethys self-index) |
| Cardinality: deprecated with zero callers | C6 (clean list) |
| Cardinality: many callers, multi-file, duplicates | C3 + C9 (no dedupe in v1; ordering fixed) |
| Workspace: mixed Rust + C#, `[Obsolete]` | C11 — detection out of scope: **tethys-haw5** |
| note text: quotes, newlines (multi-line attrs) | C2/C9 (JSON escaping via serde) |

## Subtractive sweep (step 2b)

Purely additive: new read-only query + new CLI subcommand; no constraint, guard,
lock, or ordering removed. The only touched existing surface is `attributes.args`
gaining values where it stored NULL (name-value attrs); sole consumers today are
an INSERT, a `derive`-name lookup, count queries, and test fences — none assume
NULL for name-value forms. `idxperf_golden` is the guarded surface; its
reconciliation path is documented in that test.

## Claims and falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| C1 | A symbol is reported as deprecated iff `attributes` has `(symbol_id, 'deprecated')`, for every extracted symbol kind | Fixture with deprecated fn/struct/variant + decoys; diff reported set vs attribute-to-item list | `oracle-q1.sh` (grep/awk) | 5m | **passed** (fixture dump 6/6, zbus 12/12) | integration test `deprecated_callers::detects_all_kinds` |
| C2 | All three attr forms yield correct note/since (bare → none; `= "msg"` → note; parens → parsed keys) | Index fixture with all forms; inspect report fields | sqlite direct + hand-read source | 5m | **falsified pre-fix** (name-value RHS lost) → extractor slice in scope | integration test `deprecated_callers::attr_forms` (fails on today's code, passes post-slice) |
| C3 | Every resolved ref to a deprecated symbol yields a site row (file, line, caller-or-null), including top-level refs | Fixture: imported cross-file call + call inside `mod tests` `#[test]` fn; compare row set | rustc `--force-warn deprecated` on the fixture crate (hand-recorded expected lines) | 15m | pending | integration test `deprecated_callers::resolved_sites` |
| C4 | Every unresolved ref whose name ends `::<dep name>` yields a Maybe row | Fixture `crate::`/`super::` calls; assert 3 Maybe rows present and bare decoy absent | hand-known source truth (prototype SQL run recorded here) | 5m | **passed** (prototype: 3/3 recovered, decoy dropped) | integration test `deprecated_callers::qualified_unresolved` |
| C5 | Resolved row is Definite iff every same-named indexed symbol is deprecated, else Maybe; Path B rows always Maybe | Fixture: unique name, name shared with non-deprecated decoy, name shared only among deprecated pair | hand-computed tier table | 10m | **passed** (prototype: old_bare Definite, old_eq Maybe) | integration test `deprecated_callers::tiers` |
| C6 | Zero-site deprecated symbols appear in a clean list, never omitted | Fixture `Turbo`, `old_unreferenced`; assert both listed clean | grep (attribute exists) + grep (no calls) | 5m | **passed** (prototype: exactly those two) | integration test `deprecated_callers::clean_list` |
| C7 | Symbols without the attribute never appear as deprecated entries, even same-named ones | Fixture decoy `Widget::old_eq` with callers; assert absent from deprecated set | grep | 5m | **passed** (prototype output) | same test as C5 (distinct assert) |
| C8 | On zbus 4.4.0: Definite rows = exactly the 5 rustc-confirmed sites; Path B adds 0 rows | Run CLI on zbus index; compare to rustc warning list | rustc `--force-warn deprecated` (captured in findings.md) | 30m | **passed** (prototype SQL; re-run with real CLI before merge) | fixture embeds zbus bug classes (same-file phantom + ambiguity); zbus run itself = one-shot audit, `manual` (approval requested below) |
| C9 | `--json` twice on unchanged index is byte-identical; rebuild + rerun on unchanged fixture identical | Run twice, `diff`; `--rebuild` then diff | `diff` | 10m | pending | integration test `deprecated_callers::deterministic` |
| C10 | Workspace with zero deprecated symbols → empty findings, exit 0 | Run on tethys self-index | `grep -rn '#\[deprecated' src/` = 0 attribute uses | 5m | pending | integration test `deprecated_callers::empty_workspace` (synthetic clean fixture) |
| C11 | C# `[Obsolete]` yields no findings; `--help` cites tethys-haw5 | Mixed fixture with `[Obsolete]` class | grep | 5m | pending | integration test `deprecated_callers::csharp_out_of_scope` |

Cheapest falsifiers (C1, C4, C5, C6, C7) ran against the fixture and zbus indexes
before this document was finalized; C2's falsifier ran and **falsified the
current extractor**, which is why the extractor slice exists. No claim contradicts
the probe.

### Non-vacuity (named buggy implementations per fence)

- C1: query filters `kind = 'function'` → variant/struct rows vanish.
- C2: extractor keeps parens-only capture → name-value note NULL (today's actual code).
- C3: joining `call_edges` instead of `refs` → the `mod tests` top-level site vanishes.
- C4: pattern `LIKE '%' || dep.name` (missing `::`) → bare decoy leaks in as extra row; or Path B omitted → 3 rows vanish.
- C5: EXISTS subquery counts deprecated instead of non-deprecated same-named symbols → tiers invert.
- C6: `INNER JOIN refs` → clean symbols disappear entirely.
- C7: joining attributes by name instead of symbol_id → decoy inherits deprecation.
- C8 fence: same-file phantom pattern in fixture flags as Definite if tier rule regresses.
- C9: missing `ORDER BY` → SQLite scan order drifts after rebuild.
- C10: unconditional header row / non-zero exit on empty set.
- C11: attribute name matched case-insensitively or `Obsolete` mapped to `deprecated`.

### Verification distinctness

Each claim has its own integration test (or named assert within one — C5/C7 share
a fixture but assert on disjoint outputs: tier values vs symbol-set membership).
A failure names the claim via the test name.

## Negative space (deliberately not doing)

1. **C# `[Obsolete]`** — `tethys-haw5` (verified open, blocked on this issue).
2. **Deprecated `pub use` alias re-exports** — `tethys-tthy` (filed from probe; 73% of zbus's deprecations; schema cannot attach attributes to refs).
3. **`cfg_attr`-conditional deprecation** — `tethys-n7nf` (filed during this design).
4. **Recovering bare ambiguous declined calls / fixing same-file phantoms** — resolver work, `tethys-53iv` and `tethys-9z7i` (verified open); this analysis only *tiers around* them.
5. **Lint-gate semantics** — exit code is always 0 on success; parity with panic-points/unused-imports (settled rationale, not deferred work).
6. **Dedupe of multi-ref lines** — v1 reports refs as stored; determinism (C9) makes output stable.
7. **`use` statements importing deprecated items** (rustc warns on these; found during S3's oracle run) — excluded by definition: import lines vanish with their call sites during migration, and a call-less deprecated import is already flagged by unused-imports (settled rationale, not deferred work).

## Known residual risk (accepted)

A `tethys-53iv` external-receiver call (`x.frob()` on an external type where the
only indexed `frob` is deprecated) would tier **Definite** wrongly — uniqueness
cannot see external candidates. Not observed on zbus (0 instances); accepted with
citation to `tethys-53iv`, whose fix (or `tethys-9z7i` bands) removes the class.

## Approval requests

1. C8's zbus measurement is one-shot (`Regression fence: manual` + fixture
   analogue) — needs explicit sign-off per the fence rule.
2. The tiering amendment contradicts the issue's original "no confidence tiers"
   sentence — jdly's notes already record the falsification; sign-off on the
   amended contract is implicit in option (a) but stated here for the record.
