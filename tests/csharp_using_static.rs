//! Regression fences for the C# `using static` static-member arm
//! (usgf claims C2, C3, C4, C5).
//!
//! `using static Ns.Type;` brings Type's static methods into scope so a bare
//! method call colliding across types disambiguates to the imported type's
//! method. Workspace-unique names already resolve via the fallback (the
//! jwf9 lesson) — the gain is collision disambiguation only. Baselines
//! (UNRESOLVED for the colliding cases) were probed 2026-06-07.

use rusqlite::params;

mod common;

use common::{open_db, workspace_with_files};

/// Resolution target of the single ref `name` in cs/App.cs, or None.
fn ref_target(tethys: &tethys::Tethys, name: &str) -> Option<String> {
    let conn = open_db(tethys);
    conn.query_row(
        "SELECT ts.qualified_name
         FROM refs r
         JOIN files f ON r.file_id = f.id
         LEFT JOIN symbols ts ON r.symbol_id = ts.id
         WHERE f.path = 'cs/App.cs'
           AND (r.reference_name = ?1
                OR (r.reference_name IS NULL AND ts.name = ?1))",
        params![name],
        |row| row.get::<_, Option<String>>(0),
    )
    .expect("ref row must exist")
}

/// C2: a colliding bare method name with `using static Ns.Helper;` resolves
/// to Helper's method (baseline: UNRESOLVED — two Assist, fallback declines).
#[test]
fn static_using_disambiguates_colliding_method() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "cs/App.cs",
            r"
using static My.Models.Helper;

namespace My.App
{
    public class Runner { public void Go() { Assist(); } }
}
",
        ),
        (
            "cs/Helper.cs",
            "namespace My.Models { public static class Helper { public static void Assist() { } } }\n",
        ),
        (
            "cs/Other.cs",
            "namespace Some.Where { public static class Other { public static void Assist() { } } }\n",
        ),
    ]);
    tethys.index().expect("index");
    assert_eq!(
        ref_target(&tethys, "Assist"),
        Some("Helper::Assist".to_string()),
        "must disambiguate to the statically-imported Helper::Assist"
    );
}

/// C3: a name that is BOTH a type (via plain using) and a static method (via
/// static using) is a cross-arm collision — declines (decision #3 union).
#[test]
fn cross_arm_type_vs_method_collision_declines() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "cs/App.cs",
            r"
using My.Types;
using static My.Util.Tools;

namespace My.App
{
    public class Runner { public void Go() { var x = Foo(); } }
}
",
        ),
        (
            "cs/Types.cs",
            "namespace My.Types { public class Foo { } }\n",
        ),
        (
            "cs/Tools.cs",
            "namespace My.Util { public static class Tools { public static int Foo() { return 1; } } }\n",
        ),
    ]);
    tethys.index().expect("index");
    assert_eq!(
        ref_target(&tethys, "Foo"),
        None,
        "type Foo + method Foo across arms is ambiguous → decline"
    );
}

/// C4: prefix-scoping — a method of a DIFFERENT type in the SAME namespace
/// files is not matched. `using static Ns.Helper` + bare `Zap` → `Helper::Zap`,
/// never `Other::Zap` (both in Ns).
#[test]
fn static_using_scopes_to_the_imported_type() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "cs/App.cs",
            r"
using static My.Models.Helper;

namespace My.App
{
    public class Runner { public void Go() { Zap(); } }
}
",
        ),
        (
            "cs/Models.cs",
            r"
namespace My.Models
{
    public static class Helper { public static void Zap() { } }
    public static class Other { public static void Zap() { } }
}
",
        ),
    ]);
    tethys.index().expect("index");
    assert_eq!(
        ref_target(&tethys, "Zap"),
        Some("Helper::Zap".to_string()),
        "scopes to Helper, never Other (both in My.Models)"
    );
}

/// C5: an external static using (prefix not a workspace namespace) declines.
#[test]
fn external_static_using_declines() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "cs/App.cs",
            r"
using static System.Math;

namespace My.App
{
    public class Runner { public void Go() { var x = Sqrt(); var y = Sqrt(); } }
}
",
        ),
        // TWO workspace Sqrt definitions (neither statically imported) keep
        // the name non-unique, so the fallback declines — isolating the
        // external-static-using behavior: only the static arm could resolve
        // it, and System.Math (external) contributes no candidate.
        (
            "cs/One.cs",
            "namespace Some.Where { public static class C { public static int Sqrt() { return 0; } } }\n",
        ),
        (
            "cs/Two.cs",
            "namespace Else.Where { public static class D { public static int Sqrt() { return 1; } } }\n",
        ),
    ]);
    tethys.index().expect("index");
    assert_eq!(
        ref_target(&tethys, "Sqrt"),
        None,
        "System.Math is not a workspace namespace → no static-arm resolution; \
         the two workspace Sqrt collide so the fallback declines too"
    );
}
