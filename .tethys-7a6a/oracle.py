#!/usr/bin/env python3
"""Oracle for tethys-7a6a. Computes reachability from the RAW sqlite index
with no tethys code: load symbols + call_edges, build adjacency sorted by
qualified_name (the neighbor-order contract the tethys queries encode in
`ORDER BY s.qualified_name`), then plain FIFO BFS with a first-discovery
parent map — at most one predecessor per symbol, paths reconstructed by
walking parents (independent of the legacy partial-path cloning).

Emits the same META/ENTRY lines as the probe, plus the REAL is_test column
and the discovery edge's call_count so the legacy forward projection can be
audited. Slices mirror the probe: fwd@N, bwd@N, fwd@0, fwd@1.

Usage: oracle.py <db> <qualified_symbol> <max_depth>
"""
import sqlite3
import sys
from collections import deque

db, symbol, max_depth = sys.argv[1], sys.argv[2], int(sys.argv[3])
conn = sqlite3.connect(db)
syms = conn.execute("SELECT id, qualified_name, is_test FROM symbols").fetchall()
qn = {i: q for i, q, _ in syms}
real_test = {i: str(bool(t)).lower() for i, _, t in syms}
src = next((i for i, q, _ in syms if q == symbol), None)
if src is None:
    print(f"ERR symbol={symbol}: not found")
    sys.exit(1)

fwd = {}
counts = {}
for caller, callee, cnt in conn.execute(
    "SELECT caller_symbol_id, callee_symbol_id, call_count FROM call_edges"
):
    fwd.setdefault(caller, []).append(callee)
    counts[(caller, callee)] = cnt
for a in fwd.values():
    a.sort(key=lambda x: qn[x])  # mirrors ORDER BY s.qualified_name
bwd = {}
for caller, callees in fwd.items():
    for c in callees:
        bwd.setdefault(c, []).append(caller)
for a in bwd.values():
    a.sort(key=lambda x: qn[x])


def reachability_slice(adj, depth, tag):
    parent = {src: None}
    d = {src: 0}
    queue = deque([src])
    out = []
    while queue:
        cur = queue.popleft()
        if d[cur] >= depth:
            continue
        for nb in adj.get(cur, []):
            if nb not in parent:  # first discovery wins; source never re-enters
                parent[nb] = cur
                d[nb] = d[cur] + 1
                out.append(nb)
                queue.append(nb)
    print(
        f"META {tag} symbol={symbol} source_id={src} max_depth={depth} "
        f"count={len(out)} dir={'forward' if tag.startswith('fwd') else 'backward'}"
    )
    for i, tgt in enumerate(out):
        path = []
        x = tgt
        while x != src:
            path.append(x)
            x = parent[x]
        path.reverse()
        edge_count = counts.get((parent[tgt], tgt))
        print(
            f"ENTRY {tag} seq={i} id={tgt} depth={d[tgt]} is_test={real_test[tgt]} "
            f"qn={qn[tgt]} path=[{','.join(map(str, path))}] edge_count={edge_count}"
        )


reachability_slice(fwd, max_depth, "fwd")
reachability_slice(bwd, max_depth, "bwd")
reachability_slice(fwd, 0, "fwd0")
reachability_slice(fwd, 1, "fwd1")
