#!/usr/bin/env python3
"""dvsw probe v3: the approved design semantics, independently in SQL+python.

probe2 + three refinements approved at the design pause:
  1. self-originated refs are not evidence (recursive dead fns reported);
  2. textual scan excludes the definition SPAN (line..end_line), not line;
  3. entry points excluded structurally (Rust bin-root main, C# Main).
Plus the S2 channels at full strength: inherit markers, container liveness
(live = resolved-non-self | unresolved-name | marker | is_test, walked up
parent_symbol_id transitively).

Output: one line per finding, "file:line kind name tier" — the shape the
binary's --json is diffed against (S4 gate). Divergences from probe2's
37-Maybe list must be explained by refinements 1-3 alone.
"""
import sqlite3, re, sys

db = sqlite3.connect(".rivets/index/tethys.db")
db.row_factory = sqlite3.Row

RUST_KINDS = ("function", "method", "struct", "enum", "trait",
              "type_alias", "const", "static", "enum_variant")
CS_KINDS = ("class", "interface", "struct", "struct_field", "method",
            "property", "event", "delegate", "function", "enum")
CONTAINERS = {"struct", "class", "enum", "trait", "interface", "type_alias"}

def last_segment(name):
    return name.rsplit("::", 1)[-1]

# unresolved name map: last segment -> set of origin in_symbol_ids
unresolved = {}
for r in db.execute("SELECT reference_name, in_symbol_id FROM refs "
                    "WHERE symbol_id IS NULL AND reference_name IS NOT NULL"):
    unresolved.setdefault(last_segment(r["reference_name"]), set()).add(r["in_symbol_id"])

def unresolved_match(name, sym_id):
    origins = unresolved.get(name, set())
    return any(o != sym_id for o in origins)

resolved_nonself = {r[0] for r in db.execute(
    "SELECT DISTINCT symbol_id FROM refs WHERE symbol_id IS NOT NULL "
    "AND (in_symbol_id IS NULL OR in_symbol_id != symbol_id)")}
markers = {r[0] for r in db.execute(
    "SELECT DISTINCT in_symbol_id FROM refs WHERE kind='inherit' "
    "AND in_symbol_id IS NOT NULL")}

# liveness + parent map over ALL symbols
parent_of, live = {}, set()
for s in db.execute("SELECT id, parent_symbol_id, name, is_test FROM symbols"):
    if s["parent_symbol_id"] is not None:
        parent_of[s["id"]] = s["parent_symbol_id"]
    if s["is_test"] or s["id"] in resolved_nonself or s["id"] in markers \
       or unresolved_match(s["name"], s["id"]):
        live.add(s["id"])
has_live_desc = set()
for i in live:
    cur = i
    while cur in parent_of:
        p = parent_of[cur]
        if p in has_live_desc:
            break
        has_live_desc.add(p)
        cur = p

def rust_binary_root(path):
    if path in ("src/main.rs", "build.rs"): return True
    if path.endswith(("/src/main.rs", "/build.rs")): return True
    segs = path.split("/")
    return any(a == "src" and b == "bin" for a, b in zip(segs, segs[1:])) \
        or "examples" in segs[:-1]

def entry_point(lang, kind, name, path):
    if lang == "rust":
        return kind == "function" and name == "main" and rust_binary_root(path)
    return lang == "csharp" and kind == "method" and name == "Main"

candidates = []
for s in db.execute(
        "SELECT s.id, s.name, s.kind, s.visibility, s.line, s.end_line, "
        "f.path, f.language FROM symbols s JOIN files f ON f.id = s.file_id "
        "WHERE s.visibility != 'public' AND s.is_test = 0"):
    kinds = RUST_KINDS if s["language"] == "rust" else CS_KINDS
    if s["kind"] not in kinds: continue
    if s["id"] in resolved_nonself or s["id"] in markers: continue
    if unresolved_match(s["name"], s["id"]): continue
    if s["kind"] in CONTAINERS and s["id"] in has_live_desc: continue
    if entry_point(s["language"], s["kind"], s["name"], s["path"]): continue
    candidates.append(s)

files = {r["path"]: open(r["path"], encoding="utf-8", errors="replace").read()
         for r in db.execute("SELECT path FROM files")}

out = []
for s in candidates:
    pat = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(s["name"]) + r"(?![A-Za-z0-9_])")
    span_end = s["end_line"] if s["end_line"] is not None else s["line"]
    hits = 0
    for path, text in files.items():
        for i, line in enumerate(text.splitlines(), 1):
            if path == s["path"] and s["line"] <= i <= span_end:
                continue
            hits += len(pat.findall(line))
    tier = "maybe" if hits else "definite"
    out.append((s["path"], s["line"], s["kind"], s["name"], tier))

for row in sorted(out):
    print(f"{row[0]}:{row[1]} {row[2]} {row[3]} {row[4]}")
print(f"TOTAL candidates={len(out)} definite={sum(1 for r in out if r[4]=='definite')} "
      f"maybe={sum(1 for r in out if r[4]=='maybe')}", file=sys.stderr)
