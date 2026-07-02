#!/usr/bin/env python3
"""
prove-it probe for tethys-6rlu.

Produces the PROPOSED FEATURE'S output: what `SELECT count(*) FROM refs
WHERE reference_name = ? AND kind='call'` would return AFTER the fix
denormalizes symbols.name back into refs.reference_name.

Post-fix, a resolved call-ref's reference_name == the name of the symbol
its symbol_id points at. So for each target name X:

  post_fix_count(X) = (resolved call-refs whose symbol_id is X's symbol)   # name restored by the fix
                    + (currently-unresolved call-refs already named X)     # untouched by the fix

Runs against the real self-index DB. Uses ONLY the resolver's own tables
(refs/symbols) — the oracle (ripgrep) is independent of this.
"""
import sqlite3, sys

DB = sys.argv[1] if len(sys.argv) > 1 else ".rivets/index/tethys.db"
TARGETS = ["node_text", "parse_scoped_identifier", "extract_call_reference",
           "node_span", "extract_struct_constructor"]

con = sqlite3.connect(DB)
for name in TARGETS:
    # term 1: resolved call-refs pointing at the (unique) function symbol named X
    resolved = con.execute(
        """SELECT COUNT(*) FROM refs r
           JOIN symbols s ON r.symbol_id = s.id
           WHERE s.name = ? AND s.kind = 'function' AND r.kind = 'call'""",
        (name,)).fetchone()[0]
    # term 2: unresolved call-refs that already carry the textual name X
    unresolved = con.execute(
        "SELECT COUNT(*) FROM refs WHERE reference_name = ? AND kind = 'call'",
        (name,)).fetchone()[0]
    print(f"{name}\t{resolved + unresolved}")
con.close()
