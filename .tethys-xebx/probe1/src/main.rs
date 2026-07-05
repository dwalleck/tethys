// probe1 (tethys-xebx): what member declarations exist in a real C# repo, and
// what member reads would a member_access_expression arm see? Raw tree-sitter
// walk; no tethys code. Usage: xebx-probe1 <corpus-dir>
use std::path::Path;

const DECLS: [&str; 5] = [
    "property_declaration",
    "field_declaration",
    "event_field_declaration",
    "event_declaration",
    "delegate_declaration",
];

fn main() {
    let root = std::env::args().nth(1).expect("usage: xebx-probe1 <dir>");
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_c_sharp::LANGUAGE.into()).unwrap();
    let mut files: Vec<_> = walk(Path::new(&root));
    files.sort();
    for file in &files {
        let src = std::fs::read_to_string(file).unwrap();
        let tree = parser.parse(&src, None).unwrap();
        let rel = file.strip_prefix(&root).unwrap_or(file);
        visit(tree.root_node(), src.as_bytes(), rel, false);
    }
}

fn walk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|x| x == "cs") {
            out.push(p.to_string_lossy().into_owned());
        }
    }
    out
}

fn visit(node: tree_sitter::Node, src: &[u8], file: &str, in_callee: bool) {
    let kind = node.kind();
    let text = |n: tree_sitter::Node| n.utf8_text(src).unwrap_or("?").to_owned();
    if DECLS.contains(&kind) {
        // field/event_field wrap declarators in a variable_declaration
        let names: Vec<String> = match node.child_by_field_name("name") {
            Some(n) => vec![text(n)],
            None => collect_declarators(node, src),
        };
        let arrow = node
            .children(&mut node.walk())
            .any(|c| c.kind() == "arrow_expression_clause");
        let attrs = node
            .children(&mut node.walk())
            .filter(|c| c.kind() == "attribute_list")
            .map(text)
            .collect::<Vec<_>>()
            .join("");
        for name in names {
            println!(
                "DECL\t{kind}\t{name}\t{file}:{}\tarrow={arrow}\tattrs={attrs}",
                node.start_position().row + 1
            );
        }
    }
    if kind == "member_access_expression" && !in_callee {
        let recv = node.child_by_field_name("expression").unwrap();
        println!(
            "READ\t{}\t{}\t{file}:{}\trecv_kind={}",
            text(node.child_by_field_name("name").unwrap()),
            text(node),
            node.start_position().row + 1,
            recv.kind()
        );
        // chained access (a.b.C): report EVERY level — a fold-to-outermost
        // design would hide `Data` in `result.Data.Length` (oracle caught this)
        visit(recv, src, file, false);
        return;
    }
    if kind == "invocation_expression" {
        // the callee member_access is a CALL today, not a read; skip it but
        // still descend into it so receiver-side reads (a.B.M()) surface
        if let Some(callee) = node.child_by_field_name("function") {
            visit(callee, src, file, true);
        }
        if let Some(args) = node.child_by_field_name("arguments") {
            visit(args, src, file, false);
        }
        return;
    }
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        visit(child, src, file, in_callee && kind == "member_access_expression");
    }
}

fn collect_declarators(node: tree_sitter::Node, src: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        if child.kind() == "variable_declaration" {
            let mut c2 = child.walk();
            for d in child.children(&mut c2) {
                if d.kind() == "variable_declarator" {
                    if let Some(n) = d.child_by_field_name("name") {
                        out.push(n.utf8_text(src).unwrap_or("?").to_owned());
                    }
                }
            }
        }
    }
    out
}
