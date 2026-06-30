//! Regression fences for the `refs_named` view (tethys-6rlu).
//!
//! `reference_name` is populated ONLY for unresolved refs (resolution nulls
//! it). The `refs_named` view restores name-queryability via
//! `COALESCE(reference_name, symbols.name)` over a LEFT JOIN. These tests pin
//! the design's falsification claims against a REAL index each builds itself
//! (the on-disk index can be stale — never query an ambient DB).

#![allow(clippy::needless_raw_string_hashes)]

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

/// Slice 2 — claim C1 (narrowed): a name query over `refs_named` returns the
/// call sites that RESOLVED (keyed by `symbols.name`) or are BARE-unresolved
/// (keyed by `reference_name`), INCLUDING cross-file callers.
///
/// `helper` is called 4×: 2 same-file bare, 1 cross-file bare via import (all
/// resolve → keyed `helper`), and 1 cross-file QUALIFIED `crate::helper()`.
/// The qualified call does not resolve (tethys-3i35) so it is stored
/// unresolved with `reference_name='crate::helper'` and is keyed by that
/// qualified path — a documented limitation, asserted here rather than papered
/// over. So a bare-name query sees **3**.
///
/// Bug targeted: cross-file resolved calls uncounted (the zp2j-style miscount)
/// → fewer than 3. Empty companion: `lonely` defined but never called → 0.
///
/// TRIPWIRE: when tethys-3i35 lands, `crate::helper()` will resolve →
/// `name='helper'` becomes 4 and `name='crate::helper'` becomes 0; update the
/// expectations below (and the design's C1) at that point.
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
        3,
        "the 3 resolved/bare call sites of `helper` must be name-queryable by bare name"
    );
    // Limitation (tethys-3i35): the unresolved qualified call is keyed by its
    // stored qualified path, not the bare tail.
    assert_eq!(
        count_named(&conn, "crate::helper", "call"),
        1,
        "an unresolved qualified call is keyed by its qualified reference_name"
    );
    assert_eq!(
        count_named(&conn, "lonely", "call"),
        0,
        "a defined-but-never-called function has zero call refs"
    );
}
