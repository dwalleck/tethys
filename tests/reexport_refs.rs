//! Regression fences for re-export references (tethys-v1w8).
//!
//! A `pub use` site emits one `kind='reexport'` ref per named leaf, resolved
//! through the same explicit-import machinery as a bare body usage. These
//! tests pin the falsifiable-design claims (C2, C5, C7 here; C6/C8–C10,
//! C12/C13 in sibling tests below) against a REAL index each test builds
//! itself — never an ambient DB.

#![allow(clippy::needless_raw_string_hashes, clippy::doc_markdown)]

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

/// C3 (resolution half) — the F8 fence `aliased_reexport_targets_original`:
/// a top-level aliased re-export (`pub use crate::inner::original_fn as
/// aliased_fn;`) records its ref under the ORIGINAL name and resolves to the
/// original symbol; the alias lives on the imports row only, and no second ref
/// is emitted under the alias. The emission-side half (original name, not
/// alias) is pinned inline by `reexport_refs_one_per_named_leaf_with_parity_gaps`;
/// this pins the resolution target and its parity with a bare body usage (C2).
///
/// Bug this fails under: recording the ref under the alias name (so
/// `original_fn` gets no ref while `aliased_fn` resolves to nothing), or
/// emitting a duplicate ref per aliased site.
#[test]
fn aliased_reexport_targets_original() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod inner;\npub mod user;\n\
             pub use crate::inner::original_fn as aliased_fn;\n",
        ),
        ("src/inner.rs", "pub fn original_fn() {}\n"),
        (
            "src/user.rs",
            "use crate::inner::original_fn;\npub fn go() {\n    original_fn();\n}\n",
        ),
    ]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    // The ref resolves under the ORIGINAL name to inner.rs's definition...
    let targets = resolved_reexport_targets(&conn, "original_fn");
    assert_eq!(
        targets.len(),
        1,
        "aliased reexport must resolve under the original name (C3)"
    );
    let (reexport_symbol, ref file) = targets[0];
    assert_eq!(
        file, "src/inner.rs",
        "aliased reexport must follow its import path to the definition"
    );

    // ...never under the alias, and never as a second ref.
    assert!(
        resolved_reexport_targets(&conn, "aliased_fn").is_empty(),
        "the alias must not be a reference target (it lives on the imports row only)"
    );
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM refs WHERE kind = 'reexport'"),
        1,
        "exactly one reexport ref for an aliased site — no duplicate under the alias"
    );

    // Parity: the aliased reexport binds the same symbol as a bare body usage
    // of the original name (C2).
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
        "aliased reexport and bare usage must resolve to the same symbol"
    );

    // The alias is preserved on the imports row, keyed by the original name
    // (C3: "the alias stays on the imports row only").
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM imports i JOIN files f ON i.file_id = f.id
             WHERE f.path = 'src/lib.rs' AND i.symbol_name = 'original_fn'
             AND i.alias = 'aliased_fn'",
        ),
        1,
        "the alias is recorded on the imports row, keyed by the original name"
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
    let (_dir, mut tethys) = workspace_with_files(&[("src/lib.rs", "pub use serde::Serialize;\n")]);
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

/// C12 — the headline: a symbol whose ONLY reference is its re-export has
/// exactly one inbound ref (the dead-code false positive dies).
///
/// Bug this fails under: any emission or resolution failure (count 0), or
/// double emission per site (count 2).
#[test]
fn reexport_only_symbol_has_exactly_one_inbound_ref() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod inner;\npub use crate::inner::only_via_reexport;\n",
        ),
        ("src/inner.rs", "pub fn only_via_reexport() {}\n"),
    ]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    let inbound = scalar(
        &conn,
        "SELECT COUNT(*) FROM refs r JOIN symbols s ON r.symbol_id = s.id
         WHERE s.name = 'only_via_reexport'",
    );
    assert_eq!(
        inbound, 1,
        "a reexport-only symbol must have exactly one inbound ref"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs r JOIN symbols s ON r.symbol_id = s.id
             WHERE s.name = 'only_via_reexport' AND r.kind = 'reexport'",
        ),
        1,
        "and that ref is the reexport itself"
    );
}

/// C6: a glob re-export emits no refs (deferred to tethys-pv7w), while a
/// single-segment module re-export (`pub use m2;`) — indistinguishable from
/// an item re-export at parse time — binds to the module's declaration
/// symbol (pinned empirically during slice 4: the `pub mod m2;` symbol in
/// the same file wins the same-file name map).
///
/// Bug this fails under: a naive emitter producing a `*` ref for globs, or
/// per-name refs synthesized for the glob target's symbols before pv7w.
#[test]
fn glob_reexport_emits_nothing_and_module_reexport_binds_module() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod inner;\npub mod m2;\npub use inner::*;\npub use m2;\n",
        ),
        ("src/inner.rs", "pub fn g1() {}\npub fn g2() {}\n"),
        ("src/m2.rs", "pub fn unrelated() {}\n"),
    ]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM refs WHERE kind = 'reexport'"),
        1,
        "glob re-export must emit nothing; only the module re-export ref exists"
    );
    let (kind, file): (String, String) = conn
        .query_row(
            "SELECT s.kind, f.path FROM refs r
             JOIN symbols s ON r.symbol_id = s.id
             JOIN files f ON s.file_id = f.id
             WHERE r.kind = 'reexport'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("resolved module reexport ref");
    assert_eq!(kind, "module", "module re-export binds the module symbol");
    assert_eq!(file, "src/lib.rs", "…the `pub mod m2;` declaration symbol");
}

/// C13: re-indexing the same unchanged workspace leaves refs, file_deps and
/// call_edges counts identical — reexport refs don't accumulate (the
/// d4d87f1/tethys-wsix stale-row class).
#[test]
fn reindexing_twice_is_idempotent_for_reexport_refs() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod inner;\npub mod user;\npub use crate::inner::x;\n",
        ),
        ("src/inner.rs", "pub fn x() {}\n"),
        (
            "src/user.rs",
            "use crate::inner::x;\npub fn go() {\n    x();\n}\n",
        ),
    ]);
    tethys.index().expect("first index");
    let counts = |conn: &rusqlite::Connection| {
        (
            scalar(conn, "SELECT COUNT(*) FROM refs"),
            scalar(conn, "SELECT COUNT(*) FROM refs WHERE kind = 'reexport'"),
            scalar(conn, "SELECT COUNT(*) FROM file_deps"),
            scalar(conn, "SELECT COUNT(*) FROM call_edges"),
        )
    };
    let first = counts(&open_db(&tethys));

    tethys.index().expect("second index");
    let second = counts(&open_db(&tethys));

    assert_eq!(
        first, second,
        "second index of an unchanged workspace must not change any counts"
    );
    assert_eq!(first.1, 1, "exactly one reexport ref both times");
}

/// C10: a re-export-only import (name never used in the file's body) creates
/// a file_deps edge — a re-export IS a real file-level dependency. Before
/// tethys-v1w8 this edge was silently missing: resolved reexport refs fell
/// between both dep paths (no call edge by design; reference_name nulled on
/// resolution so the L2 corroboration set missed them).
///
/// The plain-unused control (c.rs) pins no-spill: an unused NON-pub import
/// must still produce no edge (the tethys-msn0 corroboration family).
///
/// Bug this fails under: sourcing file deps only from call_edges (edge
/// missing), or blanket-adding resolved ref names to the corroboration set
/// (c.rs would gain a phantom edge — the 6rlu-rejected design).
#[test]
fn reexport_only_import_creates_file_dep() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod a;\npub mod b;\npub mod c;\n"),
        ("src/a.rs", "pub use crate::b::only_reexported;\n"),
        ("src/b.rs", "pub fn only_reexported() {}\n"),
        // Plain unused import: must NOT gain an edge.
        ("src/c.rs", "use crate::b::only_reexported;\n"),
    ]);
    tethys.index().expect("index");
    let dep = |conn: &rusqlite::Connection, from: &str, to: &str| {
        scalar(
            conn,
            &format!(
                "SELECT COUNT(*) FROM file_deps d
                 JOIN files f1 ON d.from_file_id = f1.id
                 JOIN files f2 ON d.to_file_id = f2.id
                 WHERE f1.path = '{from}' AND f2.path = '{to}'"
            ),
        )
    };
    {
        let conn = open_db(&tethys);
        assert_eq!(
            dep(&conn, "src/a.rs", "src/b.rs"),
            1,
            "re-export-only import must create a file dep (C10)"
        );
        assert_eq!(
            dep(&conn, "src/c.rs", "src/b.rs"),
            0,
            "plain unused import must stay edge-less (no corroboration spill)"
        );
    }

    // Re-index: the edge neither duplicates nor disappears.
    tethys.index().expect("second index");
    let conn = open_db(&tethys);
    assert_eq!(
        dep(&conn, "src/a.rs", "src/b.rs"),
        1,
        "edge stable across re-index"
    );
}

/// C10 residual, pinned as current behavior: a re-export-only import whose
/// path is a bare single segment (`pub use b2::via_bare;` from src/d.rs) gets
/// NO file_dep — the usage corroboration passes (the reexport ref supplies
/// the name), but `resolve_import_segments` declines the single-segment
/// relative path (tethys-z9mr). This is the exact shape behind the missing
/// `lib.rs → unused_imports.rs` edge on the tethys self-index (probe F4).
///
/// TRIPWIRE: when tethys-z9mr lands, this edge appears — flip the
/// expectation to 1. (Aliased re-exports have a separate corroboration gap:
/// tethys-sp24.)
#[test]
fn bare_segment_reexport_only_import_still_lacks_file_dep() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod b2;\npub mod d;\n"),
        ("src/b2.rs", "pub fn via_bare() {}\n"),
        ("src/d.rs", "pub use b2::via_bare;\n"),
    ]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM file_deps d
             JOIN files f1 ON d.from_file_id = f1.id
             JOIN files f2 ON d.to_file_id = f2.id
             WHERE f1.path = 'src/d.rs' AND f2.path = 'src/b2.rs'",
        ),
        0,
        "bare-segment path declined (tethys-z9mr): no dep today — flip when z9mr lands"
    );
    // The ref itself DID resolve (unique name → fallback), proving the gap
    // is the import-path decline, not emission or corroboration.
    assert_eq!(
        resolved_reexport_targets(&conn, "via_bare").len(),
        1,
        "the reexport ref resolves via unique-name fallback even though the dep is missing"
    );
}

/// C8 + C9: reexport refs are structurally invisible to in-symbol consumers.
/// Module-level use declarations have no enclosing symbol, so their refs
/// carry `in_symbol_id NULL` — `populate_call_edges` (`WHERE in_symbol_id IS
/// NOT NULL`) and panic-points (JOIN on `in_symbol_id`) never see them.
///
/// The fixture re-exports an EXTERNAL name `expect` — the exact panic-points
/// predicate shape (unresolved → reference_name='expect') — next to a real
/// `.expect()` method call as the positive control, so the fence
/// distinguishes "reexport leaked into panic-points" from "panic-points
/// broke".
///
/// Bug this fails under: emission attributing a file-level pseudo-symbol to
/// `in_symbol_id` (call_edges gains an edge; panic count becomes 2).
#[test]
fn reexport_refs_stay_out_of_call_edges_and_panic_points() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod inner;\npub mod user;\npub use ext::expect;\npub use crate::inner::target;\n",
        ),
        ("src/inner.rs", "pub fn target() {}\n"),
        (
            "src/user.rs",
            "use crate::inner::target;\npub fn go() {\n    target();\n    let v: Option<i32> = None;\n    let _ = v.expect(\"control panic point\");\n}\n",
        ),
    ]);
    tethys.index().expect("index");
    let conn = open_db(&tethys);

    // Every reexport ref is top-level: no enclosing symbol, ever.
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM refs WHERE kind = 'reexport' AND in_symbol_id IS NOT NULL",
        ),
        0,
        "reexport refs must never carry an enclosing symbol"
    );

    // C8: exactly one call edge (go -> target); the resolved reexport of
    // `target` contributed none.
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM call_edges"),
        1,
        "only the body call produces a call edge"
    );

    // C9: exactly one panic point — the .expect() control in go(). The
    // re-exported external `expect` (reference_name='expect', the predicate
    // shape) must not appear.
    let (production, tests) = tethys.count_panic_points().expect("panic points");
    assert_eq!(
        (production, tests),
        (1, 0),
        "the control .expect() is the only panic point; the reexport of an \
         external `expect` adds none"
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
        (
            "src/inner.rs",
            "pub fn ha() {}\npub fn hb() {}\npub fn hc() {}\n",
        ),
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
