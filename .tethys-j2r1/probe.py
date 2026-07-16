#!/usr/bin/env python3
"""prove-it-prototype probe for tethys-j2r1 (type hierarchy edges).

PROBE: independent tree-sitter walk over src/+tests/+benches/ computing the
PROPOSED hierarchy edge set:
  - Implements: impl_item with BOTH 'trait' and 'type' fields -> (Type, Trait)
  - Extends (supertraits): trait_item with bounds `trait A: B + C` -> (A, B), (A, C)
Measures the anchoring question: is each edge's SUBTYPE (and supertype) a
same-file symbol (usable as in_symbol/symbol_id) or cross-file/external?

ORACLE: grep-based textual count of `^impl ... for ` lines and `trait X:`
bound lists, hand-checked on named files.

Usage: .venv/bin/python probe.py [DB]
"""
import sqlite3, sys, pathlib, collections

import tree_sitter_rust
from tree_sitter import Language, Parser

PARSER = Parser(Language(tree_sitter_rust.language()))
ROOT = pathlib.Path(__file__).resolve().parent.parent
DB = sys.argv[1] if len(sys.argv) > 1 else str(ROOT / ".rivets/index/tethys.db")

def text(n, src):
    return src[n.start_byte:n.end_byte].decode()

def base(t):
    return t.split("<")[0].split("::")[-1].strip().lstrip("&mut ").strip()

impl_edges = []   # (file, type_base, trait_base, line)
super_edges = []  # (file, trait_name, supertrait_base, line)
containers = collections.defaultdict(set)

for path in sorted(list((ROOT/"src").rglob("*.rs")) + list((ROOT/"tests").rglob("*.rs")) + list((ROOT/"benches").rglob("*.rs"))):
    src = path.read_bytes(); tree = PARSER.parse(src); rel = str(path.relative_to(ROOT))
    def visit(n):
        if n.type in ("struct_item","enum_item","trait_item","union_item","type_item"):
            nm = n.child_by_field_name("name")
            if nm is not None:
                containers[rel].add(text(nm, src))
        if n.type == "impl_item":
            tr, ty = n.child_by_field_name("trait"), n.child_by_field_name("type")
            if tr is not None and ty is not None:
                impl_edges.append((rel, base(text(ty, src)), base(text(tr, src)),
                                   n.start_point[0]+1))
        if n.type == "trait_item":
            nm = n.child_by_field_name("name")
            bounds = n.child_by_field_name("bounds")
            if nm is not None and bounds is not None:
                for ch in bounds.children:
                    if ch.type in ("type_identifier", "scoped_type_identifier", "generic_type"):
                        super_edges.append((rel, text(nm, src), base(text(ch, src)),
                                            n.start_point[0]+1))
        for ch in n.children:
            visit(ch)
    visit(tree.root_node)

print(f"Implements edges (impl Trait for Type): {len(impl_edges)}")
sub_same = sum(1 for f, ty, tr, _ in impl_edges if ty in containers[f])
sup_same = sum(1 for f, ty, tr, _ in impl_edges if tr in containers[f])
both = sum(1 for f, ty, tr, _ in impl_edges if ty in containers[f] and tr in containers[f])
print(f"  subtype same-file: {sub_same}  supertype same-file: {sup_same}  both: {both}")
by_trait = collections.Counter(tr for _, _, tr, _ in impl_edges)
print(f"  by trait: {dict(by_trait.most_common(10))}")

print(f"Extends edges (trait A: B + C): {len(super_edges)}")
for e in super_edges:
    print("   ", e)

# in-crate supertype check against the DB symbol table
c = sqlite3.connect(DB).cursor()
names = {n for (n,) in c.execute(
    "SELECT name FROM symbols WHERE kind IN ('struct','class','enum','trait','interface','type_alias')")}
in_crate_sup = sum(1 for _, _, tr, _ in impl_edges if tr in names)
print(f"  Implements supertypes that are in-crate types: {in_crate_sup}/{len(impl_edges)}")
