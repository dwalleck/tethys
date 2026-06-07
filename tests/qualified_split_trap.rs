//! Regression fence for the qualified-split abandonment contract
//! (separator-fix claim C6).
//!
//! Within one prefix-split of a qualified reference, the first candidate
//! file present in the index claims the split; if the tail symbol is then
//! missing in that file, the split is ABANDONED — the remaining candidates
//! are not tried. This preserves the pre-seam interleaving of candidate
//! generation and index lookup (`resolve.rs` pre-extraction interleaved
//! `get_file_id` between the implicit-crate and as-written interpretations).
//!
//! The fixture embeds the bug class directly: `helper::do_thing()` is
//! called from `app/src/lib.rs`, where the implicit-crate interpretation
//! resolves to `app/src/helper.rs` (indexed, but WITHOUT `do_thing`) and
//! the as-written interpretation resolves to workspace crate `helper`
//! (whose `lib.rs` DOES define `do_thing`). The contract requires the ref
//! to stay UNRESOLVED. A flat-candidate-list driver — one that falls
//! through to the as-written interpretation after the tail miss — resolves
//! it and fails this test. Baseline verified against the pre-seam binary
//! on 2026-06-06 (`.separator-fix/probe-findings.md`).

use rusqlite::params;

mod common;

use common::{open_db, workspace_with_files};

#[test]
fn tail_miss_abandons_split_without_trying_as_written_candidate() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "Cargo.toml",
            r#"
[workspace]
members = ["app", "helper"]
resolver = "2"
"#,
        ),
        (
            "app/Cargo.toml",
            r#"
[package]
name = "app"
version = "0.1.0"
edition = "2021"
"#,
        ),
        (
            "helper/Cargo.toml",
            r#"
[package]
name = "helper"
version = "0.1.0"
edition = "2021"
"#,
        ),
        // Interpretation A target: exists and is indexed, but has NO do_thing.
        ("app/src/helper.rs", "pub fn unrelated() {}\n"),
        (
            "app/src/lib.rs",
            r"
mod helper;
pub fn use_it() {
    helper::do_thing();
}
",
        ),
        // Interpretation B target: workspace crate `helper` HAS do_thing.
        ("helper/src/lib.rs", "pub fn do_thing() {}\n"),
    ]);

    tethys.index().expect("index should succeed");

    let conn = open_db(&tethys);

    // Sanity: both interpretation targets are indexed, and do_thing exists in
    // the helper crate — without these, the test could pass vacuously.
    let do_thing_in_helper_crate: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM symbols s JOIN files f ON f.id = s.file_id
             WHERE s.name = 'do_thing' AND f.path = 'helper/src/lib.rs'",
            params![],
            |row| row.get(0),
        )
        .expect("count do_thing");
    assert_eq!(
        do_thing_in_helper_crate, 1,
        "fixture must define do_thing in the helper crate"
    );
    let trap_file_indexed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path = 'app/src/helper.rs'",
            params![],
            |row| row.get(0),
        )
        .expect("count trap file");
    assert_eq!(trap_file_indexed, 1, "fixture's trap file must be indexed");

    // The contract: helper::do_thing stays UNRESOLVED. The implicit-crate
    // candidate (app/src/helper.rs) claims the split, the tail misses, the
    // split is abandoned — the as-written candidate (helper/src/lib.rs) must
    // NOT be consulted.
    let trap_ref_state: (i64, Option<i64>) = conn
        .query_row(
            "SELECT COUNT(*), MAX(r.symbol_id)
             FROM refs r JOIN files f ON f.id = r.file_id
             WHERE f.path = 'app/src/lib.rs' AND r.reference_name = 'helper::do_thing'",
            params![],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("query trap ref");
    assert_eq!(trap_ref_state.0, 1, "exactly one helper::do_thing ref");
    assert_eq!(
        trap_ref_state.1, None,
        "helper::do_thing must remain unresolved: resolving it means the \
         driver fell through to the as-written candidate after a tail miss \
         (claim C6 violated)"
    );
}
