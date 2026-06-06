//! Regression fence for per-file `ModuleResolver` dispatch (separator-fix
//! claim C5's stress shape).
//!
//! The fixture is adversarial by name collision: a Rust workspace crate
//! literally named `System` next to a C# file with `using System;`. If
//! resolver dispatch ever keys on anything other than the individual
//! file's language (e.g., workspace-majority language), the C# import
//! could resolve through Rust crate routing and mint a phantom
//! `.cs -> .rs` file dependency. Correct dispatch sends C# imports to the
//! declining stub (tethys-jwf9 tracks the real C# implementation).

use rusqlite::params;

mod common;

use common::{open_db, workspace_with_files};

#[test]
fn csharp_imports_never_resolve_through_rust_crate_routing() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "Cargo.toml",
            r#"
[workspace]
members = ["System"]
resolver = "2"
"#,
        ),
        (
            "System/Cargo.toml",
            r#"
[package]
name = "System"
version = "0.1.0"
edition = "2021"
"#,
        ),
        ("System/src/lib.rs", "pub fn console() {}\n"),
        (
            "cs/App.cs",
            r#"
using System;

namespace App
{
    public class Runner
    {
        public void Go() { Console.WriteLine("x"); }
    }
}
"#,
        ),
    ]);

    tethys.index().expect("index should succeed");

    let conn = open_db(&tethys);

    // Sanity: both languages actually indexed — without this the phantom
    // check could pass vacuously.
    let cs_files: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path LIKE '%.cs'",
            params![],
            |row| row.get(0),
        )
        .expect("count cs files");
    let rs_files: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path LIKE '%.rs'",
            params![],
            |row| row.get(0),
        )
        .expect("count rs files");
    assert_eq!(cs_files, 1, "fixture's C# file must be indexed");
    assert_eq!(rs_files, 1, "fixture's Rust file must be indexed");

    // The fence: no .cs -> .rs file dependency may exist.
    let phantom_deps: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM file_deps d
             JOIN files ff ON d.from_file_id = ff.id
             JOIN files tf ON d.to_file_id = tf.id
             WHERE ff.path LIKE '%.cs' AND tf.path LIKE '%.rs'",
            params![],
            |row| row.get(0),
        )
        .expect("count phantom deps");
    assert_eq!(
        phantom_deps, 0,
        "a C# import resolved through Rust crate routing — ModuleResolver \
         dispatch must key on the file's language"
    );
}
