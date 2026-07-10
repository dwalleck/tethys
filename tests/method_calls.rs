//! Integration tests for receiver-gated Rust method-call resolution
//! (tethys-53iv).
//!
//! Asserts happen against `refs` columns directly (`symbol_id`, `strategy`,
//! `reference_name`), never `refs_named` — resolved refs null their
//! `reference_name`.

mod common;

use common::{open_db, workspace_with_files};

/// Row shape used across these tests: (strategy, target `qualified_name`,
/// preserved `reference_name`).
type RefRow = (Option<String>, Option<String>, Option<String>);

fn method_ref_at(tethys: &tethys::Tethys, file: &str, line: u32) -> RefRow {
    let conn = open_db(tethys);
    conn.query_row(
        "SELECT r.strategy, ts.qualified_name, r.reference_name
         FROM refs r JOIN files f ON f.id = r.file_id
         LEFT JOIN symbols ts ON ts.id = r.symbol_id
         WHERE f.path = ?1 AND r.line = ?2 AND r.kind = 'call'",
        rusqlite::params![file, line],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .expect("exactly one call ref at that line")
}

/// xebx-style name-arm tier: the two unique-or-decline arms an
/// unknown-receiver method call may legitimately bind through. Which one
/// fires depends on crate-prefix derivability (`same_crate` runs first).
fn name_arm_tier(strategy: Option<&str>) -> bool {
    matches!(strategy, Some("same_crate" | "unique_workspace"))
}

/// 53iv design C7 + C8: an unknown-receiver method call never Pass-1 binds
/// — with an ambiguous twin in another file it stays UNRESOLVED (bug
/// classes: Pass-1 leak = `same_file` strategy; ambiguity leak = any bind).
/// The same fixture proves plain fn calls still bind `same_file` (C8) and
/// a workspace-unique method still binds through the name-arm tier (C3's
/// mechanism), so the declines cannot pass vacuously.
#[test]
fn unknown_receiver_skips_pass1_unique_or_decline() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod other;\n\
             pub struct A;\n\
             impl A {\n    pub fn probe(&self) {}\n    pub fn solo(&self) {}\n}\n\
             pub fn free() {}\n\
             pub fn go(x: &A) {\n\
             \x20   helper(x);\n\
             \x20   free();\n\
             }\n\
             pub fn helper(x: &A) {\n\
             \x20   let y = make();\n\
             \x20   y.probe();\n\
             \x20   y.solo();\n\
             }\n\
             pub fn make() -> A { A }\n",
        ),
        (
            "src/other.rs",
            "pub struct B;\nimpl B {\n    pub fn probe(&self) {}\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");

    // y.probe(): unknown receiver, TWO in-crate candidates (A::probe,
    // B::probe) => declined, name preserved.
    let (strategy, target, name) = method_ref_at(&tethys, "src/lib.rs", 14);
    assert_eq!(target, None, "ambiguous method name must not bind");
    assert_eq!(strategy, None);
    assert_eq!(name.as_deref(), Some("probe"), "name preserved for Pass 3+");

    // y.solo(): unknown receiver, UNIQUE candidate => binds via a
    // unique-or-decline name arm (never same_file).
    let (strategy, target, _) = method_ref_at(&tethys, "src/lib.rs", 15);
    assert_eq!(target.as_deref(), Some("A::solo"));
    assert!(
        name_arm_tier(strategy.as_deref()),
        "expected same_crate/unique_workspace, got {strategy:?}"
    );

    // free(): plain fn call, untouched Pass-1 behavior (C8).
    let (strategy, target, _) = method_ref_at(&tethys, "src/lib.rs", 10);
    assert_eq!(target.as_deref(), Some("free"));
    assert_eq!(strategy.as_deref(), Some("same_file"));
}
