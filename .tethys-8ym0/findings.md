# tethys-8ym0 — prove-it-prototype findings

**Feature:** emit refs for call-shaped identifiers inside macro token trees
(`assert_eq!(helper(), 1)` → a ref for `helper`), which the extractor drops
today (`MACRO_INVOCATION` returns early, src/languages/rust.rs:226-236; fence
`macro_token_identifier_not_emitted_as_value` pins it).

## Probes

- `probe.py` — independent tree-sitter walk (Python bindings, NOT tethys's
  `extract_references`) over the real src/ + tests/ (99 files). Classifies
  every identifier token inside a `token_tree` by shape (bare-call /
  method-call / scoped-call / macro-name / bare-ident), matches against the
  symbol table from a fresh `tethys index`, applies a reimplementation of the
  ygjx local-binding scope guard. Writes `survivors.tsv`.
- `probe2.py` — impact: reruns the y3bx untested-code BFS with survivor edges
  added, resolved same-file-first → workspace-unique (mirrors the resolver's
  `same_file`/`unique_workspace` arms).

Repro: `tethys index -w .` then `.tethys-8ym0/.venv/bin/python .tethys-8ym0/probe.py`
then `...probe2.py`.

## Oracle (independent: grep + hand-read of one full file) — and it agrees

Slice: all bare-call survivors in `tests/value_refs.rs`.

- **Oracle (`grep -nP '\bscalar\s*\('` + hand-reading each site)**: 11
  macro-context call sites of the in-crate helper `scalar` (lines 64-211;
  line 17 is the definition; `value_snapshot`, the file's other helper, is
  called only OUTSIDE macros at 238/240 and must not appear).
- **Probe**: exactly those 11 sites, and no `value_snapshot`, and no refs for
  the `fn higher` decoys that exist only inside fixture string literals.

**Agreement: 11/11 item-by-item, no false positives, no misses.**
Suspicious high-frequency survivors were individually verified: `scalar` ×25,
`pkg` ×10, `ctx` ×7 are all genuine in-crate test-helper functions called
inside assert macros — real refs, not guard escapes.

## Measurements (tethys src/ + tests/, 99 files) — the design-driving numbers

Raw identifier shapes inside token trees: bare_ident 4550, method_call 2345,
field_or_chain 1177, **bare_call 477**, path_head 431, scoped_ident 311,
scoped_call 178, macro_name 147.

| funnel | bare_call | method_call | scoped_call |
|---|---|---|---|
| raw | 477 | 2345 | 178 |
| in-crate symbol match (fn / method) | 180 | 508 | 3 |
| after scope guard | 177 | 508 | 3 |
| name-unique | 137 | — | 3 |
| resolvable same-file-first→unique | 166 | 189 | 0 |

- bare_call survivors: **86% test-context** (152/177); macros: assert-family
  116, vec 41, proptest 11, fmt 6.
- The 11 proptest bare_calls have NO containing fn symbol — proptest-defined
  fns aren't indexed (tethys-0nar); those sites stay unattached until 0nar.
- method_call name-matching is catastrophically ambiguous: `is_empty` ×192
  (std collision), 308/508 dropped as multi-candidate. Method shapes need
  receiver typing (53iv's `ReceiverCtx`) — that is tethys-9l27, not this issue.
- scoped_call is negligible (3 sites, all proptest) — the i09d overlap is
  immaterial here.

## Impact on untested-code (probe2, the y3bx payoff)

| scenario | untested count |
|---|---|
| baseline (today's index) | 260 |
| + bare_call survivor edges | **235 (−25)** |
| + method_call edges too | 200 (−35 more, but 9l27/receiver-typing territory) |

Newly covered by bare_call alone: exactly the expected class — coupling
display helpers, `crate_glob_covers`/`module_matches`/`is_root_reachable`
(visibility), `parse_use_wildcard`, `percent_encode_path`, and the assert-only
test helpers (`scalar`, `ref_target`, `dep_count`, `strategy_of`, …).

## What I learned that I did not know before running the probe

> **The feared 591-candidate noise number from the ygjx probe was a
> conflation of shapes. Once method-call and scoped shapes are separated out
> and the ygjx scope guard runs, the bare call-shape slice is only 177
> survivors and — on every sampled item — 100% genuine. The noise lives in
> the method-call shape (60% ambiguous, `is_empty` ×192), which is 9l27's
> problem, not 8ym0's. The prescribed fix is much safer than the issue
> feared, and "reassess the 591 number" resolves to 177/clean.**

Secondary model corrections:
- "name-unique ⇒ resolvable" undercounts: `scalar` is defined in 3 test files
  (ambiguous workspace-wide) but every site resolves same-file-first — the
  real resolver recovers 29 of the 40 non-unique survivors.
- tethys-9l27's description claims ygjx shipped macro-token value refs; the
  code and fence prove otherwise — correct 9l27's wording when touched.

## prove-it-prototype hard gate
- [x] Probe written, runs against the real codebase (99 files, src/ + tests/)
- [x] Oracle defined (grep + hand-read, tests/value_refs.rs) and produces output
- [x] Probe and oracle agree on a non-trivial slice (11/11 item-by-item)
- [x] Non-obvious learning written down (bare-call slice is clean; noise is
      method-shape and belongs to 9l27)
