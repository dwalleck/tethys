//! Regression fence for C# L2 file-dep semantics (csharp-ns claims C8, C10).
//!
//! The namespace post-pass minted an edge for EVERY using-directive
//! matching an internal namespace, used or not (L1). With it deleted, C#
//! file deps derive from resolved references (call-edge phase, with
//! namespace-corroborated cross-bucket edges) — used imports only (L2,
//! spec decision #2). The fixture is the ground-truth workspace shape and
//! the expected rows are `.csharp-ns/expectations.md` E2, written at
//! slice 0 BEFORE any code:
//!
//! | edge | L1 baseline | L2 (this fence) |
//! |---|---|---|
//! | App.cs -> Models.cs | 4 (1 post-pass + 3 call-edge) | 3 |
//! | App.cs -> Other.cs (unused using) | 1 | ABSENT |
//! | GlobalUsings.cs -> Globals.cs (no refs) | 1 | ABSENT |
//! | UseScoped.cs -> Scoped.cs | 2 | 1 |
//! | UseScoped.cs -> Nested.cs | 1 (call-edge only) | 1 |
//!
//! An L1 reintroduction (e.g. removing `compute_dependencies`' glob skip)
//! resurrects the absent rows; a partial deletion shows in the counts.

use rusqlite::params;

mod common;

use common::{open_db, workspace_with_files};

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the fixture IS the test: the full ground-truth workspace inlined so the expected-set assertion is self-contained"
)]
fn csharp_file_deps_are_l2_used_only() {
    let (_dir, mut tethys) = workspace_with_files(&[
        // Virtual workspace, no members: keeps the C# files orphan-bucketed
        // (workspace_with_files would otherwise inject a root [package]
        // manifest — see tests/csharp_cross_dir_deps.rs for the trap).
        ("Cargo.toml", "[workspace]\nmembers = []\n"),
        (
            "src/App.cs",
            r#"
using System;
using My.Models;
using Other.Stuff;
using static My.Models.Helper;
using W = My.Models.Widget;

namespace My.App
{
    public class Runner
    {
        public void Go()
        {
            var w = new Widget();
            Helper.Assist();
            Assist();
            Console.WriteLine("x");
        }
    }
}
"#,
        ),
        (
            "src/Models.cs",
            r"
namespace My.Models
{
    public class Widget { }
    public static class Helper { public static void Assist() { } }
}
",
        ),
        (
            "src/Other.cs",
            r"
namespace Other.Stuff
{
    public class UnusedThing { }
}
",
        ),
        (
            "src/Scoped.cs",
            "namespace My.Scoped;\n\npublic class FileScopedThing { }\n",
        ),
        (
            "src/Nested.cs",
            r"
namespace Outer1
{
    namespace Inner1
    {
        public class NestedThing { }
    }
}
",
        ),
        (
            "src/UseScoped.cs",
            r"
using My.Scoped;
using Outer1.Inner1;

namespace My.App2
{
    public class Runner2
    {
        public void Go2()
        {
            var a = new FileScopedThing();
            var b = new NestedThing();
        }
    }
}
",
        ),
        ("src/GlobalUsings.cs", "global using My.Globals;\n"),
        (
            "src/Globals.cs",
            r"
namespace My.Globals
{
    public class GlobalThing { }
}
",
        ),
    ]);
    tethys.index().expect("index should succeed");

    let conn = open_db(&tethys);
    let all_deps: Vec<(String, String, i64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT ff.path, tf.path, d.ref_count
                 FROM file_deps d
                 JOIN files ff ON d.from_file_id = ff.id
                 JOIN files tf ON d.to_file_id = tf.id
                 ORDER BY ff.path, tf.path",
            )
            .expect("prepare");
        stmt.query_map(params![], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows")
    };

    // The COMPLETE expected L2 set (E2): exact rows, exact counts, nothing
    // extra, nothing missing — absences (App->Other, GlobalUsings->Globals)
    // are asserted by the full-set equality.
    let expected: Vec<(String, String, i64)> = vec![
        ("src/App.cs".into(), "src/Models.cs".into(), 3),
        ("src/UseScoped.cs".into(), "src/Nested.cs".into(), 1),
        ("src/UseScoped.cs".into(), "src/Scoped.cs".into(), 1),
    ];
    assert_eq!(
        all_deps, expected,
        "C# file_deps must be exactly the enumerated L2 set \
         (unused-using and no-ref edges absent, counts call-edge-sourced)"
    );
}
