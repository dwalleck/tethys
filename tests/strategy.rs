//! Integration fences for resolution provenance (tethys-9z7i slice 2,
//! ADR-0003): every write path stamps `refs.strategy`; NULL ⇔ unresolved.
//!
//! Expected labels were written in `.tethys-9z7i/plan-slice2.md` BEFORE
//! implementation; the independent oracles are raw SQL shapes (spatial
//! joins, direction-split NULL counts) and `RUST_LOG` trace events, per the
//! probe (`.tethys-9z7i/findings.md`).

mod common;

use common::{open_db, workspace_with_files};

/// Count refs matching a where-clause. Raw SQL — no analysis code.
fn count_refs(tethys: &tethys::Tethys, where_clause: &str) -> i64 {
    let conn = open_db(tethys);
    conn.query_row(
        &format!("SELECT COUNT(*) FROM refs WHERE {where_clause}"),
        [],
        |r| r.get(0),
    )
    .expect("count query")
}

/// B3 (design C2): every insert-time bind stamps `same_file`, including
/// the macro-map path (kills a stamp wired only into the general map);
/// the label count equals the same-file spatial JOIN count — the label
/// and the join are independent mechanisms (the probe's 1564==1564
/// reconciliation at fixture scale). A truly unresolved ref stays NULL.
#[test]
fn pass1_stamps_same_file_and_matches_spatial_join() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "macro_rules! shout {\n    () => {};\n}\n\
         pub fn local() {}\n\
         pub fn caller() {\n    local();\n    shout!();\n    zz::nothing();\n}\n",
    )]);
    tethys.index().expect("index failed");

    let same_file_labeled = count_refs(&tethys, "strategy = 'same_file'");
    assert!(
        same_file_labeled >= 2,
        "the fn call AND the macro invocation both stamp same_file; got {same_file_labeled}"
    );

    let conn = open_db(&tethys);
    let spatial: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM refs r
             JOIN symbols s ON s.id = r.symbol_id
             WHERE r.file_id = s.file_id",
            [],
            |r| r.get(0),
        )
        .expect("spatial join");
    assert_eq!(
        same_file_labeled, spatial,
        "same_file label count must equal the same-file spatial join count"
    );

    let unresolved_with_strategy =
        count_refs(&tethys, "symbol_id IS NULL AND strategy IS NOT NULL");
    assert_eq!(
        unresolved_with_strategy, 0,
        "unresolved rows never carry a strategy"
    );
}
