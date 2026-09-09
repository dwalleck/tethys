//! Integration tests for concurrency patterns and filesystem edge cases.
//!
//! Tests that Tethys handles real-world scenarios correctly:
//! - Concurrent reads after indexing
//! - Re-indexing consistency
//! - Deeply nested directories
//! - Many files in a single directory
//! - Symlinks, unreadable directories, special-character filenames

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use tempfile::TempDir;
use tethys::Tethys;

/// Create a temporary workspace with the given files.
fn workspace_with_files(files: &[(&str, &str)]) -> (TempDir, Tethys) {
    let dir = tempfile::tempdir().expect("should create temp dir");

    for (path, content) in files {
        let full_path = dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("should create parent dirs");
        }
        fs::write(&full_path, content).expect("should write file");
    }

    let tethys = Tethys::new(dir.path()).expect("should create Tethys");
    (dir, tethys)
}

// === Concurrency: reads after indexing ===

#[test]
fn concurrent_searches_after_indexing_return_consistent_results() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/auth.rs",
            "pub fn authenticate() {}\npub fn authorize() {}\n",
        ),
        (
            "src/db.rs",
            "pub fn connect() {}\npub fn query() {}\npub fn disconnect() {}\n",
        ),
        (
            "src/api.rs",
            "pub fn handle_request() {}\npub fn send_response() {}\n",
        ),
    ]);

    let stats = tethys.index().expect("index should succeed");
    assert_eq!(stats.files_indexed, 3);

    // Wrap in Arc for shared read access across threads
    // Tethys uses Mutex<Connection> internally, so concurrent reads are safe
    let tethys = Arc::new(tethys);

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let tethys = Arc::clone(&tethys);
            thread::spawn(move || {
                let results = tethys
                    .search_symbols("auth")
                    .expect("search should succeed");
                assert!(
                    !results.is_empty(),
                    "concurrent search should find auth symbols"
                );
                results.len()
            })
        })
        .collect();

    let counts: Vec<usize> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All threads should see the same number of results
    assert!(
        counts.windows(2).all(|w| w[0] == w[1]),
        "all concurrent searches should return same count, got: {counts:?}"
    );
}

#[test]
fn concurrent_symbol_and_file_queries_after_indexing() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        r"
pub struct Config {
    pub name: String,
}

pub fn load_config() -> Config {
    Config { name: String::new() }
}
",
    )]);

    tethys.index().expect("index should succeed");
    let tethys = Arc::new(tethys);

    // Spawn threads doing different query types concurrently
    let t1 = {
        let t = Arc::clone(&tethys);
        thread::spawn(move || t.search_symbols("Config").expect("search should succeed"))
    };

    let t2 = {
        let t = Arc::clone(&tethys);
        thread::spawn(move || {
            t.get_file(Path::new("src/lib.rs"))
                .expect("get_file should succeed")
        })
    };

    let t3 = {
        let t = Arc::clone(&tethys);
        thread::spawn(move || t.get_stats().expect("get_stats should succeed"))
    };

    let symbols = t1.join().expect("search thread should not panic");
    let file = t2.join().expect("get_file thread should not panic");
    let stats = t3.join().expect("get_stats thread should not panic");

    assert!(!symbols.is_empty(), "should find Config symbol");
    assert!(file.is_some(), "should find lib.rs");
    assert!(stats.file_count > 0, "should have indexed files");
}

// === Re-indexing consistency ===

#[test]
fn reindex_produces_same_symbol_count() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub fn alpha() {}\npub fn beta() {}\npub fn gamma() {}\n",
        ),
        ("src/util.rs", "pub fn helper() {}\n"),
    ]);

    let stats1 = tethys.index().expect("first index should succeed");
    let stats2 = tethys.index().expect("second index should succeed");

    assert_eq!(
        stats1.files_indexed, stats2.files_indexed,
        "re-index should find same number of files"
    );
    assert_eq!(
        stats1.symbols_found, stats2.symbols_found,
        "re-index should find same number of symbols"
    );
}

#[test]
fn rebuild_clears_and_reindexes_cleanly() {
    let (_dir, mut tethys) =
        workspace_with_files(&[("src/lib.rs", "pub fn original() {}\npub struct Data {}\n")]);

    let stats1 = tethys.index().expect("initial index should succeed");
    let stats2 = tethys.rebuild().expect("rebuild should succeed");

    assert_eq!(
        stats1.symbols_found, stats2.symbols_found,
        "rebuild should produce same symbol count"
    );

    // Verify queries still work after rebuild
    let symbols = tethys
        .search_symbols("original")
        .expect("search after rebuild should succeed");
    assert!(!symbols.is_empty(), "should find symbols after rebuild");
}

#[test]
fn unindexed_workspace_requires_update() {
    let (_dir, tethys) = workspace_with_files(&[("src/lib.rs", "pub fn placeholder() {}\n")]);

    assert!(
        tethys
            .needs_update()
            .expect("freshness query should succeed")
    );
}

// === Filesystem edge cases: deeply nested directories ===

#[test]
fn indexes_files_in_deeply_nested_directories() {
    let dir = tempfile::tempdir().expect("should create temp dir");

    // Create a path 20 levels deep
    let mut nested_path = String::from("src");
    for i in 0..20 {
        write!(nested_path, "/level{i}").unwrap();
    }
    let file_path = format!("{nested_path}/deep.rs");
    let full_path = dir.path().join(&file_path);
    fs::create_dir_all(full_path.parent().unwrap()).expect("should create deep dirs");
    fs::write(&full_path, "pub fn deep_function() {}\n").expect("should write deep file");

    let mut tethys = Tethys::new(dir.path()).expect("should create Tethys");
    let stats = tethys.index().expect("index should succeed");

    assert_eq!(
        stats.files_indexed, 1,
        "should index file in deeply nested dir"
    );
    assert_eq!(stats.symbols_found, 1, "should find function in deep file");

    let symbols = tethys
        .search_symbols("deep_function")
        .expect("search should succeed");
    assert_eq!(symbols.len(), 1, "should find the deeply nested function");
}

// === Filesystem edge cases: many files in one directory ===

#[test]
fn indexes_many_files_in_single_directory() {
    let dir = tempfile::tempdir().expect("should create temp dir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("should create src dir");

    let file_count = 100;
    for i in 0..file_count {
        let content = format!("pub fn func_{i}() {{}}\n");
        fs::write(src_dir.join(format!("file_{i}.rs")), content).expect("should write file");
    }

    let mut tethys = Tethys::new(dir.path()).expect("should create Tethys");
    let stats = tethys.index().expect("index should succeed");

    assert_eq!(
        stats.files_indexed, file_count,
        "should index all {file_count} files"
    );
    assert_eq!(
        stats.symbols_found, file_count,
        "should find one symbol per file"
    );
    assert!(
        stats.errors.is_empty(),
        "should have no errors indexing {file_count} files"
    );
}

// === Filesystem edge cases: unreadable directory ===

#[cfg(unix)]
#[test]
fn unreadable_directory_is_skipped_gracefully() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("should create temp dir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("should create src dir");

    // Create a readable file
    fs::write(src_dir.join("good.rs"), "pub fn accessible() {}\n").expect("should write good file");

    // Create an unreadable subdirectory
    let restricted = src_dir.join("restricted");
    fs::create_dir_all(&restricted).expect("should create restricted dir");
    fs::write(restricted.join("secret.rs"), "pub fn hidden() {}\n")
        .expect("should write secret file");
    let expected_restricted =
        fs::canonicalize(&restricted).expect("should canonicalize restricted directory");

    let mut tethys = Tethys::new(dir.path()).expect("should create Tethys");
    let permissions = fs::metadata(&restricted).unwrap().permissions();
    fs::set_permissions(&restricted, fs::Permissions::from_mode(0o000))
        .expect("should set permissions");
    let Err(permission_error) = fs::read_dir(&restricted) else {
        fs::set_permissions(&restricted, permissions).expect("should restore permissions");
        return; // Elevated users cannot exercise a directory-permission boundary.
    };

    let result = tethys.index();
    fs::set_permissions(&restricted, permissions).expect("should restore permissions");
    assert_eq!(
        permission_error.kind(),
        std::io::ErrorKind::PermissionDenied
    );
    let stats = result.expect("index should publish readable sources despite unreadable dir");

    assert_eq!(stats.files_indexed, 1);
    let readable = tethys.get_file(Path::new("src/good.rs")).unwrap().unwrap();
    assert_eq!(readable.path, Path::new("src/good.rs"));
    let symbols = tethys.list_symbols(&readable.path).unwrap();
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| (symbol.name.as_str(), symbol.file_id))
            .collect::<Vec<_>>(),
        [("accessible", readable.id)]
    );
    assert!(
        tethys
            .get_file(Path::new("src/restricted/secret.rs"))
            .unwrap()
            .is_none()
    );
    assert!(tethys.search_symbols("hidden").unwrap().is_empty());
    assert_eq!(
        stats
            .directories_skipped
            .iter()
            .map(|(path, _)| path.as_path())
            .collect::<Vec<_>>(),
        [expected_restricted.as_path()]
    );
    assert!(!stats.discovery.is_complete());
    assert_eq!(stats.discovery.issues.len(), 1);
    let issue = &stats.discovery.issues[0];
    assert_eq!(issue.path, expected_restricted);
    assert_eq!(
        issue.failure.reason,
        tethys::discovery::DiscoveryFailureReason::EvaluationFailed
    );
}

// === Filesystem edge cases: special characters in filenames ===

#[test]
fn indexes_files_with_spaces_in_path() {
    let dir = tempfile::tempdir().expect("should create temp dir");
    let src_dir = dir.path().join("src/my module");
    fs::create_dir_all(&src_dir).expect("should create dir with spaces");

    fs::write(src_dir.join("my file.rs"), "pub fn spaced_function() {}\n")
        .expect("should write file with spaces");

    let mut tethys = Tethys::new(dir.path()).expect("should create Tethys");
    let stats = tethys.index().expect("index should succeed");

    assert_eq!(
        stats.files_indexed, 1,
        "should index file with spaces in path"
    );
    assert_eq!(stats.symbols_found, 1);
}

#[test]
fn indexes_files_with_unicode_in_path() {
    let dir = tempfile::tempdir().expect("should create temp dir");
    let src_dir = dir.path().join("src/\u{00e9}t\u{00e9}");
    fs::create_dir_all(&src_dir).expect("should create unicode dir");

    fs::write(src_dir.join("caf\u{00e9}.rs"), "pub fn unicode_name() {}\n")
        .expect("should write unicode file");

    let mut tethys = Tethys::new(dir.path()).expect("should create Tethys");
    let stats = tethys.index().expect("index should succeed");

    assert_eq!(
        stats.files_indexed, 1,
        "should index file with unicode path"
    );
    assert_eq!(stats.symbols_found, 1);
}

// === Filesystem edge cases: empty directories ===

#[test]
fn empty_subdirectories_do_not_cause_errors() {
    let dir = tempfile::tempdir().expect("should create temp dir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("should create src dir");

    // Create several empty subdirectories
    for name in &["models", "services", "utils", "tests"] {
        fs::create_dir_all(src_dir.join(name)).expect("should create subdir");
    }

    // One file in src/ itself
    fs::write(src_dir.join("lib.rs"), "pub fn main_lib() {}\n").expect("should write lib file");

    let mut tethys = Tethys::new(dir.path()).expect("should create Tethys");
    let stats = tethys.index().expect("index should succeed");

    assert_eq!(stats.files_indexed, 1);
    assert!(stats.errors.is_empty());
}

// === Filesystem edge cases: large file ===

#[test]
fn indexes_large_file_with_many_symbols() {
    let dir = tempfile::tempdir().expect("should create temp dir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("should create src dir");

    // Generate a file with 500 functions
    let mut content = String::new();
    let symbol_count = 500;
    for i in 0..symbol_count {
        writeln!(content, "pub fn function_{i}() -> i32 {{ {i} }}\n").unwrap();
    }

    fs::write(src_dir.join("big.rs"), &content).expect("should write large file");

    let mut tethys = Tethys::new(dir.path()).expect("should create Tethys");
    let stats = tethys.index().expect("index should succeed");

    assert_eq!(stats.files_indexed, 1);
    assert_eq!(
        stats.symbols_found, symbol_count,
        "should extract all {symbol_count} symbols from large file"
    );
}

#[cfg(unix)]
#[test]
fn canonical_publication_resolves_relative_and_absolute_file_aliases() {
    let (dir, mut index) = workspace_with_files(&[
        (
            "src/Entry.cs",
            "public class Entry { public void Run() { var target = new Target(); } }",
        ),
        ("src/Target.cs", "public class Target {}"),
    ]);
    let alias = dir.path().join("src/Alias.cs");
    std::os::unix::fs::symlink("Target.cs", &alias).unwrap();
    // A sibling alias reaches src without introducing a traversal cycle.
    std::os::unix::fs::symlink("src", dir.path().join("linked_src")).unwrap();
    let stats = index.index().unwrap();
    assert_eq!(stats.files_indexed, 2);
    assert!(!index.needs_update().unwrap());
    let physical = index.get_file(Path::new("src/Target.cs")).unwrap().unwrap();
    assert_eq!(physical.path, Path::new("src/Target.cs"));
    let entry = index.get_file(Path::new("src/Entry.cs")).unwrap().unwrap();
    assert_eq!(entry.path, Path::new("src/Entry.cs"));
    assert_ne!(entry.id, physical.id);
    let targets = index.search_symbols("Target").unwrap();
    assert_eq!(
        targets
            .iter()
            .map(|symbol| (symbol.name.as_str(), symbol.file_id))
            .collect::<Vec<_>>(),
        [("Target", physical.id)]
    );
    let absolute_target = dir.path().join("src/Target.cs");
    let absolute_directory_alias = dir.path().join("linked_src/Target.cs");
    let absolute_dotted_alias = dir.path().join("src/../linked_src/./Alias.cs");
    for spelling in [
        Path::new("src/Target.cs"),
        absolute_target.as_path(),
        Path::new("src/Alias.cs"),
        alias.as_path(),
        Path::new("linked_src/Target.cs"),
        absolute_directory_alias.as_path(),
        Path::new("./src/Target.cs"),
        Path::new("src/../linked_src/./Alias.cs"),
        absolute_dotted_alias.as_path(),
    ] {
        let queried = index
            .get_file(spelling)
            .unwrap()
            .expect("indexed physical source");
        assert_eq!(queried.id, physical.id);
        assert_eq!(queried.path, Path::new("src/Target.cs"));
        assert_eq!(
            index
                .list_symbols(spelling)
                .unwrap()
                .iter()
                .map(|symbol| (symbol.id, symbol.name.as_str(), symbol.file_id))
                .collect::<Vec<_>>(),
            [(targets[0].id, "Target", physical.id)]
        );
        assert_eq!(
            index.get_dependents(spelling).unwrap(),
            [Path::new("src/Entry.cs").to_path_buf()]
        );
    }
    for spelling in [
        dir.path().join("src/Entry.cs"),
        Path::new("src/Entry.cs").to_path_buf(),
        Path::new("./src/Entry.cs").to_path_buf(),
        Path::new("linked_src/Entry.cs").to_path_buf(),
    ] {
        assert_eq!(
            index.get_dependencies(&spelling).unwrap(),
            [Path::new("src/Target.cs").to_path_buf()]
        );
    }
}

#[cfg(unix)]
#[test]
fn csharp_external_file_and_directory_aliases_are_not_published() {
    let (dir, mut index) = workspace_with_files(&[("src/Inside.cs", "public class Inside {}")]);
    let external = tempfile::tempdir().unwrap();
    fs::write(
        external.path().join("EscapedFile.cs"),
        "public class EscapedFile {}",
    )
    .unwrap();
    fs::create_dir(external.path().join("sources")).unwrap();
    fs::write(
        external.path().join("sources/EscapedDirectory.cs"),
        "public class EscapedDirectory {}",
    )
    .unwrap();
    let file_alias = dir.path().join("src/Outside.cs");
    let directory_alias = dir.path().join("outside_sources");
    std::os::unix::fs::symlink(external.path().join("EscapedFile.cs"), &file_alias).unwrap();
    std::os::unix::fs::symlink(external.path().join("sources"), &directory_alias).unwrap();

    let stats = index.index().unwrap();
    assert_eq!(stats.files_indexed, 1);
    let inside = index.get_file(Path::new("src/Inside.cs")).unwrap().unwrap();
    assert_eq!(inside.path, Path::new("src/Inside.cs"));
    assert_eq!(
        index
            .search_symbols("Inside")
            .unwrap()
            .iter()
            .map(|symbol| (symbol.name.as_str(), symbol.file_id))
            .collect::<Vec<_>>(),
        [("Inside", inside.id)]
    );
    assert!(index.search_symbols("Escaped").unwrap().is_empty());
    for spelling in [
        Path::new("src/Outside.cs").to_path_buf(),
        file_alias,
        Path::new("outside_sources/EscapedDirectory.cs").to_path_buf(),
        directory_alias.join("EscapedDirectory.cs"),
        external.path().join("EscapedFile.cs"),
        external.path().join("sources/EscapedDirectory.cs"),
    ] {
        assert!(index.get_file(&spelling).unwrap().is_none());
    }
}

// === Filesystem edge cases: identity failure must not delete live facts ===

#[cfg(unix)]
#[test]
fn unresolvable_source_keeps_live_facts_and_reports_once() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("should create temp dir");
    let nested = dir.path().join("src").join("nested");
    fs::create_dir_all(&nested).expect("should create nested dir");
    fs::write(nested.join("Handler.cs"), "class Handler { }\n").expect("should write source");

    let mut tethys = Tethys::new(dir.path()).expect("should create Tethys");
    tethys.index().expect("initial index");
    let indexed = tethys
        .get_file(Path::new("src/nested/Handler.cs"))
        .expect("lookup")
        .expect("indexed file");
    let symbols_before = tethys.list_symbols(&indexed.path).expect("symbols");

    let permissions = fs::metadata(&nested).unwrap().permissions();
    fs::set_permissions(&nested, fs::Permissions::from_mode(0o000)).expect("should restrict dir");
    let blocked = fs::read_dir(&nested).is_err();
    let result = tethys.index();
    fs::set_permissions(&nested, permissions).expect("should restore permissions");
    if !blocked {
        return; // Elevated users cannot exercise a directory-permission boundary.
    }
    let stats = result.expect("indexing must publish what it can read");

    let retained = tethys
        .get_file(Path::new("src/nested/Handler.cs"))
        .expect("lookup after identity failure")
        .expect("an unresolvable source must not delete live facts");
    assert_eq!(
        tethys.list_symbols(&retained.path).expect("symbols").len(),
        symbols_before.len(),
        "symbols of a live but unresolvable source must survive"
    );
    assert_eq!(
        stats
            .errors
            .iter()
            .filter(|error| error.path.ends_with("Handler.cs"))
            .count(),
        1,
        "one unresolvable source must emit exactly one diagnostic: {:?}",
        stats.errors
    );
}
