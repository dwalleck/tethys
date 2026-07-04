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

/// B4 (design C1): after a full index — with a CIRCULAR cross-file import
/// pair forcing multi-round Pass 2 — `strategy IS NULL ⇔ symbol_id IS
/// NULL`, asserted as two direction-split counts so a failure names its
/// direction. Kills: any arm (or the LSP path) forgetting the strategy
/// bind; anything stamping unresolved rows.
#[test]
fn strategy_null_iff_unresolved() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        (
            "src/a.rs",
            "use crate::b::bee;\npub fn aye() {\n    bee();\n}\n",
        ),
        (
            "src/b.rs",
            "use crate::a::aye;\npub fn bee() {\n    aye();\n}\npub fn lone() {\n    zz::nope();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");

    assert_eq!(
        count_refs(&tethys, "symbol_id IS NOT NULL AND strategy IS NULL"),
        0,
        "every resolved ref carries a strategy"
    );
    assert_eq!(
        count_refs(&tethys, "symbol_id IS NULL AND strategy IS NOT NULL"),
        0,
        "no unresolved ref carries a strategy"
    );
    assert!(
        count_refs(&tethys, "symbol_id IS NULL") >= 1,
        "the zz::nope() ref stays genuinely unresolved"
    );
}

/// B4 (design C4): the per-file memo fans ONE resolution out to every
/// duplicate ref — all three calls to the imported fn carry the same
/// `explicit_import` label. Kills a memo that caches the symbol without the
/// strategy (duplicates would land NULL or defaulted).
#[test]
fn memo_fans_strategy_to_duplicates() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod m;\npub mod u;\n"),
        ("src/m.rs", "pub fn worker() {}\n"),
        (
            "src/u.rs",
            "use crate::m::worker;\npub fn go() {\n    worker();\n    worker();\n    worker();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");

    let labeled = count_refs(
        &tethys,
        "strategy = 'explicit_import' AND symbol_id IN
         (SELECT id FROM symbols WHERE name = 'worker')",
    );
    assert_eq!(
        labeled, 3,
        "all three duplicate calls inherit the memoized strategy"
    );
}

/// B4 (design C9): reexport-kind refs (v1w8) are ordinary refs to the
/// provenance machinery — when resolved they carry a real arm label.
/// Kills kind-filtered stamping.
#[test]
fn reexport_refs_carry_strategy() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "mod inner;\npub use inner::item;\n"),
        ("src/inner.rs", "pub fn item() {}\n"),
    ]);
    tethys.index().expect("index failed");

    let resolved_reexports = count_refs(&tethys, "kind = 'reexport' AND symbol_id IS NOT NULL");
    assert_eq!(resolved_reexports, 1, "the pub use ref resolves");
    let labeled = count_refs(
        &tethys,
        "kind = 'reexport' AND symbol_id IS NOT NULL AND strategy IS NOT NULL",
    );
    assert_eq!(labeled, 1, "and carries a strategy label");
}

/// B4 drift fence (caught by the oracle step, not any fixture): the
/// outdated-schema guard must not brick its own remedy. On a pre-column
/// DB, a query command fails WITH the guidance, and `index --rebuild`
/// clears the files and succeeds — the CLI deletes the db before opening.
#[test]
fn rebuild_recovers_from_outdated_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"t\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("manifest");
    std::fs::create_dir_all(dir.path().join("src")).expect("src dir");
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn f() {}\n").expect("lib");

    // Hand-build a pre-provenance db (the tethys-xvlw shape).
    let db_dir = dir.path().join(".rivets").join("index");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    {
        let conn = rusqlite::Connection::open(db_dir.join("tethys.db")).expect("old db");
        conn.execute_batch(
            "CREATE TABLE refs (
                 id INTEGER PRIMARY KEY, symbol_id INTEGER,
                 file_id INTEGER NOT NULL, kind TEXT NOT NULL,
                 line INTEGER NOT NULL, column INTEGER NOT NULL,
                 end_line INTEGER, end_column INTEGER,
                 in_symbol_id INTEGER, reference_name TEXT);",
        )
        .expect("old refs table");
    }

    let bin = env!("CARGO_BIN_EXE_tethys");
    // Query command: clear failure with the remedy in the message.
    let out = std::process::Command::new(bin)
        .args(["stats", "-w"])
        .arg(dir.path())
        .output()
        .expect("run stats");
    assert!(!out.status.success(), "query on outdated schema must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--rebuild"),
        "error must name the remedy; stderr: {stderr}"
    );

    // The remedy itself must work.
    let out = std::process::Command::new(bin)
        .args(["index", "--rebuild", "-w"])
        .arg(dir.path())
        .output()
        .expect("run rebuild");
    assert!(
        out.status.success(),
        "--rebuild must recover from an outdated schema; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
