# Plan — tethys-usvm: SCC-restricted cycle enumeration

Five slices. All code lands in `src/db/graph.rs`; only the last slice touches
anything else. Design claims are numbered as in `.tethys-usvm/design.md`.

## Production scale used for every budget below

| Input | Value | Source |
|---|---|---|
| `V` (indexed files) | 116 on tethys itself; 10,000 in the probe fixture; 50,000 assumed for a large monorepo | probe + `tethys stats` |
| `E` (file_deps rows) | 400 on tethys itself; `≈ V` sparse, `≈ 4V` dense | probe |
| `C` (cycles returned) | 27,016 on tethys itself; 1 in the probe fixture | probe |

**Standing cost statement.** Today's enumeration is `O(V·(V+E))`
*unconditionally* — 50,005,000 visits for one cycle at V = 10,000, measured.
After this change it is `O((V+E)·min(V, C+1))`. The Tarjan passes are new work
added on top of a strictly smaller search, so the one shape that can regress is
"every node begins a non-trivial SCC" — dense cycles over few nodes, which is
exactly tethys's own index. Slice 4 fences precisely that shape. `tethys
cycles` is an on-demand query, not an always-on indexing phase, so the 10^6
always-on ceiling does not bind; the binding constraint is "must not exceed
today's cost on any shape."

---

## Slice 1: Iterative Tarjan over an induced subgraph

**Claim:** 9 — the SCC pass adds no recursion depth.

**Oracle:** Hand-computed SCC decompositions for graphs whose components are
obvious by inspection (two disjoint 2-cycles; a 3-cycle with an acyclic tail;
a chain). Independent of the search code, which does not exist yet in this
slice.

**Stress fixture:** A 100,000-node acyclic chain `n0 → n1 → … → n99999`,
induced set = all. A textbook recursive Tarjan overflows the stack here; the
iterative one returns 100,000 singleton components. This is the fixture the
slice exists for — every other Tarjan bug is caught by the decomposition
oracle, but recursion depth is invisible at unit-test scale.

Secondary fixture: empty `allowed` set → returns no components, no panic.

**Loop budget:** One `O(V + E)` pass — each node pushed and popped once, each
edge examined once. At V = 50,000 / E = 200,000 that is 2.5 × 10^5 operations
per call. Within budget for a single call; the number of calls is slice 2's
budget, not this one's.

**Files:** `src/db/graph.rs`

**Code (advisory):**
```rust
/// Strongly-connected components of the subgraph induced on `allowed`.
///
/// Iterative rather than the textbook recursion: `CycleSearch::visit` and
/// `CycleSearch::unblock` already recurse without a depth bound
/// (tethys-qqbi), and a third unbounded site would deepen that issue rather
/// than leave it where it is. Roots are taken in `order` so the result is
/// deterministic across runs.
fn strongly_connected_components(
    adj: &HashMap<FileId, Vec<FileId>>,
    allowed: &HashSet<FileId>,
    order: &[FileId],
) -> Vec<Vec<FileId>>
```
Explicit work stack of `(node, next_neighbor_index)`; `index`/`lowlink` maps;
an on-stack marker; components popped when `lowlink == index`.

**Verification:**
- [ ] Unit tests pass (decomposition oracle on 4 hand-computed graphs)
- [ ] Stress fixture produces expected outcome (100k chain: 100k singletons, no overflow)
- [ ] prove-it-prototype oracle still agrees with binary (unchanged — no call sites yet)
- [ ] Loop and wall budgets hold at fixture scale

---

## Slice 2: Drive the search from the SCC cursor

**Claim:** 1, 2, 5, 8, 10, 11, 12 — the cycle set is unchanged, no cycle is
lost, self-loops survive, blocking state does not leak, empty graphs are safe,
path-order canonicalization and direction significance are preserved.

**Oracle:** `.tethys-usvm/design_falsifier.py`'s brute-force enumerator —
exhaustive simple-path walk with no blocking, no SCC, no rank pruning. Already
agreed with both implementations on 53 graphs at design time; the Rust must
now join that agreement.

**Stress fixture:** `{0→1, 1→2, 2→1, 2000→0, 3→4}` — a 2-cycle `{1,2}` whose
minimum is reachable only *through* node 0, which is on no cycle, plus an
isolated edge `3→4` and a node ranked far away. This fixture fails under the
two most plausible bugs: advancing the cursor past the whole component instead
of `min + 1` (drops the cycle), and treating any non-empty SCC as live (never
skips anything, so slice 3's budgets blow). Paired with two disjoint cycles
`{0↔1, 2↔3}`, which fails if `blocked`/`blocked_by` leak between searches.

**Loop budget:** Outer loop runs once per SCC pass, bounded by
`min(V, C+1)` — proven at design time across 60 random graphs, tightest slack
0. Each iteration is one `O(V+E)` Tarjan plus a search confined to one
component. Sparse production shape (V = 10,000, E = 10,000, C = 1): 2 passes ×
2 × 10^4 = 4 × 10^4 operations, against 5 × 10^7 today. Dense shape
(V = 116, E = 400, C = 27,016): passes ≤ 116, so ≤ 116 × 516 ≈ 6 × 10^4
operations of new Tarjan work on top of a search that cannot grow. That
overhead is what slice 4 measures.

**Files:** `src/db/graph.rs`

**Code (advisory):**
```rust
let ranks: HashMap<FileId, usize> = nodes.iter().copied().zip(0..).collect();
let mut cursor = 0;
while cursor < nodes.len() {
    let allowed: HashSet<FileId> = nodes[cursor..].iter().copied().collect();
    outcome.scc_passes += 1;
    let live = strongly_connected_components(&adj, &allowed, &nodes[cursor..])
        .into_iter()
        .filter(|comp| hosts_cycle(comp, &adj))
        .min_by_key(|comp| comp.iter().map(|n| ranks[n]).min());
    let Some(comp) = live else { break };
    let start = /* least-ranked member of comp */;
    search.blocked.clear();
    search.blocked_by.clear();
    search.comp = comp.into_iter().collect();
    search.visit(start, start);
    cursor = ranks[&start] + 1;
}
```
`hosts_cycle(comp, adj)` = `comp.len() > 1 || adj[comp[0]].contains(comp[0])`.

`CycleSearch` gains `comp: HashSet<FileId>`; `visit` skips neighbours outside
it, in both the recursion loop and the `blocked_by` registration loop.

**Doc-comment-as-contract.** `visit`'s precondition is "`start` is the
least-ranked member of `comp`." Violating it would silently emit non-canonical
rotations — load-bearing for correctness, so it gets real enforcement rather
than a `debug_assert!`: the existing `compare_file_ids(neighbor, start) ==
Less` skip is **retained** even though component membership makes it
unreachable in the intended call. A mis-rooted component then degrades to
today's behaviour instead of emitting duplicate rotations. The doc comment
must say this is why the apparently-redundant test is there, or a later reader
will delete it as dead.

**Verification:**
- [ ] Unit tests pass (all six existing `enumerate_cycles_*` tests, unmodified)
- [ ] Stress fixture produces expected outcome (fringe cycle found; disjoint cycles both found)
- [ ] prove-it-prototype oracle still agrees with binary (`probe.py` at N ∈ {10, 100, 400})
- [ ] Loop and wall budgets hold at fixture scale

---

## Slice 3: Cost instrumentation and the cost fences

**Claim:** 3, 4, 7 — `visits == N` on a single cycle, `visits == 0` on an
acyclic graph, SCC passes within `min(V, C+1)`.

**Oracle:** Closed-form arithmetic from `findings.md`, independent of the
implementation: an N-node single cycle has exactly one non-trivial SCC, so
exactly one search runs and it walks N nodes. An acyclic graph has no
non-trivial SCC, so no search runs at all.

**Stress fixture:** The existing `enumerate_cycles_stays_output_sensitive_on_acyclic_dag`
6×10 layered DAG, with its budget tightened from `nodes²` (3,600) to
`nodes + edges` (60 + 324 = 384) — and in fact asserted at 0. This is the
fence that fails under "test SCC non-empty instead of non-trivial": that bug
leaves every singleton looking live, no start is skipped, and visits land back
near 3,600. Plus a 400-node single cycle asserting `visits == 400` exactly,
which fails under "compute SCCs but forget to confine `visit` to the
component" — the pass runs, the prune does not, and visits stay at 80,200.

**Loop budget:** No new loops. `scc_passes` is a counter increment on an
existing loop.

**Files:** `src/db/graph.rs`

**Code (advisory):** add `scc_passes: usize` to `CycleSearchOutcome`, log it
alongside `visits` in `enumerate_cycles`'s existing `tracing::debug!`. Three
new `#[test]` fns:
`enumerate_cycles_visits_each_node_once_on_single_cycle`,
`enumerate_cycles_skips_every_start_on_acyclic_graph`,
`enumerate_cycles_scc_passes_stay_bounded`.

**Output stream rule:** `scc_passes` is a **diagnostic**. It joins the
existing `tracing::debug!` on stderr, never stdout — `tethys cycles` stdout is
consumed by `| grep` and must stay parseable.

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixture produces expected outcome (DAG visits 0 within 384 budget; 400-cycle visits exactly 400)
- [ ] prove-it-prototype oracle still agrees with binary
- [ ] Loop and wall budgets hold at fixture scale

---

## Slice 4: Dense-graph no-regression fence

**Claim:** 6 — visits do not increase on a cycles-≫-nodes graph.

**Oracle:** The pre-change binary's own counter on the same fixture shape,
recorded before slice 2 lands. Independent of the new code by construction —
it is the old code's output.

**Stress fixture:** A graph in the shape that can regress: a small node set
that is one large SCC with many cycles. Six nodes fully connected both ways
plus a self-loop is dense enough to produce hundreds of cycles over a single
component, so every SCC pass finds a live component and the pass count hits
its `V` ceiling. This is the fixture that fails under "recompute SCCs once per
start node instead of jumping the cursor" — that bug leaves the pass count at
`V` unconditionally *and* re-runs Tarjan for starts that are already covered.
A happy-path sparse fixture cannot catch it; only a shape where the ceiling is
actually reached can.

The fence asserts a visit ceiling and a pass ceiling on this in-repo fixture,
not on the live index — the live index changes with the repo and would fail
CI on the next unrelated commit (design claim 13, indirect fence approved).

**Loop budget:** No new loops. Fixture is 7 nodes; enumeration cost is
whatever the implementation does, bounded by the assertion itself.

**Files:** `src/db/graph.rs`

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixture produces expected outcome (visits ≤ pre-change count, passes ≤ V)
- [ ] prove-it-prototype oracle still agrees with binary
- [ ] Loop and wall budgets hold at fixture scale

---

## Slice 5: End-to-end verification and changelog

**Claim:** 13, 14 — byte-identical output on the real index; the 10,000-file
fixture completes well under a second.

**Oracle:** md5 of the **pre-change** binary's `tethys cycles` output on this
branch's base index: `6cd0b5e753cfae4fc4c18a89ac165d61` over 27,016 cycles
(116 files, 400 edges), verified stable across three consecutive runs. For
claim 14, the recorded 9.17 s baseline at N = 10,000.

**Stress fixture:** The 10,000-file single-cycle workspace from
`.tethys-usvm/fixture.py` — the exact input the ticket measured. Re-run
`probe.py 10000`. Under a correct implementation `visits` must be 10,000 and
wall time under 1 s. This fixture fails under any bug that leaves the search
unconfined, because the closed form `N(N+1)/2` reappears immediately.

**Loop budget:** No new loops. This slice runs existing binaries.

**Files:** `changelog.d/tethys-usvm.fixed.md`, `.tethys-usvm/results.md`

**Output stream rule:** the changelog fragment is **data** for the release
tooling (`scripts/changelog-release.sh` reads it); `results.md` is audit-trail
documentation. Neither is a program output stream.

**Verification:**
- [ ] Unit tests pass (full `cargo nextest run`)
- [ ] Stress fixture produces expected outcome (10k: visits 10,000, wall < 1 s)
- [ ] prove-it-prototype oracle still agrees with binary (md5 matches `6cd0b5e7…`)
- [ ] Loop and wall budgets hold at fixture scale

---

## Plan Self-Review

### 1. Every loop — complexity stated and within budget?

| Loop | Slice | Complexity | Production scale | Verdict |
|---|---|---|---|---|
| Tarjan work-stack walk | 1 | `O(V+E)` per call | 2.5 × 10^5 at V=50k/E=200k | within budget |
| SCC cursor outer loop | 2 | `min(V, C+1)` iterations × `O(V+E)` | sparse: 2 × 2×10^4 = 4×10^4; dense (tethys): ≤116 × 516 ≈ 6×10^4 | within budget; strictly below today's `V·(V+E)` on every sparse shape, and fenced on the dense shape by slice 4 |
| `hosts_cycle` self-edge scan | 2 | `O(deg(n))`, neighbours pre-sorted and deduped | max degree ≈ 30 on tethys | negligible |
| rank map construction | 2 | `O(V)` once | 5 × 10^4 | negligible |

No loop is annotated `O(?)`. No gaps.

### 2. Every fixture — which bug class does it fail under?

| Fixture | Slice | Bug it is designed to catch |
|---|---|---|
| 100,000-node acyclic chain | 1 | recursive Tarjan (stack overflow) — invisible at unit scale |
| empty `allowed` set | 1 | missing empty-collection path |
| cycle behind an acyclic fringe | 2 | cursor advanced past the component instead of `min+1` (drops cycles) |
| two disjoint cycles | 2 | `blocked`/`blocked_by` leaking between searches |
| 6×10 layered DAG, budget 384 | 3 | "SCC non-empty" tested instead of "non-trivial" — no start ever skipped |
| 400-node single cycle, `visits == 400` | 3 | SCC computed but `visit` not confined to the component |
| dense single-SCC graph | 4 | SCCs recomputed per start instead of cursor-jumped |
| 10,000-file real workspace | 5 | any regression to the `N(N+1)/2` closed form |

No happy-path-only fixtures. No gaps.

### 3. Every doc-comment precondition — classified and enforced?

| Precondition | Slice | Class | Enforcement |
|---|---|---|---|
| `visit`: `start` is the least-ranked member of `comp` | 2 | **load-bearing for correctness** (silent non-canonical output) | the retained `compare_file_ids(neighbor, start) == Less` skip — survives release, degrades a mis-rooted component to today's behaviour rather than emitting duplicates |
| `strongly_connected_components`: `order` contains exactly the members of `allowed` | 1 | sanity hint (a mismatch yields a deterministic-but-odd root order, never wrong components — Tarjan is correct from any root order) | `debug_assert!` |

No documented precondition is left unenforced. No gaps.

### 4. Every write target — data or diagnostic?

| Target | Slice | Class | Stream |
|---|---|---|---|
| `scc_passes` / `visits` counters | 3 | diagnostic | `tracing::debug!` → stderr |
| `tethys cycles` cycle listing | unchanged | data | stdout, untouched by this plan |
| `changelog.d/tethys-usvm.fixed.md` | 5 | data (release tooling input) | file |
| `.tethys-usvm/results.md` | 5 | documentation | file |

No unexamined `println!` is introduced. No gaps.

### 5. Every tracker reference — resolves to an issue covering the deferred work?

| Reference | Where | Verified |
|---|---|---|
| tethys-qqbi | slices 1, 2 — recursion depth in `visit`/`unblock` stays out of scope; this plan only promises not to add a third site | open, P3, "unbounded recursion in detect_cycles aborts the process on deep graphs" — covers it |
| tethys-e3j1 | design negative space — dangling-endpoint posture unchanged | open, P3, "dangling-edge posture differs across sibling queries" — covers it |
| tethys-vwrn | design negative space — the SQL-side walk hazard | closed; cited as settled history, not a deferral |
| tethys-6k6b | design placement rationale — parent epic's single-adapter-seam guidance | open epic; cited as rationale, not a deferral |

No un-tracked deferrals. No gaps.
