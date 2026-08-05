#!/usr/bin/env python3
"""Cheapest falsifier for the SCC-restricted enumeration design.

Three independent implementations over the same graphs:

  unrestricted -- faithful port of the CURRENT Rust `CycleSearch`: every node
                  is a start, the walk is pruned only by `rank < start` and
                  Johnson's `blocked`/`B`.
  scc          -- the PROPOSED design: Johnson's outer loop, jumping the start
                  pointer to the least node of the least non-trivial SCC of
                  the subgraph induced on nodes at or after the pointer, with
                  the walk confined to that SCC. Tarjan is ITERATIVE (see
                  tethys-qqbi -- this change must not add a recursion site).
  brute        -- independent oracle: exhaustive simple-path walk from every
                  node with no blocking, no SCC, no rank pruning. Each cycle
                  is rediscovered once per member and collapsed by rotating
                  to its smallest-ranked node. Exponential; small graphs only.

Kill conditions -- any of these means the design is wrong:
  A  the three implementations disagree on the cycle set
  B  scc visits != N on an N-node single cycle (design buys nothing)
  C  scc visits != 0 on an acyclic graph (restriction is not restricting)
  D  scc passes > min(V, C+1)   (SCC work reintroduces the quadratic)
"""
from collections import defaultdict
import random
import sys

# --------------------------------------------------------------- helpers


def nontrivial(comp, adj):
    """An SCC hosts a cycle iff it has >1 node, or one node with a self-edge."""
    if len(comp) > 1:
        return True
    (only,) = tuple(comp)
    return only in adj.get(only, ())


def tarjan(adj, allowed, order):
    """Iterative Tarjan over the subgraph induced on `allowed`."""
    index, low, onstack, stack, comps = {}, {}, {}, [], []
    counter = 0
    for root in order:
        if root not in allowed or root in index:
            continue
        work = [(root, 0)]
        while work:
            node, pi = work[-1]
            if pi == 0:
                index[node] = low[node] = counter
                counter += 1
                stack.append(node)
                onstack[node] = True
            recursed = False
            nbrs = [n for n in adj.get(node, ()) if n in allowed]
            for i in range(pi, len(nbrs)):
                nb = nbrs[i]
                if nb not in index:
                    work[-1] = (node, i + 1)
                    work.append((nb, 0))
                    recursed = True
                    break
                if onstack.get(nb):
                    low[node] = min(low[node], index[nb])
            if recursed:
                continue
            if low[node] == index[node]:
                comp = set()
                while True:
                    w = stack.pop()
                    onstack[w] = False
                    comp.add(w)
                    if w == node:
                        break
                comps.append(comp)
            work.pop()
            if work:
                low[work[-1][0]] = min(low[work[-1][0]], low[node])
    return comps


def canon(path, rank):
    """Rotate a cycle so its smallest-ranked member leads."""
    m = min(range(len(path)), key=lambda i: rank[path[i]])
    return tuple(path[m:] + path[:m])


# --------------------------------------------------------- implementations


def unrestricted(adj, nodes):
    """Port of the current Rust search. Returns (cycle set, visits)."""
    rank = {n: i for i, n in enumerate(nodes)}
    cycles, visits = set(), 0

    def unblock(node, blocked, blocked_by):
        blocked.discard(node)
        for dep in list(blocked_by.pop(node, ())):
            if dep in blocked:
                unblock(dep, blocked, blocked_by)

    def visit(node, start, path, blocked, blocked_by):
        nonlocal visits
        visits += 1
        path.append(node)
        blocked.add(node)
        found = False
        nbrs = sorted(set(adj.get(node, ())), key=lambda n: rank[n])
        for nb in nbrs:
            if rank[nb] < rank[start]:
                continue
            if nb == start:
                cycles.add(tuple(path))
                found = True
            elif nb not in blocked and visit(nb, start, path, blocked, blocked_by):
                found = True
        if found:
            unblock(node, blocked, blocked_by)
        else:
            for nb in nbrs:
                if rank[nb] >= rank[start]:
                    blocked_by[nb].add(node)
        path.pop()
        return found

    for start in nodes:
        visit(start, start, [], set(), defaultdict(set))
    return cycles, visits


def scc(adj, nodes):
    """The proposed design. Returns (cycle set, visits, scc passes)."""
    rank = {n: i for i, n in enumerate(nodes)}
    cycles, visits, passes = set(), 0, 0

    def unblock(node, blocked, blocked_by):
        blocked.discard(node)
        for dep in list(blocked_by.pop(node, ())):
            if dep in blocked:
                unblock(dep, blocked, blocked_by)

    def visit(node, start, comp, path, blocked, blocked_by):
        nonlocal visits
        visits += 1
        path.append(node)
        blocked.add(node)
        found = False
        nbrs = sorted(set(adj.get(node, ())), key=lambda n: rank[n])
        for nb in nbrs:
            if nb not in comp:
                continue
            if nb == start:
                cycles.add(tuple(path))
                found = True
            elif nb not in blocked and visit(
                nb, start, comp, path, blocked, blocked_by
            ):
                found = True
        if found:
            unblock(node, blocked, blocked_by)
        else:
            for nb in nbrs:
                if nb in comp:
                    blocked_by[nb].add(node)
        path.pop()
        return found

    cursor = 0
    while cursor < len(nodes):
        allowed = set(nodes[cursor:])
        passes += 1
        live = [c for c in tarjan(adj, allowed, nodes[cursor:]) if nontrivial(c, adj)]
        if not live:
            break
        comp = min(live, key=lambda c: min(rank[n] for n in c))
        start = min(comp, key=lambda n: rank[n])
        visit(start, start, comp, [], set(), defaultdict(set))
        cursor = rank[start] + 1
    return cycles, visits, passes


def brute(adj, nodes):
    """Independent oracle: every simple cycle by exhaustive walk."""
    rank = {n: i for i, n in enumerate(nodes)}
    out = set()

    def dfs(start, node, path, on):
        for nb in adj.get(node, ()):
            if nb == start:
                out.add(canon(path, rank))
            elif nb not in on:
                on.add(nb)
                path.append(nb)
                dfs(start, nb, path, on)
                path.pop()
                on.discard(nb)

    for s in nodes:
        dfs(s, s, [s], {s})
    return out


# ----------------------------------------------------------------- graphs


def single_cycle(n):
    return {i: [(i + 1) % n] for i in range(n)}, list(range(n))


def layered_dag(width, layers):
    adj = defaultdict(list)
    for layer in range(layers - 1):
        for f in range(layer * width, (layer + 1) * width):
            for t in range((layer + 1) * width, (layer + 2) * width):
                adj[f].append(t)
    return dict(adj), list(range(width * layers))


def shapes():
    """Small graphs, each exercising a distinct input shape."""
    yield "empty", ({}, [])
    yield "single node, no edge", ({}, [0])
    yield "self-loop", ({0: [0]}, [0])
    yield "self-loop + fringe", ({0: [1], 1: [1]}, [0, 1])
    yield "two-cycle", ({0: [1], 1: [0]}, [0, 1])
    yield "single cycle n=8", single_cycle(8)
    yield "acyclic chain", ({0: [1], 1: [2], 2: [3]}, [0, 1, 2, 3])
    yield "layered dag 3x4", layered_dag(3, 4)
    yield "two disjoint cycles", ({0: [1], 1: [0], 2: [3], 3: [2]}, [0, 1, 2, 3])
    yield "figure eight", ({0: [1], 1: [2, 0], 2: [0]}, [0, 1, 2])
    yield "cycle + in/out fringe", (
        {0: [1], 1: [2], 2: [1], 2000: [0], 3: [4]},
        [0, 1, 2, 3, 4, 2000],
    )
    yield "one-way pair (no cycle)", ({0: [1]}, [0, 1])
    yield "duplicate edges", ({0: [1, 1], 1: [0, 0]}, [0, 1])
    rng = random.Random(20260805)
    for k in range(40):
        n = rng.randint(2, 7)
        adj = defaultdict(list)
        for a in range(n):
            for b in range(n):
                if rng.random() < 0.32:
                    adj[a].append(b)
        yield f"random#{k} n={n}", (dict(adj), list(range(n)))


# ------------------------------------------------------------------ main

failures = []


def check(label, cond, detail=""):
    if not cond:
        failures.append(f"{label}: {detail}")
    return cond


print("A. cycle-set agreement (unrestricted vs scc vs brute oracle)")
disagreements = 0
for name, (adj, nodes) in shapes():
    u_cycles, _ = unrestricted(adj, nodes)
    s_cycles, _, _ = scc(adj, nodes)
    b_cycles = brute(adj, nodes)
    if not (u_cycles == s_cycles == b_cycles):
        disagreements += 1
        print(f"   DISAGREE {name}")
        print(f"      unrestricted={sorted(u_cycles)}")
        print(f"      scc         ={sorted(s_cycles)}")
        print(f"      brute       ={sorted(b_cycles)}")
check("A", disagreements == 0, f"{disagreements} graphs disagreed")
print(f"   {'PASS' if disagreements == 0 else 'FAIL'} — 53 graphs, all three agree\n")

print("B. single-cycle visit count: scc must be N, current is N(N+1)/2")
for n in (10, 50, 100, 400, 1000):
    adj, nodes = single_cycle(n)
    sys.setrecursionlimit(max(10000, n * 12))
    _, u_visits = unrestricted(adj, nodes)
    _, s_visits, passes = scc(adj, nodes)
    ok = check(
        f"B n={n}", s_visits == n, f"scc visits {s_visits} != {n}"
    ) and check(
        f"B' n={n}",
        u_visits == n * (n + 1) // 2,
        f"unrestricted visits {u_visits} != {n * (n + 1) // 2}",
    )
    print(
        f"   n={n:>5}  current={u_visits:>9}  scc={s_visits:>6}  "
        f"passes={passes:>3}  {'ok' if ok else 'FAIL'}"
    )
print()

print("C. acyclic graphs: scc must visit nothing at all")
for name, (adj, nodes) in (
    ("chain n=200", ({i: [i + 1] for i in range(199)}, list(range(200)))),
    ("layered dag 6x10", layered_dag(6, 10)),
):
    cycles, visits, passes = scc(adj, nodes)
    ok = check(f"C {name}", not cycles and visits == 0, f"visits={visits}")
    print(
        f"   {name:<18} cycles={len(cycles)} visits={visits} passes={passes} "
        f"{'ok' if ok else 'FAIL'}"
    )
print()

print("D. scc passes bounded by min(V, C+1) — the dense-graph no-regression claim")
rng = random.Random(7)
worst = []
for k in range(60):
    n = rng.randint(3, 11)
    adj = defaultdict(list)
    for a in range(n):
        for b in range(n):
            if rng.random() < 0.42:
                adj[a].append(b)
    adj, nodes = dict(adj), list(range(n))
    cycles, _, passes = scc(adj, nodes)
    bound = min(len(nodes), len(cycles) + 1)
    worst.append((passes - bound, passes, bound, n, len(cycles)))
    check(f"D#{k}", passes <= bound, f"passes={passes} > bound={bound}")
worst.sort(reverse=True)
slack, passes, bound, n, c = worst[0]
print(
    f"   tightest of 60 random graphs: n={n} cycles={c} passes={passes} "
    f"bound=min(V,C+1)={bound}  slack={slack}"
)
print(f"   {'PASS' if slack <= 0 else 'FAIL'}\n")

if failures:
    print(f"DESIGN FALSIFIED — {len(failures)} check(s) failed:")
    for f in failures[:10]:
        print(f"  {f}")
    raise SystemExit(1)
print("design survived all four kill conditions")
