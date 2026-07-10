// probe2 (tethys-53iv): for every method call `recv.m(...)` in a real Rust
// codebase, classify the receiver shape and whether its type is locally
// derivable without LSP. Also collects in-crate method names (fns inside
// impl blocks) so the "at-stake" subset — calls whose name collides with an
// in-crate method — can be split out. Usage: probe2-53iv <src-dir>
use std::path::Path;

fn main() {
    let root = std::env::args().nth(1).expect("usage: probe2-53iv <dir>");
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let mut files = Vec::new();
    walk_dir(Path::new(&root), &mut files);
    files.sort();

    let mut impl_methods = std::collections::HashSet::new();
    let mut calls = Vec::new();
    for file in &files {
        let src = std::fs::read_to_string(file).unwrap();
        let tree = parser.parse(&src, None).unwrap();
        let rel = file.strip_prefix(&root).unwrap_or(file).to_owned();
        visit(
            tree.root_node(),
            src.as_bytes(),
            &rel,
            None,
            &mut impl_methods,
            &mut calls,
        );
    }
    for (file, line, method, class) in &calls {
        let stake = if impl_methods.contains(method.as_str()) {
            "AT_STAKE"
        } else {
            "no_collision"
        };
        println!("CALL\t{method}\t{class}\t{stake}\t{file}:{line}");
    }
}

fn walk_dir(dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_dir(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p.to_string_lossy().into_owned());
        }
    }
}

fn visit(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    enclosing_fn: Option<tree_sitter::Node>,
    impl_methods: &mut std::collections::HashSet<String>,
    calls: &mut Vec<(String, u32, String, &'static str)>,
) {
    let text = |n: tree_sitter::Node| n.utf8_text(src).unwrap_or("").to_owned();
    match node.kind() {
        "impl_item" => {
            if let Some(body) = node.child_by_field_name("body") {
                let mut c = body.walk();
                for item in body.children(&mut c) {
                    if item.kind() == "function_item"
                        && let Some(n) = item.child_by_field_name("name")
                    {
                        impl_methods.insert(text(n));
                    }
                }
            }
        }
        "call_expression" => {
            if let Some(f) = node.child_by_field_name("function")
                && f.kind() == "field_expression"
                && let (Some(recv), Some(field)) =
                    (f.child_by_field_name("value"), f.child_by_field_name("field"))
            {
                let class = match recv.kind() {
                    "self" => "self",
                    "identifier" => classify_ident(&text(recv), enclosing_fn, src),
                    "field_expression" => "field_recv",
                    "call_expression" => "call_result",
                    other => {
                        let _ = other;
                        "other"
                    }
                };
                calls.push((
                    file.to_owned(),
                    field.start_position().row as u32 + 1,
                    text(field),
                    class,
                ));
            }
        }
        _ => {}
    }
    let fn_ctx = if node.kind() == "function_item" {
        Some(node)
    } else {
        enclosing_fn
    };
    let mut c = node.walk();
    for child in node.children(&mut c) {
        visit(child, src, file, fn_ctx, impl_methods, calls);
    }
}

/// Rough local-derivability check for an identifier receiver: does the
/// enclosing fn's text contain `ident:` (param or let annotation) or
/// `let ident = <Capital...>` (constructor-shaped initializer)? Textual on
/// purpose — hand-verified by sampling, not trusted blindly.
fn classify_ident(
    ident: &str,
    enclosing_fn: Option<tree_sitter::Node>,
    src: &[u8],
) -> &'static str {
    let Some(f) = enclosing_fn else {
        return "ident_no_fn";
    };
    let body = f.utf8_text(src).unwrap_or("");
    let annotated = body.contains(&format!("{ident}:")) || body.contains(&format!("{ident} :"));
    let constructed = body
        .split(&format!("let {ident} = "))
        .nth(1)
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()));
    if annotated {
        "ident_annotated"
    } else if constructed {
        "ident_constructed"
    } else {
        "ident_unknown"
    }
}
