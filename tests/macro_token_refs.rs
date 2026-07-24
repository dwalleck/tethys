//! Integration regression fences for macro-token call extraction
//! (tethys-8ym0). Each test builds its OWN real index — the on-disk index
//! can be stale, so never query an ambient DB.
//!
//! Unit-level fences for token-shape classification live in
//! `src/languages/rust.rs`; these cover what only a full index exercises:
//! Pass-2 resolution + strategy stamping, the unresolved drop, call-edge
//! exclusion (posture D-A), same-file-first binding under name collisions,
//! and deprecated-callers consumption.

#![allow(clippy::needless_raw_string_hashes, clippy::doc_markdown)]

mod common;

use common::{open_db, workspace_with_files};
use rusqlite::Connection;

fn scalar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect("scalar query")
}

/// F1 + F7 fixture: the exact case tethys-y3bx was parked on — a `#[test]`
/// exercising a product fn ONLY through `assert_eq!`.
fn y3bx_blocker_workspace() -> (tempfile::TempDir, tethys::Tethys) {
    workspace_with_files(&[(
        "src/lib.rs",
        "pub fn helper() -> i32 {\n    1\n}\n\
         #[cfg(test)]\nmod tests {\n\
         \x20   use super::*;\n\
         \x20   #[test]\n\
         \x20   fn t() {\n\
         \x20       assert_eq!(helper(), 1);\n\
         \x20   }\n\
         }\n",
    )])
}

/// F1 (claims C1, C2, C5): `assert_eq!(helper(), 1)` inside a `#[test]`
/// produces exactly one `macro_call` ref, bound to `helper`, attributed to
/// the enclosing test fn `t`, with `same_file` strategy. This row is the
/// edge untested-code (tethys-y3bx) needs; it was red before the token walk
/// landed (the old MACRO_INVOCATION early-return emitted nothing).
#[test]
fn macro_token_call_ref_emitted_and_resolved() {
    let (_dir, mut tethys) = y3bx_blocker_workspace();
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    let (symbol, in_symbol, strategy): (String, String, String) = conn
        .query_row(
            "SELECT s.name, cs.name, r.strategy
             FROM refs r
             JOIN symbols s  ON r.symbol_id = s.id
             LEFT JOIN symbols cs ON r.in_symbol_id = cs.id
             WHERE r.kind = 'macro_call'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("exactly one macro_call ref");
    assert_eq!(symbol, "helper", "binds the product fn");
    assert_eq!(in_symbol, "t", "attributed to the enclosing #[test] fn");
    assert_eq!(strategy, "same_file", "Pass-1 same-file binding");
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM refs WHERE kind = 'macro_call'"),
        1,
        "exactly one macro_call row for the single call site"
    );
}

/// F1b (claim C2, collision class): with a same-named fn in ANOTHER file,
/// the macro-token ref still binds the SAME-FILE definition — a
/// unique-name-only resolver would decline (2 candidates) and the row would
/// be dropped; a wrong-file bind would carry a different fn body.
#[test]
fn macro_token_call_binds_same_file_under_name_collision() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/a.rs",
            "pub fn helper() -> i32 {\n    1\n}\n\
             #[cfg(test)]\nmod tests {\n\
             \x20   use super::*;\n\
             \x20   #[test]\n\
             \x20   fn t() {\n\
             \x20       assert_eq!(helper(), 1);\n\
             \x20   }\n\
             }\n",
        ),
        ("src/b.rs", "pub fn helper() -> i32 {\n    2\n}\n"),
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
    ]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    let bound_file: String = conn
        .query_row(
            "SELECT f.path FROM refs r
             JOIN symbols s ON r.symbol_id = s.id
             JOIN files f   ON s.file_id = f.id
             WHERE r.kind = 'macro_call'",
            [],
            |row| row.get(0),
        )
        .expect("the collision ref still resolves");
    assert_eq!(bound_file, "src/a.rs", "same-file definition wins");
}

/// F2 (claim C1, shadow + collision combined): a local closure named like a
/// REAL fn in another file must be suppressed at extraction — otherwise the
/// unique-workspace arm would happily bind the foreign fn and fabricate a
/// test→product edge.
#[test]
fn macro_token_local_binding_suppressed_even_when_shadowing_real_fn() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod other;\n\
             #[cfg(test)]\nmod tests {\n\
             \x20   #[test]\n\
             \x20   fn t() {\n\
             \x20       let shadow = |x: i32| x;\n\
             \x20       assert_eq!(shadow(1), 1);\n\
             \x20   }\n\
             }\n",
        ),
        ("src/other.rs", "pub fn shadow(x: i32) -> i32 {\n    x\n}\n"),
    ]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM refs WHERE kind = 'macro_call'"),
        0,
        "closure call inside the macro must not become a ref to other::shadow"
    );
}

/// F3 (claim C3): a call-shaped token naming nothing in-crate is emitted
/// speculatively and dropped post-resolution — skip the widened drop sweep
/// and this row survives.
#[test]
fn unresolved_macro_token_call_dropped() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub fn present() -> bool {\n    true\n}\n\
         #[cfg(test)]\nmod tests {\n\
         \x20   use super::*;\n\
         \x20   #[test]\n\
         \x20   fn t() {\n\
         \x20       assert!(undefined_fn(1) == 1 || present());\n\
         \x20   }\n\
         }\n",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE kind = 'macro_call' AND symbol_id IS NULL",
        ),
        0,
        "no unresolved macro_call rows survive indexing"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE reference_name = 'undefined_fn'",
        ),
        0,
        "the speculative row for the unresolvable token is gone entirely"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs r JOIN symbols s ON r.symbol_id = s.id
             WHERE r.kind = 'macro_call' AND s.name = 'present'",
        ),
        1,
        "the resolvable token in the same macro is retained (drop is not over-broad)"
    );
}

/// F4 (TRIPWIREs tethys-9l27 / tethys-ewa7): method shapes and path shapes
/// inside macros stay unextracted. FLIP the relevant assert when either
/// issue ships its extension.
#[test]
fn method_and_path_shapes_not_emitted() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub struct R;\n\
         impl R {\n    pub fn meth(&self) -> bool {\n        true\n    }\n}\n\
         pub mod m {\n    pub fn pf() -> i32 {\n        1\n    }\n}\n\
         #[cfg(test)]\nmod tests {\n\
         \x20   use super::*;\n\
         \x20   #[test]\n\
         \x20   fn t() {\n\
         \x20       let r = R;\n\
         \x20       assert!(r.meth());\n\
         \x20       assert_eq!(m::pf(), 1);\n\
         \x20   }\n\
         }\n",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM refs WHERE kind = 'macro_call'"),
        0,
        "method shape (r.meth → tethys-9l27) and path shape (m::pf → \
         tethys-ewa7) must not emit macro_call refs"
    );
}

/// F5 (TRIPWIRE tethys-7dqj + claim C1 nesting): the call inside a NESTED
/// macro's token tree emits; the nested macro NAME does not.
#[test]
fn nested_macro_call_emits_inner_name_does_not() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub fn g(x: i32) -> i32 {\n    x\n}\n\
         #[cfg(test)]\nmod tests {\n\
         \x20   use super::*;\n\
         \x20   #[test]\n\
         \x20   fn t() {\n\
         \x20       assert!(matches!(g(1), 1));\n\
         \x20   }\n\
         }\n",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs r JOIN symbols s ON r.symbol_id = s.id
             WHERE r.kind = 'macro_call' AND s.name = 'g'",
        ),
        1,
        "call nested inside matches!'s token tree emits"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE kind = 'macro_call' AND symbol_id IN
             (SELECT id FROM symbols WHERE name = 'matches')",
        ),
        0,
        "nested macro NAME is not a macro_call ref (tethys-7dqj)"
    );
}

/// F6 (claim C8): `macro_rules!` definition templates emit nothing even when
/// the template's call-shape token matches a real in-crate fn — a walk
/// hooked generically at token_tree level would bind `inner_call` here.
#[test]
fn macro_rules_template_emits_no_refs() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub fn inner_call() -> i32 {\n    1\n}\n\
         macro_rules! deffy {\n    () => {\n        inner_call()\n    };\n}\n\
         pub fn user() -> i32 {\n    deffy!()\n}\n",
    )]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM refs WHERE kind = 'macro_call'"),
        0,
        "definition template must not emit (deffy!() invocation has an empty tree)"
    );
}

/// F7 (claim C4, posture D-A): the macro-token edge exists in `refs` but
/// NEVER in `call_edges` — `callers` stays pristine. Forgetting to extend
/// populate_call_edges' NOT IN list fails this.
#[test]
fn macro_call_excluded_from_call_edges_and_callers() {
    let (_dir, mut tethys) = y3bx_blocker_workspace();
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM refs WHERE kind = 'macro_call'"),
        1,
        "precondition: the macro-token ref exists"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM call_edges ce
             JOIN symbols s ON ce.callee_symbol_id = s.id
             WHERE s.name = 'helper'",
        ),
        0,
        "macro_call must not reach call_edges"
    );

    let qualified: String = conn
        .query_row(
            "SELECT qualified_name FROM symbols WHERE name = 'helper'",
            [],
            |row| row.get(0),
        )
        .expect("helper symbol");
    let callers = tethys
        .get_callers(
            &qualified,
            tethys::CallerMode::Indexed {
                call_edges: tethys::CallEdgeSelection::All,
            },
        )
        .expect("get_callers succeeds");
    assert!(
        callers.is_empty(),
        "callers of a macro-only-called fn stay empty: {callers:?}"
    );
}

/// F11 (claim C7): deprecated-callers consumes macro_call rows from `refs`
/// (kind-blind Path A) — a `#[deprecated]` fn whose ONLY call site lives
/// inside `assert!` gets a listed site. rustc oracle: `--force-warn
/// deprecated` warns on macro-argument calls the same way (the expansion
/// contains the call) — the jdly fixture recorded this ground truth.
#[test]
fn deprecated_caller_inside_macro_listed() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/util.rs",
            "#[deprecated]\npub fn old_api() -> bool {\n    true\n}\n",
        ),
        (
            "src/lib.rs",
            "pub mod util;\n\
             #[cfg(test)]\nmod tests {\n\
             \x20   #[test]\n\
             \x20   fn t() {\n\
             \x20       use crate::util::old_api;\n\
             \x20       assert!(old_api());\n\
             \x20   }\n\
             }\n",
        ),
    ]);
    tethys.index().expect("index");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query");
    let old_api = findings
        .iter()
        .find(|f| f.symbol.name == "old_api")
        .expect("old_api is a deprecated finding");
    assert!(
        old_api
            .sites
            .iter()
            .any(|s| s.file == "src/lib.rs" && s.caller.as_deref() == Some("t")),
        "the assert!-context call site is listed with its test caller: {:?}",
        old_api.sites
    );
}

/// Canonical, order-independent dump of every `macro_call` ref and every
/// `file_deps` row — the comparison surface for F9/F10.
fn macro_and_deps_dump(conn: &Connection) -> Vec<String> {
    let mut out = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT f.path, r.line, r.column, s.name, r.strategy
             FROM refs r
             JOIN files f ON r.file_id = f.id
             JOIN symbols s ON r.symbol_id = s.id
             WHERE r.kind = 'macro_call'",
        )
        .expect("prep macro dump");
    let rows = stmt
        .query_map([], |r| {
            Ok(format!(
                "ref|{}|{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?
            ))
        })
        .expect("macro dump rows");
    for row in rows {
        out.push(row.expect("row"));
    }
    let mut deps = conn
        .prepare(
            "SELECT ff.path, tf.path FROM file_deps d
             JOIN files ff ON d.from_file_id = ff.id
             JOIN files tf ON d.to_file_id = tf.id",
        )
        .expect("prep deps dump");
    let rows = deps
        .query_map([], |r| {
            Ok(format!(
                "dep|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?
            ))
        })
        .expect("deps rows");
    for row in rows {
        out.push(row.expect("dep row"));
    }
    out.sort();
    out
}

/// The F9/F10 fixture: a cross-file import consumed ONLY inside `assert!`,
/// with a DUPLICATE call line (attacks row-collapse/count bugs and the
/// import-corroboration path — no non-macro usage of `helper` exists).
const PARITY_FILES: &[(&str, &str)] = &[
    ("src/lib.rs", "pub mod a;\npub mod b;\n"),
    (
        "src/a.rs",
        "use crate::b::helper;\n\
         pub fn twice() -> bool {\n\
         \x20   assert!(helper());\n\
         \x20   assert!(helper());\n\
         \x20   true\n\
         }\n",
    ),
    ("src/b.rs", "pub fn helper() -> bool {\n    true\n}\n"),
];

/// F9 (claim C9): two rebuilds of the same workspace produce identical
/// macro_call/file_deps content — nondeterministic iteration in the token
/// walk or resolution would flake here.
#[test]
fn macro_token_refs_deterministic_across_rebuilds() {
    let (_dir, mut tethys) = workspace_with_files(PARITY_FILES);
    tethys.rebuild().expect("first rebuild");
    let first = macro_and_deps_dump(&open_db(&tethys));
    tethys.rebuild().expect("second rebuild");
    let second = macro_and_deps_dump(&open_db(&tethys));
    assert_eq!(first, second, "rebuild must be content-deterministic");
    assert_eq!(
        first.iter().filter(|l| l.starts_with("ref|")).count(),
        2,
        "both duplicate call lines carry their own row: {first:?}"
    );
}

/// F10 (claim C10): batch and streaming produce identical macro_call refs
/// AND the import consumed only inside a macro corroborates its file_dep
/// (a.rs -> b.rs) in every mode — a streaming path computing deps from a
/// refs_set that lacks macro tokens would lose the dep edge.
#[test]
fn batch_and_streaming_parity_with_macro_only_corroboration() {
    let (dir, mut tethys) = workspace_with_files(PARITY_FILES);
    tethys.rebuild().expect("rebuild (batch)");
    let batch = macro_and_deps_dump(&open_db(&tethys));
    drop(tethys);

    let mut streaming = tethys::Tethys::new(dir.path()).expect("Tethys::new");
    streaming
        .rebuild_with_options(tethys::IndexOptions::with_streaming())
        .expect("rebuild (streaming)");
    let stream_default = macro_and_deps_dump(&open_db(&streaming));

    streaming
        .rebuild_with_options(tethys::IndexOptions::with_streaming_batch_size(1))
        .expect("rebuild (streaming, batch=1)");
    let stream_one = macro_and_deps_dump(&open_db(&streaming));

    assert_eq!(batch, stream_default, "batch == streaming(default)");
    assert_eq!(batch, stream_one, "batch == streaming(batch_size=1)");
    assert!(
        batch.contains(&"dep|src/a.rs|src/b.rs".to_string()),
        "macro-only usage corroborates the a->b file dep: {batch:?}"
    );
}

/// C2's cross-file half (spec-review finding): a macro-token call whose
/// target lives in ANOTHER file, unique across the workspace, binds via the
/// `unique_workspace` arm — a resolver that only ran the same-file map
/// would leave this row unresolved and the drop would erase it.
#[test]
fn macro_token_call_cross_file_binds_unique_workspace() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod other;\n\
             #[cfg(test)]\nmod tests {\n\
             \x20   #[test]\n\
             \x20   fn t() {\n\
             \x20       assert!(cross_helper() > 0);\n\
             \x20   }\n\
             }\n",
        ),
        ("src/other.rs", "pub fn cross_helper() -> i32 {\n    7\n}\n"),
    ]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);
    let (bound_file, strategy): (String, String) = conn
        .query_row(
            "SELECT f.path, r.strategy FROM refs r
             JOIN symbols s ON r.symbol_id = s.id
             JOIN files f   ON s.file_id = f.id
             WHERE r.kind = 'macro_call'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the cross-file macro-token ref resolves");
    assert_eq!(bound_file, "src/other.rs");
    assert_eq!(
        strategy, "unique_workspace",
        "cross-file unique binding arm"
    );
}
