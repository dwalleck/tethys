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

/// xebx design C5 fence: member symbols carry `qualified_name` in the same
/// `EnclosingType::Member` shape methods use — the premise that lets
/// type-receiver reads resolve via `qualified_exact`. Kills: a property
/// extractor that forgets `parent_name` (`qualified_name` collapses to the
/// bare member name).
#[test]
fn property_qualified_name_matches_method_convention() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "Api.cs",
        "namespace Lib\n{\n    public class Api\n    {\n        \
         public int Data { get; }\n        public void Run() { }\n    }\n}\n",
    )]);
    tethys.index().expect("index failed");

    let conn = open_db(&tethys);
    let qualified = |name: &str| -> String {
        conn.query_row(
            "SELECT qualified_name FROM symbols WHERE name = ?1",
            [name],
            |row| row.get(0),
        )
        .expect("symbol should exist")
    };
    assert_eq!(qualified("Run"), "Api::Run", "the method convention");
    assert_eq!(
        qualified("Data"),
        "Api::Data",
        "property follows the same `EnclosingType::Member` shape"
    );
}

/// xebx design C14 CLI fence (plan slice 2 stress fixture): `search --kind
/// property` must return only property symbols when a same-named METHOD
/// exists. Kills: a `--kind` filter that parses but is never applied
/// (returns both), and a missing `parse_kind` arm (CLI errors out).
#[test]
fn search_kind_property_filters_out_same_named_method() {
    let (dir, mut tethys) = workspace_with_files(&[(
        "Mixed.cs",
        "namespace Lib\n{\n    public class Api\n    {\n        \
         public object Data { get; set; }\n    }\n\n    public class Svc\n    {\n        \
         public void Data() { }\n    }\n}\n",
    )]);
    tethys.index().expect("index failed");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .args(["search", "Data", "--kind", "property", "-w"])
        .arg(dir.path())
        .output()
        .expect("tethys binary should run");
    assert!(
        out.status.success(),
        "search --kind property failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("(property)"),
        "property hit missing from output:\n{stdout}"
    );
    assert!(
        !stdout.contains("(method)"),
        "--kind property leaked a method hit:\n{stdout}"
    );
}

/// xebx D10 applies to Rust struct fields too (`StructField` is emitted by the
/// Rust extractor): a struct field declared AFTER a same-named fn must not
/// steal the fn's same-file call binds under the last-wins map. Kills: the
/// pre-D10 kind-blind `name_to_id` (the call would bind kind
/// `struct_field`).
#[test]
fn rust_call_does_not_bind_same_file_struct_field() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub fn reload() {}\n\
         pub struct Config {\n    pub reload: u32,\n}\n\
         pub fn go(c: &Config) -> u32 {\n    reload();\n    c.reload\n}\n",
    )]);
    tethys.index().expect("index failed");

    let conn = open_db(&tethys);
    let bound_kind: String = conn
        .query_row(
            "SELECT ts.kind FROM refs r JOIN symbols ts ON ts.id = r.symbol_id
             WHERE r.kind = 'call' AND ts.name = 'reload'",
            [],
            |row| row.get(0),
        )
        .expect("the reload() call should resolve");
    assert_eq!(
        bound_kind, "function",
        "call bound the later-declared struct field instead of the fn"
    );
}
