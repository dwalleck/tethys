#!/usr/bin/env python3
"""prove-it-prototype probe for tethys-ygjx.

Produces the PROPOSED feature output — fn-as-value (cat1) and macro-token-tree
(cat2) identifier references — over tethys's real `src/` tree, using an
INDEPENDENT tree-sitter walk (NOT tethys's own extract_references). The point is
to measure, against real production-shape code, (a) how many such references the
fixed extractor would emit and (b) the noise ratio a conservative guard must
survive.

Oracle for a slice: grep hand-count of a specific function used as a value
(see probe_oracle.sh).
"""
import sys, pathlib
import tree_sitter_rust
from tree_sitter import Language, Parser

RUST = Language(tree_sitter_rust.language())
PARSER = Parser(RUST)

ROOT = pathlib.Path(__file__).resolve().parent.parent  # repo root
SRC = ROOT / "src"

# In-crate symbol names (functions + methods) dumped from the fresh tethys index.
fn_names = set(
    (ROOT / ".tethys-ygjx" / "fn_names.txt").read_text().split()
)


def field_child(node, field):
    return node.child_by_field_name(field)


def ancestor_types(node):
    types = []
    p = node.parent
    while p is not None:
        types.append(p.type)
        p = p.parent
    return types


def classify(node):
    """Return a category tag for an `identifier` node, or None to skip.

    cat1_arg  : bare ident in call/method argument position (`.map(foo)`, `foo(bar)`)
    cat1_let  : bare ident as the value of a let  (`let g = foo;`)
    cat1_ret  : bare ident as a return / tail expression
    cat2_macro: bare ident inside a macro token_tree
    """
    p = node.parent
    if p is None:
        return None

    # --- already-handled or definitely-not-a-fn-value positions: skip ---
    # callee of a call: `foo()` -> handled by zp2j
    if p.type == "call_expression" and field_child(p, "function") == node:
        return None
    # macro name: `foo!` -> handled
    if p.type == "macro_invocation" and field_child(p, "macro") == node:
        return None
    # binding name of a let: `let foo = ...`
    if p.type == "let_declaration" and field_child(p, "pattern") == node:
        return None
    # part of a scoped path / field access / definition name
    if p.type in (
        "scoped_identifier", "scoped_type_identifier", "field_expression",
        "function_item", "parameter", "closure_parameters", "for_expression",
        "generic_type", "type_identifier",
    ):
        return None

    # --- cat2: any identifier living inside a macro token tree ---
    if "token_tree" in ancestor_types(node):
        return "cat2_macro"

    # --- cat1: value positions the AC names ---
    if p.type == "arguments":
        return "cat1_arg"
    if p.type == "let_declaration" and field_child(p, "value") == node:
        return "cat1_let"
    if p.type == "return_expression":
        return "cat1_ret"
    return None


def walk(node, out):
    if node.type == "identifier":
        tag = classify(node)
        if tag:
            out.append((tag, node.text.decode(), node.start_point[0] + 1))
    for c in node.children:
        walk(c, out)


def main():
    per_cat = {}          # tag -> list of (name, file, line)
    for path in sorted(SRC.rglob("*.rs")):
        src = path.read_bytes()
        tree = PARSER.parse(src)
        hits = []
        walk(tree.root_node, hits)
        rel = path.relative_to(ROOT)
        for tag, name, line in hits:
            per_cat.setdefault(tag, []).append((name, str(rel), line))

    print("=== RAW candidate counts (all identifiers in these positions) ===")
    for tag in ("cat1_arg", "cat1_let", "cat1_ret", "cat2_macro"):
        rows = per_cat.get(tag, [])
        in_crate = [r for r in rows if r[0] in fn_names]
        print(f"{tag:11s}: {len(rows):5d} raw   {len(in_crate):5d} match in-crate fn name")

    print("\n=== cat1 fn-as-value that match an in-crate fn name (proposed refs) ===")
    cat1 = [
        r for tag in ("cat1_arg", "cat1_let", "cat1_ret")
        for r in per_cat.get(tag, []) if r[0] in fn_names
    ]
    # Dedup by (name, file, line)
    cat1 = sorted(set(cat1))
    for name, f, line in cat1:
        print(f"  {name:30s} {f}:{line}")
    print(f"  -- {len(cat1)} cat1 in-crate fn-as-value refs --")

    print("\n=== cat2 macro-token that match an in-crate fn name (sample) ===")
    cat2 = sorted(set(r for r in per_cat.get("cat2_macro", []) if r[0] in fn_names))
    for name, f, line in cat2[:25]:
        print(f"  {name:30s} {f}:{line}")
    print(f"  -- {len(cat2)} cat2 in-crate macro-token refs (showing first 25) --")


if __name__ == "__main__":
    main()
