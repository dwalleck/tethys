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
            // tests-mod call: now attributed to the enclosing unit-test fn
            // (was top-level/None before tethys-s8hv indexed inline mod bodies).
            (
                "src/caller.rs:14".to_string(),
                Some("exercises_old".to_string()),
            ),
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

/// C11 (flipped by tethys-haw5): a C# `[Obsolete]` class surfaces alongside
/// Rust findings in the same mixed workspace, with the Obsolete message as
/// its note and the identical JSON field set. This test was the pre-haw5
/// gap fence ("C# yields no findings"); the haw5 design's invariant sweep
/// declared the flip intended.
#[test]
fn csharp_obsolete_detected_in_mixed_workspace() {
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

    // Ordered by (file, line, name): Legacy.cs precedes src/lib.rs.
    assert_eq!(
        findings.len(),
        2,
        "both languages surface in one report; got {:?}",
        findings
            .iter()
            .map(|f| (&f.symbol.name, &f.symbol.file))
            .collect::<Vec<_>>()
    );
    let legacy = &findings[0].symbol;
    assert_eq!(legacy.name, "LegacyService");
    assert_eq!(legacy.file, "Legacy.cs");
    assert_eq!(legacy.note.as_deref(), Some("use NewService"));
    assert_eq!(legacy.since, None, "since is Rust-only");
    assert_eq!(legacy.error, None, "no bool argument in the fixture");
    assert!(
        findings[0].sites.is_empty(),
        "nothing references LegacyService in this fixture (clean verdict)"
    );
    assert_eq!(findings[1].symbol.name, "old_rust");
    assert_eq!(
        findings[1].sites.len(),
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

/// haw5 S5 shared fixture: an [Obsolete("use New")] static method with two
/// cross-file callers and one same-file caller, an [Obsolete("gone", true)]
/// class constructed once, and an uncalled bare-[Obsolete] method. The
/// `with_decoy` variant adds a same-language same-named non-obsolete method
/// (tier demotion). Static-receiver + `using` corroboration mirrors the
/// probe2 shape proven on real data (Result.Combine, 12/12).
fn build_csharp_fixture(with_decoy: bool) -> (tempfile::TempDir, Vec<DeprecatedFinding>) {
    let mut files = vec![
        (
            "Legacy.cs",
            "using System;\n\nnamespace Lib\n{\n    public class Legacy\n    {\n        \
             [Obsolete(\"use New\")]\n        public static void Old() { }\n\n        \
             [Obsolete]\n        public static void Dormant() { }\n\n        \
             public static void Inside()\n        {\n            Old();\n        }\n    }\n\n    \
             [Obsolete(\"gone\", true)]\n    public class LegacyService\n    {\n        \
             public LegacyService() { }\n    }\n}\n",
        ),
        (
            "Caller.cs",
            "using Lib;\n\nnamespace App\n{\n    public class User\n    {\n        \
             public void Go()\n        {\n            Legacy.Old();\n            \
             Legacy.Old();\n            var s = new LegacyService();\n        }\n    }\n}\n",
        ),
    ];
    if with_decoy {
        files.push((
            "Other.cs",
            "namespace App2\n{\n    public class Other\n    {\n        \
             public static void Old() { }\n    }\n}\n",
        ));
    }
    let (dir, mut tethys) = workspace_with_files(&files);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");
    (dir, findings)
}

/// haw5 S5 (design C6 + C7): resolved static-receiver sites (cross-file and
/// same-file), construction sites, the Clean bucket, and the error flag —
/// unique names tier Definite. Site lists are literals from the fixture
/// source (grep oracle mechanism). Kills: `call_edges` join (drops top-level
/// sites), r.kind='call' filtering (drops construct refs), tier
/// always-Maybe.
#[test]
fn csharp_resolved_construction_and_clean_definite() {
    let (_dir, findings) = build_csharp_fixture(false);

    let names: Vec<(&str, &str)> = findings
        .iter()
        .map(|f| (f.symbol.name.as_str(), f.symbol.file.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("Old", "Legacy.cs"),
            ("Dormant", "Legacy.cs"),
            ("LegacyService", "Legacy.cs")
        ],
        "ordered by (file, line, name)"
    );

    let old = &findings[0];
    assert_eq!(old.symbol.note.as_deref(), Some("use New"));
    let old_sites: Vec<(&str, u32, Via, Tier)> = old
        .sites
        .iter()
        .map(|s| (s.file.as_str(), s.line, s.via, s.tier))
        .collect();
    assert_eq!(
        old_sites,
        [
            ("Caller.cs", 9, Via::Resolved, Tier::Definite),
            ("Caller.cs", 10, Via::Resolved, Tier::Definite),
            ("Legacy.cs", 15, Via::Resolved, Tier::Definite),
        ],
        "two cross-file + one same-file resolved site, all Definite"
    );

    let dormant = &findings[1];
    assert!(
        dormant.sites.is_empty(),
        "uncalled obsolete method is the Clean verdict"
    );
    assert_eq!(dormant.symbol.note, None, "bare [Obsolete]");
    assert_eq!(dormant.symbol.error, None);

    let service = &findings[2];
    assert_eq!(service.symbol.note.as_deref(), Some("gone"));
    assert_eq!(
        service.symbol.error,
        Some(true),
        "[Obsolete(msg, true)] surfaces the error flag (AC3)"
    );
    let service_sites: Vec<(&str, u32, Via)> = service
        .sites
        .iter()
        .map(|s| (s.file.as_str(), s.line, s.via))
        .collect();
    assert_eq!(
        service_sites,
        [("Caller.cs", 11, Via::Resolved)],
        "construction site listed (design C7)"
    );
}

/// haw5 S5 (design C6, demotion direction): a same-language same-named
/// non-obsolete method demotes every resolved site of the obsolete one to
/// Maybe — name-only reference resolution could have misattributed.
#[test]
fn csharp_same_named_decoy_demotes_to_maybe() {
    let (_dir, findings) = build_csharp_fixture(true);
    let old = findings
        .iter()
        .find(|f| f.symbol.name == "Old")
        .expect("Old finding present");
    assert!(
        !old.sites.is_empty(),
        "sites still listed under ambiguity, only the tier changes"
    );
    for site in &old.sites {
        assert_eq!(
            site.tier,
            Tier::Maybe,
            "decoy Other.Old must demote {}:{}",
            site.file,
            site.line
        );
    }
}

/// haw5 S6 (design C8): a variable-receiver instance call (`client.Fetch()`
/// stored unresolved as `client::Fetch`) surfaces via Path B as tier=Maybe,
/// via=unresolved-qualified — and fans out to BOTH obsolete candidates
/// sharing the name, per Maybe semantics ("possibly calls this one").
/// CI form of the design-time falsifier that passed 19/19 on real data
/// (Tethys.Results, `GetValueOrDefault`). Kills: Path B requiring reference
/// resolution, single-candidate attachment, Path B gated to Rust.
#[test]
fn csharp_variable_receiver_surfaces_as_maybe_for_all_candidates() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "Client.cs",
            "using System;\n\nnamespace Lib\n{\n    public class Client\n    {\n        \
             [Obsolete(\"use FetchAsync\")]\n        public void Fetch() { }\n    }\n\n    \
             public class Backup\n    {\n        [Obsolete]\n        \
             public void Fetch() { }\n    }\n}\n",
        ),
        (
            "Use.cs",
            "using Lib;\n\nnamespace App\n{\n    public class Runner\n    {\n        \
             public void Go()\n        {\n            var client = new Client();\n            \
             client.Fetch();\n        }\n    }\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");

    let fetches: Vec<&DeprecatedFinding> = findings
        .iter()
        .filter(|f| f.symbol.name == "Fetch")
        .collect();
    assert_eq!(fetches.len(), 2, "both obsolete Fetch candidates listed");
    for finding in fetches {
        let sites: Vec<(&str, u32, Tier, Via)> = finding
            .sites
            .iter()
            .map(|s| (s.file.as_str(), s.line, s.tier, s.via))
            .collect();
        assert_eq!(
            sites,
            [("Use.cs", 10, Tier::Maybe, Via::UnresolvedQualified)],
            "the variable-receiver site attaches to candidate at {}:{}",
            finding.symbol.file,
            finding.symbol.line
        );
    }
}

/// haw5 S6 (design C10, binary-level): the CLI JSON's symbol objects carry
/// the identical key set in both languages — `since` null for C#, `error`
/// null for Rust, both present-as-null rather than absent. Site objects
/// likewise. Kills: `skip_serializing_if` on either field, per-language
/// serialization paths.
#[test]
fn cli_json_key_set_identical_across_languages() {
    const SYMBOL_KEYS: [&str; 7] = ["error", "file", "kind", "line", "name", "note", "since"];
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "#[deprecated(since = \"1.0\", note = \"use replacement\")]\n\
             pub fn old_rust() {}\npub fn go() {\n    old_rust();\n}\n",
        ),
        (
            "Legacy.cs",
            "using System;\n\nnamespace App\n{\n    [Obsolete(\"use NewService\", true)]\n    \
             public class LegacyService\n    {\n    }\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let stdout = run_cli(&dir, &["--json"]);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout parses as JSON");

    let entries = value["deprecated"].as_array().expect("deprecated array");
    assert_eq!(entries.len(), 2, "one finding per language");
    for entry in entries {
        let mut keys: Vec<&str> = entry["symbol"]
            .as_object()
            .expect("symbol object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, SYMBOL_KEYS, "identical key set (design C10 / AC4)");
    }
    let csharp = &entries[0]["symbol"];
    assert_eq!(csharp["name"], "LegacyService");
    assert_eq!(csharp["since"], serde_json::Value::Null);
    assert_eq!(csharp["note"], "use NewService");
    assert_eq!(csharp["error"], true);
    let rust = &entries[1]["symbol"];
    assert_eq!(rust["name"], "old_rust");
    assert_eq!(rust["since"], "1.0");
    assert_eq!(rust["error"], serde_json::Value::Null);

    let site = &entries[1]["sites"][0];
    let mut site_keys: Vec<&str> = site
        .as_object()
        .expect("site object")
        .keys()
        .map(String::as_str)
        .collect();
    site_keys.sort_unstable();
    assert_eq!(
        site_keys,
        ["caller", "column", "file", "line", "tier", "via"],
        "site key set stable"
    );
}

/// haw5 S7 (design C12): a C# workspace whose only attributes are
/// test-framework markers yields the empty envelope — summary zeros, empty
/// array, exit 0 (`run_cli` asserts success). Kills: detection matching
/// `[Fact]`/`[Test]`/`[TestMethod]` rows, which now exist in the index.
#[test]
fn csharp_without_obsolete_yields_empty_report() {
    let (dir, mut tethys) = workspace_with_files(&[(
        "Tests.cs",
        "using Xunit;\n\nnamespace T\n{\n    public class Suite\n    {\n        \
         [Fact]\n        public void A() { }\n\n        [Test]\n        \
         public void B() { }\n\n        [TestMethod]\n        public void C() { }\n    }\n}\n",
    )]);
    tethys.index().expect("index failed");
    let stdout = run_cli(&dir, &["--json"]);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout parses as JSON");
    assert_eq!(value["summary"]["symbol_count"], 0);
    assert_eq!(value["summary"]["with_callers"], 0);
    assert_eq!(value["summary"]["clean"], 0);
    assert_eq!(value["summary"]["site_count"], 0);
    assert_eq!(
        value["deprecated"].as_array().map(Vec::len),
        Some(0),
        "empty deprecated array, not absent"
    );
}

/// haw5 S7 (design C13): mixed-workspace summary counts sum both languages —
/// one Rust deprecated fn with one caller plus one clean C# obsolete class.
/// Kills: per-language early return, UNION dropping a language.
#[test]
fn mixed_workspace_summary_sums_both_languages() {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "#[deprecated]\npub fn old_rust() {}\npub fn go() {\n    old_rust();\n}\n",
        ),
        (
            "Legacy.cs",
            "using System;\n\nnamespace App\n{\n    [Obsolete(\"use NewService\")]\n    \
             public class LegacyService\n    {\n    }\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let stdout = run_cli(&dir, &["--json"]);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout parses as JSON");
    assert_eq!(value["summary"]["symbol_count"], 2, "one per language");
    assert_eq!(value["summary"]["with_callers"], 1, "the Rust fn");
    assert_eq!(value["summary"]["clean"], 1, "the C# class");
    assert_eq!(value["summary"]["site_count"], 1);
}

/// haw5 S4 (design C9): Path B attachment and ambiguity tiering are
/// same-language only. Four bug classes, one mixed fixture:
/// 1. cross-language tier demotion — a C# method named `old_api` must not
///    demote the Rust `old_api` finding from Definite;
/// 2. Rust→C# Path B bleed — an unresolved Rust `crate::Run` ref must not
///    attach to the C# obsolete `Run`;
/// 3. C#→Rust Path B bleed (latent jdly behavior) — an unresolved C#
///    `x::legacy_shared` ref must not attach to Rust `legacy_shared`;
/// 4. over-filtering — the same-language C# `svc::Run` ref must STILL
///    attach to the C# `Run` as Maybe / unresolved-qualified.
#[test]
fn no_cross_language_attachment() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "#[deprecated(note = \"gone\")]\npub fn old_api() {}\n\
             #[deprecated]\npub fn legacy_shared() {}\n",
        ),
        (
            "src/user.rs",
            // Bare cross-file call resolves (pass 2); crate::Run stays
            // unresolved (tethys-3i35 shape) with last segment `Run`.
            "pub fn migrate() {\n    old_api();\n}\n\
             pub fn tempted() {\n    crate::Run();\n}\n",
        ),
        (
            "App.cs",
            "using System;\n\nnamespace App\n{\n    public class Svc\n    {\n        \
             [Obsolete(\"use Walk\")]\n        public void Run() { }\n    }\n}\n",
        ),
        (
            "Caller.cs",
            "namespace App\n{\n    public class User2\n    {\n        public void Go()\n        {\n            \
             var svc = new Svc();\n            svc.Run();\n        }\n\n        \
             public void Bleed(dynamic x)\n        {\n            x.legacy_shared();\n        }\n    }\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");

    let by_name = |name: &str| {
        findings
            .iter()
            .find(|f| f.symbol.name == name)
            .unwrap_or_else(|| panic!("finding {name} missing"))
    };

    // (1) No C# symbol named old_api exists in App.cs — but Run exists in
    // both worlds via crate::Run text only on the Rust side; the direct
    // demotion probe: Rust old_api's resolved site stays Definite even
    // though C# code mentions nothing of it (control), and (3)'s bleed ref
    // must not create ambiguity either.
    let old_api = by_name("old_api");
    assert_eq!(old_api.sites.len(), 1, "one resolved Rust site");
    assert_eq!(old_api.sites[0].file, "src/user.rs");
    assert_eq!(old_api.sites[0].tier, Tier::Definite);

    // (2) + (4): the C# Run finding lists exactly the same-language
    // variable-receiver site — never the Rust crate::Run ref.
    let run = by_name("Run");
    let run_files: Vec<&str> = run.sites.iter().map(|s| s.file.as_str()).collect();
    assert_eq!(
        run_files,
        ["Caller.cs"],
        "same-language Path B site only; Rust crate::Run must not attach"
    );
    assert_eq!(run.sites[0].tier, Tier::Maybe);
    assert_eq!(run.sites[0].via, Via::UnresolvedQualified);

    // (3): Rust legacy_shared gets no site from the C# x.legacy_shared()
    // call — clean verdict.
    let legacy = by_name("legacy_shared");
    assert!(
        legacy.sites.is_empty(),
        "C# bleed ref must not attach to a Rust symbol; got {:?}",
        legacy.sites
    );
}

/// haw5 (design C5, end-to-end direction): the four `[Obsolete]` spellings
/// are detected from REAL C# source through the extractor — unlike
/// `detects_obsolete_spellings_and_decoys` (db/deprecated.rs), which inserts
/// attribute rows directly and would miss an extraction regression. The
/// qualified spellings arrive as tree-sitter `qualified_name` nodes, a
/// different node kind than plain `identifier`. Kills: extractor storing
/// only the last path segment or mangling qualified names, substring
/// matching against a parsed `[NotObsolete]` decoy.
#[test]
fn csharp_obsolete_spelling_variants_detected_from_source() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "Spellings.cs",
        "using System;\n\nnamespace Lib\n{\n    public class Spellings\n    {\n        \
         [Obsolete]\n        public static void Bare() { }\n\n        \
         [ObsoleteAttribute(\"m\", true)]\n        public static void Suffixed() { }\n\n        \
         [System.Obsolete(\"x\")]\n        public static void Qualified() { }\n\n        \
         [System.ObsoleteAttribute(error: true)]\n        public static void QualifiedSuffixed() { }\n\n        \
         [NotObsolete(\"boom\")]\n        public static void Decoy() { }\n\n        \
         [Serializable]\n        public static void Marker() { }\n    }\n}\n",
    )]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");

    let entries: Vec<(&str, Option<&str>, Option<bool>)> = findings
        .iter()
        .map(|f| {
            (
                f.symbol.name.as_str(),
                f.symbol.note.as_deref(),
                f.symbol.error,
            )
        })
        .collect();
    // Single file → ordered by line: exactly the four spellings, parsed;
    // NotObsolete and Serializable decoys never appear.
    assert_eq!(
        entries,
        [
            ("Bare", None, None),
            ("Suffixed", Some("m"), Some(true)),
            ("Qualified", Some("x"), None),
            ("QualifiedSuffixed", None, Some(true)),
        ],
        "four spellings detected from parsed source with args; decoys absent"
    );

    // Design C1 "name as written": qualified spellings are stored verbatim,
    // never collapsed to a last segment (detection above would still pass
    // for a collapsed "Obsolete", so this row-level assert is the fence).
    let conn = open_db(&tethys);
    let stored: Vec<String> = conn
        .prepare("SELECT name FROM attributes ORDER BY line")
        .expect("prepare stored-name dump")
        .query_map([], |r| r.get(0))
        .expect("query stored names")
        .collect::<Result<_, _>>()
        .expect("collect stored names");
    assert_eq!(
        stored,
        [
            "Obsolete",
            "ObsoleteAttribute",
            "System.Obsolete",
            "System.ObsoleteAttribute",
            "NotObsolete",
            "Serializable",
        ],
        "attribute names stored as written in source"
    );
}

/// haw5 (design C9, ambiguity half): tier demotion is same-language only —
/// a non-deprecated symbol sharing a deprecated symbol's name across the
/// language boundary must NOT demote resolved sites to Maybe, in either
/// direction. The Path B half of C9 is fenced by
/// `no_cross_language_attachment`; without THIS fence, dropping the
/// language column from the ambiguity CTE (name-only ambiguity) would pass
/// the whole suite. Kills exactly that revert.
#[test]
fn cross_language_same_name_does_not_demote_tier() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            // Deprecated `refresh` with a resolved same-file caller; bare
            // `Publish` exists only as the cross-language decoy for the C#
            // direction (never called).
            "#[deprecated(note = \"stale\")]\npub fn refresh() {}\n\
             pub fn go() {\n    refresh();\n}\n\
             pub fn Publish() {}\n",
        ),
        (
            "Svc.cs",
            // Obsolete static `Publish` (cross-file caller below); instance
            // `refresh` exists only as the cross-language decoy for the
            // Rust direction (never called).
            "using System;\n\nnamespace Lib\n{\n    public class Svc\n    {\n        \
             [Obsolete(\"use Post\")]\n        public static void Publish() { }\n\n        \
             public void refresh() { }\n    }\n}\n",
        ),
        (
            "Caller.cs",
            "using Lib;\n\nnamespace App\n{\n    public class User\n    {\n        \
             public void Go()\n        {\n            Svc.Publish();\n        }\n    }\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");

    let refresh = finding(&findings, "refresh", "src/lib.rs");
    assert!(!refresh.sites.is_empty(), "resolved Rust caller expected");
    assert!(
        refresh.sites.iter().all(|s| s.tier == Tier::Definite),
        "C# Svc.refresh must not demote the Rust finding; got {:?}",
        refresh.sites
    );

    let publish = finding(&findings, "Publish", "Svc.cs");
    assert!(!publish.sites.is_empty(), "resolved C# caller expected");
    assert!(
        publish.sites.iter().all(|s| s.tier == Tier::Definite),
        "Rust fn Publish must not demote the C# finding; got {:?}",
        publish.sites
    );
}

/// AC1 + AC3 at the binary level: a C# `[Obsolete("msg", true)]` method with
/// a cross-file caller lists that call site through the CLI — and human mode
/// renders the error flag as `(error — msg)`. Every other CLI-level C#
/// fixture is a zero-site clean class, so without this fence a regression in
/// the C# rendering path (or in `deprecation_meta`'s error piece, which has
/// no unit test) would pass CI. The JSON run also pins the site key set on a
/// C# entry — `cli_json_key_set_identical_across_languages` can only check
/// site keys on its Rust entry.
#[test]
fn cli_csharp_error_flag_and_call_site_rendered() {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "Legacy.cs",
            "using System;\n\nnamespace Lib\n{\n    public class Legacy\n    {\n        \
             [Obsolete(\"use New\", true)]\n        public static void Old() { }\n    }\n}\n",
        ),
        (
            "Caller.cs",
            "using Lib;\n\nnamespace App\n{\n    public class User\n    {\n        \
             public void Go()\n        {\n            Legacy.Old();\n        }\n    }\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");

    let stdout = run_cli(&dir, &[]);
    assert!(
        stdout.contains("(error — use New)"),
        "human mode must render the error flag with the message (AC3):\n{stdout}"
    );
    assert!(
        stdout.contains("[Definite] Caller.cs:9 in Go"),
        "C# call site must show tier, file:line, caller (AC1):\n{stdout}"
    );

    let json = run_cli(&dir, &["--json"]);
    let value: serde_json::Value = serde_json::from_str(&json).expect("stdout parses as JSON");
    let site = &value["deprecated"][0]["sites"][0];
    let mut site_keys: Vec<&str> = site
        .as_object()
        .expect("site object")
        .keys()
        .map(String::as_str)
        .collect();
    site_keys.sort_unstable();
    assert_eq!(
        site_keys,
        ["caller", "column", "file", "line", "tier", "via"],
        "C# site key set matches the Rust one (AC4)"
    );
}

/// Fence for the `cfg_attr` bug class: `#[cfg_attr(pred, deprecated(..))]`
/// must mark the symbol deprecated. Extraction previously stored only the
/// `cfg_attr` row, so the attribute-name-keyed deprecated-callers query
/// never saw conditionally-applied `deprecated` and silently skipped the
/// symbol. The wrapped attribute now gets its own row (no conditional
/// marker — that refinement is deferred), so the fn is listed with its
/// parsed note and resolved call site like any directly-deprecated fn.
#[test]
fn cfg_attr_deprecated_fn_is_listed_with_callers() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod legacy;\npub mod caller;\n"),
        (
            "src/legacy.rs",
            "#[cfg_attr(unix, deprecated(note = \"use new_api\"))]\n\
             pub fn cond_old() {}\n",
        ),
        (
            "src/caller.rs",
            "use crate::legacy::cond_old;\n\
             pub fn migrate() {\n    cond_old();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let findings = tethys
        .get_deprecated_callers()
        .expect("deprecated-callers query failed");

    let cond_old = finding(&findings, "cond_old", "src/legacy.rs");
    assert_eq!(
        cond_old.symbol.note.as_deref(),
        Some("use new_api"),
        "note parses from the wrapped attribute's args"
    );
    assert_eq!(
        cond_old.sites.len(),
        1,
        "the resolved cross-file call site is reported"
    );
    assert_eq!(cond_old.sites[0].caller.as_deref(), Some("migrate"));
}
