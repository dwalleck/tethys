//! Integration regression fences for the dead-code analysis
//! (tethys-dvsw). Each test builds its OWN real index — the on-disk
//! index can be stale, so never query an ambient DB.
//!
//! Unit fences for the funnel channels (seeded rows) live in
//! `src/db/dead_code.rs` and the scan fences in `src/dead_code.rs`;
//! these cover what only a full index exercises: real extractor and
//! resolver behavior over fixture workspaces, the C8 seeded-dead
//! ground truth (every seeded item is one rustc's `dead_code` lint
//! would flag — verified by hand at authoring), entry-point roles,
//! the CLI envelope, and determinism.

mod common;

use common::workspace_with_files;
use tethys::Tier;

fn finding_names(report: &tethys::DeadCodeReport) -> Vec<(String, String)> {
    report
        .findings
        .iter()
        .map(|f| (f.name.clone(), format!("{:?}", f.tier)))
        .collect()
}

/// Design C1 end-to-end: candidacy filters through a real index. A
/// public dead fn (visibility), a `#[test]` fn (`is_test`), a struct
/// field (kind), a module declaration (kind), and the bin-root `main`
/// (entry point) are never reported; the private dead fn control IS.
/// Kills: any missing candidacy predicate surviving real extraction.
#[test]
fn candidacy_filters() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/main.rs",
            "mod helpers;\n\
             struct Holder {\n    field_qx: i32,\n}\n\
             pub fn pub_dead_qx() -> i32 {\n    1\n}\n\
             fn main() {\n    let h = Holder { field_qx: 1 };\n    let _ = h;\n}\n\
             #[cfg(test)]\nmod tests {\n\
             \x20   #[test]\n\
             \x20   fn probe_qx() {\n\
             \x20       assert!(true);\n\
             \x20   }\n\
             }\n",
        ),
        ("src/helpers.rs", "fn lonely_qx() -> i32 {\n    2\n}\n"),
    ]);
    tethys.index().expect("index");
    let report = tethys.find_dead_code(None).expect("report");

    let names = finding_names(&report);
    assert!(
        names.iter().any(|(n, _)| n == "lonely_qx"),
        "private dead fn control must be reported: {names:?}"
    );
    for excluded in ["pub_dead_qx", "probe_qx", "field_qx", "helpers", "main"] {
        assert!(
            !names.iter().any(|(n, _)| n == excluded),
            "{excluded} must not be reported: {names:?}"
        );
    }
}

/// Design C8: seeded dead items a `rustc dead_code` run would flag —
/// unmentioned fn, struct, const, and a RECURSIVE fn (self-refs are not
/// evidence; its own-span mention does not demote) — all Definite. The
/// decoy, mentioned only inside another file's macro token tree, tiers
/// Maybe. Kills: a broken zero-ref query (report-nothing passes the
/// self-index fence but fails here — the two-sided oracle pair), a
/// missing self-ref exclusion, line-only span exclusion.
#[test]
fn seeded_dead_items_definite() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod live;\n\
             fn dead_fn_qy() -> u8 {\n    1\n}\n\
             struct DeadStructQy;\n\
             const DEAD_CONST_QY: u8 = 7;\n\
             fn rec_qy(n: u8) -> u8 {\n    if n == 0 { 0 } else { rec_qy(n - 1) }\n}\n\
             fn decoy_qy() -> u8 {\n    3\n}\n",
        ),
        (
            "src/live.rs",
            "pub fn used_everywhere() -> &'static str {\n    stringify!(decoy_qy)\n}\n",
        ),
    ]);
    tethys.index().expect("index");
    let report = tethys.find_dead_code(None).expect("report");

    let names = finding_names(&report);
    for definite in ["dead_fn_qy", "DeadStructQy", "DEAD_CONST_QY", "rec_qy"] {
        assert!(
            names.iter().any(|(n, t)| n == definite && t == "Definite"),
            "{definite} must be Definite: {names:?}"
        );
    }
    assert!(
        names.iter().any(|(n, t)| n == "decoy_qy" && t == "Maybe"),
        "macro-mentioned decoy must be Maybe, not Definite: {names:?}"
    );
    assert_eq!(report.summary.definite, 4, "exactly the seeded four");
}

/// Design C9: a bin-only crate's unmentioned `main` is never reported —
/// entry points are excluded structurally, not by textual luck (the
/// probe measured `main` surviving only via 203 unrelated hits on the
/// self-index). Kills: dropping the entry-point rule and relying on
/// the textual channel.
#[test]
fn entry_points_excluded() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/main.rs",
        "fn main() {\n    helper_qz();\n}\n\
         fn helper_qz() {}\n\
         fn lonely_qz() {}\n",
    )]);
    tethys.index().expect("index");
    let report = tethys.find_dead_code(None).expect("report");

    let names = finding_names(&report);
    assert!(
        !names.iter().any(|(n, _)| n == "main"),
        "bin-root main must not be reported: {names:?}"
    );
    assert!(
        names.iter().any(|(n, _)| n == "lonely_qz"),
        "the dead sibling control must be reported: {names:?}"
    );
    assert!(
        !names.iter().any(|(n, _)| n == "helper_qz"),
        "called helper is alive: {names:?}"
    );
}

/// Design C11: the CLI envelope through the binary seam — pure-JSON
/// stdout with {findings, summary}; findings sorted by (file, line)
/// including a same-file tie broken by line; --limit truncates findings
/// while the summary keeps full counts. Kills: limit-before-sort,
/// post-truncation counting, envelope drift.
#[test]
fn cli_json_envelope_sort_and_limit() {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod zz;\nfn beta_qw() -> i32 {\n    1\n}\nfn alpha_qw() -> i32 {\n    2\n}\n",
        ),
        ("src/zz.rs", "fn last_qw() -> i32 {\n    3\n}\n"),
    ]);
    tethys.index().expect("index");
    drop(tethys);

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .args(["dead-code", "--json", "-w"])
        .arg(dir.path())
        .output()
        .expect("run binary");
    assert!(out.status.success(), "exit 0: {out:?}");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is pure JSON");
    let summary = &json["summary"];
    for field in ["candidates", "definite", "maybe"] {
        assert!(
            summary[field].is_u64(),
            "summary.{field} present: {summary}"
        );
    }
    let findings = json["findings"].as_array().expect("findings array");
    assert_eq!(summary["candidates"].as_u64().unwrap(), 3);
    assert_eq!(findings.len(), 3);
    let keys: Vec<(String, u64)> = findings
        .iter()
        .map(|f| {
            (
                f["file"].as_str().unwrap().to_string(),
                f["line"].as_u64().unwrap(),
            )
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "findings sorted by (file, line): {keys:?}");
    assert_eq!(
        keys[0].0, "src/lib.rs",
        "same-file findings precede src/zz.rs and tie-break by line"
    );

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .args(["dead-code", "--json", "--limit", "1", "-w"])
        .arg(dir.path())
        .output()
        .expect("run binary");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("limit JSON");
    assert_eq!(json["findings"].as_array().unwrap().len(), 1);
    assert_eq!(
        json["summary"]["candidates"].as_u64().unwrap(),
        3,
        "summary counts the full population, not the truncation"
    );
}

/// Design C11: a workspace whose only symbols are public has zero
/// candidates — a legitimately CLEAN empty report (exit 0, zeroed
/// summary), not an indeterminate one. Kills: y3bx-style root-set
/// preconditions leaking into an analysis that has none.
#[test]
fn zero_candidates_is_clean_empty() {
    let (_dir, mut tethys) =
        workspace_with_files(&[("src/lib.rs", "pub fn only_public() -> i32 {\n    1\n}\n")]);
    tethys.index().expect("index");
    let report = tethys.find_dead_code(None).expect("report");
    assert!(report.findings.is_empty());
    assert_eq!(
        (
            report.summary.candidates,
            report.summary.definite,
            report.summary.maybe
        ),
        (0, 0, 0)
    );
}

/// Design C12: consecutive runs on the same index serialize to
/// byte-identical JSON. Kills: hash-iteration order leaking into
/// output.
#[test]
fn deterministic_output() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "fn one_qv() {}\nfn two_qv() {}\nfn three_qv() {}\n",
    )]);
    tethys.index().expect("index");
    let a = serde_json::to_string(&tethys.find_dead_code(None).expect("run a")).expect("ser a");
    let b = serde_json::to_string(&tethys.find_dead_code(None).expect("run b")).expect("ser b");
    assert_eq!(a, b, "same index, same bytes");
}

/// Tier serialization is part of the JSON contract consumed by probe
/// diffs and future MCP tools: exactly "Definite" / "Maybe".
#[test]
fn tier_serialization_contract() {
    assert_eq!(
        serde_json::to_string(&Tier::Definite).unwrap(),
        "\"Definite\""
    );
    assert_eq!(serde_json::to_string(&Tier::Maybe).unwrap(), "\"Maybe\"");
}

/// Design C10: C# flows the same funnel. Verified against the S6
/// scratch probe: implicit-this and `this.`-qualified calls bind
/// (`UsedVx` alive), construct refs bind the type (`InnerVx` alive),
/// nested-class members reach depth-2 (`Worker` suppressed via
/// linkage), properties are candidates (`DeadPropVx` reported), and
/// `static void Main` — which the extractor classifies as kind
/// `function`, not `method` — is entry-point-excluded AND confers
/// liveness on its `Program` container. The Rust sibling file pins the
/// cross-language textual scan: a C# name mentioned in Rust text
/// demotes to Maybe. Kills: a method-only C# Main rule (the S6
/// discovery), language-blind kind lists, liveness without entry
/// points.
#[test]
fn csharp_funnel() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/App.cs",
            "namespace Fixture\n{\n\
             \x20   internal class Worker\n    {\n\
             \x20       private int DeadHelperVx() { return 1; }\n\
             \x20       private int UsedVx() { return 2; }\n\
             \x20       public int Run() { return this.UsedVx() + UsedVx(); }\n\
             \x20       private int DeadPropVx { get; set; }\n\
             \x20       private int MentionedVx() { return 4; }\n\
             \x20       private class InnerVx\n        {\n\
             \x20           public int Ping() { return 3; }\n\
             \x20       }\n\
             \x20       public int CallInner() { var i = new InnerVx(); return i.Ping(); }\n\
             \x20   }\n\
             \x20   internal class Program\n    {\n\
             \x20       private static void Main() { }\n\
             \x20   }\n}\n",
        ),
        (
            "src/lib.rs",
            "// migration note: MentionedVx moves to the Rust side\n",
        ),
    ]);
    tethys.index().expect("index");
    let report = tethys.find_dead_code(None).expect("report");

    let names = finding_names(&report);
    for (definite, kind) in [("DeadHelperVx", "method"), ("DeadPropVx", "property")] {
        assert!(
            names.iter().any(|(n, t)| n == definite && t == "Definite"),
            "{definite} ({kind}) must be Definite: {names:?}"
        );
    }
    assert!(
        names
            .iter()
            .any(|(n, t)| n == "MentionedVx" && t == "Maybe"),
        "Rust-file comment mention must demote MentionedVx to Maybe: {names:?}"
    );
    for absent in [
        "Main", "Program", "UsedVx", "InnerVx", "Ping", "Worker", "Run", "Fixture",
    ] {
        assert!(
            !names.iter().any(|(n, _)| n == absent),
            "{absent} must not be reported: {names:?}"
        );
    }
}

/// Design C7 — the PRD's self-index oracle as a permanent CI fence.
/// tethys compiles warning-free (rustc `dead_code` on, clippy -D
/// warnings in CI), so the true dead set is empty and any Definite
/// finding on tethys's own source is by definition a false positive.
/// Indexes a COPY of the repo source (never the ambient index; the
/// design-time falsifier measured 0 Definite / 37 would-be FPs all
/// absorbed). Kills: any suppression-channel regression — dropping the
/// textual channel alone would resurrect ~30 known FPs as Definite.
#[test]
fn self_index_zero_definite() {
    fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).expect("mkdir");
        for entry in std::fs::read_dir(from).expect("read_dir") {
            let entry = entry.expect("entry");
            let target = to.join(entry.file_name());
            if entry.file_type().expect("file_type").is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).expect("copy");
            }
        }
    }

    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::copy(repo.join("Cargo.toml"), dir.path().join("Cargo.toml")).expect("Cargo.toml");
    for tree in ["src", "tests", "benches", "examples"] {
        copy_tree(&repo.join(tree), &dir.path().join(tree));
    }

    let mut tethys = tethys::Tethys::new(dir.path()).expect("Tethys::new");
    tethys.index().expect("index the self-copy");
    let report = tethys.find_dead_code(None).expect("report");

    let definite: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.tier == Tier::Definite)
        .map(|f| format!("{}:{} {}", f.file, f.line, f.name))
        .collect();
    assert!(
        definite.is_empty(),
        "warning-free self-index must have ZERO Definite findings; \
         every entry here is a false positive: {definite:#?}"
    );
}
