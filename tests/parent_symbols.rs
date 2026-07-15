//! Integration regression fences for parent_symbol_id population
//! (tethys-aay4) and the impl-identity fix (tethys-dl7l). Each test builds
//! its OWN real index.

#![allow(clippy::needless_raw_string_hashes, clippy::doc_markdown)]

mod common;

use common::{open_db, workspace_with_files};
use rusqlite::Connection;

/// (child_name, parent_name) pairs via the persisted linkage.
fn links(conn: &Connection) -> Vec<(String, String, String)> {
    let mut stmt = conn
        .prepare(
            "SELECT s.name, p.name, s.kind FROM symbols s
             JOIN symbols p ON s.parent_symbol_id = p.id
             ORDER BY s.name",
        )
        .expect("prep");
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect")
}

fn parent_of(conn: &Connection, child: &str) -> Option<String> {
    conn.query_row(
        "SELECT p.name FROM symbols s
         JOIN symbols p ON s.parent_symbol_id = p.id
         WHERE s.name = ?1",
        [child],
        |r| r.get(0),
    )
    .ok()
}

/// F-P1 (claims C3, C4): fields → struct, variants → enum, inherent-impl
/// methods → type — including an impl block ABOVE the type declaration
/// (S5, the file-order class the two-phase linkage exists for).
#[test]
fn same_file_members_link_to_their_containers() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "impl Widget {\n    pub fn early_method(&self) -> i32 {\n        1\n    }\n}\n\
         pub struct Widget {\n    pub field_a: i32,\n}\n\
         pub enum Mode {\n    Fast,\n    Slow,\n}\n\
         impl Mode {\n    pub fn pick(&self) -> i32 {\n        2\n    }\n}\n",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    let got = links(&conn);
    let expected = [
        ("early_method", "Widget", "method"),
        ("field_a", "Widget", "struct_field"),
        ("Fast", "Mode", "enum_variant"),
        ("Slow", "Mode", "enum_variant"),
        ("pick", "Mode", "method"),
    ];
    for (c, p, k) in expected {
        assert!(
            got.iter().any(|(gc, gp, gk)| gc == c && gp == p && gk == k),
            "missing link {c} -> {p} ({k}): {got:?}"
        );
    }
    assert_eq!(got.len(), 5, "exactly the expected links: {got:?}");
}

/// F-P2 (claim C1, tethys-dl7l): `impl Trait for Type` methods link to and
/// are qualified by the TYPE. Red before slice 1 (the trait was recorded).
#[test]
fn trait_impl_method_links_to_implementing_type() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub trait Anchor {\n    fn hold(&self) -> i32;\n}\n\
         pub struct Widget {}\n\
         impl Anchor for Widget {\n    fn hold(&self) -> i32 {\n        7\n    }\n}\n",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    assert_eq!(
        parent_of(&conn, "hold").as_deref(),
        Some("Widget"),
        "parent is the implementing TYPE, not the trait"
    );
    let qn: String = conn
        .query_row(
            "SELECT qualified_name FROM symbols WHERE name = 'hold'",
            [],
            |r| r.get(0),
        )
        .expect("hold");
    assert_eq!(qn, "Widget::hold", "qualified by the type (approved D-B)");
}

/// F-P3 (claim C2): dl7l heals qualified_exact — a receiver-typed call to
/// a trait-impl method resolves against the type-qualified symbol row.
/// Red before slice 1: the ref carried path [RustLike] but the symbol was
/// stored as `Anchorable::anchor`, so qualified_exact could never match.
#[test]
fn receiver_typed_call_to_trait_impl_method_resolves_qualified_exact() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub trait Anchorable {\n    fn anchor(&self) -> i32;\n}\n\
         pub struct RustLike {}\n\
         impl Anchorable for RustLike {\n    fn anchor(&self) -> i32 {\n        3\n    }\n}\n\
         pub fn caller() -> i32 {\n    let r: RustLike = RustLike {};\n    r.anchor()\n}\n",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    let (symbol_qn, strategy): (String, String) = conn
        .query_row(
            "SELECT p.qualified_name, r.strategy FROM refs r
             JOIN symbols p ON r.symbol_id = p.id
             WHERE p.name = 'anchor' AND r.kind = 'call'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the receiver-typed call resolves");
    assert_eq!(symbol_qn, "RustLike::anchor");
    assert_eq!(
        strategy, "qualified_exact",
        "receiver-derived path binds the type-qualified symbol"
    );
}

/// F-P4 (claims C5, C6): a same-file container-name collision (struct and
/// trait share a name — distinct Rust namespaces, both container kinds)
/// leaves NULL; a cross-file impl target leaves NULL.
#[test]
fn collision_and_cross_file_targets_stay_null() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod other;\n\
             pub struct Dup {}\n\
             pub trait Dup2 {}\n\
             pub struct Dup2 {}\n\
             impl Dup2 {\n    pub fn collided(&self) {}\n}\n",
        ),
        (
            "src/other.rs",
            "impl crate::Dup {\n    pub fn crossed(&self) {}\n}\n",
        ),
    ]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    assert_eq!(
        parent_of(&conn, "collided"),
        None,
        "ambiguous same-file parent (struct Dup2 + trait Dup2) must stay NULL"
    );
    assert_eq!(
        parent_of(&conn, "crossed"),
        None,
        "cross-file impl target must stay NULL (suppression, not fabrication)"
    );
}

/// F-P5 (claim C7): C# members link to their class; a NESTED class member
/// links to the INNERMOST class.
#[test]
fn csharp_members_link_to_innermost_class() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "cs/Lib.cs",
        r"
namespace My.Lib
{
    public class Outer
    {
        public int OuterField;
        public void OuterMethod() { }

        public class Inner
        {
            public void InnerMethod() { }
        }
    }
}
",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    assert_eq!(parent_of(&conn, "OuterMethod").as_deref(), Some("Outer"));
    assert_eq!(parent_of(&conn, "OuterField").as_deref(), Some("Outer"));
    assert_eq!(
        parent_of(&conn, "InnerMethod").as_deref(),
        Some("Inner"),
        "nested-class member links to the innermost class"
    );
}

/// F-P6 (claims C8, C9): batch ≡ streaming for parent links; rebuilds are
/// content-deterministic.
#[test]
fn parent_links_identical_across_paths_and_rebuilds() {
    const FILES: &[(&str, &str)] = &[(
        "src/lib.rs",
        "pub struct S {\n    pub f: i32,\n}\n\
         impl S {\n    pub fn m(&self) -> i32 {\n        self.f\n    }\n}\n",
    )];
    let dump = |conn: &Connection| {
        let mut v = links(conn);
        v.sort();
        v
    };

    let (_d1, mut batch) = workspace_with_files(FILES);
    batch.rebuild().expect("batch");
    let b1 = dump(&open_db(&batch));
    batch.rebuild().expect("batch again");
    let b2 = dump(&open_db(&batch));
    assert_eq!(b1, b2, "rebuild determinism");

    let (_d2, mut streaming) = workspace_with_files(FILES);
    streaming
        .rebuild_with_options(tethys::IndexOptions::with_streaming_batch_size(1))
        .expect("streaming");
    let s1 = dump(&open_db(&streaming));
    assert_eq!(b1, s1, "batch == streaming(batch_size=1)");
    assert_eq!(b1.len(), 2, "f -> S and m -> S: {b1:?}");
}

/// F-P7 (claim C10): re-indexing one file preserves the other file's rows —
/// the same-file-only invariant keeps the parent CASCADE from ever crossing
/// files, and cross-file NULLs stay NULL through reindex.
#[test]
fn reindex_preserves_cross_file_rows_and_links() {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod other;\n\
             pub struct Kept {\n    pub k: i32,\n}\n",
        ),
        (
            "src/other.rs",
            "impl crate::Kept {\n    pub fn ext(&self) -> i32 {\n        9\n    }\n}\n",
        ),
    ]);
    tethys.index().expect("index");

    // Modify the file holding the type and re-index the workspace.
    std::fs::write(
        dir.path().join("src/lib.rs"),
        "pub mod other;\n\
         pub struct Kept {\n    pub k: i32,\n    pub k2: i32,\n}\n",
    )
    .expect("rewrite");
    tethys.index().expect("reindex");

    let conn = open_db(&tethys);
    assert_eq!(
        parent_of(&conn, "k2").as_deref(),
        Some("Kept"),
        "new field links after reindex"
    );
    let ext_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols WHERE name = 'ext'", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(ext_count, 1, "other file's method survives the reindex");
    assert_eq!(
        parent_of(&conn, "ext"),
        None,
        "cross-file impl target stays NULL after reindex"
    );
}
