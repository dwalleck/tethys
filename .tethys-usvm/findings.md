# Probe findings — tethys-usvm

## Smallest proven question

On an N-file graph containing exactly ONE directed cycle, how many node
visits does cycle enumeration make?

Chosen because it isolates the ticket's claim: the answer size is fixed at
one cycle, so anything that grows with N is cost the enumeration should not
be paying.

## Probe and independent oracle

- **Probe** — `.tethys-usvm/fixture.py` generates a real Rust workspace of N
  files where `f{i}` has `use crate::f{i+1}`, closing with `f{N-1}` →
  `f0`, and indexes it with the release binary. `.tethys-usvm/probe.py` runs
  `tethys cycles` against it and reads the `visits` counter out of the
  binary's own `tethys=debug` log.
- **Oracle** — hand-derived arithmetic, no code shared with the
  implementation. From start `f_k` the search is confined to nodes ≥ `f_k`
  in path order. Walking forward it reaches `f_{N-1}`, whose only edge
  points back to `f0`; `f0 < f_k`, so that edge is skipped and the walk
  dies. Start `f_k` therefore costs exactly `N − k` visits and finds
  nothing, for every `k > 0`; only `f0` closes the cycle. Total =
  `Σ(N−k) = N(N+1)/2`.

Filenames are zero-padded so lexicographic path order equals numeric order
— `enumerate_cycles` canonicalizes on path order, and the derivation above
is only valid when the two coincide.

### Agreement

Exact at all nine sizes measured, with the answer fixed at one cycle:

| files | probe visits | oracle N(N+1)/2 | cycles | secs |
|-------:|-------------:|----------------:|-------:|-----:|
| 10 | 55 | 55 | 1 | 0.02 |
| 50 | 1,275 | 1,275 | 1 | 0.01 |
| 100 | 5,050 | 5,050 | 1 | 0.01 |
| 200 | 20,100 | 20,100 | 1 | 0.02 |
| 400 | 80,200 | 80,200 | 1 | 0.02 |
| 1,000 | 500,500 | 500,500 | 1 | 0.15 |
| 2,000 | 2,001,000 | 2,001,000 | 1 | 0.43 |
| 4,000 | 8,002,000 | 8,002,000 | 1 | 1.59 |
| 10,000 | 50,005,000 | 50,005,000 | 1 | **9.17** |

Fitted exponent over 2,000 → 10,000: **1.90**. The ticket reported 1.93 and
9.43 s at 10,000 files; reproduced.

A second, cheaper oracle confirmed the fixture itself: the six `file_deps`
rows tethys recorded for the N=6 workspace match the six `use crate::`
statements grepped straight out of the generated sources, one for one.

## What I learned that I did not know before

**The cost is an exact closed form, not an asymptotic estimate.** `visits`
equals `N(N+1)/2` to the unit at every size tested. That converts the fix
from "should be faster" into a falsifiable number: under an SCC
restriction, starts `f_1..f_{N-1}` sit in trivial SCCs of the subgraph
induced on nodes ≥ themselves and must be skipped outright, so the visit
count has to land on exactly **N**. Anything else means the restriction is
not doing what it claims.

## Three further findings

1. **The ticket's byte-for-byte oracle is stale.** AC 4 pins `tethys
   cycles` on tethys's own index to 24,025 cycles / md5
   `e80b712d9855e6807fef26e623625ab1`. On this branch's base that is now
   **116 files, 400 edges, 27,016 cycles, md5
   `6cd0b5e753cfae4fc4c18a89ac165d61`** (stable across three consecutive
   runs) — main gained a file since the ticket was filed on 2026-08-03.
   The invariant is sound and worth keeping; the constant has to be
   re-pinned to this branch's merge-base or the check tests the calendar
   rather than the code.

2. **The dense case is already output-sensitive, and is the regression
   risk.** On tethys's own index the search makes 135,888 visits for 27,016
   cycles — about 5 visits per cycle returned, over only 116 nodes. The
   blocking added in PR #40 review is working there. So the SCC restriction
   has nothing to win on dense graphs and something to lose: it must not
   make 0.13 s worse. Baseline to hold: **min 0.130 s, median 0.170 s**
   over five runs.

3. **The gap is entirely the fruitless-tail case** — start nodes that
   cannot reach back to themselves. Sparse or near-acyclic workspaces pay
   for it; densely cyclic ones do not.

## Carried into design as an unverified claim

Standard Johnson advances the start pointer to the least vertex of the
least non-trivial SCC, so the number of SCC computations is bounded by
`min(V, C+1)` rather than `V`. That is read off the algorithm, not
measured, and reasoning of exactly this shape has been wrong here before.
It is the load-bearing assumption behind "the dense case does not regress",
so the design's cheapest falsifier must instrument the SCC-pass count and
measure both fixtures — not argue it.

## Scope boundary

This probe establishes the current cost and the two output baselines. It
does not claim the SCC restriction is correct, cheap, or free of recursion
hazards; those are the design's falsifiable claims.
