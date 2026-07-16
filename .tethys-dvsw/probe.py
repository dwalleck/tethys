#!/usr/bin/env python3
"""dvsw probe: layered dead-code funnel over the fresh tethys self-index.

Q: which non-public, non-test symbols have zero inbound evidence, and what
does each suppression channel absorb? Survivors listed for the grep oracle.
"""
import sqlite3, sys

db = sqlite3.connect(".rivets/index/tethys.db")
db.row_factory = sqlite3.Row
q = lambda sql, *p: db.execute(sql, p).fetchall()

one = lambda sql, *p: db.execute(sql, p).fetchone()[0]

total = one("SELECT COUNT(*) FROM symbols")
cand0 = one("SELECT COUNT(*) FROM symbols WHERE visibility != 'public' AND is_test = 0")
print(f"L0 symbols={total}  L1 non-public non-test candidates={cand0}")

# Evidence channels, as EXISTS predicates against symbol s.id
RESOLVED = "EXISTS(SELECT 1 FROM refs r WHERE r.symbol_id = s.id)"
SPEC_ONLY = f"""{RESOLVED} AND NOT EXISTS(
    SELECT 1 FROM refs_banded rb WHERE rb.symbol_id = s.id AND rb.band != 'speculative')"""
TEXTUAL = """EXISTS(SELECT 1 FROM refs r WHERE r.symbol_id IS NULL
    AND (r.reference_name = s.name OR r.reference_name LIKE '%::' || s.name))"""
INHERIT_MARK = "EXISTS(SELECT 1 FROM refs r WHERE r.kind = 'inherit' AND r.in_symbol_id = s.id)"

base = "FROM symbols s WHERE s.visibility != 'public' AND s.is_test = 0"
alive_resolved = one(f"SELECT COUNT(*) {base} AND {RESOLVED}")
spec_only = one(f"SELECT COUNT(*) {base} AND {SPEC_ONLY}")
after_resolved = cand0 - alive_resolved
alive_textual = one(f"SELECT COUNT(*) {base} AND NOT ({RESOLVED}) AND {TEXTUAL}")
alive_inherit = one(
    f"SELECT COUNT(*) {base} AND NOT ({RESOLVED}) AND NOT ({TEXTUAL}) AND {INHERIT_MARK}")
print(f"L2 alive via resolved ref: {alive_resolved} (of which speculative-band-only: {spec_only})")
print(f"L3 alive via unresolved textual match: {alive_textual}")
print(f"L4 alive via inherit method marker: {alive_inherit}")

# L5: containers with a live (L2/L3/L4) descendant via parent_symbol_id
ALIVE_ANY = f"({RESOLVED} OR {TEXTUAL} OR {INHERIT_MARK})"
LIVE_DESC = f"""s.kind IN ('module','struct','enum','trait') AND EXISTS(
  WITH RECURSIVE desc(id) AS (
    SELECT id FROM symbols WHERE parent_symbol_id = s.id
    UNION ALL
    SELECT c.id FROM symbols c JOIN desc d ON c.parent_symbol_id = d.id)
  SELECT 1 FROM desc JOIN symbols s2 ON s2.id = desc.id
  WHERE {ALIVE_ANY.replace('s.id', 's2.id').replace('s.name', 's2.name')})"""
dead_pred = f"NOT ({RESOLVED}) AND NOT ({TEXTUAL}) AND NOT ({INHERIT_MARK})"
alive_container = one(f"SELECT COUNT(*) {base} AND {dead_pred} AND {LIVE_DESC}")
print(f"L5 alive via live descendant (containers): {alive_container}")

survivors = q(f"""SELECT s.id, s.name, s.kind, s.visibility, s.line, f.path
    {base} AND {dead_pred} AND NOT ({LIVE_DESC})
    JOIN_FILES ORDER BY s.kind, f.path, s.line""".replace(
    "FROM symbols s WHERE", "FROM symbols s JOIN files f ON f.id = s.file_id WHERE").replace(
    "JOIN_FILES", ""))
print(f"\nSURVIVORS: {len(survivors)}  (funnel: {cand0} - {alive_resolved} - "
      f"{alive_textual} - {alive_inherit} - {alive_container})")

by_kind = {}
for s in survivors:
    by_kind.setdefault(s["kind"], []).append(s)
for kind, rows in sorted(by_kind.items(), key=lambda kv: -len(kv[1])):
    print(f"\n== {kind}: {len(rows)}")
    for s in rows[: int(sys.argv[1]) if len(sys.argv) > 1 else 400]:
        print(f"  {s['name']:40s} {s['visibility']:8s} {s['path']}:{s['line']}")
