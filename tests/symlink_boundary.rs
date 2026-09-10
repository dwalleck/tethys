//! Symlink boundary tests for Tethys indexing.
//!
//! Tests that verify how symlink resolution interacts with the `workspace_root`
//! boundary during file discovery and indexing.
//!
//! ## Current Behavior (documented, not necessarily desired)
//!
//! `walk_dir` follows symlinks via `is_dir()`/`is_file()`, collecting files
//! through symlinked paths. `parse_file_static` validates that file paths are
//! under `workspace_root` via `strip_prefix`, but this checks the **logical**
//! path (through the symlink), not the canonical target. As a result:
//!
//! - Symlinks pointing outside the workspace ARE followed and indexed
//! - The indexed path uses the logical (in-workspace) path
//! - Circular symlinks are handled by the OS (ELOOP)

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use tethys::Tethys;

// === Symlink pointing outside workspace ===

#[cfg(unix)]
#[test]
fn symlink_to_file_outside_workspace_is_indexed_through_logical_path() {
    // Create two separate temp directories: workspace and external
    let workspace = tempfile::tempdir().expect("should create workspace dir");
    let external = tempfile::tempdir().expect("should create external dir");

    // Create a Rust file in the external directory (outside workspace)
    fs::write(
        external.path().join("external_module.rs"),
        "pub fn external_function() {}\n",
    )
    .expect("should write external file");

    // Create src/ in workspace with a symlink to the external file
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("should create src dir");
    std::os::unix::fs::symlink(
        external.path().join("external_module.rs"),
        src_dir.join("linked.rs"),
    )
    .expect("should create symlink");

    let mut tethys = Tethys::new(workspace.path()).expect("should create Tethys");
    let stats = tethys.index().expect("index should succeed");

    // Current behavior: symlink is followed and file is indexed through
    // the logical path (src/linked.rs), even though the real file is
    // outside the workspace. This is documented behavior -- a future
    // hardening pass may add canonical-path boundary checks.
    assert_eq!(
        stats.files_indexed, 1,
        "symlinked file outside workspace is currently indexed via logical path"
    );

    let symbols = tethys
        .search_symbols("external_function")
        .expect("search should succeed");
    assert_eq!(
        symbols.len(),
        1,
        "should find symbol from symlinked external file"
    );
    let logical_path = std::path::Path::new("src/linked.rs");
    let file = tethys.get_file(logical_path).unwrap().unwrap();
    assert_eq!(file.path, logical_path);
    assert_eq!(symbols[0].file_id, file.id);
    assert_eq!(
        tethys.list_symbols(logical_path).unwrap()[0].id,
        symbols[0].id
    );
}

#[cfg(unix)]
#[test]
fn symlink_to_directory_outside_workspace_is_traversed() {
    let workspace = tempfile::tempdir().expect("should create workspace dir");
    let external = tempfile::tempdir().expect("should create external dir");

    // Create files in the external directory
    let ext_src = external.path().join("src");
    fs::create_dir_all(&ext_src).expect("should create external src dir");
    fs::write(ext_src.join("secret.rs"), "pub fn secret_function() {}\n")
        .expect("should write external file");

    // Create a symlink in the workspace pointing to the external directory
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("should create src dir");
    std::os::unix::fs::symlink(&ext_src, src_dir.join("external"))
        .expect("should create dir symlink");

    let mut tethys = Tethys::new(workspace.path()).expect("should create Tethys");
    let stats = tethys.index().expect("index should succeed");
    assert_eq!(stats.files_indexed, 1);

    // Current behavior: the symlinked directory outside the workspace is
    // traversed and its files are indexed through the logical path
    // (src/external/secret.rs).
    let symbols = tethys
        .search_symbols("secret_function")
        .expect("search should succeed");
    let logical_path = std::path::Path::new("src/external/secret.rs");
    let file = tethys.get_file(logical_path).unwrap().unwrap();
    assert_eq!(file.path, logical_path);
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| (symbol.name.as_str(), symbol.file_id))
            .collect::<Vec<_>>(),
        [("secret_function", file.id)]
    );
    assert_eq!(
        tethys.list_symbols(logical_path).unwrap()[0].id,
        symbols[0].id
    );
}

// === Circular symlinks ===

#[cfg(unix)]
#[test]
fn circular_symlink_does_not_cause_infinite_loop() {
    let workspace = tempfile::tempdir().expect("should create workspace dir");
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("should create src dir");

    // Create a real file
    fs::write(src_dir.join("real.rs"), "pub fn real_function() {}\n")
        .expect("should write real file");

    // Create a circular symlink: src/loop -> src
    std::os::unix::fs::symlink(&src_dir, src_dir.join("loop"))
        .expect("should create circular symlink");

    let mut tethys = Tethys::new(workspace.path()).expect("should create Tethys");

    // Should not hang or panic. The OS returns ELOOP or the directory walker
    // naturally terminates because is_dir() returns false for circular symlinks
    // after the OS reaches its symlink follow limit.
    let stats = tethys
        .index()
        .expect("index should not hang on circular symlink");

    // Should at least index the real file
    assert!(
        stats.files_indexed >= 1,
        "should index at least the real file, got {}",
        stats.files_indexed
    );

    let symbols = tethys
        .search_symbols("real_function")
        .expect("search should succeed");
    assert!(
        !symbols.is_empty(),
        "should find symbol from real file despite circular symlink"
    );
}

#[cfg(unix)]
#[test]
fn mutual_circular_symlinks_do_not_cause_infinite_loop() {
    let workspace = tempfile::tempdir().expect("should create workspace dir");
    let dir_a = workspace.path().join("src/dir_a");
    let dir_b = workspace.path().join("src/dir_b");
    fs::create_dir_all(&dir_a).expect("should create dir_a");
    fs::create_dir_all(&dir_b).expect("should create dir_b");

    // Create a file in each directory
    fs::write(dir_a.join("a.rs"), "pub fn func_a() {}\n").expect("should write a.rs");
    fs::write(dir_b.join("b.rs"), "pub fn func_b() {}\n").expect("should write b.rs");

    // Create mutual symlinks: dir_a/link_b -> dir_b, dir_b/link_a -> dir_a
    std::os::unix::fs::symlink(&dir_b, dir_a.join("link_b")).expect("should create symlink a->b");
    std::os::unix::fs::symlink(&dir_a, dir_b.join("link_a")).expect("should create symlink b->a");

    let mut tethys = Tethys::new(workspace.path()).expect("should create Tethys");
    let stats = tethys
        .index()
        .expect("index should not hang on mutual symlinks");

    // Should index the real files without entering an infinite loop
    assert!(
        stats.files_indexed >= 2,
        "should index at least the two real files, got {}",
        stats.files_indexed
    );
}

// === Nested symlink chains ===

#[cfg(unix)]
#[test]
fn nested_symlink_chain_is_followed() {
    let workspace = tempfile::tempdir().expect("should create workspace dir");
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("should create src dir");

    // Create a real file
    fs::write(src_dir.join("target.rs"), "pub fn target_function() {}\n")
        .expect("should write target file");

    // Create a chain of symlinks: link3 -> link2 -> link1 -> target.rs
    std::os::unix::fs::symlink(src_dir.join("target.rs"), src_dir.join("link1.rs"))
        .expect("should create link1");
    std::os::unix::fs::symlink(src_dir.join("link1.rs"), src_dir.join("link2.rs"))
        .expect("should create link2");
    std::os::unix::fs::symlink(src_dir.join("link2.rs"), src_dir.join("link3.rs"))
        .expect("should create link3");

    let mut tethys = Tethys::new(workspace.path()).expect("should create Tethys");
    let stats = tethys.index().expect("index should succeed");

    // All four paths (target + 3 links) point to the same content.
    // Each symlink is a separate directory entry, so walk_dir discovers
    // each one and they're all indexed as separate files.
    assert!(
        stats.files_indexed >= 1,
        "should index at least the target file"
    );

    let symbols = tethys
        .search_symbols("target_function")
        .expect("search should succeed");
    assert!(!symbols.is_empty(), "should find symbol from symlink chain");
}

// === Dangling symlink ===

#[cfg(unix)]
#[test]
fn dangling_symlink_does_not_cause_error() {
    let workspace = tempfile::tempdir().expect("should create workspace dir");
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("should create src dir");

    // Create a real file
    fs::write(src_dir.join("good.rs"), "pub fn good_function() {}\n")
        .expect("should write good file");

    // Create a dangling symlink (target doesn't exist)
    std::os::unix::fs::symlink("/nonexistent/path/file.rs", src_dir.join("dangling.rs"))
        .expect("should create dangling symlink");

    let mut tethys = Tethys::new(workspace.path()).expect("should create Tethys");
    let stats = tethys
        .index()
        .expect("index should succeed despite dangling symlink");

    // The dangling symlink should be silently skipped (is_file() returns false
    // for dangling symlinks)
    assert_eq!(
        stats.files_indexed, 1,
        "should index only the real file, not the dangling symlink"
    );

    let symbols = tethys
        .search_symbols("good_function")
        .expect("search should succeed");
    assert_eq!(symbols.len(), 1);
}

// === Symlink to file within workspace (normal case) ===

#[cfg(unix)]
#[test]
fn symlink_within_workspace_indexes_through_link_path() {
    let workspace = tempfile::tempdir().expect("should create workspace dir");
    let src_dir = workspace.path().join("src");
    let lib_dir = workspace.path().join("lib");
    fs::create_dir_all(&src_dir).expect("should create src dir");
    fs::create_dir_all(&lib_dir).expect("should create lib dir");

    // Create a real file in lib/
    fs::write(lib_dir.join("utils.rs"), "pub fn utility_function() {}\n")
        .expect("should write utils file");

    // Create a symlink in src/ pointing to the file in lib/
    std::os::unix::fs::symlink(lib_dir.join("utils.rs"), src_dir.join("utils_link.rs"))
        .expect("should create symlink");

    let mut tethys = Tethys::new(workspace.path()).expect("should create Tethys");
    let stats = tethys.index().expect("index should succeed");

    // Both the original and the symlink are discovered as separate directory
    // entries, so both are indexed
    assert_eq!(stats.files_indexed, 2);
    let original_path = std::path::Path::new("lib/utils.rs");
    let link_path = std::path::Path::new("src/utils_link.rs");
    let original = tethys.get_file(original_path).unwrap().unwrap();
    let linked = tethys.get_file(link_path).unwrap().unwrap();
    assert_eq!(original.path, original_path);
    assert_eq!(linked.path, link_path);
    assert_ne!(original.id, linked.id);

    let original_symbols = tethys.list_symbols(original_path).unwrap();
    let linked_symbols = tethys.list_symbols(link_path).unwrap();
    for (symbols, file_id) in [
        (&original_symbols, original.id),
        (&linked_symbols, linked.id),
    ] {
        assert_eq!(
            symbols
                .iter()
                .map(|symbol| (symbol.name.as_str(), symbol.file_id))
                .collect::<Vec<_>>(),
            [("utility_function", file_id)]
        );
    }
    assert_ne!(original_symbols[0].id, linked_symbols[0].id);
    let mut found = tethys
        .search_symbols("utility_function")
        .unwrap()
        .iter()
        .map(|symbol| (symbol.id, symbol.file_id))
        .collect::<Vec<_>>();
    found.sort_unstable_by_key(|(symbol, file)| (symbol.as_i64(), file.as_i64()));
    let mut expected = vec![
        (original_symbols[0].id, original.id),
        (linked_symbols[0].id, linked.id),
    ];
    expected.sort_unstable_by_key(|(symbol, file)| (symbol.as_i64(), file.as_i64()));
    assert_eq!(found, expected);
}
