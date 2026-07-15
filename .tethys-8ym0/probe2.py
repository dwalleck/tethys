#!/usr/bin/env python3
"""probe2 for tethys-8ym0 — impact on untested-code (tethys-y3bx).

Reruns the y3bx multi-root BFS (is_test roots over refs) three ways:
  baseline            — refs as indexed today
  + bare_call edges   — probe survivors, resolved same-file-first then
                        workspace-unique (mirrors same_file/unique_workspace)
  + method_call edges — ditto for method shapes (9l27 territory; upper bound)

Usage: .venv/bin/python probe2.py [DB]   (run from repo root, after probe.py)
"""
import sqlite3, sys, pathlib, collections

ROOT = pathlib.Path(__file__).resolve().parent.parent
DB = sys.argv[1] if len(sys.argv) > 1 else str(ROOT / ".rivets/index/tethys.db")
c = sqlite3.connect(DB).cursor()

roots = [r[0] for r in c.execute("SELECT id FROM symbols WHERE is_test=1")]
base_edges = collections.defaultdict(set)
for a, b in c.execute("SELECT in_symbol_id, symbol_id FROM refs "
                      "WHERE in_symbol_id IS NOT NULL AND symbol_id IS NOT NULL"):
    base_edges[a].add(b)

# Symbol lookup tables for resolving survivors.
by_name = collections.defaultdict(list)      # (kind, name) -> [(id, path)]
spans = collections.defaultdict(list)        # path -> [(start, end, id)]
for sid, name, kind, path, s, e in c.execute(
        "SELECT s.id, s.name, s.kind, f.path, s.line, s.end_line FROM symbols s "
        "JOIN files f ON s.file_id=f.id"):
    by_name[(kind, name)].append((sid, path))
    if kind in ("function", "method"):
        spans[path].append((s, e or s, sid))

def containing_fn(path, line):
    best = None
    for s, e, sid in spans.get(path, ()):
        if s <= line <= e and (best is None or s > best[0]):
            best = (s, e, sid)
    return best[2] if best else None

def resolve(kind, name, path):
    cands = by_name.get((kind, name), [])
    same_file = [sid for sid, p in cands if p == path]
    if len(same_file) == 1:
        return same_file[0], "same_file"
    if len(cands) == 1:
        return cands[0][0], "unique_workspace"
    return None, f"ambiguous({len(cands)})" if cands else "no-candidate"

new_edges = {"bare_call": [], "method_call": [], "scoped_call": []}
unresolved = collections.Counter()
for row in (ROOT / ".tethys-8ym0/survivors.tsv").read_text().splitlines():
    shape, name, path, line, macro, is_test = row.split("\t")
    src = containing_fn(path, int(line))
    kind = "method" if shape == "method_call" else "function"
    dst, how = resolve(kind, name, path)
    if src is None or dst is None:
        unresolved[f"{shape}:{'no-src' if src is None else how}"] += 1
        continue
    new_edges[shape].append((src, dst))

def bfs(extra):
    edges = {k: set(v) for k, v in base_edges.items()}
    for a, b in extra:
        edges.setdefault(a, set()).add(b)
    seen = set(roots); stack = list(roots)
    while stack:
        n = stack.pop()
        for m in edges.get(n, ()):
            if m not in seen:
                seen.add(m); stack.append(m)
    return seen

prod = list(c.execute(
    "SELECT s.id, s.name, f.path FROM symbols s JOIN files f ON s.file_id=f.id "
    "WHERE s.is_test=0 AND s.kind IN ('function','method')"))

def untested(reach):
    return {(sid, n, p) for sid, n, p in prod if sid not in reach}

u_base = untested(bfs([]))
u_bare = untested(bfs(new_edges["bare_call"]))
u_meth = untested(bfs(new_edges["bare_call"] + new_edges["method_call"]))

print(f"roots={len(roots)} prod_fns={len(prod)}")
print(f"edge resolution drops: {dict(unresolved)}")
print(f"new edges: bare={len(new_edges['bare_call'])} "
      f"method={len(new_edges['method_call'])}")
print(f"untested baseline           = {len(u_base)}")
print(f"untested + bare_call edges  = {len(u_bare)}  "
      f"(newly covered: {len(u_base) - len(u_bare)})")
print(f"untested + method_call too  = {len(u_meth)}  "
      f"(method shapes add: {len(u_bare) - len(u_meth)})")
print("--- newly covered by bare_call (the 8ym0 payoff) ---")
for sid, n, p in sorted(u_base - u_bare, key=lambda r: (r[2], r[1])):
    print(f"  {n:35s} {p}")
