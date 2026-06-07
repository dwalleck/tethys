//! Regression fence for per-file `ModuleResolver` dispatch (separator-fix
//! claim C5's stress shape) and for C# import storage format.
//!
//! The fixture is adversarial by name collision: a Rust workspace crate
//! literally named `System` next to a C# file with `using System;`. If
//! resolver dispatch ever keys on anything other than the individual
//! file's language (e.g., workspace-majority language), the C# import
//! could resolve through Rust crate routing and mint a phantom
//! `.cs -> .rs` file dependency. Correct dispatch sends C# imports to the
//! declining stub (tethys-jwf9 tracks the real C# implementation).
//!
//! Parameterized across batch and streaming modes (PR-review findings
//! I2/I3): the two modes store imports through different copies of the
//! separator logic (`Tethys::store_imports` vs the batch writer's) and
//! compute dependencies through different paths (`compute_dependencies`
//! vs `compute_dependencies_from_stored`). A C#-separator or dispatch
//! regression in the streaming copies would be invisible to batch-only
//! assertions.

use rstest::rstest;
use rusqlite::params;
use tethys::IndexOptions;

mod common;

use common::{open_db, workspace_with_files};

#[rstest]
#[case::batch(IndexOptions::default)]
#[case::streaming(IndexOptions::with_streaming)]
fn csharp_imports_never_resolve_through_rust_crate_routing(
    #[case] options_factory: fn() -> IndexOptions,
) {
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
using My.Models;

namespace App
{
    public class Runner
    {
        public void Go()
        {
            var w = new Widget();
            Console.WriteLine("x");
        }
    }
}
"#,
        ),
        (
            "cs/Models.cs",
            r"
namespace My.Models
{
    public class Widget { }
}
",
        ),
    ]);

    tethys
        .index_with_options(options_factory())
        .expect("index should succeed");

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
    assert_eq!(cs_files, 2, "fixture's C# files must be indexed");
    assert_eq!(rs_files, 1, "fixture's Rust file must be indexed");

    // I2: the dotted C# namespace import is stored in C#'s own format —
    // a swapped separator constant in this mode's store_imports copy would
    // store 'My::Models'.
    let dotted_import: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM imports i JOIN files f ON f.id = i.file_id
             WHERE f.path = 'cs/App.cs' AND i.source_module = 'My.Models'",
            params![],
            |row| row.get(0),
        )
        .expect("count dotted import");
    assert_eq!(
        dotted_import, 1,
        "C# import must be stored as 'My.Models' (dotted) in this indexing mode"
    );

    // The dispatch fence: no .cs -> .rs file dependency may exist (I3
    // covers the streaming-mode dependency path).
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

    // Positive control: a USED C# namespace import (new Widget() under
    // `using My.Models;`) still yields the cs->cs dep through the
    // call-edge + namespace-corroboration path (L2 semantics, csharp-ns
    // decision #2 — the L1 per-using post-pass is gone), proving the zero
    // above comes from correct dispatch, not from C# deps being broken.
    let cs_cs_deps: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM file_deps d
             JOIN files ff ON d.from_file_id = ff.id
             JOIN files tf ON d.to_file_id = tf.id
             WHERE ff.path = 'cs/App.cs' AND tf.path = 'cs/Models.cs'",
            params![],
            |row| row.get(0),
        )
        .expect("count cs->cs deps");
    assert_eq!(
        cs_cs_deps, 1,
        "the used C# namespace import must produce the App.cs -> Models.cs dep"
    );
}
