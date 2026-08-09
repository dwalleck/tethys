# tethys-7a6a prove-it probe: current reachability vs raw-SQLite oracle

**Question (smallest non-trivial fact informing the unified traversal):**
Does the *current* public reachability implementation (`Tethys::get_forward_reachable` /
`get_backward_reachable`, BFS over `get_callees`/`get_callers`) produce, on the
production self-index, exactly the entries, depths, discovery paths, and BFS
order that an independent raw-SQLite BFS computes — and what does the legacy
`is_test` projection actually report on real data?

## Mechanism

- **Probe** (`.tethys-7a6a/probe.rs`, 63 lines): compiled with `rustc` against the
  current branch's `target/debug/libtethys.rlib` (targeted `cargo build` ran first;
  the prebuilt rlib was 21 h older than `src/db/graph.rs`). Uses ONLY the public
  API (`Tethys::new`, `get_forward_reachable`, `get_backward_reachable`). Runs on a
  **byte-copy** of the production index (`.rivets/index/tethys.db` → `/tmp/t7a6a-probe/ws/`,
  sha256 `e56b580e…` identical before and after; `Index::open`'s idempotent schema
  batch writes nothing on a current-schema index, so the live index was never touched).
  Dumps `META` + one `ENTRY` per result (seq, target id, depth, legacy `is_test`,
  qualified name, path ids) for slices `fwd@N`, `bwd@N`, `fwd@0`, `fwd@1`.
- **Oracle** (`.tethys-7a6a/oracle.py`): raw `sqlite3` over the same snapshot —
  symbols + `call_edges` only. Builds adjacency sorted by `qualified_name`
  (the neighbor-order contract the tethys queries encode as `ORDER BY
  s.qualified_name`), then plain FIFO BFS with a **first-discovery parent map**
  (paths reconstructed by walking one predecessor per symbol — no partial-path
  cloning). Emits the same lines plus the REAL `is_test` column and the discovery
  edge's `call_count`.
- **Comparator** (`.tethys-7a6a/compare.py`): item-by-item; also pins empty slices
  via META counts and asserts source-never-returned. `--legacy-is-test` audits
  the pre-cutover defect; the default requires real `symbols.is_test`.

## Commands (all ran in repo root; committed audit artifacts stay under `.tethys-7a6a/`; generated build and database copies stayed in `target/` and `/tmp/`)

```
cargo build --quiet                                   # refresh rlib/binary vs branch (Main-approved)
rm -rf /tmp/t7a6a-probe && mkdir -p /tmp/t7a6a-probe/ws/.rivets/index
cp .rivets/index/tethys.db /tmp/t7a6a-probe/ws/.rivets/index/tethys.db
rustc --edition 2024 .tethys-7a6a/probe.rs -L dependency=target/debug/deps \
      --extern tethys=target/debug/libtethys.rlib -o /tmp/t7a6a-probe/probe
python3 .tethys-7a6a/oracle.py /tmp/t7a6a-probe/ws/.rivets/index/tethys.db Tethys::get_forward_reachable 4 > .tethys-7a6a/oracle-a.out
python3 .tethys-7a6a/oracle.py /tmp/t7a6a-probe/ws/.rivets/index/tethys.db extract_references_recursive 4 > .tethys-7a6a/oracle-b.out
/tmp/t7a6a-probe/probe /tmp/t7a6a-probe/ws Tethys::get_forward_reachable 4 > .tethys-7a6a/probe-a.out
/tmp/t7a6a-probe/probe /tmp/t7a6a-probe/ws extract_references_recursive 4 > .tethys-7a6a/probe-b.out
python3 .tethys-7a6a/compare.py --legacy-is-test .tethys-7a6a/probe-a.out .tethys-7a6a/oracle-a.out > .tethys-7a6a/compare-a.txt   # rc=0
python3 .tethys-7a6a/compare.py --legacy-is-test .tethys-7a6a/probe-b.out .tethys-7a6a/oracle-b.out > .tethys-7a6a/compare-b.txt   # rc=0
# CLI surface (direction spellings, default 10, config error posture):
target/debug/tethys -w /tmp/t7a6a-probe/ws reachable Tethys::get_forward_reachable --direction forward -n 3   # rc=0
target/debug/tethys -w /tmp/t7a6a-probe/ws reachable extract_references_recursive --direction backward -n 2  # rc=0
target/debug/tethys -w /tmp/t7a6a-probe/ws reachable Tethys::get_forward_reachable --direction sideways -n 3 # rc=1, "configuration error: Invalid direction 'sideways'"
```

Sources: `Tethys::get_forward_reachable` (id 1747; fwd 22 / bwd 13 entries, depths
1–4 / 1–3) and `extract_references_recursive` (id 889; fwd 14 / bwd 17 entries,
depths 1–4 / 1–3). 66 traversed entries + direct-only and empty slices, both
directions.

## Comparison (compare-a.txt / compare-b.txt, both `RESULT: AGREE`)

| Slice | Traversal (ids/depths/paths/order) | is_test audit |
|---|---|---|
| fwd@4 (22 / 14) | identical, exactly in discovery order | 22 / 14 legacy flips, all real non-test targets; every flip == `edge_count != 0` |
| bwd@4 (13 / 17) | identical | matches real `symbols.is_test` exactly |
| fwd@0 | empty on both sides (count=0) | — |
| fwd@1 (4 / 7) | identical (direct-only) | all flips == `edge_count != 0` |

Invariants asserted per entry on both sides: `path.len() == depth`, no duplicate
target ids, source never returned as its own target (self-loop/cycle fact).

## Post-cutover oracle rerun

After `get_forward_reachable` and `get_backward_reachable` became canonical
delegators, the rebuilt probe was compared without `--legacy-is-test`. Both
production sources again returned `RESULT: AGREE`: all 66 entries matched on
IDs, depths, paths, discovery order, and real `symbols.is_test` in both
directions. Running the committed pre-cutover outputs with
`--legacy-is-test` still returns `RESULT: AGREE`, preserving the original
defect evidence rather than rewriting it.

## Corrections (documented)

1. First `compare.py` run crashed (`ValueError` parsing oracle's `edge_count=None`)
   and the tee'd artifacts were empty; fixed the parser (`ec != "None"`), reran,
   regenerated non-empty PASS artifacts with rc captured.
2. My initial exploratory closure counts (33/34 for `extract_references_recursive`)
   were computed from the LAST duplicate row of that qualified name; the probe's
   `get_symbol_by_qualified_name` (`query_row`, first row) resolves id 889. The
   oracle mirrors first-row semantics and agrees at 14/17. No probe/oracle
   disagreement ever existed — the error was in the throwaway exploration, not
   the shipped scripts.

## What was learned that was not known beforehand

1. **The `is_test` projection defect (tethys-6bui) is live in FORWARD reachability
   on real data, at 100% incidence:** every forward-reachable target of both
   sources — 47 entries total, all real non-test symbols — is reported
   `is_test=true` by the legacy API because `get_callees` feeds `ce.call_count`
   into `row_to_symbol`'s `is_test` slot. Backward is unaffected (real column).
   The unified bulk query must decode the real `is_test` column; a forward
   non-test fence is the correct guard.
2. **Duplicate qualified names are common in the production index** (75 groups,
   e.g. `extract_references_recursive` in BOTH `src/languages/csharp.rs` and
   `src/languages/rust.rs`), and the current lookup silently returns the first
   row. The canonical operation inherits this ambiguity at source resolution;
   the spec's NotFound edge does not cover duplicates.
3. **Discovery order is exactly plain FIFO BFS with qualified-name-ordered
   adjacency** — the legacy implementation's result sequence matches the
   independent oracle in both directions, so the spec's BFS-order contract is
   already the observed behavior. The CLI printer re-sorts each depth group by
   qualified name and truncates at 15/depth (`src/cli/reachable.rs`), so CLI
   output cannot witness discovery order or paths — only the library surface
   exposes the order criterion.
4. **Depth contract holds on real data:** depth 0 validates and returns empty;
   depth 1 is direct-only; bounded depths traverse monotonically; paths are
   shortest (`len == depth`), source-excluded, target-last — on all 66 entries.
