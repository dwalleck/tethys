#!/usr/bin/env python3
"""dvsw probe slice 2: textual word-boundary suppression over probe-1 survivors.

Definite = zero inbound refs AND zero word-boundary textual occurrences
outside the definition line, across all indexed files. Expected on the
warning-free self-index: ZERO Definite survivors (rustc dead_code oracle).
"""
import sqlite3, re

db = sqlite3.connect(".rivets/index/tethys.db")
db.row_factory = sqlite3.Row

RESOLVED = "EXISTS(SELECT 1 FROM refs r WHERE r.symbol_id = s.id)"
TEXTUAL_UNRES = """EXISTS(SELECT 1 FROM refs r WHERE r.symbol_id IS NULL
    AND (r.reference_name = s.name OR r.reference_name LIKE '%::' || s.name))"""
INHERIT_MARK = "EXISTS(SELECT 1 FROM refs r WHERE r.kind = 'inherit' AND r.in_symbol_id = s.id)"

survivors = db.execute(f"""
    SELECT s.id, s.name, s.kind, s.line, f.path
    FROM symbols s JOIN files f ON f.id = s.file_id
    WHERE s.visibility != 'public' AND s.is_test = 0
      AND s.kind NOT IN ('module', 'struct_field')
      AND NOT ({RESOLVED}) AND NOT ({TEXTUAL_UNRES}) AND NOT ({INHERIT_MARK})
    ORDER BY s.kind, f.path, s.line""").fetchall()

files = [r["path"] for r in db.execute("SELECT path FROM files").fetchall()]
print(f"probe-1 survivors after kind exclusion (module, struct_field): {len(survivors)}")

definite, maybe = [], []
for s in survivors:
    pat = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(s["name"]) + r"(?![A-Za-z0-9_])")
    hits = 0
    for path in files:
        try:
            text = open(path, encoding="utf-8").read()
        except OSError:
            continue
        for i, line in enumerate(text.splitlines(), 1):
            if path == s["path"] and i == s["line"]:
                continue  # definition line
            hits += len(pat.findall(line))
    (maybe if hits else definite).append((s, hits))

print(f"\nMAYBE (textual occurrences elsewhere -> suppressed): {len(maybe)}")
for s, h in maybe:
    print(f"  {s['name']:40s} {s['kind']:10s} {h:4d} hits  {s['path']}:{s['line']}")
print(f"\nDEFINITE (zero refs AND zero textual): {len(definite)}")
for s, _ in definite:
    print(f"  {s['name']:40s} {s['kind']:10s} {s['path']}:{s['line']}")
