//! Integration tests for the deprecated-callers analysis (tethys-jdly).
//!
//! Each test builds its own index from a fixture workspace (never an ambient
//! DB, per the issue's acceptance criteria). Expected call-site lists were
//! hand-recorded from the fixture sources BEFORE implementation and
//! cross-checked against `cargo rustc -- --force-warn deprecated` on an
//! equivalent crate (the design's independent oracle; see
//! `.tethys-jdly/design.md` C3/C5/C7).

mod common;

use common::{open_db, workspace_with_files};
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

/// Fixture for C4/C6: root-level deprecated fns whose only callers use
/// `crate::`/`super::` paths (the shape Pass 2 declines — tethys-3i35 /
/// tethys-z9mr), a suffix-boundary decoy (`xold_bare`), a bare ambiguous
/// decoy that the resolver declines, and a zero-caller symbol.
fn build_path_b_fixture() -> (tempfile::TempDir, Vec<DeprecatedFinding>) {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod consumer;\npub mod gadget;\n\
             #[deprecated(note = \"q\")]\npub fn old_q() {}\n\
             #[deprecated]\npub fn old_clean() {}\n\
             #[deprecated]\npub fn old_bare() {}\n\
             pub fn xold_bare() {}\n\
             #[deprecated]\npub fn old_amb() {}\n\
             pub mod nested {\n\
             \x20   pub fn use_super() {\n\
             \x20       super::old_q();\n\
             \x20   }\n\
             }\n",
        ),
        (
            "src/gadget.rs",
            "pub struct Gadget;\n\
             impl Gadget {\n    pub fn old_amb(&self) -> u32 {\n        1\n    }\n}\n",
        ),
        (
            "src/consumer.rs",
            "pub fn use_q() {\n    crate::old_q();\n}\n\
             pub fn use_x() {\n    crate::xold_bare();\n}\n\
             pub fn use_amb(g: &crate::gadget::Gadget) -> u32 {\n    g.old_amb()\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");
    (dir, findings)
}

/// C4: unresolved refs whose qualified name ends `::<deprecated name>` are
/// recovered as Maybe sites (via = unresolved-qualified); bare unresolved
/// names and non-matching suffixes are NOT (zbus measurement: bare matches
/// were 36/36 noise; suffix must respect the `::` boundary).
///
/// Empirical note (this fixture): the same-file `super::old_q()` call
/// RESOLVES via Pass 2 (Definite/Resolved — correct, rustc agrees); only
/// the cross-file `crate::old_q()` is declined and needs Path B recovery.
#[test]
fn qualified_unresolved_recovered_as_maybe() {
    let (_dir, findings) = build_path_b_fixture();

    let old_q = finding(&findings, "old_q", "src/lib.rs");
    let mut sites: Vec<(String, Tier, Via)> = old_q
        .sites
        .iter()
        .map(|s| (format!("{}:{}", s.file, s.line), s.tier, s.via))
        .collect();
    sites.sort();
    assert_eq!(
        sites,
        vec![
            (
                "src/consumer.rs:2".to_string(),
                Tier::Maybe,
                Via::UnresolvedQualified,
            ),
            ("src/lib.rs:14".to_string(), Tier::Definite, Via::Resolved),
        ],
        "cross-file crate:: caller recovered as Maybe; same-file super:: caller stays resolved"
    );
}

/// C4 boundary: `crate::xold_bare` must not match deprecated `old_bare`
/// (kills suffix matching without the `::` separator), and the declined
/// bare method call `g.old_amb()` must not surface (qualified-only sweep).
#[test]
fn path_b_respects_suffix_boundary_and_excludes_bare() {
    let (_dir, findings) = build_path_b_fixture();

    let old_bare = finding(&findings, "old_bare", "src/lib.rs");
    assert!(
        old_bare.sites.is_empty(),
        "crate::xold_bare must not suffix-match old_bare; got {:?}",
        old_bare.sites
    );

    let old_amb = finding(&findings, "old_amb", "src/lib.rs");
    assert!(
        old_amb.sites.is_empty(),
        "bare unresolved g.old_amb() is excluded (qualified-only); got {:?}",
        old_amb.sites
    );
}

/// C8 fence: the zbus phantom pattern. A bare same-file call on ANOTHER
/// type's same-named method can be misattributed to the deprecated method
/// by name-only resolution (tethys-53iv). Whatever it binds to, such a
/// site must never tier Definite — Definite is reserved for names with no
/// non-deprecated candidate.
#[test]
fn same_file_phantoms_never_definite() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        // Header::path (non-deprecated) first, deprecated Message::path
        // second: same-file last-wins binding attributes `h.path()` to the
        // deprecated method — the phantom edge this fence guards.
        "pub struct Header;\n\
         impl Header {\n    pub fn path(&self) -> u32 {\n        2\n    }\n}\n\
         pub struct Message;\n\
         impl Message {\n\
         \x20   #[deprecated(note = \"use header\")]\n\
         \x20   pub fn path(&self) -> u32 {\n        1\n    }\n}\n\
         pub fn debug_fmt(h: &Header) -> u32 {\n    h.path()\n}\n",
    )]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");

    let path = finding(&findings, "path", "src/lib.rs");
    assert!(
        path.sites.iter().all(|s| s.tier == Tier::Maybe),
        "phantom-capable sites must never be Definite; got {:?}",
        path.sites
    );
    // TRIPWIRE (tethys-53iv): today the h.path() call IS misattributed to
    // the deprecated method (1 phantom site). When 53iv lands and the
    // resolver kind-gates or declines this binding, this count drops to 0 —
    // flip this assertion then; the Maybe-only fence above must keep passing.
    assert_eq!(
        path.sites.len(),
        1,
        "documents current 53iv same-file phantom binding; see TRIPWIRE note"
    );
}

/// C10: a workspace with zero deprecated symbols yields an empty findings
/// set (the CLI renders "No deprecated symbols found." and exits 0 — the
/// tethys self-index is the live example).
#[test]
fn empty_workspace_reports_nothing() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub fn modern() {}\npub fn caller() {\n    modern();\n}\n",
    )]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");
    assert!(
        findings.is_empty(),
        "no #[deprecated] attribute exists (grep oracle: zero occurrences); got {findings:?}"
    );
}

/// C11: C# `[Obsolete]` symbols yield no findings (attribute extraction is
/// tethys-haw5's scope) while Rust findings in the same mixed workspace
/// still appear — the C# file must neither surface nor abort the analysis.
#[test]
fn csharp_obsolete_out_of_scope_in_mixed_workspace() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "#[deprecated]\npub fn old_rust() {}\npub fn go() {\n    old_rust();\n}\n",
        ),
        (
            "Legacy.cs",
            "using System;\n\nnamespace App\n{\n    [Obsolete(\"use NewService\")]\n    public class LegacyService\n    {\n        public void Run() { }\n    }\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");

    assert_eq!(
        findings.len(),
        1,
        "only the Rust symbol should surface; got {:?}",
        findings
            .iter()
            .map(|f| (&f.symbol.name, &f.symbol.file))
            .collect::<Vec<_>>()
    );
    assert_eq!(findings[0].symbol.name, "old_rust");
    assert_eq!(
        findings[0].sites.len(),
        1,
        "the Rust call site still appears"
    );
}

/// Budget fence (plan S4): the unresolved-refs sweep must run off the
/// partial index `idx_refs_unresolved` — a full refs scan would break the
/// O(u) single-pass budget at production scale (refs ≈ 10^7, u ≈ 10^6).
#[test]
fn path_b_uses_partial_unresolved_index() {
    let (_dir, tethys) = build_path_b_fixture_raw();
    let conn = open_db(&tethys);
    let plan: Vec<String> = conn
        .prepare(
            "EXPLAIN QUERY PLAN
             SELECT r.reference_name FROM refs r
             WHERE r.symbol_id IS NULL AND r.reference_name LIKE '%::%'",
        )
        .expect("explain should prepare")
        .query_map([], |row| row.get::<_, String>(3))
        .expect("explain should run")
        .collect::<Result<_, _>>()
        .expect("explain rows");
    assert!(
        plan.iter().any(|d| d.contains("idx_refs_unresolved")),
        "unresolved sweep must use the partial index; plan: {plan:?}"
    );
}

/// Same fixture as [`build_path_b_fixture`] but returning the Tethys handle
/// for direct DB inspection.
fn build_path_b_fixture_raw() -> (tempfile::TempDir, tethys::Tethys) {
    let (dir, mut tethys) =
        workspace_with_files(&[("src/lib.rs", "#[deprecated]\npub fn old_q() {}\n")]);
    tethys.index().expect("index failed");
    (dir, tethys)
}

/// C9: report content is deterministic — byte-identical JSON across a full
/// re-index (row ids change, output must not), with two same-line calls
/// forcing the column tie-break to actually fire (a fixture whose primary
/// sort keys are always unique would let a missing tie-break pass).
#[test]
fn json_deterministic_across_reindex_with_same_line_tie() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod caller;\n#[deprecated]\npub fn old_twice() {}\n",
        ),
        (
            "src/caller.rs",
            "use crate::old_twice;\npub fn double() {\n    old_twice(); old_twice();\n}\n",
        ),
    ]);
    tethys.index().expect("first index failed");
    let first = serde_json::to_string_pretty(&tethys.get_deprecated_callers().expect("query 1"))
        .expect("serialize 1");

    tethys.index().expect("re-index failed");
    let second = serde_json::to_string_pretty(&tethys.get_deprecated_callers().expect("query 2"))
        .expect("serialize 2");
    assert_eq!(first, second, "re-index must not change report bytes");

    let findings = tethys.get_deprecated_callers().expect("query 3");
    let old_twice = finding(&findings, "old_twice", "src/lib.rs");
    let coords: Vec<(u32, u32)> = old_twice.sites.iter().map(|s| (s.line, s.column)).collect();
    assert_eq!(coords.len(), 2, "both same-line calls must appear");
    assert_eq!(coords[0].0, coords[1].0, "same line");
    assert!(
        coords[0].1 < coords[1].1,
        "column tie-break must order same-line sites; got {coords:?}"
    );

    // JSON stays machine-parseable and uses the documented enum spellings.
    let value: serde_json::Value = serde_json::from_str(&first).expect("parse");
    let site = &value[0]["sites"][0];
    assert_eq!(site["tier"], "Definite");
    assert_eq!(site["via"], "resolved");
}

/// C6: zero-site deprecated symbols are still reported (clean — migration
/// done), and a symbol whose ONLY caller is a `crate::`-qualified path is
/// NOT clean.
#[test]
fn clean_list_exact() {
    let (_dir, findings) = build_path_b_fixture();

    let mut clean: Vec<&str> = findings
        .iter()
        .filter(|f| f.sites.is_empty())
        .map(|f| f.symbol.name.as_str())
        .collect();
    clean.sort_unstable();
    assert_eq!(
        clean,
        vec!["old_amb", "old_bare", "old_clean"],
        "old_q has a qualified-unresolved caller and must NOT be clean"
    );
}

// ---------------------------------------------------------------------------
// CLI-level fences. Everything above exercises the library facade; these run
// the actual binary (acceptance criterion: "lists that call site ... via the
// CLI"), so a regression in `src/cli/deprecated_callers.rs` or the clap
// wiring fails CI instead of only the one-shot manual audit.
// ---------------------------------------------------------------------------

/// Two-file fixture for the CLI fences: a `#[deprecated]` fn (with since +
/// note) called once cross-file. Indexed through the library helper (still
/// "builds its own index, never an ambient DB"); the Tethys handle drops at
/// return so the subprocess owns the only connection.
fn cli_fixture() -> tempfile::TempDir {
    let (dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod legacy;\npub mod consumer;\n"),
        (
            "src/legacy.rs",
            "#[deprecated(since = \"1.0\", note = \"use replacement\")]\n\
             pub fn old_api() {}\n",
        ),
        (
            "src/consumer.rs",
            "use crate::legacy::old_api;\n\npub fn migrate() {\n    old_api();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    dir
}

/// Run the tethys binary's `deprecated-callers` against `dir`, asserting
/// exit success; returns stdout.
fn run_cli(dir: &tempfile::TempDir, extra: &[&str]) -> String {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .args(["deprecated-callers", "-w"])
        .arg(dir.path())
        .args(extra)
        .output()
        .expect("run tethys deprecated-callers");
    assert!(
        output.status.success(),
        "exited {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// AC1: a `#[deprecated]` function called from another file lists that call
/// site (caller symbol, file, line) via the CLI — table mode.
#[test]
fn cli_table_lists_cross_file_call_site() {
    let dir = cli_fixture();
    let stdout = run_cli(&dir, &[]);

    assert!(
        stdout.contains("old_api"),
        "deprecated symbol must be named:\n{stdout}"
    );
    assert!(
        stdout.contains("(since 1.0 — use replacement)"),
        "since/note must be surfaced:\n{stdout}"
    );
    assert!(
        stdout.contains("[Definite] src/consumer.rs:4 in migrate"),
        "call site must show tier, file:line, caller symbol:\n{stdout}"
    );
}

/// AC2: JSON output mode is stable and parseable — pins the CLI's
/// `{summary, deprecated}` envelope. The library-level determinism fence
/// (`json_deterministic_across_reindex_with_same_line_tie`) serializes the
/// findings vec directly, so envelope drift would otherwise pass CI.
#[test]
fn cli_json_envelope_stable_and_parseable() {
    let dir = cli_fixture();
    let first = run_cli(&dir, &["--json"]);
    let second = run_cli(&dir, &["--json"]);
    assert_eq!(first, second, "same index must render identical JSON bytes");

    let value: serde_json::Value = serde_json::from_str(&first).expect("stdout parses as JSON");
    let summary = &value["summary"];
    assert_eq!(summary["symbol_count"], 1);
    assert_eq!(summary["with_callers"], 1);
    assert_eq!(summary["clean"], 0);
    assert_eq!(summary["site_count"], 1);

    let symbol = &value["deprecated"][0]["symbol"];
    assert_eq!(symbol["name"], "old_api");
    assert_eq!(symbol["file"], "src/legacy.rs");
    assert_eq!(symbol["since"], "1.0");
    assert_eq!(symbol["note"], "use replacement");

    let site = &value["deprecated"][0]["sites"][0];
    assert_eq!(site["file"], "src/consumer.rs");
    assert_eq!(site["line"], 4);
    assert_eq!(site["caller"], "migrate");
    assert_eq!(site["tier"], "Definite");
    assert_eq!(site["via"], "resolved");
}
