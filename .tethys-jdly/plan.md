# Plan: deprecated-callers (tethys-jdly)

Decomposition of `.tethys-jdly/design.md` (approved 2026-07-03, incl. C8 manual
fence and tier amendment). Claim coverage: C1→S2, C2→S1+S2, C3→S3, C5→S3, C7→S3,
C4→S4, C6→S4, C9→S6, C10→S7, C11→S6+S7, C8→S3 (fence) + S8 (audit).

Production-scale constants used in budgets (extrapolated from zbus: 61 files →
1.5k symbols, 10.3k refs; tethys self-index ≈ 6k refs):
`files ≈ 50k, symbols s ≈ 10^6, refs r ≈ 10^7, unresolved refs u ≈ 10^6,
deprecated symbols d ≈ 10^2`. All new code runs in a one-shot CLI command
(no always-on phase → no wall budgets).

Test seam: integration tests use `tests/common/mod.rs::workspace_with_files`
(builds a real index in a TempDir — never an ambient DB, per AC) and `open_db`.
Verified empirically: a call inside `#[cfg(test)] mod tests` yields a resolved
ref with `in_symbol_id NULL` (the zbus builder.rs:392 shape) — S3's fixture
relies on this.

---

## Slice 1: Extractor captures name-value attribute arguments

**Claim:** C2 (storage): `#[deprecated = "msg"]` stores the raw RHS text in
`attributes.args` (currently NULL — falsified by design F2); paren and bare
forms unchanged.
**Oracle:** sqlite direct dump of `attributes` vs hand-read fixture source
(same oracle as the design's F2 run).
**Stress fixture:** `#[deprecated = "uses :: and \" quote and déjà vu"]` —
RHS containing `::` (adversary for downstream suffix logic), an escaped quote
(adversary for naive quote scanning), and non-ASCII. Plus regression rows: all
three paren forms byte-identical to today's capture, `#[must_use = "..."]`
also captured (change is attribute-generic, not deprecated-special-cased).
**Loop budget:** none new — constant extra work per attribute node inside the
existing `extract_preceding_attributes` walk, O(attributes) unchanged.
**Files:** `src/languages/rust.rs`, `tests/attributes.rs` (extend existing).

**Code (advisory):** in the attribute-node parse, when no parenthesized
token-tree child exists, look for the expression following `=` and store its
node text (trimmed) as `args`. tree-sitter-rust grammar: `attribute = path
(token_tree | '=' expression)?`.

**Verification:**
- [ ] Unit tests pass (`tests/attributes.rs` new cases + existing)
- [ ] Stress fixture: RHS stored verbatim incl. `::`, quote, unicode
- [ ] prove-it oracle: re-run design F2 query on rebuilt jdly fixture — `old_eq` args non-NULL
- [ ] `tests/idxperf_golden.rs` passes — if its synthetic fixture gains coverage, reconcile golden per that test's documented probe-dump procedure (content change is intentional)

## Slice 2: Analysis module — deprecated-symbol listing + args parsing

**Claim:** C1 (a symbol is reported iff it has a `deprecated` attribute row,
kind-agnostic) + C2 (parsing: NULL → neither; bare string → note; key-value
list → since/note).
**Oracle:** design F1 — `oracle-q1.sh` grep/awk item list vs module output on
the jdly fixture (12/12 parity already established on zbus for the SQL shape).
**Stress fixture:** args = `note = "a, b, since = fake"` → expected
note = `a, b, since = fake`, since = None (kills naive split-on-comma and
substring-keyed parsers — expected output written here, before implementation).
Also: bare-string with escaped quote; args NULL; `since` only; both keys
reversed order (`note = ..., since = ...`).
**Loop budget:** listing query O(d) rows via `idx_attributes_name`; args parse
= single quote-aware scan O(len(args)), len ≤ ~200 chars. d ≈ 10^2 → trivial.
**Files:** `src/deprecated_callers.rs` (new: `DeprecatedSymbol`,
`parse_deprecation_args`, listing query), `src/lib.rs` (`mod` decl + type
re-exports only).

**Code (advisory):** `parse_deprecation_args(Option<&str>) -> (Option<String>,
Option<String>)` — total function, no preconditions: if input starts with a
string literal → note; else split on top-level commas (quote-aware scanner),
match `since =` / `note =` prefixes, unquote and unescape values.

**Verification:**
- [ ] Unit tests pass (parser table-driven incl. stress rows)
- [ ] Stress fixture: comma-inside-quotes row produces the pre-written expectation
- [ ] prove-it oracle: module symbol list == oracle-q1.sh list on jdly fixture
- [ ] Loop budget holds (single query, single scan)

## Slice 3: Path A resolved call sites + tier rule

**Claim:** C3 (every resolved ref to a deprecated symbol yields a site row —
file, line, column, caller-or-None — including `in_symbol_id NULL` refs) +
C5 (Definite iff every same-named indexed symbol is deprecated, else Maybe) +
C7 (attribute-less symbols never appear as deprecated entries).
**Oracle:** rustc `--force-warn deprecated` on the fixture crate — expected
warning lines hand-recorded in the test file as comments before implementation
(compiler resolution is the ground truth, per design F3).
**Stress fixture:** one workspace containing (a) `use`-imported cross-file call
to unique-named deprecated fn → Definite; (b) deprecated `old_eq` + same-named
NON-deprecated method in another file (name collision across scopes) →
resolved sites tier Maybe, and the method never appears as a deprecated entry;
(c) two same-named deprecated fns in different modules (all-candidates-
deprecated) → Definite; (d) call inside `#[cfg(test)] mod tests` → row with
caller = None (verified shape).
**Loop budget:** sites query O(total sites) via `idx_refs_symbol` per symbol
(d ≈ 10^2 indexed lookups); tier check O(d × log s + same-name matches) via
`idx_symbols_name`. ≈ 10^2 × 20 + ε ≪ 10^6. ✓
**Files:** `src/deprecated_callers.rs`, `tests/deprecated_callers.rs` (new).

**Verification:**
- [ ] Unit tests pass (fixture asserts per shape (a)–(d), distinct asserts)
- [ ] Stress fixture: collision tiers Maybe; all-deprecated pair tiers Definite; decoy absent from entries
- [ ] prove-it oracle: site set matches hand-recorded rustc warning lines for the fixture
- [ ] Loop budget holds

## Slice 4: Path B qualified-unresolved recovery + clean list + facade

**Claim:** C4 (every unresolved ref whose `reference_name` ends with
`::<dep-name>` yields a Maybe row, via='unresolved-qualified') + C6 (zero-site
deprecated symbols are listed clean, never omitted).
**Oracle:** hand-known fixture source truth (the design's F4 prototype run,
recorded in design.md: 3/3 recovered, bare decoy dropped, clean = exact pair).
**Stress fixture:** (a) `crate::`/`super::`-qualified calls to deprecated fns →
recovered Maybe (the tethys-3i35 shape); (b) suffix-boundary adversary:
unresolved `crate::inner::xold_bare` must NOT match deprecated `old_bare`
(kills `LIKE '%' || name` / suffix-without-separator bugs); (c) bare unresolved
`old_eq` (ambiguous decoy) NOT reported (Path B is qualified-only per zbus
36/36-noise measurement); (d) clean list = exactly the zero-site symbols, and a
symbol whose ONLY caller is `crate::`-qualified is NOT clean.
**Loop budget:** one query over unresolved refs via partial index
`idx_refs_unresolved` filtered `reference_name LIKE '%::%'`: O(u) row scan,
u ≈ 10^6, then Rust `rsplit("::")` last-segment + `HashSet<&str>` lookup O(1)
per row → ≈ 10^6 ops, one-shot command: **at budget ceiling, justified** —
single linear pass, no per-row syscalls, SQLite scan of ~10^6 rows ≈ hundreds
of ms; acceptable for an analysis CLI (matches unused-imports' full-workspace
re-parse cost). No d × u nested loop (that would be 10^8 — avoided by design).
Clean check O(d) set lookups.
**Files:** `src/deprecated_callers.rs`, `src/lib.rs` (facade
`Tethys::deprecated_callers() -> Result<DeprecatedReport>`).

**Doc contracts:** "unresolved refs carry `reference_name`" — enforced
structurally in SQL (`WHERE symbol_id IS NULL AND reference_name IS NOT NULL`),
not by assert; a NULL name row is simply excluded (sanity, not load-bearing).
Facade precondition "index must exist" follows `find_unused_imports`
convention: `Tethys::new` errors on missing DB (runtime-enforced already).

**Verification:**
- [ ] Unit tests pass (asserts (a)–(d) distinct)
- [ ] Stress fixture: `xold_bare` non-match; qualified-only exclusion of bare decoy
- [ ] prove-it oracle: report equals design F4 prototype output on jdly fixture
- [ ] Loop budget holds (log query plan uses idx_refs_unresolved — verify with EXPLAIN QUERY PLAN in test)

## Slice 5: CLI module — human table output

**Claim:** rendering half of C3/C4/C6 — the table groups sites under each
deprecated symbol with tier, via, since/note shown when present, and a clean
section; follows panic-points/unused-imports visual conventions.
**Oracle:** design F4 prototype rows (jdly fixture) rendered by hand into the
expected table — written in the test before wiring.
**Stress fixture:** a note containing a newline (zbus multi-line attrs produce
embedded newlines — table must not break row alignment; render first line +
ellipsis) and a non-ASCII note (`déjà`); empty report (renders "none found"
without a header-only orphan table).
**Loop budget:** O(entries + sites) single render pass over already-sorted
report ≈ 10^3 → trivial.
**Files:** `src/cli/deprecated_callers.rs` (new), `src/cli/mod.rs`
(registration line).

**Output streams:** findings table → stdout (data: pipeable). tracing `debug!`
→ stderr via existing subscriber (diagnostic). No other writes.

**Verification:**
- [ ] Unit tests pass (render fn unit-tested on a fixed report value)
- [ ] Stress fixture: multi-line note doesn't corrupt table; empty report path exists
- [ ] prove-it oracle: table content matches prototype rows
- [ ] Loop budget holds

## Slice 6: Subcommand wiring, --json, determinism

**Claim:** C9 (two `--json` runs on an unchanged index byte-identical; rebuild
+ rerun identical) + C11 (help text cites C# `[Obsolete]` as out of scope
pending tethys-haw5).
**Oracle:** `diff` on captured stdout (design F9); `jq`-style parse via
`serde_json::from_str` round-trip in test.
**Stress fixture:** two calls to the same deprecated fn on ONE source line —
forces the (file, line) sort tie and proves the column tie-break is wired
(the "secondary sort key never fires" bug class); plus `--rebuild` between
runs (kills rowid-order dependence).
**Loop budget:** final sort O(n log n), n = sites ≈ 10^3–10^4 → trivial.
**Files:** `src/main.rs` (`Commands::DeprecatedCallers { json }` + dispatch +
clap doc-comment with haw5 note), `src/cli/deprecated_callers.rs` (json fn,
serde structs: `symbol, kind, file, line, since, note, tier, via, call_sites[
{file, line, column, caller}]`; `caller` null for top-level).

**Output streams:** JSON → stdout (data). Errors propagate to main's existing
stderr handler.

**Verification:**
- [ ] Unit tests pass (serde round-trip; ordering asserts)
- [ ] Stress fixture: same-line duplicate sites stable across rebuild
- [ ] prove-it oracle: `--json` on jdly fixture == design F4 prototype rows
- [ ] Loop budget holds

## Slice 7: Boundary fences — empty workspace and C# out-of-scope

**Claim:** C10 (workspace with zero deprecated symbols → empty findings, exit
0) + C11 behavior half (`[Obsolete]` C# symbol yields no findings while Rust
findings in the same workspace still appear).
**Oracle:** grep over the fixture sources (zero `#[deprecated]` occurrences /
one `[Obsolete]` occurrence) — independent of the index.
**Stress fixture:** mixed-language workspace: one C# file with `[Obsolete]`
class AND one Rust file with a deprecated fn + caller — asserts C# absent and
Rust present in one report (kills "any attribute named obsolete mapped to
deprecated" and "C# file aborts the analysis" bug classes). Empty case:
Rust-only workspace with zero deprecated symbols.
**Loop budget:** none new (test-only slice; queries covered by S2–S4 budgets).
**Files:** `tests/deprecated_callers.rs` (extend).

**Verification:**
- [ ] Unit tests pass (two new integration tests, distinct asserts)
- [ ] Stress fixture: mixed workspace partitions correctly
- [ ] prove-it oracle: grep counts match report counts
- [ ] Loop budget: n/a (no new code paths)

## Slice 8: C8 regression fence + one-shot zbus audit (manual — approved)

**Claim:** C8 — Definite precision: same-file phantom resolutions never tier
Definite (fence); on zbus 4.4.0 the CLI's Definite set equals the 5
rustc-confirmed sites and Path B adds zero rows (one-shot audit, `manual`
fence approved by user 2026-07-03).
**Oracle:** rustc `--force-warn deprecated` warning list captured in
`findings.md` (audit); hand-verified fixture expectations (fence).
**Stress fixture (the fence):** fixture reproducing the zbus phantom pattern —
deprecated `path()` method + same-file bare calls to a same-named method on
another type → asserts those sites tier Maybe, never Definite (embeds the bug
class per the design's fence rule; pre-tier code would fail it).
**Loop budget:** none new (test + audit only).
**Files:** `tests/deprecated_callers.rs` (extend), `.tethys-jdly/audit.md`
(new: real-CLI zbus run output diffed against findings.md rustc list).

**Verification:**
- [ ] Unit tests pass (phantom fence)
- [ ] Stress fixture: phantom sites Maybe-tiered
- [ ] prove-it oracle: zbus audit — CLI Definite set == rustc 5; Path B == 0; recorded in audit.md
- [ ] Loop budget: n/a

---

## Plan Self-Review

1. **Loops:** S1 none-new (O(attributes) unchanged); S2 O(d) + O(len) parse;
   S3 O(d log s) tier + indexed site lookups; S4 O(u + d) single pass, u ≈ 10^6
   at ceiling — justified in-slice (one-shot CLI, no per-row syscalls, partial
   index `idx_refs_unresolved` verified to exist in schema.rs:63); S5 O(report);
   S6 O(n log n) sort. No unstated loops; no always-on phases → no wall budgets.
   **No gaps.**
2. **Fixtures:** every slice names its bug class — S1 escaped-quote/`::`/unicode
   RHS; S2 comma-inside-quotes parser trap; S3 cross-scope name collision +
   all-deprecated pair + top-level ref; S4 suffix-boundary non-match +
   qualified-only exclusion + false-clean; S5 multi-line note + empty report;
   S6 same-line tie-break + rebuild; S7 mixed-language partition + empty
   workspace; S8 same-file phantom. **No happy-path-only fixtures; no gaps.**
3. **Doc-comment preconditions:** facade "index must exist" → runtime-enforced
   by `Tethys::new` (existing); Path B "reference_name non-null" → structural
   SQL filter (excluded, not asserted); `parse_deprecation_args` → total
   function, no preconditions. No `debug_assert!`-only load-bearing contracts.
   **No gaps.**
4. **Write targets:** table (S5) and JSON (S6) → stdout, data; tracing →
   stderr, diagnostic; audit.md → repo file, artifact. No other writes.
   **No gaps.**
5. **Tracker references:** tethys-haw5 (verified open, blocked-on-jdly),
   tethys-3i35 (open, S4 shape), tethys-53iv / tethys-9z7i (open, residual-risk
   citations), tethys-tthy + tethys-n7nf (filed this feature), tethys-ygjx
   (open, annotated), tethys-rylk (open, alias rationale). All resolve; content
   verified during design. **No gaps.**

Hard-gate check: all slices have all mandatory fields; every loop has a
complexity statement; every slice has a stress fixture; claim coverage C1–C11
complete (map at top); tracker references resolve. Ready for checkpointed-build.
