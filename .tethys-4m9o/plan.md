# tethys-4m9o plan — batch shortest dependency-chain queries

Five slices. Claim coverage maps to design.md C1–C10 as noted per slice.

## Slice 1: batched file hydration helper

**Claim:** C7 (hydration half) — a set of file ids hydrates in exactly one
statement, zero per-id lookups, any count.
**Oracle:** raw `sqlite3` rows for the same ids (independent of rusqlite
path); trace-hook statement count lands in slice 3.
**Stress fixture:** empty id slice (must issue ZERO statements — SQL
`IN ()` is invalid); duplicate ids in input (one row each, no error);
an id with no row (absent from result map, no error at this layer);
51 ids (cap-sized IN-list, well under SQLite's 999-placeholder floor).
Expected outputs written first: empty→empty map; dup→map len equals
distinct count; missing→map lacks the key; 51→map len 51.
**Loop budget:** O(k) placeholder build + O(k) map insert, k ≤ 51 by the
depth cap; production ceiling identical (cap is the bound, not index
size). One statement, one round trip.
**Files:** src/db/files.rs (helper + cfg(test) unit tests).

**Code (advisory):**
```rust
/// Hydrate a set of file ids in ONE statement (tethys-4m9o C7).
/// Absent ids are simply absent from the map; the CALLER decides
/// whether a hole is an error (chains: yes, Error::NotFound).
pub fn get_files_by_ids(&self, ids: &[FileId]) -> Result<HashMap<FileId, IndexedFile>> {
    if ids.is_empty() { return Ok(HashMap::new()); }
    let placeholders = vec!["?"; ids.len()].join(",");
    // SELECT {FILES_COLUMNS} FROM files WHERE id IN ({placeholders})
    // params_from_iter(ids.iter().map(FileId::as_i64)) → row_to_indexed_file
}
```

**Verification:**
- [ ] Unit tests pass (empty / dup / missing / 51-wide)
- [ ] Stress fixture produces expected outcome
- [ ] prove-it-prototype oracle still agrees with binary (unchanged paths)
- [ ] Loop budget holds (k ≤ 51)

## Slice 2: visited-set BFS replaces the walk-enumerating CTE

**Claim:** C1 termination, C2 shortest-by-edge-count, C3 edge validity +
endpoint inclusion, C4 equal→single, C5 disconnected→None.
**Oracle:** `.tethys-4m9o/oracle_bfs.py` (Python BFS over raw sqlite dump)
against the same fixture db file; design-prototype agreement already on
record for the real index.
**Stress fixture:** db-unit graph with a 2-edge route AND a 3-edge route
`a→b→t` / `a→c→d→t` (tie-break bug class: must return the 2-edge route,
len 3); a cycle `a→b→a` reachable from the source with target OUTSIDE it
(pre-fix behavior class: hang; expected: `None` in ms); equal endpoints
(len 1); zero-edge graph (None). Expected outputs written first, above.
**Loop budget:** adjacency load O(E) rows (396 real; 5×10⁵ at a 50k-file
production index — one statement, in-memory build); BFS O(V+E) ≤ ~10⁶ at
production scale, per-query not always-on; reconstruction O(L), L ≤ 51.
Within budget.
**Wall budget:** < 100 ms per query on tethys's own index (probe rerun
records actual; prototype measured 0.03 ms).
**Files:** src/db/graph.rs (rewrite `find_dependency_path`, delete
`parse_path_ids`; db-unit tests in existing cfg(test) space).

**Code (advisory):** early-return equal via slice-1 helper;
`build_adjacency_list()`; `VecDeque` BFS, `HashMap<FileId, FileId>`
parents, depth tracked per node, expand only while `depth < DEFAULT_MAX_DEPTH`;
on hit, reconstruct ids, hydrate via `get_files_by_ids`, reorder, any
hole → `Error::NotFound(format!("file id: {id}"))` (today's message).

**Verification:**
- [ ] Unit tests pass (routes / cycle / equal / empty)
- [ ] Stress fixture produces expected outcome (cycle case returns, fast)
- [ ] Oracle agrees on fixture db (oracle_bfs.py run against it)
- [ ] Budgets hold (fixture + real-index timing)

## Slice 3: probe fences — statement counts, cap boundary, self-loop, dangling

**Claim:** C7 statement counts (2 connected / 1 disconnected / 1 equal,
zero per-id at any length), C8 cap (50-edge chain → Some(51 nodes),
51-edge → None), C9 self-loop harmless, C10 dangling member → NotFound.
**Oracle:** rusqlite `trace` hook counting SQL shapes (n8pu fence
pattern — mechanism independent of the query builder); hand-built chain
arithmetic; raw-SQL-fabricated dangling row (`PRAGMA foreign_keys=OFF`).
**Stress fixture:** len-2 and len-4 paths (count must NOT grow with
length — the current per-id loop fails exactly this); 50/51-edge chains
(off-by-one class: `<=` vs `<`); `(x,x)` self-loop edge on the route's
source (unvisited-set class: hang); dangling `to_file_id` on the
shortest route (lenient-skip class: silently shorter chain). Expected:
counts (2,0)/(1,0)/(1,0); Some(51)/None; unperturbed shortest; NotFound.
**Loop budget:** fixture builds O(52) inserts; no new production loops.
**Files:** src/db/graph.rs (cfg(test) `chain_4m9o_fences` module, sibling
of the n8pu fence style in file_deps.rs).

**Verification:**
- [ ] Unit tests pass (counts / cap / self-loop / dangling)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle (trace hook + arithmetic) fires per-claim, distinct asserts
- [ ] Budgets hold (test-only)

## Slice 4: integration hardening through the Tethys facade

**Claim:** C1/C5 at facade level (cyclic fixture terminates; island →
None), C2/C3 exact shortest sequence, C4 `Some(vec![f])` replacing the
hedged same-file test, C6 missing-from / missing-to / both-missing with
from-checked-first order.
**Oracle:** hand-computed fixture topology (workspace files with real
`use` imports, indexed by `Tethys::index` — production pipeline, not
hand-inserted rows); NotFound message text.
**Stress fixture:** workspace whose import graph contains a genuine
cycle (mod-level `use` both directions via two modules imported by a
root) plus an island file imported by nothing and importing nothing —
pre-fix, the cyclic case hangs the test; competing short/long routes
via real imports; both endpoints missing (expects the FROM error text,
order class). Expected outputs written in-test before implementing.
**Loop budget:** none new (test code); fixture ≤ 8 files.
**Files:** tests/graph.rs (harden `..._returns_none_for_same_file` →
exact `Some(vec![...])`; `..._finds_shortest_path` → assert `Some` +
exact len; add cyclic/island/order tests).

**Verification:**
- [ ] Unit tests pass (whole tests/graph.rs)
- [ ] Stress fixture produces expected outcome (cyclic terminates fast)
- [ ] prove-it-prototype oracle agrees (probe pairs re-run post-build)
- [ ] Budgets hold (test-only)

## Slice 5: post-fix probe rerun, dogfood, changelog fragment

**Claim:** closes the loop on C1 (recorded measurement) and the design's
Consumers section (no caller left behind).
**Oracle:** `examples/probe_4m9o` rerun on the six recorded pairs vs
`oracle_bfs.py` (same pairs, same oracle as pre-fix — apples to apples);
`tethys callers` precision tier + `grep` recall tier for
`find_dependency_path` / `get_files_by_ids` / deleted `parse_path_ids`.
EXCEPTION RULE CHECK: this slice edits tethys's own graph query code, so
per AGENTS.md the tool cannot oracle itself — `grep` is the source of
truth, `tethys callers` recorded as advisory only.
**Stress fixture:** the deep pair (`src/lsp/provider.rs → src/cli/mod.rs`)
and the disconnected pair — the two that timed out pre-fix; expected:
CHAIN len=8 and NONE, each < 100 ms.
**Loop budget:** none (docs + measurements).
**Files:** .tethys-4m9o/probe2-output.txt + dogfood.md;
changelog.d/tethys-4m9o.fixed.md (category `fixed` — CLI users hit this
through the library API; fragment names `get_dependency_chain`,
the hang, and the fix; no rivets IDs in the fragment).

**Verification:**
- [ ] Probe rerun matches oracle on all six pairs, all < 100 ms
- [ ] grep shows zero remaining `parse_path_ids` references
- [ ] Changelog fragment passes `tests/changelog_lint.rs`
- [ ] Full gate: nextest + clippy pedantic + fmt + doctests

## Plan Self-Review

1. **Loops:** helper build/insert O(k≤51); adjacency load O(E) one
   statement; BFS O(V+E) ≤ ~10⁶ at 50k-file scale, per-query; path
   reconstruction O(L≤51); fixture builds O(52). All stated, all within
   budget; no always-on phases.
2. **Fixtures:** each targets a named bug class — tie-break (competing
   routes), non-termination (reachable cycle), off-by-one (50/51 cap),
   missing visited set (self-loop), lenient-skip (dangling), count-grows-
   with-length (len-2 vs len-4), order (both-missing), empty (zero-edge,
   empty id list), dup ids. No happy-path-only fixture.
3. **Doc-comment preconditions:** `get_files_by_ids` documents "absent
   ids are absent from the map" — not a precondition, a postcondition the
   caller consumes; chains enforce the hole→NotFound rule at runtime
   (load-bearing, survives release). No `debug_assert`-only contracts
   introduced. Existing `FilePath::new` empty→None contract untouched.
4. **Write targets:** production code writes nothing new to stdout/stderr;
   tracing `warn!`/`trace!` only (diagnostics). Probe example prints data
   to stdout (it is a data tool); oracle script likewise. No violations.
5. **Tracker references:** tethys-vwrn (fixed by this work, verified),
   tethys-r77e (orphan-edge misattribution, verified open), tethys-71if
   (result-DTO contract phase, verified open), tethys-u5o5 (cycles,
   verified open), tethys-syau (symbol paths, verified open). No
   uncited deferrals.
