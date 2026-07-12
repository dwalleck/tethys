"""Probe for tethys-3i35: how do crate::-qualified refs land in the index today,
and what WOULD the proposed rule (bare `crate` prefix -> crate entry-point file)
newly resolve?

Usage: python probe.py <workspace_root>
Reads <workspace_root>/.rivets/index/tethys.db (index it first).
No tethys code is imported: raw SQL only.
"""

import sqlite3
import sys
from pathlib import Path

ws = Path(sys.argv[1])
db = sqlite3.connect(ws / ".rivets" / "index" / "tethys.db")
db.row_factory = sqlite3.Row

print("== all refs (name, symbol_id, strategy, file, line) ==")
rows = db.execute(
    """SELECT r.reference_name, r.symbol_id, r.strategy,
              f.path AS ref_file, r.line,
              s.name AS target_name, tf.path AS target_file
       FROM refs r
       JOIN files f ON r.file_id = f.id
       LEFT JOIN symbols s ON r.symbol_id = s.id
       LEFT JOIN files tf ON s.file_id = tf.id
       ORDER BY f.path, r.line"""
).fetchall()
for r in rows:
    status = (
        f"-> {r['target_name']} @ {r['target_file']} ({r['strategy']})"
        if r["symbol_id"]
        else "UNRESOLVED"
    )
    print(f"  {r['ref_file']}:{r['line']}  {r['reference_name'] or '(name nulled)'}  {status}")

print("\n== unresolved refs with a crate:: prefix ==")
unresolved = db.execute(
    """SELECT r.reference_name, f.path AS ref_file, r.line
       FROM refs r JOIN files f ON r.file_id = f.id
       WHERE r.symbol_id IS NULL AND r.reference_name LIKE 'crate::%'"""
).fetchall()
print(f"  count: {len(unresolved)}")

print("\n== proposed-rule simulation ==")
# Proposed rule: the ["crate"] prefix split maps to the ref's own crate's
# entry-point file; the remaining tail is looked up there by qualified_name.
# Simulate: entry-point file = the lib.rs/main.rs sharing the ref file's
# first path segment (crate dir). Independent reimplementation, not tethys code.
for r in unresolved:
    tail = r["reference_name"].removeprefix("crate::")
    crate_dir = r["ref_file"].split("/")[0].split("\\")[0]
    hits = db.execute(
        """SELECT s.name, s.qualified_name, s.kind, f.path
           FROM symbols s JOIN files f ON s.file_id = f.id
           WHERE s.qualified_name = ? AND (f.path = ? OR f.path = ?)""",
        (tail, f"{crate_dir}/src/lib.rs", f"{crate_dir}/src/main.rs"),
    ).fetchall()
    verdict = (
        f"WOULD RESOLVE -> {hits[0]['qualified_name']} ({hits[0]['kind']}) @ {hits[0]['path']}"
        if hits
        else "still unresolved (tail not in entry-point file)"
    )
    print(f"  {r['ref_file']}:{r['line']}  {r['reference_name']}  {verdict}")

print("\n== cross-crate decoy check: helper symbols in the index ==")
for s in db.execute(
    """SELECT s.qualified_name, f.path FROM symbols s
       JOIN files f ON s.file_id = f.id WHERE s.name = 'helper'"""
):
    print(f"  {s['qualified_name']} @ {s['path']}")
