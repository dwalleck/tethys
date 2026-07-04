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
            // `helper`/`mixin` reproduce the probe's `is_amzn_user` shape
            // exactly: cfg-twin definitions (defeat the unique-name
            // fallback) behind a root re-export whose relative path stays
            // unresolved (tethys-z9mr), which also defeats the
            // explicit-import arm's binding at lib.rs. Net effect: the
            // cross-package use produces NO resolved ref — only the
            // importing crate's `imports` row can exclude them.
            "crates/a-lib/src/lib.rs",
            "mod detail;\nmod detail2;\n\
             pub use detail::helper;\n\
             pub use detail2::mixin;\n\
             pub fn used_fn() {}\n\
             pub fn lonely_fn() {}\n\
             pub(crate) fn tight_fn() {}\n\
             pub(in crate) fn scoped_fn() {}\n\
             pub struct Widget {\n    pub field: u32,\n}\n\
             impl Widget {\n    pub fn method(&self) -> u32 {\n        self.field\n    }\n}\n\
             fn internal() -> Widget {\n    Widget { field: 1 }\n}\n\
             pub fn bare2() {}\n\
             pub fn qonly() {}\n",
        ),
        (
            "crates/a-lib/src/detail.rs",
            "#[cfg(unix)]\npub fn helper() {}\n#[cfg(windows)]\npub fn helper() {}\n",
        ),
        (
            "crates/a-lib/src/detail2.rs",
            "#[cfg(unix)]\npub fn mixin() {}\n#[cfg(windows)]\npub fn mixin() {}\n",
        ),
        (
            "crates/b-app/Cargo.toml",
            "[package]\nname = \"b-app\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        (
            // `a_lib::qonly()` exercises channel (c): a ROOT-level
            // qualified call with no import stays unresolved (the
            // tethys-3i35 decline class — deeper paths like a_lib::m::f()
            // resolve via qualified_module_fallback and land in channel
            // (a) instead), so only its qualified text can exclude qonly.
            // The `zz::xbare2()` decoy shares a suffix with candidate
            // `bare2` — a match without the `::` boundary would wrongly
            // suppress it.
            "crates/b-app/src/main.rs",
            "use a_lib::helper;\n\
             use a_lib::mixin as mx;\n\
             use a_lib::used_fn;\n\n\
             fn main() {\n    used_fn();\n    helper();\n    mx();\n    \
             a_lib::qonly();\n    zz::xbare2();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    (dir, tethys)
}

/// S2 (design C2): a pub symbol whose only cross-package use is an
/// `imports` row is never reported — the imported bare call stays
/// unresolved (collision defeats the unique-name fallback; the probe
/// measured this exact shape as a naive-rule false candidate), so channel
/// (a) is silent and the imports row alone must exclude. Covers the
/// aliased form (`use a_lib::mixin as mx` — the row's `symbol_name`, not
/// its alias, names the item) and the hyphenated-package form (`a-lib`
/// manifest name vs `a_lib::` path — kills missing `-`→`_` normalization).
#[test]
fn cross_package_import_excludes() {
    let (_dir, tethys) = two_crate_fixture();
    let findings = tethys
        .get_visibility_candidates(false)
        .expect("visibility query failed");

    for absent in ["helper", "mixin"] {
        assert!(
            !findings.iter().any(|f| f.name == absent),
            "a_lib::{absent} is imported by b-app — the imports row must \
             exclude it; got {findings:?}"
        );
    }
}

/// S3 (design C3): a pub symbol whose only cross-crate use is a ROOT-level
/// fully qualified call with no import (`a_lib::qonly()`) stays unresolved
/// (the tethys-3i35 decline class, the probe's `fig_auth::refresh_token`
/// shape), and its qualified `reference_name` text is the only evidence —
/// the last `::` segment from another package's ref must exclude it. The
/// `zz::xbare2()` decoy fences the `::` boundary: `bare2` must SURVIVE as
/// a candidate even though `xbare2` ends with its name.
#[test]
fn unresolved_qualified_excludes() {
    let (_dir, tethys) = two_crate_fixture();
    let findings = tethys
        .get_visibility_candidates(false)
        .expect("visibility query failed");

    assert!(
        !findings.iter().any(|f| f.name == "qonly"),
        "a_lib::qonly is called (unresolved-qualified) from b-app — the \
         qualified text must exclude it; got {findings:?}"
    );
    assert!(
        findings.iter().any(|f| f.name == "bare2"),
        "bare2 must survive the zz::xbare2 suffix decoy (:: boundary); \
         got {findings:?}"
    );
}

/// S2 (design C2, glob widening): a cross-package `use g_lib::*;` makes
/// every pub root item of `g_lib` nameable in the importing crate, so it
/// is use evidence for ALL of that crate's candidates — `beta` has no ref
/// anywhere, only the crate-glob row, and must still be excluded.
/// (Suppression-safe widening beyond the design's per-name claim; noted
/// in the slice commit.)
#[test]
fn cross_package_glob_import_excludes_all() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/g-lib\", \"crates/u-app\"]\n",
        ),
        (
            "crates/g-lib/Cargo.toml",
            "[package]\nname = \"g-lib\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        (
            "crates/g-lib/src/lib.rs",
            "pub fn alpha() {}\npub fn beta() {}\n",
        ),
        (
            "crates/u-app/Cargo.toml",
            "[package]\nname = \"u-app\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        (
            "crates/u-app/src/main.rs",
            "use g_lib::*;\n\nfn main() {\n    alpha();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_visibility_candidates(false)
        .expect("visibility query failed");

    for absent in ["alpha", "beta"] {
        assert!(
            !findings.iter().any(|f| f.name == absent),
            "g_lib::{absent} is covered by u-app's crate glob import; \
             got {findings:?}"
        );
    }
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
        [
            ("lonely_fn", "function"),
            ("Widget", "struct"),
            ("bare2", "function"),
        ],
        "exactly the unused pub items with no cross-package evidence, \
         ordered by (file, line, name); got {findings:?}"
    );
    let lonely = &findings[0];
    assert_eq!(lonely.file, "crates/a-lib/src/lib.rs");
    assert_eq!(lonely.line, 6, "declared on line 6 of the fixture lib.rs");
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
