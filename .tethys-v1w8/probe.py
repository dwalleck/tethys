#!/usr/bin/env python3
"""tethys-v1w8 probe: do `pub use` re-export sites produce refs?

PROBE mechanism : SQL over a freshly built tethys index (system under test).
ORACLE mechanism: textual regex scan of the same source tree (no tree-sitter).

Usage: probe.py <indexed-workspace-dir>
"""
import re
import sqlite3
import sys
from pathlib import Path

WS = Path(sys.argv[1])
DB = WS / ".rivets" / "index" / "tethys.db"

# ---------- ORACLE: regex scan for pub-use statements --------------------
# (file, line, name) triples; glob re-exports recorded with name "*".
PUB_USE = re.compile(r"^[ \t]*pub(?:[ \t]*\([^)]*\))?[ \t]+use[ \t]+([^;]+);", re.M | re.S)
BRACE = re.compile(r"\{([^}]*)\}", re.S)
# Rust string literals — replaced with same-length newline-preserving filler so
# pub-use text inside test fixtures doesn't count (found the hard way: rust.rs:1743).
STRING_LIT = re.compile(r'"(?:[^"\\]|\\.)*"', re.S)

def strip_strings(text: str) -> str:
    return STRING_LIT.sub(lambda m: "".join(c if c == "\n" else " " for c in m.group(0)), text)

oracle: list[tuple[str, int, str]] = []
for f in sorted((WS / "src").rglob("*.rs")):
    text = strip_strings(f.read_text())
    rel = str(f.relative_to(WS))
    for m in PUB_USE.finditer(text):
        stmt, line = m.group(1), text[: m.start()].count("\n") + 1
        g = BRACE.search(stmt)
        if g:
            names = [n.strip().split(" as ")[-1].strip() for n in g.group(1).split(",") if n.strip()]
        elif stmt.strip().endswith("::*"):
            names = ["*"]
        else:
            tail = stmt.strip().split(" as ")
            names = [tail[-1].strip() if len(tail) > 1 else stmt.strip().split("::")[-1].strip()]
        oracle.extend((rel, line, n) for n in names)

print(f"[ORACLE] {len(oracle)} re-exported names at {len({(f, l) for f, l, _ in oracle})} pub-use sites")

con = sqlite3.connect(DB)
q = lambda sql, *p: con.execute(sql, p).fetchall()

# ---------- Q1a: does the imports table know each re-exported name? ------
missing_imports = [
    (f, l, n) for f, l, n in oracle
    if not q("SELECT 1 FROM imports i JOIN files fl ON i.file_id = fl.id WHERE fl.path = ? AND i.symbol_name = ?", f, n)
]
print(f"[Q1a] imports rows missing for {len(missing_imports)}/{len(oracle)} oracle names: {missing_imports or 'NONE'}")

# ---------- Q1b: any refs AT the pub-use site for the name? --------------
refs_at_site = [
    (f, l, n, cnt) for f, l, n in oracle if n != "*"
    for (cnt,) in [q(
        "SELECT COUNT(*) FROM refs_named r JOIN files fl ON r.file_id = fl.id "
        "WHERE fl.path = ? AND r.line = ? AND r.name = ?", f, l, n)[0]]
    if cnt > 0
]
print(f"[Q1b] refs recorded at pub-use sites: {len(refs_at_site)} (issue claims 0): {refs_at_site or 'NONE'}")

# ---------- Q2: re-exported in-crate symbols with ZERO inbound refs ------
# For each non-glob re-exported name that names an in-crate symbol, count all
# inbound refs anywhere in the workspace (via refs_named, the sanctioned surface).
zero_ref, nonzero = [], 0
for name in sorted({n for _, _, n in oracle if n != "*"}):
    sym = q("SELECT COUNT(*) FROM symbols WHERE name = ?", name)[0][0]
    if not sym:
        continue  # not an in-crate symbol under that bare name (e.g. type alias groups)
    refs = q("SELECT COUNT(*) FROM refs_named WHERE name = ?", name)[0][0]
    if refs == 0:
        zero_ref.append(name)
    else:
        nonzero += 1
print(f"[Q2] re-exported in-crate symbols with ZERO inbound refs: {len(zero_ref)} -> {zero_ref}")
print(f"[Q2] (for contrast, re-exported symbols WITH refs elsewhere: {nonzero})")
