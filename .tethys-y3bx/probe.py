#!/usr/bin/env python3
# probe.py — tethys-y3bx untested-code: symbols unreachable from any test root.
#
# PROBE: multi-root forward BFS from is_test symbols over the REFS table
# (in_symbol_id -> symbol_id). Untested = product fns/methods not in the
# reachable closure. Also computes the same over CALL_EDGES to measure the
# gap the AC warns about ("consume refs, not only call_edges").
#
# Usage: python3 .tethys-y3bx/probe.py [DB]
import sqlite3, sys

db = sqlite3.connect(sys.argv[1] if len(sys.argv) > 1 else ".rivets/index/tethys.db")
c = db.cursor()

roots = [r[0] for r in c.execute("SELECT id FROM symbols WHERE is_test=1")]

def closure(edge_sql):
    edges = {}
    for caller, callee in c.execute(edge_sql):
        edges.setdefault(caller, set()).add(callee)
    seen = set(roots); stack = list(roots)
    while stack:
        n = stack.pop()
        for m in edges.get(n, ()):
            if m not in seen:
                seen.add(m); stack.append(m)
    return seen

reach_refs = closure(
    "SELECT in_symbol_id, symbol_id FROM refs "
    "WHERE in_symbol_id IS NOT NULL AND symbol_id IS NOT NULL")
reach_ce = closure("SELECT caller_symbol_id, callee_symbol_id FROM call_edges")

# Product functions/methods (exclude tests themselves).
prod = list(c.execute(
    "SELECT s.id, s.name, f.path FROM symbols s JOIN files f ON s.file_id=f.id "
    "WHERE s.is_test=0 AND s.kind IN ('function','method')"))

untested_refs = [(n, p) for sid, n, p in prod if sid not in reach_refs]
untested_ce = [(n, p) for sid, n, p in prod if sid not in reach_ce]

print(f"roots={len(roots)} prod_fns={len(prod)}")
print(f"reachable(refs)={len(reach_refs)}  untested(refs)={len(untested_refs)}")
print(f"reachable(call_edges)={len(reach_ce)}  untested(call_edges)={len(untested_ce)}")
print(f"AC gap (refs covers these but call_edges calls them untested): "
      f"{len(set(untested_ce) - set(untested_refs))}")

# Oracle spot-checks: is a specific symbol classified tested/untested?
def classify(name):
    ids = [sid for sid, n, p in prod if n == name]
    if not ids: return f"{name}: NOT a product fn symbol"
    tested = any(sid in reach_refs for sid in ids)
    return f"{name}: {'TESTED (reachable)' if tested else 'UNTESTED'}"

for q in sys.argv[2:]:
    print("  " + classify(q))

print("--- sample untested (refs) ---")
for n, p in sorted(untested_refs)[:15]:
    print(f"  {n}  {p}")
