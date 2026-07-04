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
             fn internal() -> Widget {\n    lonely_fn();\n    Widget { field: 1 }\n}\n\
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
    // workspace_closed lifts the root-reachability ceiling so this test
    // observes C4's demotion in isolation (the fixture's candidates all
    // sit at crate root and would otherwise also carry root-reachable).
    let findings = tethys
        .get_visibility_candidates(true)
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

/// S9 (design C11): byte-identical JSON across a full re-index (row ids
/// change; output must not), with two same-line symbols forcing the name
/// tie-break to actually fire and one finding carrying TWO demotions
/// (shared-name + root-reachable) so demotion-vec ordering is exercised.
#[test]
fn json_deterministic_across_reindex_with_tie_break() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "mod other;\npub struct Aa; pub struct Bb;\npub fn multi() {}\n",
        ),
        ("src/other.rs", "fn multi() {}\n"),
    ]);
    tethys.index().expect("first index failed");
    let first =
        serde_json::to_string_pretty(&tethys.get_visibility_candidates(false).expect("query 1"))
            .expect("serialize 1");

    tethys.index().expect("re-index failed");
    let second =
        serde_json::to_string_pretty(&tethys.get_visibility_candidates(false).expect("query 2"))
            .expect("serialize 2");
    assert_eq!(first, second, "re-index must not change report bytes");

    let findings = tethys.get_visibility_candidates(false).expect("query 3");
    let same_line: Vec<(&str, u32)> = findings
        .iter()
        .filter(|f| f.kind == "struct")
        .map(|f| (f.name.as_str(), f.line))
        .collect();
    assert_eq!(
        same_line,
        [("Aa", 2), ("Bb", 2)],
        "same-line structs ordered by the name tie-break"
    );
    let multi = findings
        .iter()
        .find(|f| f.name == "multi")
        .expect("multi present");
    assert_eq!(
        multi.demotions,
        [
            tethys::Demotion::SharedName,
            tethys::Demotion::RootReachable
        ],
        "two demotions in canonical (enum) order"
    );
}

/// Single-package fixture for the ceiling and CLI fences: `api::exposed`
/// behind an all-pub chain, `internal::buried` under a private mod.
fn single_package_fixture() -> (tempfile::TempDir, tethys::Tethys) {
    let (dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod api;\nmod internal;\n"),
        ("src/api.rs", "pub fn exposed() {}\n"),
        ("src/internal.rs", "pub fn buried() {}\n"),
    ]);
    tethys.index().expect("index failed");
    (dir, tethys)
}

/// Run the tethys binary's `visibility-tightening` against `dir`,
/// asserting exit success; returns stdout.
fn run_cli(dir: &tempfile::TempDir, extra: &[&str]) -> String {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .args(["visibility-tightening", "-w"])
        .arg(dir.path())
        .args(extra)
        .output()
        .expect("run tethys visibility-tightening");
    assert!(
        output.status.success(),
        "exited {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// S8 (design C12): tier is visible in BOTH output modes, the JSON key set
/// is fixed with `demotions` present-not-absent even when empty (the haw5
/// C10 key-set lesson), and `--workspace-closed` reaches the analysis.
#[test]
fn cli_tier_visible_both_modes() {
    let (dir, _tethys) = single_package_fixture();

    let table = run_cli(&dir, &[]);
    assert!(
        table.contains("[Definite]") && table.contains("buried"),
        "table shows the Definite finding:\n{table}"
    );
    assert!(
        table.contains("[Maybe]") && table.contains("exposed"),
        "table shows the Maybe finding:\n{table}"
    );
    assert!(
        table.contains("root-reachable"),
        "table shows the demotion reason:\n{table}"
    );

    let json = run_cli(&dir, &["--json"]);
    let value: serde_json::Value = serde_json::from_str(&json).expect("stdout parses as JSON");
    let findings = value["findings"].as_array().expect("findings array");
    assert_eq!(findings.len(), 2);
    for finding in findings {
        let mut keys: Vec<&str> = finding
            .as_object()
            .expect("finding object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["demotions", "file", "kind", "line", "name", "tier"],
            "fixed key set; demotions present even when empty"
        );
    }
    let exposed = findings
        .iter()
        .find(|f| f["name"] == "exposed")
        .expect("exposed present");
    assert_eq!(exposed["tier"], "Maybe");
    assert_eq!(exposed["demotions"][0], "root-reachable");

    let closed = run_cli(&dir, &["--json", "--workspace-closed"]);
    let value: serde_json::Value = serde_json::from_str(&closed).expect("parses");
    let exposed = value["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|f| f["name"] == "exposed")
        .expect("exposed present")
        .clone();
    assert_eq!(exposed["tier"], "Definite", "flag lifts the ceiling");
}

/// S8 (design C10): a workspace with zero candidates renders the empty
/// envelope — summary zeros, `findings: []` (present, not absent), exit 0.
#[test]
fn cli_empty_envelope() {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/g-lib\", \"crates/u-app\"]\n",
        ),
        (
            "crates/g-lib/Cargo.toml",
            "[package]\nname = \"g-lib\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        ("crates/g-lib/src/lib.rs", "pub fn alpha() {}\n"),
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
    drop(tethys);

    let json = run_cli(&dir, &["--json"]);
    let value: serde_json::Value = serde_json::from_str(&json).expect("stdout parses as JSON");
    assert_eq!(value["summary"]["candidate_count"], 0);
    assert_eq!(value["summary"]["definite"], 0);
    assert_eq!(value["summary"]["maybe"], 0);
    assert_eq!(
        value["findings"].as_array().map(Vec::len),
        Some(0),
        "empty findings array, not absent"
    );
}

/// S7 (design C7 + C13): the root-reachability ceiling, on a
/// SINGLE-package workspace (which also proves the evidence sweep doesn't
/// vacuously exclude everything when no second package exists — C13's
/// buggy-impl). `api::exposed` sits behind an all-pub chain: by default it
/// caps at Maybe/root-reachable because a consumer outside the indexed
/// workspace could name it; `internal::buried` (pub fn under a private
/// mod) is not externally nameable, so it may be Definite even by
/// default. `workspace_closed = true` asserts nothing external exists,
/// lifting the ceiling: `exposed` becomes Definite with no demotion left.
#[test]
fn root_reachable_ceiling() {
    let (_dir, tethys) = single_package_fixture();

    let default_run = tethys
        .get_visibility_candidates(false)
        .expect("visibility query failed");
    let by_name = |findings: &[tethys::VisibilityFinding], n: &str| -> tethys::VisibilityFinding {
        findings
            .iter()
            .find(|f| f.name == n)
            .unwrap_or_else(|| panic!("{n} missing from {findings:?}"))
            .clone()
    };

    let exposed = by_name(&default_run, "exposed");
    assert_eq!(exposed.tier, tethys::Tier::Maybe);
    assert_eq!(
        exposed.demotions,
        [tethys::Demotion::RootReachable],
        "externally nameable ⇒ Maybe by default; got {default_run:?}"
    );
    let buried = by_name(&default_run, "buried");
    assert_eq!(
        (buried.tier, buried.demotions.as_slice()),
        (tethys::Tier::Definite, &[][..]),
        "pub under a private mod is not externally nameable; got {default_run:?}"
    );
    let definite: Vec<&str> = default_run
        .iter()
        .filter(|f| f.tier == tethys::Tier::Definite)
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(
        definite,
        ["buried"],
        "C13: single-package default run yields no Definite except \
         non-root-reachable items"
    );

    let closed_run = tethys
        .get_visibility_candidates(true)
        .expect("visibility query failed");
    let exposed = by_name(&closed_run, "exposed");
    assert_eq!(
        (exposed.tier, exposed.demotions.as_slice()),
        (tethys::Tier::Definite, &[][..]),
        "workspace_closed lifts the ceiling entirely; got {closed_run:?}"
    );
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

/// S2 (design C2, glob widening): a cross-package glob import makes every
/// pub item OF THE GLOBBED MODULE nameable in the importing crate — and
/// only that module. `use g_lib::*;` covers root items `alpha`/`beta`;
/// `use g_lib::sub::*;` covers `sub::subbed`; `sub2::untouched` is covered
/// by NEITHER and must survive as a candidate. (The q-cli oracle run
/// caught a head-keyed implementation that suppressed the whole crate on
/// one submodule glob — `fig_auth` went from 5 true candidates to 0.)
#[test]
fn cross_package_glob_import_excludes_globbed_module_only() {
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
            "pub mod sub;\npub mod sub2;\npub fn alpha() {}\npub fn beta() {}\n",
        ),
        ("crates/g-lib/src/sub.rs", "pub fn subbed() {}\n"),
        ("crates/g-lib/src/sub2.rs", "pub fn untouched() {}\n"),
        (
            "crates/u-app/Cargo.toml",
            "[package]\nname = \"u-app\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        (
            "crates/u-app/src/main.rs",
            "use g_lib::*;\nuse g_lib::sub::*;\n\nfn main() {\n    alpha();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_visibility_candidates(false)
        .expect("visibility query failed");

    for absent in ["alpha", "beta", "subbed"] {
        assert!(
            !findings.iter().any(|f| f.name == absent),
            "g_lib::{absent} is covered by a u-app glob import; \
             got {findings:?}"
        );
    }
    assert!(
        findings.iter().any(|f| f.name == "untouched"),
        "sub2::untouched is covered by NO glob — a head-keyed \
         implementation over-suppresses it; got {findings:?}"
    );
}

/// S1 (design C1): a pub symbol with a cross-package resolved ref
/// (`used_fn`, workspace-unique so Pass 2 resolves the imported call) is
/// never reported; a pub fn used ONLY within its own package (`lonely_fn`,
/// called by `internal` — AC1's flagged shape) and a struct with only
/// same-package refs (`Widget`, constructed by `internal`) are both
/// candidates — same-package use is the candidate condition, not evidence
/// against it. The no-refs shape stays covered by `glob_item`/`dup_fn`/
/// `twin`.
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

/// Regression fence for tethys-s8hv / INV-1. Once inline-module bodies became
/// indexed, a `#[cfg(test)] mod tests` unit test produces reference edges into
/// the code it exercises. A `pub` item used ONLY by such a same-crate unit test
/// must STILL be a Definite tightening candidate: visibility-tightening's
/// keep-public evidence is cross-PACKAGE usage, and a same-crate unit test is
/// same-package. The design predicted this would regress; a red-first experiment
/// disproved it. This fence locks the behavior so a future change that starts
/// counting same-crate test refs as keep-public evidence fails loudly.
#[test]
fn same_crate_unit_test_usage_does_not_suppress_tightening_candidate() {
    let (dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod api;\nmod internal;\n"),
        ("src/api.rs", "pub fn exposed() {}\n"),
        (
            "src/internal.rs",
            "pub fn buried() {}\n\
             \n\
             #[cfg(test)]\n\
             mod tests {\n\
             use super::*;\n\
             #[test]\n\
             fn exercises_buried() {\n\
             buried();\n\
             }\n\
             }\n",
        ),
    ]);
    tethys.index().expect("index failed");

    // Parse JSON and pin `buried`'s tier specifically (not just "some Definite
    // exists and 'buried' appears somewhere") so the fence stays precise if the
    // fixture ever grows another candidate.
    let json = run_cli(&dir, &["--json"]);
    let value: serde_json::Value = serde_json::from_str(&json).expect("stdout parses as JSON");
    let findings = value["findings"].as_array().expect("findings array");
    let buried = findings
        .iter()
        .find(|f| f["name"] == "buried")
        .unwrap_or_else(|| panic!("buried must be a tightening candidate; findings: {findings:?}"));
    assert_eq!(
        buried["tier"], "Definite",
        "a pub fn used only by a same-crate #[cfg(test)] unit test must remain a \
         Definite tightening candidate (same-crate test usage is not keep-public evidence)"
    );
}
