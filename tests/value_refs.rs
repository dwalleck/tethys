//! Integration regression fences for fn-as-value reference extraction
//! (tethys-ygjx). Each test builds its OWN real index — the on-disk index can
//! be stale, so never query an ambient DB.
//!
//! Unit-level fences for the extractor itself (emit / suppress / macro-token
//! exclusion) live in `src/languages/rust.rs`; these cover the properties that
//! only a full index exercises: Pass-2 resolution, the drop of unresolved
//! value refs, call-edge exclusion, and cross-index determinism.

#![allow(clippy::needless_raw_string_hashes, clippy::doc_markdown)]

mod common;

use common::{open_db, workspace_with_files};
use rusqlite::Connection;

fn scalar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect("scalar query")
}

/// Ordered snapshot of every `value` ref, keyed by position + resolved name
/// (falling back to the unresolved `reference_name`). Sorted, so it captures
/// the SET of value refs independent of insertion order.
fn value_snapshot(conn: &Connection) -> Vec<(i64, i64, String)> {
    let mut stmt = conn
        .prepare(
            "SELECT r.line, r.column, COALESCE(s.name, r.reference_name, '?')
             FROM refs r LEFT JOIN symbols s ON r.symbol_id = s.id
             WHERE r.kind = 'value'
             ORDER BY r.line, r.column, 3",
        )
        .expect("prepare snapshot");
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("query snapshot")
        .collect::<Result<_, _>>()
        .expect("collect snapshot")
}

/// Claim 4: a value-position identifier that resolves to no in-crate symbol
/// leaves no ref row (`drop_unresolved_value_refs`), while a genuine in-crate
/// fn-as-value is retained. Non-vacuous: skip the drop and `nonexistent_xyz`
/// survives; over-broaden the drop and `keeper` goes to 0.
#[test]
fn unresolved_value_ref_dropped() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        r"
pub fn keeper() {}

pub fn entry() {
    let _a = higher(keeper);
    let _b = higher(nonexistent_xyz);
}

fn higher<F>(f: F) -> F {
    f
}
",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    assert!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs r JOIN symbols s ON r.symbol_id = s.id
             WHERE r.kind = 'value' AND s.name = 'keeper'",
        ) >= 1,
        "genuine in-crate fn-as-value must be retained"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE kind = 'value' AND reference_name = 'nonexistent_xyz'",
        ),
        0,
        "value ref that resolves to nothing must be dropped"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE kind = 'value' AND symbol_id IS NULL",
        ),
        0,
        "no unresolved value refs should survive indexing"
    );
}

/// Claim 6: value refs never enter `call_edges` — passing a fn as a value is a
/// use, not a call. `cb` is called once and passed as a value twice; its
/// call-edge count must be 1, not 3. Non-vacuous: drop the `kind <> 'value'`
/// filter in `populate_call_edges` and this reads 3.
#[test]
fn value_refs_not_in_call_edges() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        r"
pub fn cb() {}

pub fn entry() {
    cb();
    let _x = higher(cb);
    let _y = higher(cb);
}

fn higher<F>(f: F) -> F {
    f
}
",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs r JOIN symbols s ON r.symbol_id = s.id
             WHERE s.name = 'cb' AND r.kind = 'call'",
        ),
        1,
        "cb is called exactly once"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs r JOIN symbols s ON r.symbol_id = s.id
             WHERE s.name = 'cb' AND r.kind = 'value'",
        ),
        2,
        "cb is passed as a value exactly twice"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COALESCE(SUM(ce.call_count), 0) FROM call_edges ce
             JOIN symbols s ON ce.callee_symbol_id = s.id WHERE s.name = 'cb'",
        ),
        1,
        "value uses must not inflate the call graph"
    );
}

/// Claim 5: adding value refs must not perturb the existing reference kinds.
/// A fixture exercising call / macro / type / construct alongside a
/// fn-as-value; the existing-kind counts are pinned. Non-vacuous: if value
/// emission reclassified or double-counted an existing arm, a count shifts.
#[test]
fn existing_ref_kinds_unchanged() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        r#"
pub struct S {
    pub f: i32,
}

pub fn callee() {}

pub fn entry() {
    callee();
    let _s = S { f: 1 };
    println!("{}", 1);
    let _cb = higher(callee);
}

fn use_type(_x: S) {}

fn higher<F>(f: F) -> F {
    f
}
"#,
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    // Exactly one direct call to `callee` (the `higher(callee)` use is a
    // `value` ref, not a call — that separation is the whole point).
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs r JOIN symbols s ON r.symbol_id = s.id
             WHERE s.name = 'callee' AND r.kind = 'call'",
        ),
        1,
        "callee called exactly once; the value-use must not become a call"
    );
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM refs WHERE kind = 'construct'"),
        1,
        "exactly one struct construction of S"
    );
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM refs WHERE kind = 'macro'"),
        1,
        "one macro invocation (println!)"
    );
    // The struct name `S` still produces `type` refs (unchanged by value
    // emission, which only touches bare identifiers in value position, not
    // type_identifier nodes). Pin > 0 rather than an exact count so the fence
    // targets "value emission stole a type node" without coupling to the
    // struct-expression's type-ref quirk.
    assert!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs r JOIN symbols s ON r.symbol_id = s.id
             WHERE s.name = 'S' AND r.kind = 'type'",
        ) >= 1,
        "S must still be referenced in type position"
    );
    // And value emission itself: exactly the one fn-as-value we wrote.
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM refs WHERE kind = 'value'"),
        1,
        "exactly one value ref (higher(callee))"
    );
}

/// Claim 9: value-ref extraction is deterministic across a full re-index.
#[test]
fn value_ref_determinism() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        r"
pub fn a() {}
pub fn b() {}

pub fn entry() {
    let _1 = higher(a);
    let _2 = higher(b);
    let _3 = higher(a);
}

fn higher<F>(f: F) -> F {
    f
}
",
    )]);
    tethys.index().expect("first index");
    let snap1 = value_snapshot(&open_db(&tethys));
    tethys.rebuild().expect("rebuild");
    let snap2 = value_snapshot(&open_db(&tethys));

    assert!(
        !snap1.is_empty(),
        "fixture should produce value refs (a x2, b x1)"
    );
    assert_eq!(
        snap1, snap2,
        "value refs must be identical across re-indexes"
    );
}
