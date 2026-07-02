//! Regression fences for re-export references (tethys-v1w8).
//!
//! A `pub use` site emits one `kind='reexport'` ref per named leaf, resolved
//! through the same explicit-import machinery as a bare body usage. These
//! tests pin the falsifiable-design claims (C2, C5, C7 here; C6/C8–C10,
//! C12/C13 in sibling tests below) against a REAL index each test builds
//! itself — never an ambient DB.

#![allow(clippy::needless_raw_string_hashes)]

mod common;

use common::{open_db, workspace_with_files};

/// (symbol_id, defining file path) for every resolved reexport ref of `name`.
fn resolved_reexport_targets(conn: &rusqlite::Connection, name: &str) -> Vec<(i64, String)> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, f.path FROM refs r
             JOIN symbols s ON r.symbol_id = s.id
             JOIN files f ON s.file_id = f.id
             WHERE r.kind = 'reexport' AND s.name = ?1",
        )
        .expect("prepare");
    let rows = stmt
        .query_map([name], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query");
    rows.map(|r| r.expect("row")).collect()
}

fn scalar(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get(0)).expect("scalar")
}

/// C2 + collision stress (tethys-53iv family), pinning BOTH parity outcomes
/// against same-named decoys in another file:
///
/// 1. Anchored path (`pub use crate::inner::helper`): the reexport resolves
///    to inner.rs via its import path — never the decoy — and agrees with the
///    bare body call in user.rs using the same anchored import.
/// 2. Bare single-segment path (`pub use inner::dup` — tethys-z9mr): the
///    resolver declines the path, the unique-name fallback declines the
///    ambiguity, and BOTH the reexport ref and a same-file bare call stay
///    unresolved. A conservative decline, not a wrong edge — parity holds.
///
/// Bug this fails under: name-only resolution binding either ref to
/// `other.rs`'s decoy, or the reexport diverging from bare-usage behavior.
///
/// TRIPWIRE: when tethys-z9mr lands, arm 2 resolves to inner.rs — flip its
/// expectations (and drop the unresolved asserts) at that point.
#[test]
fn reexport_resolves_like_bare_usage_despite_same_named_decoy() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod inner;\npub mod other;\npub mod user;\n\
             pub use crate::inner::helper;\npub use inner::dup;\n\
             pub fn lib_go() {\n    dup();\n}\n",
        ),
        ("src/inner.rs", "pub fn helper() {}\npub fn dup() {}\n"),
        // Decoys: same names, different file, never imported anywhere.
        ("src/other.rs", "pub fn helper() {}\npub fn dup() {}\n"),
        (
            "src/user.rs",
            "use crate::inner::helper;\npub fn go() {\n    helper();\n}\n",
        ),
    ]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    // Arm 1: anchored path resolves past the decoy.
    let reexport_targets = resolved_reexport_targets(&conn, "helper");
    assert_eq!(
        reexport_targets.len(),
        1,
        "exactly one resolved reexport ref for helper (anchored path)"
    );
    let (reexport_symbol, ref file) = reexport_targets[0];
    assert_eq!(
        file, "src/inner.rs",
        "reexport must follow its import path to inner.rs, never the decoy"
    );
    let call_symbol: i64 = conn
        .query_row(
            "SELECT r.symbol_id FROM refs r
             JOIN files f ON r.file_id = f.id
             WHERE r.kind = 'call' AND f.path = 'src/user.rs' AND r.symbol_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("resolved call ref in user.rs");
    assert_eq!(
        reexport_symbol, call_symbol,
        "reexport and bare-usage must resolve to the same symbol (C2)"
    );

    // Arm 2: bare single-segment path declines — for BOTH ref kinds equally.
    assert_eq!(
        resolved_reexport_targets(&conn, "dup").len(),
        0,
        "bare-segment reexport with a decoy must decline, not guess (tethys-z9mr)"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE kind = 'reexport'
             AND symbol_id IS NULL AND reference_name = 'dup'",
        ),
        1,
        "the declined reexport ref is stored unresolved with its name"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs r JOIN files f ON r.file_id = f.id
             WHERE r.kind = 'call' AND f.path = 'src/lib.rs'
             AND r.symbol_id IS NULL AND r.reference_name = 'dup'",
        ),
        1,
        "parity: the same-file bare call declines identically"
    );
}

/// C5: a re-export of a non-workspace name stores an UNRESOLVED ref —
/// symbol_id NULL, reference_name populated — per the existing convention
/// for unresolved references.
///
/// Bug this fails under: skipping external targets entirely (no row), or
/// force-binding them to a same-named local symbol.
#[test]
fn external_reexport_stored_unresolved() {
    let (_dir, mut tethys) =
        workspace_with_files(&[("src/lib.rs", "pub use serde::Serialize;\n")]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE kind = 'reexport'
             AND symbol_id IS NULL AND reference_name = 'Serialize'",
        ),
        1,
        "external reexport target must be stored unresolved with its name"
    );
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM refs WHERE kind = 'reexport'"),
        1,
        "and it must be the only reexport ref"
    );
}

/// C7: `self::` and `crate::` prefixed re-exports resolve with parity to the
/// plain-path form — all three land on the defining file via the imports
/// table (pinned empirically during slice 3; unlike qualified CALLS, import
/// paths handle the crate prefix — contrast tethys-3i35).
///
/// Bug this fails under: a bespoke path resolution in the emitter diverging
/// from ModuleResolver semantics for prefixed paths.
#[test]
fn path_prefix_reexports_resolve_like_plain_imports() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod inner;\npub use inner::ha;\npub use self::inner::hb;\npub use crate::inner::hc;\n",
        ),
        ("src/inner.rs", "pub fn ha() {}\npub fn hb() {}\npub fn hc() {}\n"),
    ]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    for name in ["ha", "hb", "hc"] {
        let targets = resolved_reexport_targets(&conn, name);
        assert_eq!(
            targets.len(),
            1,
            "reexport of `{name}` must resolve exactly once"
        );
        assert_eq!(
            targets[0].1, "src/inner.rs",
            "reexport of `{name}` must resolve to its defining file"
        );
    }
}
