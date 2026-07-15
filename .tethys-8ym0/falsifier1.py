#!/usr/bin/env python3
"""Cheapest falsifier for the tethys-8ym0 design (run pre-implementation).

Claim under attack: adding resolved macro-token call refs to the self-index
does NOT change unused-imports / visibility-tightening / deprecated-callers /
panic-points output (analyses are kind-blind on evidence, macro refs are
same-package usage), and callers/impact stay untouched (call_edges excluded).

Method: insert the probe's bare_call survivors as resolved refs directly into
the LIVE self-index DB (kind='value' stand-in — same pipeline posture as the
proposed 'macro_call': excluded from call_edges, parseable by analyses today),
resolved same-file-first -> workspace-unique. The caller must snapshot
analysis outputs before and after, then restore the DB backup.

Usage: .venv/bin/python falsifier1.py apply   (from repo root)
"""
import sqlite3, sys, pathlib, collections

ROOT = pathlib.Path(__file__).resolve().parent.parent
db = sqlite3.connect(str(ROOT / ".rivets/index/tethys.db"))
c = db.cursor()

by_name = collections.defaultdict(list)
spans = collections.defaultdict(list)
files = {p: fid for fid, p in c.execute("SELECT id, path FROM files")}
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

rows = []
for rec in (ROOT / ".tethys-8ym0/survivors.tsv").read_text().splitlines():
    shape, name, path, line, macro, is_test = rec.split("\t")
    if shape != "bare_call":
        continue
    cands = by_name.get(("function", name), [])
    same = [sid for sid, p in cands if p == path]
    if len(same) == 1:
        dst, strat = same[0], "same_file"
    elif len(cands) == 1:
        dst, strat = cands[0][0], "unique_workspace"
    else:
        continue
    rows.append((dst, files[path], int(line), 1,
                 containing_fn(path, int(line)), None, strat))

c.executemany(
    "INSERT INTO refs (symbol_id, file_id, kind, line, column, in_symbol_id,"
    " reference_name, strategy) VALUES (?, ?, 'value', ?, ?, ?, ?, ?)",
    [(d, f, l, col, src, rn, s) for d, f, l, col, src, rn, s in rows])
db.commit()
print(f"inserted {len(rows)} stand-in macro_call refs (kind='value')")
