//! Regression fences for the `refs_named` view (tethys-6rlu).
//!
//! `reference_name` is populated ONLY for unresolved refs (resolution nulls
//! it). The `refs_named` view restores name-queryability via
//! `COALESCE(reference_name, symbols.name)` over a LEFT JOIN. These tests pin
//! the design's falsification claims against a REAL index each builds itself
//! (the on-disk index can be stale — never query an ambient DB).

#![allow(clippy::needless_raw_string_hashes, clippy::doc_markdown)]

mod common;

use common::{open_db, workspace_with_files};

fn count_named(conn: &rusqlite::Connection, name: &str, kind: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM refs_named WHERE name = ?1 AND kind = ?2",
        rusqlite::params![name, kind],
        |row| row.get(0),
    )
    .expect("query refs_named")
}

fn scalar(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get(0))
        .expect("scalar query")
}

/// Slice 2 — claim C1 (narrowed): a name query over `refs_named` returns the
/// call sites that RESOLVED (keyed by `symbols.name`) or are BARE-unresolved
/// (keyed by `reference_name`), INCLUDING cross-file callers.
///
/// `helper` is called 4×: 2 same-file bare, 1 cross-file bare via import,
/// and 1 cross-file QUALIFIED `crate::helper()`. All four resolve —
/// tethys-3i35 landed, so the bare-`crate` qualified call binds the
/// crate-root `helper` and is keyed by `symbols.name` like the rest. A
/// bare-name query sees **4** and the qualified spelling keys **0**.
/// (This flip was the TRIPWIRE planted here pre-fix; it fired exactly as
/// predicted: 3→4 and 1→0.)
///
/// Bug targeted: cross-file resolved calls uncounted (the zp2j-style miscount)
/// → fewer than 4. Empty companion: `lonely` defined but never called → 0.
#[test]
fn name_query_counts_all_callsites_including_cross_file() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            r"
pub mod b;

pub fn helper() {}
pub fn lonely() {}

pub fn entry() {
    helper();
    helper();
}
",
        ),
        (
            "src/b.rs",
            r"
use crate::helper;

pub fn use_it() {
    helper();
    crate::helper();
}
",
        ),
    ]);

    tethys.index().expect("index should succeed");
    let conn = open_db(&tethys);

    assert_eq!(
        count_named(&conn, "helper", "call"),
        4,
        "all 4 call sites of `helper` (incl. the resolved crate::helper()) must \
         be name-queryable by bare name"
    );
    // tethys-3i35: the qualified call resolves, so nothing is keyed by the
    // qualified spelling anymore.
    assert_eq!(
        count_named(&conn, "crate::helper", "call"),
        0,
        "a resolved qualified call must not be keyed by its qualified spelling"
    );
    assert_eq!(
        count_named(&conn, "lonely", "call"),
        0,
        "a defined-but-never-called function has zero call refs"
    );
}

/// Slice 5 — claim C4: the view cannot introduce panic-points false positives.
///
/// `panic_points` filters `WHERE reference_name IN ('unwrap','expect')` with NO
/// `symbol_id` guard, so a resolved ref becomes a panic site iff it gains such
/// a `reference_name`. The view never writes `reference_name`, so this stays 0.
///
/// DEVIATION from the plan: the planned fence asserted panic-points CLI output
/// directly, but that output is entangled with name-only method misresolution
/// (tethys-53iv): here BOTH `Option::unwrap()` and the in-crate `Thing::unwrap`
/// call resolve to the same in-crate symbol, so the CLI reports 0 regardless —
/// a brittle thing to pin. This invariant is the robust form: it fails under a
/// future `reference_name`-overload impl (resolved unwrap refs would gain the
/// name → count > 0) but not under unrelated resolver changes.
#[test]
fn view_cannot_make_resolved_refs_into_panic_points() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        r"
pub struct Thing;
impl Thing {
    pub fn unwrap(&self) {}
}
pub fn use_external() {
    let x: Option<i32> = Some(1);
    x.unwrap();
}
pub fn use_internal() {
    let t = Thing;
    t.unwrap();
}
",
    )]);
    tethys.index().expect("index should succeed");
    let conn = open_db(&tethys);

    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE symbol_id IS NOT NULL
                 AND reference_name IN ('unwrap', 'expect')"
        ),
        0,
        "no RESOLVED ref may carry a panic-keyword reference_name (would be a panic-points FP)"
    );
}

/// Slice 7 — claims C3, C6 (and C5 by folding): the root invariant the whole
/// "additive view, not overload" design rests on — `reference_name` still means
/// "unresolved". This is what keeps `refs` byte-stable (C3), the unresolved set
/// stable (C6), and the file_deps streaming path's input stable (C5, see
/// design). Bug targeted: ANY `reference_name`-overload impl writes names onto
/// resolved refs → first count > 0 → fails.
#[test]
fn resolved_refs_carry_no_reference_name() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        r"
pub fn helper() {}

pub fn entry() {
    helper();
    let x: Option<i32> = Some(1);
    x.unwrap();
}
",
    )]);
    tethys.index().expect("index should succeed");
    let conn = open_db(&tethys);

    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE symbol_id IS NOT NULL AND reference_name IS NOT NULL"
        ),
        0,
        "a resolved ref must not carry a reference_name (reference_name == unresolved marker)"
    );
    assert!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE symbol_id IS NULL AND reference_name IS NOT NULL"
        ) > 0,
        "unresolved refs (e.g. the external .unwrap()) must still carry their name"
    );
}
