//! Integration tests for the visibility-tightening analysis (tethys-xoxq).
//!
//! Each test builds its own index from a fixture workspace with real
//! `Cargo.toml` manifests so `arch_packages` gets manifest attribution.
//! Expected candidate lists were written in `.tethys-xoxq/plan.md` BEFORE
//! implementation (fixture-source hand-read is the oracle; the real-data
//! oracle is the probe3 grep audit, re-run at the CLI slice and in the
//! final integration check).

mod common;

use common::workspace_with_files;

/// Two-crate workspace shared by the evidence-channel tests. `a-lib` is
/// deliberately hyphenated: its items are referenced as `a_lib::…`, so any
/// missing `-`→`_` normalization in later slices fails loudly.
fn two_crate_fixture() -> (tempfile::TempDir, tethys::Tethys) {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/a-lib\", \"crates/b-app\"]\n",
        ),
        (
            "crates/a-lib/Cargo.toml",
            "[package]\nname = \"a-lib\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        (
            "crates/a-lib/src/lib.rs",
            "pub fn used_fn() {}\n\
             pub fn lonely_fn() {}\n\
             pub(crate) fn tight_fn() {}\n\
             pub(in crate) fn scoped_fn() {}\n\
             pub struct Widget {\n    pub field: u32,\n}\n\
             impl Widget {\n    pub fn method(&self) -> u32 {\n        self.field\n    }\n}\n\
             fn internal() -> Widget {\n    Widget { field: 1 }\n}\n",
        ),
        (
            "crates/b-app/Cargo.toml",
            "[package]\nname = \"b-app\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        (
            "crates/b-app/src/main.rs",
            "use a_lib::used_fn;\n\nfn main() {\n    used_fn();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    (dir, tethys)
}

/// S1 (design C1): a pub symbol with a cross-package resolved ref
/// (`used_fn`, workspace-unique so Pass 2 resolves the imported call) is
/// never reported; a pub symbol with no refs (`lonely_fn`) and one with
/// only SAME-package refs (`Widget`, constructed by `internal`) are both
/// candidates — same-package use is the candidate condition, not evidence
/// against it.
#[test]
fn cross_package_ref_excludes() {
    let (_dir, tethys) = two_crate_fixture();
    let findings = tethys
        .get_visibility_candidates(false)
        .expect("visibility query failed");

    let names: Vec<(&str, &str)> = findings
        .iter()
        .map(|f| (f.name.as_str(), f.kind.as_str()))
        .collect();
    assert_eq!(
        names,
        [("lonely_fn", "function"), ("Widget", "struct")],
        "exactly the unused pub fn and the internally-used pub struct, \
         ordered by (file, line, name); got {findings:?}"
    );
    let lonely = &findings[0];
    assert_eq!(lonely.file, "crates/a-lib/src/lib.rs");
    assert_eq!(lonely.line, 2);
}

/// S1 (design C9): non-public visibilities (`pub(crate)`, `pub(in crate)`,
/// private) and member kinds (method, struct field) never appear.
#[test]
fn scope_excludes_nonpublic_and_members() {
    let (_dir, tethys) = two_crate_fixture();
    let findings = tethys
        .get_visibility_candidates(false)
        .expect("visibility query failed");

    for absent in ["tight_fn", "scoped_fn", "internal", "method", "field"] {
        assert!(
            !findings.iter().any(|f| f.name == absent),
            "{absent} must not appear (visibility or kind out of scope); got {findings:?}"
        );
    }
}
