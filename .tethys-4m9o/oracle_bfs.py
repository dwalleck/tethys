#!/usr/bin/env python3
"""Oracle for tethys-4m9o: shortest dependency chain by plain BFS.

Independent mechanism: raw sqlite3 edge dump + hand-rolled BFS queue,
no recursive CTE, no tethys code. Usage:
  oracle_bfs.py <db> <from_path> <to_path>
Prints: MISSING <which> | NONE | CHAIN len=<n> followed by the path.
"""
import sqlite3
import sys
from collections import deque

db, src, dst = sys.argv[1], sys.argv[2], sys.argv[3]
conn = sqlite3.connect(db)
files = {p: i for i, p in conn.execute("SELECT id, path FROM files")}
by_id = {i: p for p, i in files.items()}
adj = {}
for f, t in conn.execute("SELECT from_file_id, to_file_id FROM file_deps"):
    adj.setdefault(f, []).append(t)

for name, p in (("from", src), ("to", dst)):
    if p not in files:
        print(f"MISSING {name}")
        sys.exit(0)

start, goal = files[src], files[dst]
parent = {start: None}
q = deque([start])
while q:
    cur = q.popleft()
    if cur == goal:
        path = []
        while cur is not None:
            path.append(by_id[cur])
            cur = parent[cur]
        path.reverse()
        print(f"CHAIN len={len(path)}")
        for p in path:
            print(f"  {p}")
        sys.exit(0)
    for nxt in adj.get(cur, []):
        if nxt not in parent:
            parent[nxt] = cur
            q.append(nxt)
print("NONE")
