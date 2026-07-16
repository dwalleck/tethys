//! `tethys dead-code` command implementation.

use std::fmt::Write as _;
use std::path::Path;

use colored::Colorize;
use tethys::{DeadCodeReport, Tethys, Tier};
use tracing::debug;

/// Run the dead-code command.
///
/// Reports non-public, non-test symbols with zero inbound evidence,
/// tiered by a textual word-boundary scan: Definite = the name occurs
/// nowhere outside its own definition span; Maybe = it appears somewhere
/// reference extraction cannot see (verify before deleting). Known
/// false-positive sources are documented on the facade. `limit`
/// truncates the listing; the summary always counts the full population.
pub fn run(workspace: &Path, limit: Option<usize>, json: bool) -> Result<(), tethys::Error> {
    debug!(workspace = %workspace.display(), "Opening tethys index");
    let tethys = Tethys::new(workspace)?;

    let report = tethys.find_dead_code(limit)?;
    debug!(
        candidates = report.summary.candidates,
        definite = report.summary.definite,
        maybe = report.summary.maybe,
        "Dead-code scan complete"
    );

    let rendered = if json {
        super::to_json_pretty(&report, "dead-code")?
    } else {
        render_human(&report)
    };
    super::write_report(&rendered)
}

/// Render the report in human-readable format (data → stdout).
fn render_human(report: &DeadCodeReport) -> String {
    let mut buf = String::new();
    let _ = writeln!(buf, "{}", "Dead Code Analysis".cyan().bold());
    let _ = writeln!(buf, "{}", "=".repeat(63).dimmed());
    let _ = writeln!(buf);

    let _ = writeln!(buf, "{}:", "Summary".white().bold());
    let _ = writeln!(
        buf,
        "  {:<28}{}",
        "Candidates:".dimmed(),
        report.summary.candidates
    );
    let _ = writeln!(
        buf,
        "  {:<28}{}",
        "Definite:".dimmed(),
        format!("{}", report.summary.definite).red()
    );
    let _ = writeln!(
        buf,
        "  {:<28}{}",
        "Maybe:".dimmed(),
        format!("{}", report.summary.maybe).yellow()
    );
    let _ = writeln!(buf);

    if report.summary.candidates == 0 {
        let _ = writeln!(buf, "{}", "No dead-code candidates found.".dimmed());
        return buf;
    }
    if report.findings.len() < report.summary.candidates {
        let _ = writeln!(
            buf,
            "{}",
            format!(
                "Showing {} of {} findings (--limit).",
                report.findings.len(),
                report.summary.candidates
            )
            .dimmed()
        );
        let _ = writeln!(buf);
    }

    let mut current_file = "";
    for finding in &report.findings {
        if finding.file != current_file {
            current_file = &finding.file;
            let _ = writeln!(buf, "{}", current_file.white().bold());
        }
        let tier = match finding.tier {
            Tier::Definite => "DEFINITE".red().bold(),
            Tier::Maybe => "maybe".yellow(),
        };
        let _ = writeln!(
            buf,
            "  {:>5}  {:<8}  {} {}",
            finding.line.to_string().dimmed(),
            tier,
            finding.kind.dimmed(),
            finding.name
        );
    }
    let _ = writeln!(buf);
    let _ = writeln!(
        buf,
        "{}",
        "Definite = name occurs nowhere outside its own definition; Maybe = \
         it appears in text the reference extractor cannot see (macro token \
         trees, format-string captures, fn-as-value shapes) — verify before \
         deleting. Public symbols are never reported."
            .dimmed()
    );
    buf
}
