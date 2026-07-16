//! Integration regression fences for type-hierarchy edges and the
//! hierarchy walk (tethys-j2r1). Each test builds its OWN real index.

#![allow(clippy::needless_raw_string_hashes, clippy::doc_markdown)]

mod common;

use common::{open_db, workspace_with_files};
use rusqlite::Connection;
use tethys::HierarchyDirection;

fn scalar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect("scalar query")
}

/// F-H1 + F-H4 + C9 (claims C1, C9): the mixed fixture — a type with BOTH
/// a trait impl and an inherent impl. Exactly the trait-impl methods carry
/// markers (the dvsw suppression join), the inherent method carries none,
/// and one type-level edge exists.
#[test]
fn markers_select_exactly_trait_impl_methods() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub trait Anchor {\n    fn a(&self) -> i32;\n    fn b(&self) -> i32;\n}\n\
         pub struct Widget {}\n\
         impl Anchor for Widget {\n    fn a(&self) -> i32 {\n        1\n    }\n    fn b(&self) -> i32 {\n        2\n    }\n}\n\
         impl Widget {\n    pub fn inherent(&self) -> i32 {\n        3\n    }\n}\n",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    // The suppression join: methods with an inherit marker.
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT m.name FROM refs r
             JOIN symbols m ON r.in_symbol_id = m.id
             WHERE r.kind = 'inherit' AND m.kind = 'method'
             ORDER BY m.name",
        )
        .expect("prep");
    let marked: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        marked,
        vec!["a", "b"],
        "exactly the trait-impl methods are marked; inherent is not"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs r JOIN symbols s ON r.in_symbol_id = s.id
             WHERE r.kind = 'inherit' AND s.name = 'Widget'",
        ),
        1,
        "one type-level edge anchored to Widget"
    );
}

/// F-H3 (claim C3, the retention inversion): edges to EXTERNAL supertypes
/// survive indexing with their bare name queryable — and bare names carry
/// no '::', so deprecated-callers' qualified-suffix recovery scans past
/// them (the Path-B non-pollution fence).
#[test]
fn external_supertype_edges_retained_with_bare_names() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub struct Widget {}\n\
         impl std::fmt::Display for Widget {\n\
         \x20   fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n\
         \x20       write!(f, \"w\")\n\
         \x20   }\n\
         }\n",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    assert!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE kind = 'inherit'
             AND symbol_id IS NULL AND reference_name = 'Display'",
        ) >= 1,
        "external-trait edge retained, name-queryable"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE kind = 'inherit'
             AND reference_name LIKE '%::%'",
        ),
        0,
        "inherit reference_names are bare — no phantom qualified paths"
    );
}

/// F-H5 (claim C4): a RESOLVED inherit edge (in-crate trait, same file)
/// never reaches call_edges.
#[test]
fn inherit_edges_never_enter_call_edges() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub trait Anchor {\n    fn a(&self) -> i32;\n}\n\
         pub struct Widget {}\n\
         impl Anchor for Widget {\n    fn a(&self) -> i32 {\n        1\n    }\n}\n",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    assert!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE kind = 'inherit' AND symbol_id IS NOT NULL",
        ) >= 1,
        "precondition: resolved inherit edges exist"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM call_edges ce
             JOIN symbols s ON ce.callee_symbol_id = s.id
             WHERE s.name = 'Anchor'",
        ),
        0,
        "implementing a trait is not a call"
    );
}

/// F-H2 + F-H7 (claims C2, C6): transitive up/down walks over a chain —
/// `trait A; trait B: A; struct S; impl B for S` — with correct depths,
/// and a NotFound error for unknown types.
#[test]
fn hierarchy_walks_are_transitive_with_depths() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub trait A {\n    fn base(&self) -> i32;\n}\n\
         pub trait B: A {\n    fn mid(&self) -> i32;\n}\n\
         pub struct S {}\n\
         impl B for S {\n    fn mid(&self) -> i32 {\n        1\n    }\n}\n",
    )]);
    tethys.index().expect("index");

    let up = tethys
        .get_type_hierarchy("S", HierarchyDirection::Up)
        .expect("up walk");
    let got: Vec<(&str, u32)> = up.up.iter().map(|n| (n.name.as_str(), n.depth)).collect();
    assert_eq!(
        got,
        vec![("B", 1), ("A", 2)],
        "transitive supertypes: {got:?}"
    );
    assert!(up.down.is_empty(), "direction respected");

    let down = tethys
        .get_type_hierarchy("A", HierarchyDirection::Down)
        .expect("down walk");
    let got: Vec<(&str, u32)> = down
        .down
        .iter()
        .map(|n| (n.name.as_str(), n.depth))
        .collect();
    assert_eq!(
        got,
        vec![("B", 1), ("S", 2)],
        "transitive subtypes: {got:?}"
    );

    let err = tethys.get_type_hierarchy("Nope", HierarchyDirection::Both);
    assert!(err.is_err(), "unknown type errors NotFound");
}

/// S12 cycle guard: a cyclic supertrait pair — illegal Rust, but tethys
/// indexes without compiling — must terminate with each node visited once.
#[test]
fn cyclic_hierarchy_terminates() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub trait Ca: Cb {\n    fn x(&self) -> i32;\n}\n\
         pub trait Cb: Ca {\n    fn y(&self) -> i32;\n}\n",
    )]);
    tethys.index().expect("index");
    let up = tethys
        .get_type_hierarchy("Ca", HierarchyDirection::Up)
        .expect("cycle walk terminates");
    let names: Vec<&str> = up.up.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, vec!["Cb"], "each node visited once: {names:?}");
}

/// F-H6 (claim C5): C# base lists e2e — `class X : Base, IFace` walks both
/// ways; interface extension chains too.
#[test]
fn csharp_base_lists_walk_both_directions() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "cs/Lib.cs",
        r"
namespace My.Lib
{
    public interface IRoot { }
    public interface IFace : IRoot { }
    public class Base { }
    public class X : Base, IFace
    {
        public void M() { }
        public class NestedChild : Base { }
    }
}
",
    )]);
    tethys.index().expect("index");
    let up = tethys
        .get_type_hierarchy("X", HierarchyDirection::Up)
        .expect("up");
    let names: Vec<&str> = up.up.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Base", "IFace", "IRoot"],
        "base-list entries plus the transitive interface extension: {names:?}"
    );

    let down = tethys
        .get_type_hierarchy("Base", HierarchyDirection::Down)
        .expect("down");
    let subs: Vec<&str> = down.down.iter().map(|n| n.name.as_str()).collect();
    assert!(
        subs.contains(&"X") && subs.contains(&"NestedChild"),
        "both direct implementors incl. the NESTED class: {subs:?}"
    );
}

/// F-H8 (claim C7): the binary seam — `--json` envelope fields, pure-JSON
/// stdout, exit 0; unknown type exits non-zero.
#[test]
fn cli_json_envelope_through_binary_seam() {
    let (dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub trait Anchor {\n    fn a(&self) -> i32;\n}\n\
         pub struct Widget {}\n\
         impl Anchor for Widget {\n    fn a(&self) -> i32 {\n        1\n    }\n}\n",
    )]);
    tethys.index().expect("index");
    drop(tethys);

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .args(["hierarchy", "Widget", "--json", "-w"])
        .arg(dir.path())
        .output()
        .expect("run binary");
    assert!(out.status.success(), "exit 0: {out:?}");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is pure JSON");
    assert!(json["summary"]["supertypes"].is_u64());
    assert!(json["summary"]["subtypes"].is_u64());
    assert_eq!(json["hierarchy"]["name"], "Widget");
    assert_eq!(json["hierarchy"]["up"][0]["name"], "Anchor");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .args(["hierarchy", "NoSuchType", "-w"])
        .arg(dir.path())
        .output()
        .expect("run binary");
    assert!(!out.status.success(), "unknown type exits non-zero");
}
