//! Integration tests for the deprecated-callers analysis (tethys-jdly).
//!
//! Each test builds its own index from a fixture workspace (never an ambient
//! DB, per the issue's acceptance criteria). Expected call-site lists were
//! hand-recorded from the fixture sources BEFORE implementation and
//! cross-checked against `cargo rustc -- --force-warn deprecated` on an
//! equivalent crate (the design's independent oracle; see
//! `.tethys-jdly/design.md` C3/C5/C7).

mod common;

use common::workspace_with_files;
use tethys::{DeprecatedFinding, Tier, Via};

/// Fixture shared by the C3/C5/C7 tests: unique-name deprecated fn with an
/// imported cross-file caller and a tests-mod caller; ambiguous-name
/// deprecated fn (same-named non-deprecated method elsewhere) with a
/// same-file caller; a deprecated same-named pair (all candidates
/// deprecated); and the non-deprecated decoy method with its own caller.
fn build_fixture() -> (tempfile::TempDir, Vec<DeprecatedFinding>) {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod util;\npub mod other;\npub mod widget;\npub mod dup_a;\npub mod dup_b;\npub mod caller;\n",
        ),
        (
            "src/util.rs",
            "#[deprecated(since = \"1.0\", note = \"use fresh\")]\n\
             pub fn old_api() {}\n\
             pub fn fresh() {}\n",
        ),
        (
            "src/other.rs",
            "#[deprecated]\n\
             pub fn old_eq() {}\n\
             pub fn same_file_caller() {\n    old_eq();\n}\n",
        ),
        (
            "src/widget.rs",
            "pub struct Widget;\n\
             impl Widget {\n    pub fn old_eq(&self) -> bool {\n        true\n    }\n}\n\
             pub fn decoy_caller(w: &Widget) -> bool {\n    w.old_eq()\n}\n",
        ),
        ("src/dup_a.rs", "#[deprecated]\npub fn legacy_shared() {}\n"),
        ("src/dup_b.rs", "#[deprecated]\npub fn legacy_shared() {}\n"),
        (
            "src/caller.rs",
            "use crate::util::old_api;\n\
             use crate::dup_a::legacy_shared;\n\
             pub fn migrate() {\n    old_api();\n}\n\
             pub fn migrate_dup() {\n    legacy_shared();\n}\n\
             #[cfg(test)]\n\
             mod tests {\n\
             \x20   #[test]\n\
             \x20   fn exercises_old() {\n\
             \x20       use crate::util::old_api;\n\
             \x20       old_api();\n\
             \x20   }\n\
             }\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");
    (dir, findings)
}

fn finding<'a>(findings: &'a [DeprecatedFinding], name: &str, file: &str) -> &'a DeprecatedFinding {
    findings
        .iter()
        .find(|f| f.symbol.name == name && f.symbol.file == file)
        .unwrap_or_else(|| panic!("expected a deprecated entry for {name} in {file}"))
}

/// C3: every resolved ref to a deprecated symbol yields a site row with
/// (file, line, caller-or-None) — including refs with `in_symbol_id NULL`
/// (the verified `#[cfg(test)] mod tests` shape).
///
/// rustc oracle for this fixture (recorded from `cargo rustc -- --force-warn
/// deprecated --test` on the equivalent crate): call-site warnings at
/// `caller.rs:4` (`migrate` → `old_api`), `caller.rs:7` (`migrate_dup` →
/// `legacy_shared`), `caller.rs:14` (tests → `old_api`), `other.rs:4`
/// (`same_file_caller` → `old_eq`). No warning in `widget.rs`. rustc also
/// warns on the `use` statements importing deprecated items (`caller.rs:1`,
/// `:2`, `:13`); those are excluded by definition — import lines vanish with
/// their call sites during migration, and a call-less deprecated import is
/// already flagged by unused-imports.
#[test]
fn resolved_sites_cross_file_and_top_level() {
    let (_dir, findings) = build_fixture();

    let old_api = finding(&findings, "old_api", "src/util.rs");
    let mut sites: Vec<(String, Option<String>)> = old_api
        .sites
        .iter()
        .map(|s| (format!("{}:{}", s.file, s.line), s.caller.clone()))
        .collect();
    sites.sort();
    assert_eq!(
        sites,
        vec![
            ("src/caller.rs:14".to_string(), None), // tests-mod call: top-level ref
            ("src/caller.rs:4".to_string(), Some("migrate".to_string())),
        ],
        "old_api should list exactly the imported cross-file call and the tests-mod call"
    );
    assert!(
        old_api.sites.iter().all(|s| s.via == Via::Resolved),
        "both old_api sites come from resolved refs"
    );

    let dup = finding(&findings, "legacy_shared", "src/dup_a.rs");
    assert_eq!(
        dup.sites.len(),
        1,
        "imported legacy_shared call resolves to dup_a's symbol"
    );
    assert_eq!(dup.sites[0].caller.as_deref(), Some("migrate_dup"));
}

/// C5: a site is Definite iff every same-named indexed symbol is deprecated;
/// ambiguous names (non-deprecated `Widget::old_eq` exists) tier Maybe.
#[test]
fn tier_definite_iff_all_same_named_deprecated() {
    let (_dir, findings) = build_fixture();

    let old_api = finding(&findings, "old_api", "src/util.rs");
    assert!(
        old_api.sites.iter().all(|s| s.tier == Tier::Definite),
        "unique-name old_api sites must be Definite, got {:?}",
        old_api.sites
    );

    let old_eq = finding(&findings, "old_eq", "src/other.rs");
    assert_eq!(old_eq.sites.len(), 1, "same-file call site expected");
    assert_eq!(
        old_eq.sites[0].tier,
        Tier::Maybe,
        "old_eq shares its name with non-deprecated Widget::old_eq → Maybe"
    );

    // Both legacy_shared symbols are deprecated: no non-deprecated candidate
    // exists, so whichever one a call binds to, the finding is Definite.
    let dup = finding(&findings, "legacy_shared", "src/dup_a.rs");
    assert!(
        dup.sites.iter().all(|s| s.tier == Tier::Definite),
        "all-deprecated same-name pair must tier Definite"
    );
}

/// C7: symbols without the attribute never appear as deprecated entries —
/// the same-named decoy method (which has its own caller) must be absent.
#[test]
fn decoy_never_appears_as_deprecated_entry() {
    let (_dir, findings) = build_fixture();

    assert!(
        !findings.iter().any(|f| f.symbol.file == "src/widget.rs"),
        "nothing in widget.rs is deprecated; entries: {:?}",
        findings
            .iter()
            .map(|f| (&f.symbol.name, &f.symbol.file))
            .collect::<Vec<_>>()
    );
}

/// C1 fence: detection is kind-agnostic — struct and enum-variant
/// deprecations are listed alongside functions, with parsed since/note.
#[test]
fn detects_all_kinds() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "#[deprecated]\npub fn old_bare() {}\n\
         #[deprecated = \"use the new one\"]\npub fn old_eq_form() {}\n\
         #[deprecated(note = \"gone in 2.0\")]\npub struct OldStruct;\n\
         pub enum Mode {\n\
         \x20   Fast,\n\
         \x20   #[deprecated(since = \"1.1.0\", note = \"use Fast\")]\n\
         \x20   Turbo,\n\
         }\n",
    )]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");

    let mut entries: Vec<(String, String, Option<String>, Option<String>)> = findings
        .iter()
        .map(|f| {
            (
                f.symbol.name.clone(),
                f.symbol.kind.clone(),
                f.symbol.since.clone(),
                f.symbol.note.clone(),
            )
        })
        .collect();
    entries.sort();
    assert_eq!(
        entries,
        vec![
            (
                "OldStruct".to_string(),
                "struct".to_string(),
                None,
                Some("gone in 2.0".to_string()),
            ),
            (
                "Turbo".to_string(),
                "enum_variant".to_string(),
                Some("1.1.0".to_string()),
                Some("use Fast".to_string()),
            ),
            ("old_bare".to_string(), "function".to_string(), None, None),
            (
                "old_eq_form".to_string(),
                "function".to_string(),
                None,
                Some("use the new one".to_string()),
            ),
        ],
        "all symbol kinds detected with parsed since/note"
    );
}
