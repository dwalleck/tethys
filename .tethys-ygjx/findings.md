# tethys-ygjx — prove-it-prototype findings

**Feature:** emit refs for (cat1) free-function identifiers used as *values*
(`iter.map(foo)`, `let g = foo;`) and (cat2) identifiers inside macro token
trees, which the Rust extractor currently drops.

## The probe

`probe.py` — an **independent** tree-sitter walk (Python `tree_sitter` +
`tree_sitter_rust`, ABI-matched to tethys's `tree-sitter 0.24` / grammar), run
over tethys's own real `src/` (92 files). It reimplements the *proposed*
extraction from scratch — it does **not** call tethys's `extract_references` —
so it is a genuinely different mechanism from the code under change.
`probe.py` classifies every `identifier` node by syntactic role (call
argument, `let` value, return, macro token-tree) and matches against the
in-crate symbol table dumped from a fresh `tethys index`.

Repro: `tethys index -w .` then `.tethys-ygjx/.venv/bin/python .tethys-ygjx/probe.py`.

## Oracle (independent: `grep` hand-count) — and it agrees

Slice: the free function `row_to_symbol` (`pub(crate) fn`, `src/db/helpers.rs:151`).

- **Oracle (`grep`)**: passed as a value (`.query_map(…, row_to_symbol)`) at
  **13** sites in `src/db/symbols.rs` alone; called directly (`row_to_symbol(row)?`)
  at 3 sites in `src/db/graph.rs`.
- **Probe**: independently finds the same 13 value-use sites.
- **Current tethys index (by `symbol_id`, not name — see 6rlu note)**: 4 refs
  total — 3 `call` (the graph.rs closures, captured by zp2j) + 1 `reexport`.
  **Zero of the 13 value-use sites are recorded.**

Probe and oracle **agree** on the ground truth (13 value-uses exist); the live
system is blind to all of them. That gap is exactly ygjx category 1, confirmed
against the real codebase.

## What I learned that I did not know before running the probe

> **Name-matching a value-position identifier against the symbol table is
> ~55% false positives, and even a params+`let` scope guard is insufficient —
> for-loop / closure / match-bound locals (`sym` ×25) still leak. Correct
> fn-as-value extraction therefore requires *either* full local-scope tracking
> (new complexity in a currently scope-free extractor) *or* a speculative band
> that defers the judgment. The design must choose explicitly.**

This was NOT obvious from the ticket, which frames the fix as "emit a ref for
the identifier." The measurements show the naive emit is dominated by noise.

## Measurements (tethys src/, 92 files) — the design-driving numbers

### Category 1 — fn-as-value
| filter | count | notes |
|---|---|---|
| raw identifiers in call-argument position | 1515 | far too broad |
| …matching ANY in-crate fn/method name | 141 | naive approach |
| …of those, matching a **method** name only | 58 | accessor noise: `symbols`, `command`, `files`, `from`, `start_line` — locals/fields passed by value |
| …matching a **free-function** name | 83 | still ~50% noise (`sym`×25, `workspace`×15, `ctx`×8 are locals) |
| after params+`let` scope guard | 56 | kills `workspace`, `ctx`; **`sym`×25 survives** (for/closure/match-bound) |
| **genuine callback family** | **~25–30** | `row_to_symbol`, `row_to_indexed_file`, `row_to_reference`, `row_to_import`, `saturating_depth_to_u32`, `ignore_broken_pipe`, `parse` |

`let`-value (35 raw / 2 in-crate) and return (38 raw / 1 in-crate) positions are
negligible — the argument slot is where fn-as-value actually lives.

### Category 2 — macro token-tree identifiers
| filter | count | notes |
|---|---|---|
| raw identifiers inside `token_tree` | 7078 | |
| …matching an in-crate fn name | 893 | dominated by `format!`/`assert!`/`vec!` args |
| restricted to **call-shape** (`ident(...)` inside macro) | 591 | still huge, still noisy |

**Category 2 is a strong out-of-scope candidate** (AC explicitly permits
documenting it out-of-scope with rationale): 591–893 candidate refs,
overwhelmingly argument noise, needing scope-aware guards beyond this fix's
budget. Recommend documenting out-of-scope + filing a targeted follow-up
(guarded call-shape-only macro extraction). **Flag at design pause:** tethys-y3bx
was parked *specifically* on this gap — confirm y3bx can proceed on cat1 alone
or must wait.

## Impact confirmed (why the blocked issues need this)
- **dvsw (dead code):** a fn used *only* as a value has 0 inbound call-refs →
  false-positive dead code. (Most tethys `row_to_*` fns dodge this only because
  they also carry a `reexport` ref; a non-exported value-only fn would be a live
  false positive — the `parse` unit-test fixture is the minimal case.)
- **7p54 (hotspots):** `row_to_symbol`'s ~13 value-uses are invisible → it ranks
  far colder than reality.

## Downstream interaction (negative space for the design)
`src/unused_imports.rs` already carries a **textual guard** working around this
bug — test `function_passed_as_value_is_suppressed` (line 497) asserts
"fn-as-value produces no extracted reference." Emitting a real value ref must
keep `find_unused_imports` correct (findings still empty) and should let that
textual fallback become a real-ref path — verify, don't break.

## Grammar ground truth (for the fix)
- cat1 call-arg: `identifier` is a child of an `arguments` node, NOT the
  `function` field of the `call_expression`.
- cat1 let-value: `let_declaration`'s `value` field is a bare `identifier`.
- cat2: `identifier` under `macro_invocation` → `token_tree`; even `foo()`
  inside a macro is `identifier` + sibling `token_tree`, never a `call_expression`.
- `TOKEN_TREE` node-kind constant already exists at `src/languages/rust.rs:64`.
- `ExtractedReferenceKind` (`src/languages/common.rs:141`) has
  Call/Constructor/Macro/Reexport/Type — a `Value` variant is the natural add.

## prove-it-prototype hard gate
- [x] Probe written, runs against the real codebase (tethys src/, 92 files)
- [x] Oracle defined (`grep` hand-count of `row_to_symbol`) and produces output
- [x] Probe and oracle agree on a non-trivial slice (13 value-uses; 0 in index)
- [x] Wrote down one non-obvious thing learned (the ~55% false-positive /
      scope-tracking finding above)
