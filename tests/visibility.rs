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
            // `pub use detail3::item;` is a NAMED re-export whose ref
            // resolves (item is workspace-unique) — C5 excludes item
            // entirely. `pub use inner2::*;` is a GLOB re-export: pv7w
            // means glob_item carries no ref at all, so C6 demotes it via
            // the same-package glob import row (whose source_module is
            // stored RELATIVE — 'inner2' — defeating exact-path matching).
            "crates/a-lib/src/lib.rs",
            "mod detail;\nmod detail2;\nmod detail3;\nmod inner2;\n\
             pub use detail::helper;\n\
             pub use detail2::mixin;\n\
             pub use detail3::item;\n\
             pub use inner2::*;\n\
             pub fn used_fn() {}\n\
             pub fn lonely_fn() {}\n\
             pub(crate) fn tight_fn() {}\n\
             pub(in crate) fn scoped_fn() {}\n\
             pub struct Widget {\n    pub field: u32,\n}\n\
             impl Widget {\n    pub fn method(&self) -> u32 {\n        self.field\n    }\n}\n\
             fn internal() -> Widget {\n    Widget { field: 1 }\n}\n\
             pub fn bare2() {}\n\
             pub fn qonly() {}\n\
             pub fn dup_fn() {}\n\
             #[cfg(unix)]\npub fn twin() {}\n\
             #[cfg(windows)]\npub fn twin() {}\n",
        ),
        ("crates/a-lib/src/detail3.rs", "pub fn item() {}\n"),
        ("crates/a-lib/src/inner2.rs", "pub fn glob_item() {}\n"),
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
             a_lib::qonly();\n    zz::xbare2();\n}\n\n\
             fn dup_fn() {}\n",
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

/// S5 (design C4): a candidate whose name is shared by ANY other indexed
/// symbol is capped at Maybe with the shared-name demotion — evidence for
/// or against it could be misattributed (tethys-53iv steals refs AND
/// destroys their qualified text, so absence-of-evidence is untrustworthy
/// for collided names). Three shapes: `dup_fn` collides with a PRIVATE fn
/// in another package (collider visibility must not matter); the cfg-twin
/// `twin` pair collides with itself (COUNT of rows, not DISTINCT
/// locations); unique `lonely_fn` stays Definite.
#[test]
fn shared_name_demotes() {
    let (_dir, tethys) = two_crate_fixture();
    let findings = tethys
        .get_visibility_candidates(false)
        .expect("visibility query failed");

    let by_name = |n: &str| -> Vec<&tethys::VisibilityFinding> {
        findings.iter().filter(|f| f.name == n).collect()
    };

    let lonely = by_name("lonely_fn");
    assert_eq!(lonely.len(), 1);
    assert_eq!(lonely[0].tier, tethys::Tier::Definite);
    assert!(lonely[0].demotions.is_empty(), "unique name stays Definite");

    let dup = by_name("dup_fn");
    assert_eq!(dup.len(), 1, "only a_lib's dup_fn is pub");
    assert_eq!(
        dup[0].demotions,
        [tethys::Demotion::SharedName],
        "b-app's PRIVATE dup_fn still demotes; got {findings:?}"
    );
    assert_eq!(dup[0].tier, tethys::Tier::Maybe);

    let twins = by_name("twin");
    assert_eq!(twins.len(), 2, "both cfg twins are candidates");
    for twin in twins {
        assert_eq!(
            twin.demotions,
            [tethys::Demotion::SharedName],
            "cfg twins collide with each other; got {findings:?}"
        );
    }
}

/// S4 (design C5): a symbol carrying a resolved reexport-kind ref (named
/// `pub use detail3::item;` — v1w8 refs) is never reported, even with zero
/// other refs: re-export is affirmative API-surface intent (AC2). Kills:
/// reexport refs treated as ordinary same-package refs, which would leave
/// the item listed as a candidate.
#[test]
fn reexported_item_excluded() {
    let (_dir, tethys) = two_crate_fixture();
    let findings = tethys
        .get_visibility_candidates(false)
        .expect("visibility query failed");

    assert!(
        !findings.iter().any(|f| f.name == "item"),
        "detail3::item is re-exported at the crate root — never a \
         candidate; got {findings:?}"
    );
}

/// S4 (design C6): a candidate whose declaring module is glob-imported in
/// its own package (`pub use inner2::*;`) is capped at Maybe with the
/// glob-reexport-risk demotion — pv7w means the glob re-export produces no
/// per-item ref, so the item would otherwise read Definite. The stored
/// `source_module` is RELATIVE (`inner2` vs `module_path` `crate::inner2`):
/// an exact-equality implementation fails this fence.
#[test]
fn glob_module_demotes() {
    let (_dir, tethys) = two_crate_fixture();
    let findings = tethys
        .get_visibility_candidates(false)
        .expect("visibility query failed");

    let glob_item = findings
        .iter()
        .find(|f| f.name == "glob_item")
        .expect("glob_item listed (pv7w: no refs exist for it)");
    assert_eq!(glob_item.tier, tethys::Tier::Maybe);
    assert_eq!(
        glob_item.demotions,
        [tethys::Demotion::GlobReexportRisk],
        "exactly the glob demotion; got {findings:?}"
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
            ("glob_item", "function"),
            ("lonely_fn", "function"),
            ("Widget", "struct"),
            ("bare2", "function"),
            ("dup_fn", "function"),
            ("twin", "function"),
            ("twin", "function"),
        ],
        "exactly the unused pub items with no cross-package evidence \
         (glob_item survives as Maybe; re-exported `item` is excluded), \
         ordered by (file, line, name); got {findings:?}"
    );
    let lonely = &findings[1];
    assert_eq!(lonely.file, "crates/a-lib/src/lib.rs");
    assert_eq!(lonely.line, 10, "declared on line 10 of the fixture lib.rs");
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
