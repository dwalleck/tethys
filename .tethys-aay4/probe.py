#!/usr/bin/env python3
"""prove-it-prototype probe for tethys-aay4 (populate parent_symbol_id).

PROBE: independent tree-sitter walk over the real src/ + tests/ computing
the PROPOSED parent linkage — for methods (enclosing impl's base type name),
struct fields (enclosing struct), enum variants (enclosing enum) — then
resolving each parent name against same-file type symbols, mirroring the
proposed insert-time rule.

ORACLE (different mechanism): the DB's qualified_name column — built
independently by the Rust conversion (parse_file_static) — prefix before
the last '::' joined to same-file symbols by name (SQL).

Usage: .venv/bin/python probe.py [DB]   (run from repo root)
"""
import sqlite3, sys, pathlib, collections

import tree_sitter_rust
from tree_sitter import Language, Parser

PARSER = Parser(Language(tree_sitter_rust.language()))
ROOT = pathlib.Path(__file__).resolve().parent.parent
DB = sys.argv[1] if len(sys.argv) > 1 else str(ROOT / ".rivets/index/tethys.db")

CONTAINER_KINDS = ("struct", "enum", "trait", "type_alias")

def node_name(node, src, field="name"):
    n = node.child_by_field_name(field)
    return src[n.start_byte:n.end_byte].decode() if n is not None else None

def impl_base_name(node, src):
    t = node.child_by_field_name("type")
    if t is None:
        return None
    text = src[t.start_byte:t.end_byte].decode()
    # base name: strip generics and path segments (mirror impl_type_base_name)
    return text.split("<")[0].split("::")[-1].strip()

pairs = []          # (file, child_kind, child_name, child_line, parent_name)
containers = collections.defaultdict(list)  # (file) -> [(name, kind)]

for path in sorted(list((ROOT / "src").rglob("*.rs")) + list((ROOT / "tests").rglob("*.rs")) + list((ROOT / "benches").rglob("*.rs"))):
    src = path.read_bytes()
    tree = PARSER.parse(src)
    rel = str(path.relative_to(ROOT))

    def visit(n):
        k = n.type
        if k in ("struct_item", "enum_item", "trait_item", "union_item", "type_item"):
            name = node_name(n, src)
            if name:
                containers[rel].append((name, k))
            # struct fields / enum variants
            body = n.child_by_field_name("body")
            if body is not None and name:
                for ch in body.children:
                    if ch.type == "field_declaration":
                        f = node_name(ch, src)
                        if f:
                            pairs.append((rel, "struct_field", f,
                                          ch.start_point[0] + 1, name))
                    elif ch.type == "enum_variant":
                        v = node_name(ch, src)
                        if v:
                            pairs.append((rel, "enum_variant", v,
                                          ch.start_point[0] + 1, name))
        if k == "impl_item":
            base = impl_base_name(n, src)
            body = n.child_by_field_name("body")
            if body is not None and base:
                for ch in body.children:
                    if ch.type == "function_item":
                        m = node_name(ch, src)
                        if m:
                            pairs.append((rel, "method", m,
                                          ch.start_point[0] + 1, base))
        for ch in n.children:
            visit(ch)

    visit(tree.root_node)

# Resolve each pair's parent against same-file containers (proposed rule).
resolved, orphan, ambiguous = 0, 0, 0
probe_pairs = set()
for rel, kind, name, line, parent in pairs:
    matches = [c for c, k in containers[rel] if c == parent]
    if len(matches) == 1:
        resolved += 1
        probe_pairs.add((rel, name, line, parent))
    elif matches:
        ambiguous += 1
    else:
        orphan += 1

by_kind = collections.Counter(k for _, k, _, _, _ in pairs)
print(f"probe pairs (Rust src/+tests/): {len(pairs)} {dict(by_kind)}")
print(f"same-file parent: resolved={resolved} orphan={orphan} ambiguous={ambiguous}")

# ORACLE: DB qualified_name prefix -> same-file symbol name (Rust files only).
conn = sqlite3.connect(DB)
c, c2 = conn.cursor(), conn.cursor()  # separate cursors: the inner lookup
# must not clobber the outer iteration (first probe run's oracle bug).
oracle_pairs = set()
o_resolved = o_orphan = o_multi = 0
for name, qn, path, line in c.execute(
        "SELECT s.name, s.qualified_name, f.path, s.line FROM symbols s "
        "JOIN files f ON f.id = s.file_id "
        "WHERE s.qualified_name LIKE '%::%' AND f.language = 'rust'"):
    parent = qn.rsplit("::", 1)[0].split("::")[-1]
    n = c2.execute(
        "SELECT COUNT(*) FROM symbols p JOIN files pf ON pf.id = p.file_id "
        "WHERE pf.path = ? AND p.name = ? AND p.kind IN "
        "('struct','enum','trait','type_alias')",
        (path, parent)).fetchone()[0]
    if n == 1:
        o_resolved += 1
        oracle_pairs.add((path, name, line, parent))
    elif n > 1:
        o_multi += 1
    else:
        o_orphan += 1
print(f"oracle (DB qualified_name): resolved={o_resolved} orphan={o_orphan} multi={o_multi}")

only_probe = probe_pairs - oracle_pairs
only_oracle = oracle_pairs - probe_pairs
print(f"agreement: shared={len(probe_pairs & oracle_pairs)} "
      f"probe-only={len(only_probe)} oracle-only={len(only_oracle)}")
for p in sorted(only_probe)[:8]:
    print("  probe-only:", p)
for p in sorted(only_oracle)[:8]:
    print("  oracle-only:", p)
