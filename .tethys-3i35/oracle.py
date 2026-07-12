import re, sqlite3, sys
from pathlib import Path

ws = Path(sys.argv[1])
db = sqlite3.connect(ws / ".rivets" / "index" / "tethys.db")

files = [r[0] for r in db.execute("SELECT path FROM files WHERE path LIKE '%.rs'")]
call = re.compile(r"crate::[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*\s*\(")
text_hits = set()
for rel in files:
    p = ws / rel
    if not p.exists():
        continue
    for i, line in enumerate(p.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
        s = line.lstrip()
        if s.startswith(("//", "///", "//!")):
            continue
        if call.search(line):
            text_hits.add((rel, i))

positions = ",".join("'{}:{}'".format(f, l) for f, l in text_hits)
q = ("SELECT f.path, r.line, r.symbol_id IS NOT NULL, r.reference_name "
     "FROM refs r JOIN files f ON r.file_id = f.id "
     "WHERE r.reference_name LIKE 'crate::' || '%' "
     "OR (f.path || ':' || r.line) IN (" + positions + ")")
db_rows = db.execute(q).fetchall()
db_positions = {(p, l) for p, l, _, _ in db_rows}

print("text scan call-shaped crate:: sites:", len(text_hits))
print("refs rows at those positions or crate::-named:", len(db_rows))
unresolved = [(p, l, n) for p, l, res, n in db_rows if not res]
print("unresolved among them:", len(unresolved))
print("\ntext sites with NO refs row:")
for f, l in sorted(text_hits - db_positions):
    print("  {}:{}".format(f, l))
print("\nunresolved crate:: refs NOT seen by text scan (should be none):")
for p, l, n in unresolved:
    if (p, l) not in text_hits:
        print("  {}:{}  {}".format(p, l, n))
