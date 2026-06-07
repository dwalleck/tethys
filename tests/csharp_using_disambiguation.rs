//! Regression fences for the C# using-arm (csharp-ns claims C3, C4, C5).
//!
//! The fixture embeds the collision bug classes directly: `Widget` exists
//! in two namespaces (only one used), `Gear` is workspace-unique inside the
//! used namespace, and `Assist` METHODS exist in two namespaces. Expected
//! outcomes were pre-written in `.csharp-ns/expectations.md` (E5) and the
//! collision baseline (UNRESOLVED pre-change) was probed on 2026-06-06.

use rusqlite::params;

mod common;

use common::{open_db, workspace_with_files};

fn fixture() -> (tempfile::TempDir, tethys::Tethys) {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "cs/App.cs",
            r"
using My.Models;

namespace My.App
{
    public class Runner
    {
        public void Go()
        {
            var w = new Widget();
            var g = new Gear();
            Assist();
        }
    }
}
",
        ),
        (
            "cs/Models.cs",
            r"
namespace My.Models
{
    public class Widget { }
    public class Gear { }
    public static class Util { public static void Assist() { } }
}
",
        ),
        (
            "cs/Dupe.cs",
            r"
namespace Dupe.Ns
{
    public class Widget { }
    public static class Util2 { public static void Assist() { } }
}
",
        ),
    ]);
    tethys.index().expect("index should succeed");
    (dir, tethys)
}

/// Resolution state of the single ref named `name` in cs/App.cs:
/// `(resolved_target_path, resolved_symbol_kind)` or `None` when unresolved.
fn ref_target(tethys: &tethys::Tethys, name: &str) -> Option<(String, String)> {
    let conn = open_db(tethys);
    conn.query_row(
        "SELECT tf.path, ts.kind
         FROM refs r
         JOIN files f ON r.file_id = f.id
         LEFT JOIN symbols ts ON r.symbol_id = ts.id
         LEFT JOIN files tf ON ts.file_id = tf.id
         WHERE f.path = 'cs/App.cs'
           AND (r.reference_name = ?1
                OR (r.reference_name IS NULL AND r.symbol_id IN
                    (SELECT id FROM symbols WHERE name = ?1)))",
        params![name],
        |row| {
            Ok(row
                .get::<_, Option<String>>(0)?
                .zip(row.get::<_, Option<String>>(1)?))
        },
    )
    .expect("ref row must exist")
}

/// C3: a type name colliding across namespaces resolves to the USED
/// namespace's symbol (baseline: UNRESOLVED — unique-fallback declined).
#[test]
fn colliding_type_resolves_to_used_namespace() {
    let (_dir, tethys) = fixture();
    assert_eq!(
        ref_target(&tethys, "Widget"),
        Some(("cs/Models.cs".to_string(), "class".to_string())),
        "Widget must resolve to the used namespace's class, not Dupe.Ns's"
    );
}

/// C4: arm-order safety — a workspace-unique type inside the used namespace
/// keeps the target the fallback already produced (no flip; the using-arm
/// resolves it to the same symbol earlier in the chain).
#[test]
fn unique_type_keeps_same_target() {
    let (_dir, tethys) = fixture();
    assert_eq!(
        ref_target(&tethys, "Gear"),
        Some(("cs/Models.cs".to_string(), "class".to_string()))
    );
}

/// C5: bare MEMBER names are not disambiguated by plain usings (types-only
/// kind filter); colliding Assist methods stay unresolved.
#[test]
fn colliding_member_stays_unresolved() {
    let (_dir, tethys) = fixture();
    assert_eq!(
        ref_target(&tethys, "Assist"),
        None,
        "bare member collision must NOT resolve through a plain using \
         (using static is tethys-usgf territory)"
    );
}
