//! Integration regression fences for the untested-code analysis
//! (tethys-y3bx). Each test builds its OWN real index — the on-disk index
//! can be stale, so never query an ambient DB.
//!
//! Unit fences for the closure algorithm live in `src/db/untested.rs`;
//! these cover what only a full index exercises: root detection, the
//! refs-vs-call_edges substrate divergence (approved D-D), kind/is_test
//! scoping, C# parity, and the CLI envelope through the binary seam.

#![allow(clippy::needless_raw_string_hashes, clippy::doc_markdown)]

mod common;

use common::{open_db, workspace_with_files};

fn untested_names(report: &tethys::UntestedReport) -> Vec<(String, String)> {
    report
        .findings
        .iter()
        .map(|f| (f.name.clone(), f.file.clone()))
        .collect()
}

/// F-U1 (claims C1, C6): tested fn absent; untested fn present; an
/// untested a→b pair BOTH present (closure starts at roots only, S10);
/// same-named untested fns in two files both present by id (S18); the
/// test fn itself, a struct, and a const never reported (kind + is_test
/// scope).
#[test]
fn report_is_product_fns_outside_test_closure() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod twin_a;\npub mod twin_b;\n\
             pub struct NotAFn;\n\
             pub const NOT_A_FN: i32 = 1;\n\
             pub fn tested() -> i32 {\n    1\n}\n\
             pub fn untested_a() -> i32 {\n    untested_b()\n}\n\
             pub fn untested_b() -> i32 {\n    2\n}\n\
             #[cfg(test)]\nmod tests {\n\
             \x20   use super::*;\n\
             \x20   #[test]\n\
             \x20   fn t() {\n\
             \x20       let _ = tested();\n\
             \x20   }\n\
             }\n",
        ),
        ("src/twin_a.rs", "pub fn twin() -> i32 {\n    1\n}\n"),
        ("src/twin_b.rs", "pub fn twin() -> i32 {\n    2\n}\n"),
    ]);
    tethys.index().expect("index");
    let report = tethys.get_untested_code().expect("report");

    assert!(report.test_roots >= 1, "the #[test] fn is a root");
    assert!(!report.is_indeterminate());
    let names = untested_names(&report);
    assert!(
        !names.iter().any(|(n, _)| n == "tested"),
        "directly-tested fn must not be reported: {names:?}"
    );
    for expected in ["untested_a", "untested_b"] {
        assert!(
            names.iter().any(|(n, _)| n == expected),
            "{expected} must be reported (untested pair, S10): {names:?}"
        );
    }
    assert_eq!(
        names.iter().filter(|(n, _)| n == "twin").count(),
        2,
        "same-named fns in two files are independent by id (S18): {names:?}"
    );
    assert!(
        !names
            .iter()
            .any(|(n, _)| n == "t" || n == "NotAFn" || n == "NOT_A_FN"),
        "test fns and non-fn kinds are out of scope (C6): {names:?}"
    );
}

/// F-U2 (claim C2, the D-D pin): a fn tested ONLY through an assert macro
/// is covered — and the SAME fixture's call_edges provably lack the edge,
/// so a traversal that switched back to call_edges would report it
/// untested. Two independent asserts: report absence + SQL divergence.
#[test]
fn assert_only_tested_fn_is_covered_and_call_edges_diverge() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub fn helper() -> i32 {\n    1\n}\n\
         #[cfg(test)]\nmod tests {\n\
         \x20   use super::*;\n\
         \x20   #[test]\n\
         \x20   fn t() {\n\
         \x20       assert_eq!(helper(), 1);\n\
         \x20   }\n\
         }\n",
    )]);
    tethys.index().expect("index");
    let report = tethys.get_untested_code().expect("report");
    assert!(
        !untested_names(&report).iter().any(|(n, _)| n == "helper"),
        "assert-only-tested fn must be covered via the macro_call ref"
    );

    let conn = open_db(&tethys);
    let edges_to_helper: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM call_edges ce
             JOIN symbols s ON ce.callee_symbol_id = s.id
             WHERE s.name = 'helper'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(
        edges_to_helper, 0,
        "call_edges lacks the macro edge — the refs substrate is load-bearing"
    );
}

/// F-U3 (claim C3): transitive closure through a chain and a cycle covers
/// everything reached; a self-loop no test reaches stays reported once.
#[test]
fn closure_is_transitive_through_cycles() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub fn a() -> i32 {\n    b()\n}\n\
         pub fn b() -> i32 {\n    c()\n}\n\
         pub fn c() -> i32 {\n    b()\n}\n\
         pub fn lonely() -> i32 {\n    lonely()\n}\n\
         #[cfg(test)]\nmod tests {\n\
         \x20   use super::*;\n\
         \x20   #[test]\n\
         \x20   fn t() {\n\
         \x20       let _ = a();\n\
         \x20   }\n\
         }\n",
    )]);
    tethys.index().expect("index");
    let report = tethys.get_untested_code().expect("report");
    let names = untested_names(&report);
    for covered in ["a", "b", "c"] {
        assert!(
            !names.iter().any(|(n, _)| n == covered),
            "{covered} is transitively tested: {names:?}"
        );
    }
    assert_eq!(
        names.iter().filter(|(n, _)| n == "lonely").count(),
        1,
        "self-recursive unreached fn reported exactly once: {names:?}"
    );
}

/// F-U4 (claim C4): zero test roots → indeterminate (no findings, flag
/// set); an all-test workspace → determinate empty report.
#[test]
fn zero_roots_is_indeterminate_not_a_dump() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "pub fn nobody_tests_me() -> i32 {\n    1\n}\n",
    )]);
    tethys.index().expect("index");
    let report = tethys.get_untested_code().expect("report");
    assert_eq!(report.test_roots, 0);
    assert!(report.is_indeterminate());
    assert!(
        report.findings.is_empty(),
        "indeterminate must not accuse: {:?}",
        report.findings
    );
    assert!(report.product_fns >= 1, "the product fn was still counted");

    let (_dir2, mut all_tests) = workspace_with_files(&[(
        "src/lib.rs",
        "#[cfg(test)]\nmod tests {\n\
         \x20   #[test]\n\
         \x20   fn t() {\n\
         \x20       assert!(true);\n\
         \x20   }\n\
         }\n",
    )]);
    all_tests.index().expect("index");
    let report = all_tests.get_untested_code().expect("report");
    assert!(!report.is_indeterminate(), "roots exist");
    assert!(report.findings.is_empty(), "no product fns → empty report");
}

/// F-U5 (claim C5): C# parity — a `[Fact]` method is a root; the method it
/// calls is covered, its untested sibling is reported.
#[test]
fn csharp_fact_roots_cover_their_callees() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "cs/Lib.cs",
            r"
namespace My.Lib
{
    public static class Ops
    {
        public static int Tested() { return 1; }
        public static int Untested() { return 2; }
    }
}
",
        ),
        (
            "cs/Tests.cs",
            r"
using My.Lib;

namespace My.Tests
{
    public class OpsTests
    {
        [Fact]
        public void TestedWorks()
        {
            var x = Ops.Tested();
        }
    }
}
",
        ),
    ]);
    tethys.index().expect("index");
    let report = tethys.get_untested_code().expect("report");
    assert!(report.test_roots >= 1, "[Fact] method is a root");
    let names = untested_names(&report);
    assert!(
        !names.iter().any(|(n, _)| n == "Tested"),
        "C# method called from a [Fact] test is covered: {names:?}"
    );
    assert!(
        names.iter().any(|(n, _)| n == "Untested"),
        "untested C# sibling is reported: {names:?}"
    );
}

/// F-U7 (claim C7): drive the BINARY with --json — stable envelope fields,
/// findings sorted by (file, line), stdout pure JSON, exit 0; the
/// zero-roots fixture warns on stderr and still exits 0.
#[test]
fn cli_json_envelope_through_binary_seam() {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod zz;\n\
             pub fn beta_unreached() -> i32 {\n    1\n}\n\
             pub fn alpha_unreached() -> i32 {\n    2\n}\n",
        ),
        ("src/zz.rs", "pub fn last_file() -> i32 {\n    3\n}\n"),
        (
            "tests/t.rs",
            "#[test]\nfn root() {\n    assert!(true);\n}\n",
        ),
    ]);
    tethys.index().expect("index");
    drop(tethys);

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .args(["untested-code", "--json", "-w"])
        .arg(dir.path())
        .output()
        .expect("run binary");
    assert!(out.status.success(), "exit 0: {out:?}");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is pure JSON");
    let summary = &json["summary"];
    for field in ["test_roots", "product_fns", "untested_count"] {
        assert!(
            summary[field].is_u64(),
            "summary.{field} present: {summary}"
        );
    }
    assert_eq!(summary["indeterminate"], serde_json::Value::Bool(false));
    let findings = json["findings"].as_array().expect("findings array");
    assert_eq!(
        summary["untested_count"].as_u64().unwrap(),
        findings.len() as u64
    );
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
    assert_eq!(keys, sorted, "findings sorted by (file, line)");
    assert!(
        keys.iter().any(|(f, _)| f == "src/zz.rs"),
        "second file present so the sort key actually fires: {keys:?}"
    );

    // Zero-roots arm: stderr warning, exit 0, indeterminate JSON.
    let (dir2, mut no_tests) =
        workspace_with_files(&[("src/lib.rs", "pub fn only_product() -> i32 {\n    1\n}\n")]);
    no_tests.index().expect("index");
    drop(no_tests);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .args(["untested-code", "--json", "-w"])
        .arg(dir2.path())
        .output()
        .expect("run binary");
    assert!(out.status.success(), "indeterminate still exits 0");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is pure JSON");
    assert_eq!(
        json["summary"]["indeterminate"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(json["findings"].as_array().unwrap().len(), 0);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("indeterminate"),
        "diagnostic goes to stderr: {stderr}"
    );
}
