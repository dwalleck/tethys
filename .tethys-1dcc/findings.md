# tethys-1dcc — prove-it-prototype findings

## Smallest question

"Which groups of non-rstest `#[test]` functions have near-identical bodies
(differing only in data), and how many functions would each collapse?"

## Probe

`probe.py` — strips string literals (fixture code!), extracts `#[test]` fn
bodies by brace-matching, normalizes (comments/char/int literals), clusters
per-file: exact tier (identical normalized body) + fuzzy tier
(`difflib` ratio ≥ 0.90). Full output: `probe-output.txt`.

**Result: 987 non-rstest tests; 87 groups ≥2; 142 collapsible fns.**

## Oracle (independent mechanisms) and agreement

1. **Extraction completeness** — `grep -c '#\[test\]'` vs probe regex, per
   file, on string-stripped text. Every delta (24 items) hand-classified:
   doc-comment mentions (backticked per doc_markdown) or fixture-string
   remnants. Zero missed real tests, zero phantoms after the fix. AGREE.
2. **Top group** (`src/indexing.rs` `is_excluded_dir` ×13) — hand-read:
   genuine one-line asserts differing only in a `&str` + expected bool. AGREE.
3. **Fuzzy group** (`src/unused_imports.rs` ×6) — hand-read: bodies match
   modulo *whole workspace-fixture arrays*. Parameterizable but cases would
   be multi-line fixture blobs. AGREE (with scoping caveat below).
4. **Probe-silent file** (`tests/deprecated_callers.rs`, 29 tests) —
   hand-read: heterogeneous asserts over shared fixture-builder fns; probe
   correctly reports no groups. AGREE.

## Probe v1 failure (kept for the record)

v1 parsed raw text and reported a 6-fn `test_add` "group" in
`test_topology.rs` — all inside `r#"..."#` fixture strings (tethys is an
indexer; its tests embed source code as strings). Fixed by stripping string
literals before extraction, which also removed a latent brace-matching
hazard. 1004 → 987 tests, 99 → 87 groups.

## What I learned (didn't know before)

The repo ALREADY has an rstest convention (8 files, 24 `#[rstest]` fns,
named `#[case::name(...)]` cases, batch/streaming axis) — the issue text
reads as if adoption were new; the remaining candidate pool splits into
scalar-data groups (ideal, concentrated in src/ unit-test modules) and
fixture-heavy groups (conversion would hurt readability); and no real async
tests exist (every `#[tokio::test]` is fixture content), so rstest async
mode is out of scope.

## Facts the design must respect

- Convention exists: `#[rstest]` + `#[case::snake_name(args)]`, doc comment
  preserved on the fn, case names carry the old fn names' discriminating
  suffix.
- clippy pedantic + doc_markdown runs on tests: backtick identifiers in any
  doc comments we touch.
- Candidate ceiling 142 fns; readable subset (scalar case data) is smaller —
  design must give a crisp inclusion rule, not chase the ceiling.
- `tests/deprecated_callers.rs`-style shared-fixture files are a DIFFERENT
  good pattern; do not disturb.
- nextest counts each `#[case]` as a separate test: total test count must
  not DROP after conversion (cases ≥ replaced fns, per group).
