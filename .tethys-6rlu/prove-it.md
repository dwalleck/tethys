# prove-it-prototype — tethys-6rlu

**Feature (smallest question):** After the fix denormalizes `symbols.name` into
`refs.reference_name`, does `SELECT count(*) FROM refs WHERE reference_name='foo'
AND kind='call'` return the real call sites of an in-crate free function `foo`?

## Probe
`.tethys-6rlu/probe.py` — produces the proposed feature's output from the real
index: for each target name, `(resolved call-refs whose symbol_id is that
function) + (unresolved call-refs already named it)`. The first term is exactly
what the fix would re-label `reference_name` to. Uses only refs/symbols.

## Oracle (independent)
`.tethys-6rlu/oracle.sh` — counts free-function call sites textually with
ripgrep, no DB and no resolver: `occurrences of "X(" − ".X(" (method) − "fn X("
(definition)`. Independent mechanism (lexical), unaffected by any resolver bug.

## Agreement (fresh index, 78 files / 1430 symbols / 14356 refs)
| function | probe (post-fix name-query) | oracle (textual call sites) | |
|---|---|---|---|
| extract_call_reference | 1 | 1 | AGREE |
| extract_struct_constructor | 1 | 1 | AGREE |
| node_span | 14 | 14 | AGREE |
| node_text | 59 | 59 | AGREE |
| parse_scoped_identifier | 5 | 5 | AGREE |

Non-trivial slice: 5 distinct functions, counts 1→59 (80 call sites total),
exact agreement. The resolved call-refs the fix would re-name correspond 1:1 to
real call sites — not phantom, not missing — so post-fix name queries are sound.

## What I learned (wasn't obvious before)
The on-disk index `.rivets/index/tethys.db` was **stale** relative to the working
tree (built against an older snapshot: line numbers offset by ~16, and one
`node_text` call site had since been deleted) — the apparent off-by-one (60 vs
59) was pure staleness, NOT a phantom edge. Consequence for the design: the
regression fence for this fix **must build its own fresh index from a fixture**,
never query an ambient/pre-existing DB, or it will measure the wrong snapshot.

## Carried into falsifiable-design
Name-collision input shape surfaced: a post-fix `WHERE reference_name='X'`
returns refs to *every* symbol named X (function + method + struct…). The probe
filtered `kind='function'`; the real feature won't. That shape needs a claim.
