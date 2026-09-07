#!/usr/bin/env python3
"""C13: independent source/diff census for the approved tethys-82a6 ledger."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

try:
    import tree_sitter
    import tree_sitter_rust
except ImportError:
    if os.environ.get("TETHYS_SHAPE_ENV"):
        raise
    os.environ["TETHYS_SHAPE_ENV"] = "1"
    os.execvp("uv", ["uv", "run", "--with", "tree-sitter==0.25.2", "--with",
                     "tree-sitter-rust==0.24.0", "python", __file__, *sys.argv[1:]])


def git(root, *args):
    return subprocess.check_output(
        ["git", "-C", str(root), *args], text=True, stderr=subprocess.PIPE
    ).strip()


def source_nodes(parser, source):
    root = parser.parse(source).root_node
    if root.has_error:
        raise ValueError("Rust parse failure")
    stack = [root]
    while stack:
        node = stack.pop()
        yield node
        # Tests are not production ownership. Rust attributes are sibling nodes.
        children = node.named_children
        skip_next = False
        accepted = []
        for child in children:
            text = source[child.start_byte:child.end_byte]
            if child.type == "attribute_item" and b"cfg(test)" in text.replace(b" ", b""):
                skip_next = True
                continue
            if skip_next:
                if child.type in ("line_comment", "block_comment", "attribute_item"):
                    continue
                skip_next = False
                continue
            accepted.append(child)
        stack.extend(reversed(accepted))


def production_lines(parser, source):
    tree = parser.parse(source)
    if tree.root_node.has_error:
        raise ValueError("Rust parse failure")
    excluded = set()
    stack = [tree.root_node]
    while stack:
        parent = stack.pop()
        pending = None
        for child in parent.named_children:
            text = source[child.start_byte:child.end_byte]
            if child.type == "attribute_item" and b"cfg(test)" in text.replace(b" ", b""):
                pending = child.start_point.row
                continue
            if pending is not None:
                if child.type in ("line_comment", "block_comment", "attribute_item"):
                    continue
                excluded.update(range(pending, child.end_point.row + 1))
                pending = None
            else:
                stack.append(child)
    return [line for number, line in enumerate(source.splitlines()) if number not in excluded]


def main():
    args = argparse.ArgumentParser()
    args.add_argument("--stage", required=True, choices=[f"S{i}" for i in range(1, 7)])
    args.add_argument("--base")
    options = args.parse_args()
    root = Path(git(Path.cwd(), "rev-parse", "--show-toplevel"))
    ledger = json.loads((Path(__file__).parent / "ledger.json").read_text())
    # Explicit baseline is pinned in the ledger; discover the actual tracking ref.
    try:
        upstream = git(root, "rev-parse", "--abbrev-ref", "@{upstream}")
    except subprocess.CalledProcessError:
        remotes = git(root, "remote").splitlines()
        heads = []
        for remote in remotes:
            try:
                heads.append(git(root, "symbolic-ref", f"refs/remotes/{remote}/HEAD"))
            except subprocess.CalledProcessError:
                continue
        if len(heads) != 1:
            raise RuntimeError("C13 cannot uniquely discover upstream; pass --base")
        upstream = heads[0]
    base = options.base or ledger["baseline"]
    git(root, "rev-parse", "--verify", base)
    parser = tree_sitter.Parser(tree_sitter.Language(tree_sitter_rust.language()))
    failures = []
    changed = set(git(root, "diff", "--name-only", base, "--", "src", "tools").splitlines())
    changed.update(git(root, "ls-files", "--others", "--exclude-standard", "--", "src", "tools").splitlines())
    allowed = set(ledger["allowed_production"])
    for path in sorted(changed):
        if Path(path).suffix in (".rs", ".cs", ".csproj") and path not in allowed:
            if path.endswith(".rs") and (root / path).is_file():
                previous = subprocess.run(
                    ["git", "-C", str(root), "show", f"{base}:{path}"],
                    capture_output=True,
                )
                if previous.returncode == 0 and production_lines(parser, previous.stdout) == production_lines(
                    parser, (root / path).read_bytes()
                ):
                    continue
            failures.append(f"C13 {path}: production path outside approved ledger")
    for stage, paths in ledger["required"].items():
        if int(stage[1:]) <= int(options.stage[1:]):
            for path in paths:
                if not (root / path).is_file():
                    failures.append(f"C13 {path}: required by {stage}")
    for path, rule in ledger["protected"].items():
        source = (root / path).read_bytes()
        baseline = subprocess.check_output(["git", "-C", str(root), "show", f"{base}:{path}"])
        delta = len(production_lines(parser, source)) - len(production_lines(parser, baseline))
        if delta > rule["growth"]:
            failures.append(f"C13 {path}: production delta +{delta} exceeds tripwire +{rule['growth']}")
        for node in source_nodes(parser, source):
            text = source[node.start_byte:node.end_byte].decode()
            if node.type == "function_item":
                name = node.child_by_field_name("name")
                if name and source[name.start_byte:name.end_byte].decode() in rule["forbidden_functions"]:
                    failures.append(f"C13 {path}:{node.start_point.row+1}: forbidden function ownership")
            if node.type == "scoped_identifier" and text in ("Command::new", "std::process::Command::new"):
                failures.append(f"C13 {path}:{node.start_point.row+1}: process launching in protected parent")
            if path == "src/batch_writer.rs" and node.type == "scoped_identifier" and text == "Index::open":
                failures.append(f"C13 {path}:{node.start_point.row+1}: independent writer connection")
    for directory in (root / "src/discovery",):
        if directory.exists():
            for path in directory.rglob("*.rs"):
                source = path.read_bytes()
                for node in source_nodes(parser, source):
                    if node.type == "use_declaration":
                        text = source[node.start_byte:node.end_byte]
                        if b"rusqlite" in text or b"crate::db" in text:
                            failures.append(f"C13 {path.relative_to(root)}:{node.start_point.row+1}: discovery depends on DB")
    for path in (root / "tools/tethys-msbuild-evaluate").glob("*.cs"):
        text = path.read_text()
        for forbidden in ("BuildManager", "MSBuildWorkspace", "Microsoft.CodeAnalysis", ".Build("):
            if forbidden in text:
                failures.append(f"C13 {path.relative_to(root)}: forbidden target/semantic API {forbidden}")
    if failures:
        print("\n".join(failures))
        return 1
    print(f"C13 PASS {options.stage}: approved paths/ownership/dependencies; baseline={base}; upstream={upstream}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
