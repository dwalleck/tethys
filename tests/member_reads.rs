//! Integration tests for C# member-read reference resolution (tethys-xebx).
//!
//! Asserts happen against `refs` columns directly (`symbol_id`, `strategy`,
//! `reference_name`) — NOT `refs_named` — because resolved refs null their
//! `reference_name` (tethys-6rlu corollary: assert bound refs by
//! `symbol_id`, never by name).

mod common;

use common::{open_db, workspace_with_files};

/// xebx design C8: a cross-file variable-receiver read stays unresolved with
/// its receiver-qualified `reference_name` — even when the property is
/// UNIQUE in the workspace. Kills: treating `r::Data` as a simple name (the
/// `unique_workspace` arm would happily bind the unique property, losing the
/// conservative posture) and any `drop_unresolved`-style cleanup applied to
/// `field_access` refs (the row would vanish and Path B with it).
#[test]
fn member_read_cross_file_stays_qualified() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "Lib.cs",
            "namespace Lib\n{\n    public class Result\n    {\n        \
             public int Data => 1;\n    }\n}\n",
        ),
        (
            "Reader.cs",
            "using Lib;\n\nnamespace App\n{\n    public class Reader\n    {\n        \
             public int Go(Result r)\n        {\n            return r.Data;\n        }\n    }\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");

    let conn = open_db(&tethys);
    let (symbol_id, strategy, reference_name): (Option<i64>, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT r.symbol_id, r.strategy, r.reference_name
             FROM refs r JOIN files f ON f.id = r.file_id
             WHERE r.kind = 'field_access' AND f.path = 'Reader.cs'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("exactly one field_access ref in Reader.cs");

    assert_eq!(symbol_id, None, "variable-receiver read must not bind");
    assert_eq!(strategy, None, "unresolved refs carry NULL strategy");
    assert_eq!(
        reference_name.as_deref(),
        Some("r::Data"),
        "receiver-qualified name preserved for Path B"
    );
}

/// xebx design C11 fence / D10 documentation: a bare cross-file call whose
/// workspace-wide candidate set now contains BOTH a method and a same-named
/// property declines (unique-or-decline conservatism computes ambiguity
/// before the kind gate) — the chosen conservative semantics, not an
/// accident; kind-aware candidate filtering is tracked at tethys-0aqj.
///
/// Empirical note (diagnosed while writing this fence): bare static-method
/// calls in this shape resolve through the `unique_workspace` fallback, not
/// the `import_union` arm — the union arm does not search class members via
/// namespace usings. The `Solo()` control proves the fallback works in this
/// fixture, so the decline assert cannot pass vacuously on broken plumbing.
#[test]
fn call_resolution_with_member_symbol_declines_ambiguous() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "A.cs",
            "namespace Lib\n{\n    public class A\n    {\n        \
             public static void Work() { }\n        \
             public static void Solo() { }\n    }\n}\n",
        ),
        (
            "B.cs",
            "namespace Lib\n{\n    public class B\n    {\n        \
             public static int Work => 1;\n    }\n}\n",
        ),
        (
            "Caller.cs",
            "using Lib;\n\nnamespace App\n{\n    public class Caller\n    {\n        \
             public void Go()\n        {\n            Work();\n            Solo();\n        }\n    }\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");

    let conn = open_db(&tethys);
    let strategy_for = |name: &str| -> Option<String> {
        conn.query_row(
            "SELECT r.strategy FROM refs r JOIN files f ON f.id = r.file_id
             WHERE f.path = 'Caller.cs' AND r.kind = 'call'
               AND (r.reference_name = ?1
                    OR r.symbol_id IN (SELECT id FROM symbols WHERE name = ?1))",
            [name],
            |row| row.get(0),
        )
        .expect("call ref should exist")
    };

    assert_eq!(
        strategy_for("Solo"),
        Some("unique_workspace".to_string()),
        "control: the workspace-unique method still resolves"
    );
    assert_eq!(
        strategy_for("Work"),
        None,
        "method + same-named property behind one using => decline (tethys-0aqj)"
    );
}
