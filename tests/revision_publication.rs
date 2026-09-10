//! Whole-index publication must not expose successfully written files after a later failure.

use std::fs;

use rusqlite::Connection;
use tethys::discovery::{DiscoveryOptions, EvaluationContext};
use tethys::{Error, IndexOptions, Tethys};

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
        fs::write(dir.path().join("Before.csproj"), "<Project />").expect("old project");
        index.index().expect("initial revision");
        let previous_discovery = index
            .discovery_snapshot()
            .expect("discovery publication")
            .clone();
        assert_eq!(previous_discovery.projects[0].key.as_str(), "Before.csproj");
        assert_eq!(index.crates()[0].name, "revision_fixture");
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
        fs::remove_file(dir.path().join("Before.csproj")).expect("remove old project");
        fs::write(dir.path().join("After.csproj"), "<Project />").expect("new project");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"replacement_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("changed Cargo context");
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
        assert_eq!(
            index.discovery_snapshot().expect("discovery publication"),
            &previous_discovery,
            "fatal publication must restore the live discovery context"
        );
        assert_eq!(index.crates()[0].name, "revision_fixture");
        let project_keys = || {
            observer
                .prepare("SELECT project_key FROM projects ORDER BY project_key")
                .unwrap()
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        assert_eq!(project_keys(), ["Before.csproj"]);
        let reopened = Tethys::new(dir.path()).expect("reopen committed revision");
        assert_eq!(
            reopened
                .discovery_snapshot()
                .expect("discovery publication"),
            &previous_discovery
        );
        assert_eq!(reopened.crates()[0].name, "revision_fixture");
        drop(reopened);
        observer
            .execute_batch("DROP TRIGGER reject_architecture")
            .expect("remove injected failure");
        index.index().expect("successful publication control");
        assert_eq!(symbol_names(&observer), ["after"]);
        assert_eq!(project_keys(), ["After.csproj"]);
        assert_eq!(index.crates()[0].name, "replacement_fixture");
        assert_eq!(
            Tethys::new(dir.path())
                .unwrap()
                .discovery_snapshot()
                .expect("discovery publication"),
            index.discovery_snapshot().expect("discovery publication"),
            "reopening must load the committed discovery without fresh evaluation"
        );
    }
}

#[test]
fn failed_outer_commit_restores_source_revision_and_discovery_context() {
    let dir = fixture();
    let mut index = Tethys::new(dir.path()).expect("open index");
    index.index().expect("initial revision");
    let previous_discovery = index
        .discovery_snapshot()
        .expect("discovery publication")
        .clone();
    let observer =
        Connection::open(dir.path().join(".rivets/index/tethys.db")).expect("independent observer");
    let revision = || {
        observer
            .query_row(
                "SELECT revision FROM index_revision WHERE singleton = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("published revision")
    };
    let previous_revision = revision();
    assert_eq!(symbol_names(&observer), ["before"]);
    observer
        .execute_batch(
            "CREATE TABLE commit_fence_parent (id INTEGER PRIMARY KEY);
             CREATE TABLE commit_fence_child (
                 parent_id INTEGER NOT NULL REFERENCES commit_fence_parent(id)
                     DEFERRABLE INITIALLY DEFERRED
             );
             CREATE TRIGGER reject_changed_publication AFTER INSERT ON symbols
             WHEN NEW.name = 'after'
             BEGIN INSERT INTO commit_fence_child(parent_id) VALUES (1); END;",
        )
        .expect("install deferred commit failure");
    fs::write(dir.path().join("src/lib.rs"), "pub fn after() {}\n").expect("changed source");
    let requested_context = EvaluationContext {
        configuration: Some("Release".to_owned()),
        ..EvaluationContext::default()
    };
    assert_ne!(previous_discovery.context, requested_context);
    let requested_options = || {
        IndexOptions::default().with_discovery(DiscoveryOptions {
            context: requested_context.clone(),
            ..DiscoveryOptions::default()
        })
    };

    // The invalid child survives nested savepoint release: only the outer COMMIT
    // checks this deferred constraint, after discovery and architecture finish.
    let error = index
        .index_with_options(requested_options())
        .expect_err("outer commit must reject the deferred foreign key");
    assert!(
        matches!(
            &error,
            Error::Database(rusqlite::Error::SqliteFailure(sqlite, _))
                if sqlite.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
        ),
        "expected SQLite foreign-key failure at commit, got {error:?}"
    );
    assert_eq!(
        index.discovery_snapshot().expect("discovery publication"),
        &previous_discovery,
        "failed COMMIT must restore the live discovery snapshot"
    );
    assert_eq!(symbol_names(&observer), ["before"]);
    assert_eq!(revision(), previous_revision);
    assert_eq!(
        Tethys::new(dir.path())
            .expect("reopen previous publication")
            .discovery_snapshot()
            .expect("discovery publication"),
        &previous_discovery,
        "failed COMMIT must preserve persisted discovery"
    );

    observer
        .execute_batch("DROP TRIGGER reject_changed_publication")
        .expect("remove deferred failure");
    index
        .index_with_options(requested_options())
        .expect("same index must publish the same request after rollback");
    assert_eq!(symbol_names(&observer), ["after"]);
    assert_eq!(revision(), previous_revision + 1);
    assert_eq!(
        index
            .discovery_snapshot()
            .expect("discovery publication")
            .context,
        requested_context
    );
    assert_eq!(
        Tethys::new(dir.path())
            .expect("reopen replacement publication")
            .discovery_snapshot()
            .expect("discovery publication"),
        index.discovery_snapshot().expect("discovery publication"),
        "retry must publish its replacement discovery with the changed source"
    );
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

#[test]
fn legacy_index_is_rebuildable_without_reading_the_evaluation_cache() {
    let dir = fixture();
    let db_path = dir.path().join(".rivets/index/tethys.db");
    fs::create_dir_all(db_path.parent().expect("index directory")).expect("index directory");
    let legacy = Connection::open(&db_path).expect("legacy database");
    legacy
        .execute_batch(
            "CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT NOT NULL);
             INSERT INTO files VALUES (7, 'legacy.rs');
             PRAGMA user_version = 42;",
        )
        .expect("legacy fixture");
    drop(legacy);
    assert!(
        matches!(Tethys::new(dir.path()), Err(Error::Config(_))),
        "an ordinary open must still refuse a legacy cache"
    );

    let stats = Tethys::rebuild_workspace(dir.path(), IndexOptions::default())
        .expect("rebuild must install the schema before any cache read");
    assert_eq!(stats.files_indexed, 1);
    let reopened = Tethys::new(dir.path()).expect("reopen rebuilt index");
    assert_eq!(reopened.crates()[0].name, "revision_fixture");
}

#[test]
fn persisted_crate_paths_outside_the_workspace_are_re_derived_on_open() {
    let dir = fixture();
    let mut index = Tethys::new(dir.path()).expect("open index");
    index.index().expect("publish");
    let canonical_root = dir.path().canonicalize().expect("canonical workspace root");
    let db_path = dir.path().join(".rivets/index/tethys.db");
    {
        let conn = Connection::open(&db_path).expect("index database");
        // The published path is JSON-escaped, so write the moved crate list
        // literally rather than string-replacing the native spelling.
        conn.execute(
            "UPDATE evaluation_context SET crates_json = ?1",
            [r#"[{"name":"revision_fixture","path":"/nonexistent/elsewhere","lib_path":"src/lib.rs","bin_paths":[]}]"#],
        )
        .expect("move the published crate root");
    }

    let reopened = Tethys::new(dir.path()).expect("reopen");
    assert!(
        reopened.crates()[0].path.starts_with(&canonical_root),
        "a crate root outside the workspace must be re-derived: {:?}",
        reopened.crates()[0].path
    );
    assert!(reopened.crates()[0].path.is_dir());
    assert_eq!(
        reopened
            .discovery_snapshot()
            .expect("publication")
            .crates
            .as_slice(),
        reopened.crates(),
        "the publication must expose the repaired crate list"
    );
}

#[test]
fn corrupt_publication_does_not_block_opening_or_indexing() {
    let dir = fixture();
    let mut index = Tethys::new(dir.path()).expect("open index");
    index.index().expect("publish");
    drop(index);
    let db_path = dir.path().join(".rivets/index/tethys.db");
    {
        let conn = Connection::open(&db_path).expect("index database");
        conn.execute(
            "UPDATE evaluation_context SET context_json = '{\"configuration\":42}'",
            [],
        )
        .expect("inject an undecodable publication row");
    }

    let mut reopened = Tethys::new(dir.path()).expect("opening must not decode the publication");
    assert_eq!(reopened.crates()[0].name, "revision_fixture");
    let error = reopened
        .discovery_snapshot()
        .expect_err("reading the corrupt publication must fail loudly");
    assert!(
        error.to_string().contains("--rebuild"),
        "the error must name the recovery command: {error}"
    );

    reopened
        .index()
        .expect("indexing replaces the publication without reading it");
    assert!(
        reopened.discovery_snapshot().is_ok(),
        "the run must publish a readable publication"
    );
}
