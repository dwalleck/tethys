# prove-it-prototype findings — tethys-y3bx (untested-code)

## Smallest question
"Do the `is_test` roots the analysis depends on match the test functions that
actually exist in the source?"

## Probe (against real self-index, freshly rebuilt with current-source binary)
`SELECT COUNT(*) FROM symbols WHERE is_test=1;` → **330**, ALL in `tests/`.

## Oracle (independent — grep the source)
`#[test]` in src/ = 467, in tests/ = 353; + `#[tokio::test]` 5 + `#[rstest]` 17
≈ **842 test functions**. Cross-checked against `cargo nextest`: 843 test cases.

## Disagreement: 330 vs ~842 (probe finds ~40% of test roots)
Not a staleness artifact — a clean re-index with a from-current-source binary
moved it 327→330. Root-caused to a real substrate bug.

## Root cause (code + data confirmed)
`src/languages/rust.rs:954` — the `MOD_ITEM` arm of `extract_symbols_recursive`
records the module as a `SymbolKind::Module` symbol but **does not recurse into
the module body (`declaration_list`)**, unlike the `_` default arm (rust.rs:959)
and `IMPL_ITEM` (rust.rs:917). So every symbol declared inside an inline
`mod { … }` block is dropped. `#[cfg(test)] mod tests { #[test] fn … }` is
exactly such a block.

Data confirming the mechanism:
- 26 `tests` module SHELL symbols exist (kind=module); their bodies are empty.
- 0 unit-test functions indexed (known unit tests `is_excluded_dir_allows_lib`,
  `normalize_path_is_idempotent`, `extracts_simple_function` → not symbols).
- 212 src/ function symbols exist — all PRODUCT top-level functions.
- 4449 refs originate from `is_test` symbols — all from tests/ integration tests.

## Why this GATES untested-code
The analysis reports symbols not reachable from any `is_test` root. With
unit tests absent as roots AND their edges into product code absent from
`refs`, every function covered ONLY by a unit test (the majority on a
unit-test-heavy codebase) would be FALSELY reported "untested." That inverts
the PRD's near-zero-false-positive requirement — the whole value proposition.

## What I learned that I didn't know before probing
tethys does not index inline module bodies at all — so `is_test` covers only
integration tests, and unit-test coverage is invisible to the reference graph.
The y3bx spec's premise ("test detection already exists") is false for the
dominant test form on this codebase.

## Recommendation
STOP untested-code. File the inline-module-body indexing bug; it blocks y3bx
(and improves is_test / reachability / dead-code broadly). The fix has a real
blast radius (indexing test code changes what every analysis sees) and deserves
its own design pass, not a bolt-on.
