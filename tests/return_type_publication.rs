//! `symbols.return_type` must survive the real parse → publish path.
//!
//! The unit tests in `src/db/files.rs` construct `SymbolData` directly, which
//! bypasses both `parse_file_static`'s extraction from
//! `FunctionSignature::return_type` and `OwnedSymbolData::as_symbol_data`
//! wiring. This exercises the whole chain through `Tethys::index`, for both
//! languages and both writer modes, and re-reads after a reopen so a value that
//! only lives in memory would not pass.

use std::fs;
use std::path::Path;

use rusqlite::Connection;
use tethys::{IndexOptions, Tethys};

/// Distinct per-symbol values, including the shapes the overview's error-flow
/// filter keys off: bare `Result`, path-qualified `Result`, `Option`, and the
/// C# `Task<OneOf<..>>` async convention. `takes_closure` pins the
/// false-positive case — a closure *parameter* returning `Result` must not be
/// read as the function's own return type.
const RUST_SOURCE: &str = r"
pub fn fallible() -> Result<(), String> { Ok(()) }
pub fn qualified() -> std::io::Result<()> { Ok(()) }
pub fn optional() -> Option<u32> { None }
pub fn infallible(x: u32) -> u32 { x }
pub fn takes_closure<F: Fn() -> Result<(), String>>(f: F) -> u32 { let _ = f; 42 }
pub struct RustWidget;
";

const CSHARP_SOURCE: &str = r"
public class CsharpWidget {
    public Task<OneOf<Success, Error>> Fetch() { return null; }
    public int Count() { return 0; }
}
";

fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temporary workspace");
    fs::create_dir(dir.path().join("src")).expect("source directory");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"return_type_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("manifest");
    fs::write(dir.path().join("src/lib.rs"), RUST_SOURCE).expect("rust source");
    fs::write(dir.path().join("src/Widget.cs"), CSHARP_SOURCE).expect("csharp source");
    dir
}

/// Read `(name, return_type)` straight from the published database, through an
/// independent connection, so the assertion observes persisted state rather
/// than an in-memory cache.
fn persisted(dir: &Path) -> Vec<(String, Option<String>)> {
    let conn = Connection::open(dir.join(".rivets/index/tethys.db")).expect("observer connection");
    conn.prepare(
        "SELECT name, return_type FROM symbols WHERE kind IN ('function', 'method') ORDER BY name",
    )
    .expect("symbol query")
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
    .expect("symbol rows")
    .collect::<rusqlite::Result<Vec<_>>>()
    .expect("collected return types")
}

fn expected() -> Vec<(String, Option<String>)> {
    let mut want: Vec<(String, Option<String>)> = [
        ("Count", "int"),
        ("Fetch", "Task<OneOf<Success, Error>>"),
        ("fallible", "Result<(), String>"),
        ("infallible", "u32"),
        ("optional", "Option<u32>"),
        ("qualified", "std::io::Result<()>"),
        ("takes_closure", "u32"),
    ]
    .into_iter()
    .map(|(name, return_type)| (name.to_string(), Some(return_type.to_string())))
    .collect();
    want.sort();
    want
}

#[test]
fn return_type_survives_parse_publish_and_reopen() {
    for (mode, options) in [
        ("batch", IndexOptions::default()),
        ("streaming", IndexOptions::with_streaming()),
    ] {
        let dir = workspace();
        let mut index = Tethys::new(dir.path()).expect("open index");
        index.index_with_options(options).expect("index workspace");

        assert_eq!(
            persisted(dir.path()),
            expected(),
            "return_type must persist through the parse -> publish path ({mode})"
        );

        // Reopen: the values must come back from the database, not from the
        // indexer that wrote them.
        drop(index);
        let reopened = Tethys::new(dir.path()).expect("reopen index");
        drop(reopened);
        assert_eq!(
            persisted(dir.path()),
            expected(),
            "return_type must survive a reopen ({mode})"
        );
    }
}

/// Non-callable symbols must not acquire a return type, so the error-flow
/// filter cannot pick up a struct or class as a fallible function.
#[test]
fn non_callable_symbols_have_no_return_type() {
    let dir = workspace();
    let mut index = Tethys::new(dir.path()).expect("open index");
    index.index().expect("index workspace");

    let conn = Connection::open(dir.path().join(".rivets/index/tethys.db")).expect("observer");
    for name in ["RustWidget", "CsharpWidget"] {
        let return_type: Option<String> = conn
            .query_row(
                "SELECT return_type FROM symbols WHERE name = ?1",
                [name],
                |row| row.get(0),
            )
            .expect("type symbol row");
        assert_eq!(return_type, None, "{name} must have no return type");
    }
}
