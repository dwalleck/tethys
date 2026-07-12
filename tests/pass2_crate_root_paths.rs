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
/// Name lookup binds the first `helper` match (tethys-bvgb); deterministic
/// here because `crate_a` indexes before `crate_b`.
#[test]
fn callers_includes_qualified_only_call_site() {
    let (_dir, tethys) = two_crate_fixture();

    let callers = tethys
        .get_callers("helper", false)
        .expect("get_callers failed");
    // One Dependent per caller symbol (not per file): aggregate b.rs rows.
    let b_rs_symbols: Vec<&str> = callers
        .iter()
        .filter(|d| d.file.to_string_lossy().replace('\\', "/") == "crate_a/src/b.rs")
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
