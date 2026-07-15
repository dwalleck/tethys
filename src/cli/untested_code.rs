//! `tethys untested-code` command implementation.

use std::fmt::Write as _;
use std::path::Path;

use colored::Colorize;
use tethys::{Tethys, UntestedReport};
use tracing::debug;

/// Run the untested-code command.
///
/// Reports product functions/methods no test can reach — a forward closure
/// from `is_test` roots over the reference graph. Reachability, not
/// verification; known false-positive sources are documented on the facade.
/// With zero test roots the result is indeterminate: no findings are
/// listed and a diagnostic goes to stderr.
pub fn run(workspace: &Path, json: bool) -> Result<(), tethys::Error> {
    debug!(workspace = %workspace.display(), "Opening tethys index");
    let tethys = Tethys::new(workspace)?;

    let report = tethys.get_untested_code()?;
    debug!(
        test_roots = report.test_roots,
        untested = report.findings.len(),
        "Untested-code scan complete"
    );

    if report.is_indeterminate() {
        // Diagnostic, not data: the report itself stays parseable.
        eprintln!(
            "warning: no test roots indexed — untested-code is indeterminate \
             (index a workspace with tests, or check test detection)"
        );
    }

    let rendered = if json {
        render_json(&report)?
    } else {
        render_human(&report)
    };
    super::write_report(&rendered)
}

/// Render the report as pretty-printed JSON (data → stdout).
fn render_json(report: &UntestedReport) -> Result<String, tethys::Error> {
    #[derive(serde::Serialize)]
    struct JsonOutput<'a> {
        summary: JsonSummary,
        findings: &'a [tethys::UntestedFinding],
    }
    #[derive(serde::Serialize)]
    struct JsonSummary {
        test_roots: usize,
        product_fns: usize,
        untested_count: usize,
        indeterminate: bool,
    }

    let output = JsonOutput {
        summary: JsonSummary {
            test_roots: report.test_roots,
            product_fns: report.product_fns,
            untested_count: report.findings.len(),
            indeterminate: report.is_indeterminate(),
        },
        findings: &report.findings,
    };
    super::to_json_pretty(&output, "untested-code")
}

/// Render the report in human-readable format (data → stdout).
fn render_human(report: &UntestedReport) -> String {
    let mut buf = String::new();
    let _ = writeln!(buf, "{}", "Untested Code Analysis".cyan().bold());
    let _ = writeln!(buf, "{}", "=".repeat(63).dimmed());
    let _ = writeln!(buf);

    let _ = writeln!(buf, "{}:", "Summary".white().bold());
    let _ = writeln!(buf, "  {:<28}{}", "Test roots:".dimmed(), report.test_roots);
    let _ = writeln!(
        buf,
        "  {:<28}{}",
        "Product fns/methods:".dimmed(),
        report.product_fns
    );
    let _ = writeln!(
        buf,
        "  {:<28}{}",
        "Untested:".dimmed(),
        format!("{}", report.findings.len()).yellow()
    );
    let _ = writeln!(buf);

    if report.is_indeterminate() {
        let _ = writeln!(
            buf,
            "{}",
            "Indeterminate: no test roots indexed — nothing to report.".dimmed()
        );
        return buf;
    }
    if report.findings.is_empty() {
        let _ = writeln!(buf, "{}", "Every product fn is test-reachable.".dimmed());
        return buf;
    }

    let mut current_file = "";
    for finding in &report.findings {
        if finding.file != current_file {
            current_file = &finding.file;
            let _ = writeln!(buf, "{}", current_file.white().bold());
        }
        let _ = writeln!(
            buf,
            "  {:>5}  {} {}",
            finding.line.to_string().dimmed(),
            finding.kind.dimmed(),
            finding.name.yellow()
        );
    }
    let _ = writeln!(buf);
    let _ = writeln!(
        buf,
        "{}",
        "Reachability, not verification: a listed fn has no test-inbound \
         reference path; see module docs for known false-positive sources."
            .dimmed()
    );
    buf
}
