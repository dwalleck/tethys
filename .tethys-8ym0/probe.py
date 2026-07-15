#!/usr/bin/env python3
"""prove-it-prototype probe for tethys-8ym0.

Produces the PROPOSED feature output — call-shaped identifier refs inside
macro token trees — over tethys's real src/ + tests/, using an INDEPENDENT
tree-sitter walk (NOT tethys's extract_references). Classifies every
identifier token inside a token_tree by call shape, matches against the
in-crate symbol table (fresh tethys index DB), and applies a reimplementation
of the ygjx local-binding scope guard.

Usage: .venv/bin/python probe.py [DB]   (run from repo root)
Writes survivors to .tethys-8ym0/survivors.tsv for probe2 + oracle sampling.
"""
import sqlite3, sys, pathlib, collections

import tree_sitter_rust
from tree_sitter import Language, Parser

PARSER = Parser(Language(tree_sitter_rust.language()))
ROOT = pathlib.Path(__file__).resolve().parent.parent
DB = sys.argv[1] if len(sys.argv) > 1 else str(ROOT / ".rivets/index/tethys.db")

c = sqlite3.connect(DB).cursor()
kind_names = collections.defaultdict(set)   # kind -> set of names
name_kinds = collections.defaultdict(set)   # name -> set of kinds
for name, kind in c.execute("SELECT name, kind FROM symbols"):
    kind_names[kind].add(name); name_kinds[name].add(kind)
fn_count = collections.Counter(
    n for (n,) in c.execute("SELECT name FROM symbols WHERE kind='function'"))
# fn/method symbols with spans, for containing-fn lookup (probe2 + test ctx)
spans = collections.defaultdict(list)       # path -> [(start,end,id,is_test,name)]
for sid, name, path, s, e, t in c.execute(
        "SELECT s.id, s.name, f.path, s.line, s.end_line, s.is_test FROM symbols s "
        "JOIN files f ON s.file_id=f.id WHERE s.kind IN ('function','method')"):
    spans[path].append((s, e or s, sid, t, name))

def containing_fn(path, line):
    best = None
    for s, e, sid, t, name in spans.get(path, ()):
        if s <= line <= e and (best is None or s > best[0]):
            best = (s, e, sid, t, name)
    return best  # innermost fn/method whose span contains line

BIND_PATTERN = {"parameter", "let_declaration", "for_expression"}
BIND_DIRECT = {"closure_parameters", "let_condition", "match_pattern",
               "tuple_struct_pattern", "struct_pattern"}

def harvest(n, out):
    if n.type == "identifier":
        out.add(n.text.decode())
    for ch in n.children:
        harvest(ch, out)

def bindings(fn_node):
    names = set()
    def rec(n):
        if n.type in BIND_PATTERN:
            pat = n.child_by_field_name("pattern")
            if pat is not None:
                harvest(pat, names)
        elif n.type in BIND_DIRECT:
            harvest(n, names)
        for ch in n.children:
            rec(ch)
    rec(fn_node)
    return names

def classify(ident):
    """Shape of an identifier token inside a token_tree."""
    prev, nxt = ident.prev_sibling, ident.next_sibling
    if nxt is not None and nxt.type == "!":
        return "macro_name"
    call = nxt is not None and nxt.type == "token_tree" and nxt.text[:1] == b"("
    if prev is not None and prev.type == ".":
        return "method_call" if call else "field_or_chain"
    if prev is not None and prev.type == "::":
        return "scoped_call" if call else "scoped_ident"
    if nxt is not None and nxt.type == "::":
        return "path_head"
    return "bare_call" if call else "bare_ident"

MACRO_FAMILY = {
    **{m: "assert" for m in ("assert", "assert_eq", "assert_ne", "debug_assert",
                             "debug_assert_eq", "debug_assert_ne")},
    **{m: "fmt" for m in ("format", "format_args", "write", "writeln", "print",
                          "println", "eprint", "eprintln", "panic", "todo",
                          "unimplemented", "unreachable", "anyhow", "bail")},
    "vec": "vec", "matches": "matches", "proptest": "proptest",
}

shape_raw = collections.Counter()
rows = []  # (shape, name, path, line, macro, guard_hit, ctx_is_test)
for path in sorted(list((ROOT / "src").rglob("*.rs")) + list((ROOT / "tests").rglob("*.rs"))):
    tree = PARSER.parse(path.read_bytes())
    rel = str(path.relative_to(ROOT))
    fn_bindings_cache = {}
    def visit(n, macro=None, fn_node=None):
        global rows
        if n.type == "function_item":
            fn_node = n
        if n.type == "macro_invocation":
            m = n.child_by_field_name("macro")
            macro = m.text.decode().split("::")[-1] if m is not None else "?"
        if n.type == "identifier" and macro is not None and n.parent is not None \
                and n.parent.type == "token_tree":
            shape = classify(n)
            shape_raw[shape] += 1
            if shape in ("bare_call", "method_call", "scoped_call"):
                name = n.text.decode()
                line = n.start_point[0] + 1
                guard = False
                if fn_node is not None:
                    if id(fn_node) not in fn_bindings_cache:
                        fn_bindings_cache[id(fn_node)] = bindings(fn_node)
                    guard = name in fn_bindings_cache[id(fn_node)]
                ctx = containing_fn(rel, line)
                rows.append((shape, name, rel, line, macro,
                             guard, bool(ctx and ctx[3])))
        for ch in n.children:
            visit(ch, macro, fn_node)
    visit(tree.root_node)

print("=== raw identifier shapes inside macro token trees (src/ + tests/) ===")
for k, v in shape_raw.most_common():
    print(f"  {k:15s} {v:6d}")

def funnel(shape, match_kind):
    sub = [r for r in rows if r[0] == shape]
    matched = [r for r in sub if match_kind in name_kinds.get(r[1], ())]
    guarded = [r for r in matched if not r[5]]
    uniq = [r for r in guarded if shape != "bare_call" or fn_count[r[1]] == 1]
    print(f"\n=== {shape} funnel (match kind={match_kind}) ===")
    print(f"  raw={len(sub)}  in-crate-match={len(matched)}  "
          f"after-scope-guard={len(guarded)}  name-unique={len(uniq)}")
    fams = collections.Counter(MACRO_FAMILY.get(r[4], "other:" + r[4]) for r in guarded)
    print("  by macro:", dict(fams.most_common(12)))
    ctx = collections.Counter("test-ctx" if r[6] else "prod-ctx" for r in guarded)
    print("  by context:", dict(ctx))
    names = collections.Counter(r[1] for r in guarded)
    print("  top names:", dict(names.most_common(12)))
    return guarded

bare = funnel("bare_call", "function")
meth = funnel("method_call", "method")
scop = funnel("scoped_call", "function")

with open(ROOT / ".tethys-8ym0/survivors.tsv", "w") as f:
    for shape, name, rel, line, macro, guard, is_test in bare + meth + scop:
        f.write(f"{shape}\t{name}\t{rel}\t{line}\t{macro}\t{int(is_test)}\n")
print(f"\nwrote {len(bare)+len(meth)+len(scop)} survivors to .tethys-8ym0/survivors.tsv")
