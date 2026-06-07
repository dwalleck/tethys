//! Regression fence for C# cross-directory file deps (csharp-ns claim C9).
//!
//! C# files in different top-level directories land in different `orphan:`
//! pseudo-crates, so the K-hybrid filter drops their call-edge candidates
//! unless corroborated. The corroboration: the caller's using-directives
//! intersect the namespaces declared in the callee file. Baseline evidence
//! (probed at design time): the edge existed pre-change only via the
//! namespace post-pass; deletion without this corroboration loses it.
//!
//! Three bug classes, three distinct asserts:
//! - corroborated cross-bucket edge LOST (deletion-without-corroboration)
//! - uncorroborated cross-bucket edge KEPT (corroboration too loose)
//! - same-bucket edge LOST (corroboration too tight / arm misrouted)

use rusqlite::params;

mod common;

use common::{open_db, workspace_with_files};

fn dep_count(tethys: &tethys::Tethys, from: &str, to: &str) -> i64 {
    let conn = open_db(tethys);
    conn.query_row(
        "SELECT COUNT(*)
         FROM file_deps d
         JOIN files ff ON d.from_file_id = ff.id
         JOIN files tf ON d.to_file_id = tf.id
         WHERE ff.path = ?1 AND tf.path = ?2",
        params![from, to],
        |row| row.get(0),
    )
    .expect("dep query")
}

#[test]
fn cross_dir_deps_follow_using_corroboration() {
    let (_dir, mut tethys) = workspace_with_files(&[
        // Explicit virtual workspace with no members: workspace_with_files
        // would otherwise inject a root [package] Cargo.toml, making the
        // whole tree ONE crate bucket and short-circuiting the K-hybrid
        // cross-bucket arm this fence exists to exercise (the C# files
        // must stay orphan-bucketed by top-level directory).
        ("Cargo.toml", "[workspace]\nmembers = []\n"),
        (
            "services/Svc.cs",
            r"
using Domain.Models;

namespace App.Services
{
    public class Svc
    {
        public void Run()
        {
            var w = new Widget();
            var u = new UtilThing();
            var h = new SvcHelper();
        }
    }
}
",
        ),
        (
            "models/Widget.cs",
            r"
namespace Domain.Models
{
    public class Widget { }
}
",
        ),
        // Referenced WITHOUT a using for its namespace (unique-fallback
        // resolves the ref; the file dep must NOT cross the bucket).
        (
            "util/Thing.cs",
            r"
namespace Util.Ns
{
    public class UtilThing { }
}
",
        ),
        // Same bucket as Svc.cs: intra-bucket edges are always kept.
        (
            "services/Helper.cs",
            r"
namespace App.Services
{
    public class SvcHelper { }
}
",
        ),
    ]);
    tethys.index().expect("index should succeed");

    // Corroborated cross-bucket edge: services -> models via `using
    // Domain.Models;`. (Count tightens to exactly 1 once the namespace
    // post-pass is deleted; ≥1 is the claim here.)
    assert!(
        dep_count(&tethys, "services/Svc.cs", "models/Widget.cs") >= 1,
        "corroborated cross-directory dep must exist"
    );

    // Uncorroborated cross-bucket edge: UtilThing resolves (workspace-
    // unique) but Svc.cs has no using for Util.Ns — no dep may cross.
    assert_eq!(
        dep_count(&tethys, "services/Svc.cs", "util/Thing.cs"),
        0,
        "uncorroborated cross-directory dep must be dropped (K-hybrid)"
    );

    // Same-bucket edge: always kept, corroboration not consulted.
    assert!(
        dep_count(&tethys, "services/Svc.cs", "services/Helper.cs") >= 1,
        "same-bucket dep must survive"
    );
}
