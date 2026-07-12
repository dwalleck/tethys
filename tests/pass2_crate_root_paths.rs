//! Integration fences for bare-`crate` qualified-path resolution
//! (tethys-3i35, design claims C1/C2/C4/C5/C6 + the C3 shadow fence).
//!
//! Every test builds its own index from a fixture workspace (recorded
//! lesson: never query an ambient DB). Fixture shapes were rustc-validated
//! during the probe/design phase (`.tethys-3i35/`): `cargo check` compiles
//! the two-crate repro, and the E0425 falsifier pinned the bin+lib rows.

mod common;

use common::{open_db, workspace_with_files};
use rusqlite::Connection;

/// Two-crate workspace: `crate_a` holds the crate-root targets; `crate_b`
/// holds a same-named decoy `helper` that a workspace-first crate mapping
/// would wrongly bind (the C1 stress shape). The qualified calls live in
/// their own caller fns so each call edge is attributable to exactly one
/// ref shape.
fn two_crate_fixture() -> (tempfile::TempDir, tethys::Tethys) {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"crate_a\", \"crate_b\"]\n",
        ),
        (
            "crate_a/Cargo.toml",
            "[package]\nname = \"crate_a\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        (
            "crate_a/src/lib.rs",
            "pub mod b;\n\npub fn helper() {}\n\npub struct Thing;\n\
             impl Thing {\n    pub fn make() -> Thing {\n        Thing\n    }\n}\n",
        ),
        (
            "crate_a/src/b.rs",
            "use crate::helper;\n\n\
             pub fn use_it() {\n    helper();\n}\n\
             pub fn use_it_qualified() {\n    crate::helper();\n}\n\
             pub fn use_it_method() {\n    crate::Thing::make();\n}\n",
        ),
        (
            "crate_b/Cargo.toml",
            "[package]\nname = \"crate_b\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        ("crate_b/src/lib.rs", "pub fn helper() {}\n"),
    ]);
    tethys.index().expect("index failed");
    (dir, tethys)
}

/// Count refs from `ref_file` that resolved to the symbol with
/// `qualified_name` defined in `target_file`, with the given strategy.
fn count_resolved(
    conn: &Connection,
    ref_file: &str,
    target_qualified: &str,
    target_file: &str,
    strategy: &str,
) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM refs r
         JOIN files rf ON r.file_id = rf.id
         JOIN symbols s ON r.symbol_id = s.id
         JOIN files tf ON s.file_id = tf.id
         WHERE rf.path = ?1 AND s.qualified_name = ?2 AND tf.path = ?3
           AND r.strategy = ?4",
        (ref_file, target_qualified, target_file, strategy),
        |row| row.get(0),
    )
    .expect("count_resolved query")
}

/// C1: `crate::helper()` resolves to `crate_a`'s crate-root `helper` under
/// `qualified_module_fallback`, with `reference_name` nulled; the
/// same-named decoy in `crate_b` gains no inbound refs.
///
/// Buggy impl this kills: mapping bare `crate` to a workspace-first crate
/// root (binds `crate_b`'s `helper`), or the pre-fix behavior (ref stays
/// unresolved with `reference_name='crate::helper'`).
#[test]
fn single_segment_tail_resolves_with_decoy() {
    let (_dir, tethys) = two_crate_fixture();
    let conn = open_db(&tethys);

    assert_eq!(
        count_resolved(
            &conn,
            "crate_a/src/b.rs",
            "helper",
            "crate_a/src/lib.rs",
            "qualified_module_fallback",
        ),
        1,
        "crate::helper() must resolve to crate_a's crate-root helper via the \
         qualified-module fallback"
    );

    let leftover: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM refs WHERE reference_name = 'crate::helper'",
            [],
            |row| row.get(0),
        )
        .expect("leftover query");
    assert_eq!(
        leftover, 0,
        "a resolved ref must have its reference_name NULLed (6rlu contract)"
    );

    let decoy_refs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM refs r
             JOIN symbols s ON r.symbol_id = s.id
             JOIN files tf ON s.file_id = tf.id
             WHERE s.name = 'helper' AND tf.path = 'crate_b/src/lib.rs'",
            [],
            |row| row.get(0),
        )
        .expect("decoy query");
    assert_eq!(
        decoy_refs, 0,
        "`crate` must anchor to the ref's own crate — the crate_b decoy \
         must gain no refs"
    );
}

/// C2: `crate::Thing::make()` — a multi-segment tail at the crate root —
/// resolves to the `Thing::make` method symbol (methods store
/// `parent::name` qualified names, so the entry-point lookup finds them).
///
/// Buggy impl this kills: an entry-file lookup that only tries
/// single-segment tails.
#[test]
fn method_tail_resolves_to_thing_make() {
    let (_dir, tethys) = two_crate_fixture();
    let conn = open_db(&tethys);

    assert_eq!(
        count_resolved(
            &conn,
            "crate_a/src/b.rs",
            "Thing::make",
            "crate_a/src/lib.rs",
            "qualified_module_fallback",
        ),
        1,
        "crate::Thing::make() must resolve to the Thing::make method symbol"
    );
}

/// C4 (ticket AC): callers of `helper` include the file whose ONLY call is
/// the qualified `crate::helper()` — proving the qualified ref itself
/// produced the call edge (the bare call lives in a different fn, so
/// `use_it_qualified` can only appear via the qualified ref).
///
/// This fixture deliberately has NO same-named decoy: `get_callers` binds
/// the first `qualified_name` match (tethys-bvgb) and file indexing order
/// is platform-dependent (CI's ubuntu runners walked `crate_b` first and
/// bound the zero-caller decoy, so the two-crate fixture false-failed
/// there). Per-crate anchoring under a decoy is C1's job
/// (`single_segment_tail_resolves_with_decoy`, order-independent SQL);
/// this test pins the call-edge AC only.
#[test]
fn callers_includes_qualified_only_call_site() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod b;\n\npub fn helper() {}\n"),
        (
            "src/b.rs",
            "use crate::helper;\n\n\
             pub fn use_it() {\n    helper();\n}\n\
             pub fn use_it_qualified() {\n    crate::helper();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");

    let callers = tethys
        .get_callers("helper", false)
        .expect("get_callers failed");
    // One Dependent per caller symbol (not per file): aggregate b.rs rows.
    let b_rs_symbols: Vec<&str> = callers
        .iter()
        .filter(|d| d.file.to_string_lossy().replace('\\', "/") == "src/b.rs")
        .flat_map(|d| d.symbols_used.iter().map(String::as_str))
        .collect();
    assert!(
        b_rs_symbols.contains(&"use_it_qualified"),
        "the crate::helper() call site (use_it_qualified) must be listed; got {b_rs_symbols:?}"
    );
}

/// C3 regression fence: a crate-root `f` must NOT shadow the submodule
/// tail — `crate::inner::f()` binds inner.rs's `f` via the longer
/// `crate::inner` split.
///
/// Buggy impl this kills: partial-tail matching in the entry-file lookup
/// (searching the last segment `f` instead of the full tail `inner::f`
/// would bind the root decoy), or split ordering that lets the bare-crate
/// split outrank longer splits.
#[test]
fn root_decoy_does_not_shadow_submodule_tail() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod inner;\npub mod caller;\n\npub fn f() {}\n",
        ),
        ("src/inner.rs", "pub fn f() {}\n"),
        (
            "src/caller.rs",
            "pub fn go() {\n    crate::inner::f();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let conn = open_db(&tethys);

    assert_eq!(
        count_resolved(
            &conn,
            "src/caller.rs",
            "f",
            "src/inner.rs",
            "qualified_module_fallback",
        ),
        1,
        "crate::inner::f() must bind inner.rs's f (longest split wins)"
    );

    let root_f_refs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM refs r
             JOIN symbols s ON r.symbol_id = s.id
             JOIN files tf ON s.file_id = tf.id
             WHERE s.name = 'f' AND tf.path = 'src/lib.rs' AND r.kind = 'call'",
            [],
            |row| row.get(0),
        )
        .expect("root decoy query");
    assert_eq!(
        root_f_refs, 0,
        "the crate-root f decoy must gain no call refs from crate::inner::f()"
    );
}

/// Count unresolved refs stored under the given qualified `reference_name`.
fn count_unresolved(conn: &Connection, reference_name: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM refs WHERE symbol_id IS NULL AND reference_name = ?1",
        [reference_name],
        |row| row.get(0),
    )
    .expect("count_unresolved query")
}

/// C5 rows (a)-(c) and (e): the crate-root-choice matrix for a bin+lib
/// crate, rustc-pinned by the design's E0425 falsifier.
///
/// - (a) `crate::x()` written in `main.rs` (a bin root) binds `main.rs`'s
///   `x` — `crate` inside a bin root denotes the BIN crate;
/// - (b) `crate::y()` written in `main.rs` with `y` only in `lib.rs` stays
///   UNRESOLVED — that line is E0425 to rustc, so binding it would
///   fabricate an edge the compiler rejects (the fixture line is
///   deliberately invalid Rust; the index must mirror the rejection);
/// - (c) `crate::y()` in a lib-owned module binds `lib.rs`'s `y`;
/// - (e) `crate::x()` in `src/bin/tool/helper.rs` (under `src/bin/`, not a
///   bin root) stays UNRESOLVED — the owning bin's module tree is
///   unknowable, decline over fabrication.
///
/// Buggy impl this kills: lib-preferred-always (binds (a) to `lib.rs` and
/// resolves (b)); treating every non-root file as lib-owned (resolves (e)).
#[test]
fn crate_root_choice_matrix_binlib() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "Cargo.toml",
            "[package]\nname = \"binlib\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\
             [[bin]]\nname = \"binlib\"\npath = \"src/main.rs\"\n\
             [[bin]]\nname = \"tool\"\npath = \"src/bin/tool/main.rs\"\n",
        ),
        ("src/lib.rs", "pub mod sub;\n\npub fn y() {}\n"),
        (
            "src/sub.rs",
            "pub fn from_lib_module() {\n    crate::y();\n}\n",
        ),
        (
            "src/main.rs",
            "fn x() {}\nfn main() {\n    crate::x();\n    crate::y();\n}\n",
        ),
        ("src/bin/tool/main.rs", "fn main() {}\n"),
        (
            "src/bin/tool/helper.rs",
            "pub fn h() {\n    crate::x();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let conn = open_db(&tethys);

    // (a) bin-root file: crate::x() -> main.rs's x. In-driver the same-file
    // arm wins before the qualified fallback (same outcome, earlier
    // strategy), so this assert is strategy-agnostic; the resolver-level
    // row-(a) fence is the `bare_crate_in_bin_root_resolves_to_that_bin`
    // unit test in src/resolver.rs.
    let x_bound_in_main: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM refs r
             JOIN files rf ON r.file_id = rf.id
             JOIN symbols s ON r.symbol_id = s.id
             JOIN files tf ON s.file_id = tf.id
             WHERE rf.path = 'src/main.rs' AND s.qualified_name = 'x'
               AND tf.path = 'src/main.rs'",
            [],
            |row| row.get(0),
        )
        .expect("row (a) query");
    assert_eq!(
        x_bound_in_main, 1,
        "(a) crate::x() in main.rs must bind main.rs's x (bin crate root)"
    );

    // (b) crate::y() in main.rs must NOT resolve (rustc: E0425). This row
    // exercises the new bin-root rule in-driver: a lib-preferred-always
    // impl would claim lib.rs, find `y`, and fabricate the edge.
    assert_eq!(
        count_unresolved(&conn, "crate::y"),
        1,
        "(b) crate::y() in main.rs must stay unresolved"
    );
    let resolved_in_main_rs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM refs r
             JOIN files rf ON r.file_id = rf.id
             WHERE rf.path = 'src/main.rs' AND r.symbol_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("resolved-in-main query");
    assert_eq!(
        resolved_in_main_rs, 1,
        "main.rs must have exactly one resolved ref (crate::x) — a second \
         one means crate::y was fabricated"
    );

    // (c) lib-owned module: crate::y() -> lib.rs's y.
    assert_eq!(
        count_resolved(
            &conn,
            "src/sub.rs",
            "y",
            "src/lib.rs",
            "qualified_module_fallback",
        ),
        1,
        "(c) crate::y() in a lib module must bind lib.rs's y"
    );

    // (e) bin-module file: crate::x() from src/bin/tool/helper.rs declines.
    assert_eq!(
        count_unresolved(&conn, "crate::x"),
        1,
        "(e) crate::x() under src/bin/ (non-root) must stay unresolved"
    );
}

/// C5 row (d): a single-bin crate with no lib — every `src/` file belongs
/// to the one bin target, so `crate::x()` from a module binds the bin
/// root's `x`.
#[test]
fn crate_root_choice_single_bin_module() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/main.rs", "mod sub;\n\nfn x() {}\n\nfn main() {}\n"),
        ("src/sub.rs", "pub fn call() {\n    crate::x();\n}\n"),
    ]);
    tethys.index().expect("index failed");
    let conn = open_db(&tethys);

    assert_eq!(
        count_resolved(
            &conn,
            "src/sub.rs",
            "x",
            "src/main.rs",
            "qualified_module_fallback",
        ),
        1,
        "(d) crate::x() in a single-bin crate's module must bind main.rs's x"
    );
}

/// C6: degenerate shapes decline without error — a member crate with no
/// entry point on disk (no `lib.rs`, no `main.rs`), and a stray file
/// belonging to no known crate. Both refs stay unresolved; indexing
/// completes.
///
/// Buggy impl this kills: `.unwrap()` on `entry_point_file()`, or a
/// fallback that treats the workspace root as everyone's crate.
#[test]
fn degenerate_crates_decline() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("Cargo.toml", "[workspace]\nmembers = [\"noentry\"]\n"),
        (
            "noentry/Cargo.toml",
            "[package]\nname = \"noentry\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        (
            "noentry/src/floating.rs",
            "pub fn a() {\n    crate::b();\n}\n",
        ),
        ("stray/orphan.rs", "pub fn c() {\n    crate::d();\n}\n"),
    ]);
    tethys
        .index()
        .expect("indexing must succeed on degenerate shapes");
    let conn = open_db(&tethys);

    assert_eq!(
        count_unresolved(&conn, "crate::b"),
        1,
        "no-entry-point crate: crate::b() must stay unresolved"
    );
    assert_eq!(
        count_unresolved(&conn, "crate::d"),
        1,
        "foreign file: crate::d() must stay unresolved"
    );
}

/// C8: `use crate::*;` in a submodule now reaches the crate-root file, so
/// a bare call to a crate-root fn resolves via the glob arm.
///
/// Buggy impl this kills: a fix plumbed only through `qualified_splits` —
/// the glob arm goes through `resolve_import_files` -> `resolve_import`
/// -> `resolve_module_path(["crate"])` and would still get the directory.
#[test]
fn glob_from_crate_root_resolves() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod user;\n\npub fn gadget() {}\n"),
        (
            "src/user.rs",
            "use crate::*;\n\npub fn go() {\n    gadget();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let conn = open_db(&tethys);

    assert_eq!(
        count_resolved(&conn, "src/user.rs", "gadget", "src/lib.rs", "glob_import",),
        1,
        "a bare call under `use crate::*;` must resolve via the glob arm"
    );
}
