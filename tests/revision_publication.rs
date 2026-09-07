//! Whole-index publication must not expose successfully written files after a later failure.

use std::fs;

use rusqlite::Connection;
use tethys::{IndexOptions, Tethys};

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temporary workspace");
    fs::create_dir(dir.path().join("src")).expect("source directory");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"revision_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("manifest");
    fs::write(dir.path().join("src/lib.rs"), "pub fn before() {}\n").expect("source");
    dir
}

fn symbol_names(conn: &Connection) -> Vec<String> {
    conn.prepare("SELECT name FROM symbols ORDER BY name")
        .expect("symbol query")
        .query_map([], |row| row.get(0))
        .expect("symbol rows")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("symbol names")
}

#[test]
fn failed_architecture_preserves_previous_batch_and_streaming_revision() {
    for options in [IndexOptions::default(), IndexOptions::with_streaming()] {
        let dir = fixture();
        let mut index = Tethys::new(dir.path()).expect("open index");
        index.index().expect("initial revision");
        let observer = Connection::open(dir.path().join(".rivets/index/tethys.db"))
            .expect("independent observer");
        assert_eq!(symbol_names(&observer), ["before"]);
        observer
            .execute_batch(
                "CREATE TRIGGER reject_architecture BEFORE INSERT ON arch_packages
                 BEGIN SELECT RAISE(ABORT, 'injected architecture failure'); END;",
            )
            .expect("install independent failure");
        fs::write(dir.path().join("src/lib.rs"), "pub fn after() {}\n").expect("changed source");
        let outcome = index.index_with_options(options);
        assert!(
            outcome.is_err(),
            "C1 architecture failure must abort the revision"
        );
        assert_eq!(
            symbol_names(&observer),
            ["before"],
            "C1 failed publication exposed new source facts"
        );
        observer
            .execute_batch("DROP TRIGGER reject_architecture")
            .expect("remove injected failure");
        index.index().expect("successful publication control");
        assert_eq!(symbol_names(&observer), ["after"]);
    }
}

#[test]
fn old_schema_refusal_does_not_modify_existing_data() {
    let dir = fixture();
    let mut index = Tethys::new(dir.path()).expect("open index");
    index.index().expect("initial revision");
    drop(index);
    let observer =
        Connection::open(dir.path().join(".rivets/index/tethys.db")).expect("independent observer");
    observer
        .execute_batch("DROP TABLE IF EXISTS index_revision; PRAGMA journal_mode=DELETE")
        .expect("construct old-schema index");
    let before = symbol_names(&observer);
    drop(observer);
    let db_path = dir.path().join(".rivets/index/tethys.db");
    let before_bytes = fs::read(&db_path).expect("read old database bytes");
    let result = Tethys::new(dir.path());
    let Err(error) = result else {
        panic!("C2 old schema must require explicit rebuild");
    };
    assert!(
        error.to_string().contains("--rebuild"),
        "C2 actionable remedy"
    );
    assert!(
        fs::read(&db_path).expect("read refused database bytes") == before_bytes,
        "C2 refusal changed database journal header or data"
    );
    assert!(!dir.path().join(".rivets/index/tethys.db-wal").exists());
    assert!(!dir.path().join(".rivets/index/tethys.db-shm").exists());
    let observer = Connection::open(db_path).expect("independent observer");
    assert_eq!(symbol_names(&observer), before, "C2 old facts changed");
    let revision_tables: i64 = observer
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'index_revision'",
            [],
            |row| row.get(0),
        )
        .expect("schema observation");
    assert_eq!(revision_tables, 0, "C2 refusal mutated old schema");
}

#[test]
fn pinned_reader_keeps_old_revision_across_successful_publication() {
    let dir = fixture();
    let mut index = Tethys::new(dir.path()).expect("open index");
    index.index().expect("initial revision");
    let observer =
        Connection::open(dir.path().join(".rivets/index/tethys.db")).expect("independent observer");
    observer
        .execute_batch("BEGIN DEFERRED")
        .expect("pin reader");
    assert_eq!(symbol_names(&observer), ["before"]);
    fs::write(dir.path().join("src/lib.rs"), "pub fn after() {}\n").expect("changed source");
    index.index().expect("publish replacement");
    assert_eq!(
        symbol_names(&observer),
        ["before"],
        "C1 pinned reader moved"
    );
    observer.execute_batch("COMMIT").expect("release reader");
    assert_eq!(
        symbol_names(&observer),
        ["after"],
        "C1 new reader missed publish"
    );
}
