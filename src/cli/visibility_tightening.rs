//! `tethys visibility-tightening` command implementation.

use std::fmt::Write as _;
use std::path::Path;

use colored::Colorize;
use tethys::{Demotion, Tethys, Tier, VisibilityFinding};
use tracing::debug;

/// Run the visibility-tightening command.
///
/// Reports pub Rust items whose observed use is consistent with
/// `pub(crate)`, tiered Definite/Maybe with per-finding demotion reasons.
/// `workspace_closed` asserts nothing outside the indexed workspace can
/// consume the code, lifting the root-reachability Maybe ceiling.
pub fn run(workspace: &Path, json: bool, workspace_closed: bool) -> Result<(), tethys::Error> {
    debug!(workspace = %workspace.display(), "Opening tethys index");
    let tethys = Tethys::new(workspace)?;

    let findings = tethys.get_visibility_candidates(workspace_closed)?;
    debug!(
        finding_count = findings.len(),
        "Visibility-tightening scan complete"
    );

    let rendered = if json {
        render_json(&findings)?
    } else {
        render_human(&findings)
    };
    super::write_report(&rendered)
}

/// Render findings as pretty-printed JSON (data → stdout).
fn render_json(findings: &[VisibilityFinding]) -> Result<String, tethys::Error> {
    #[derive(serde::Serialize)]
    struct JsonOutput<'a> {
        summary: JsonSummary,
        findings: &'a [VisibilityFinding],
    }
    #[derive(serde::Serialize)]
    struct JsonSummary {
        candidate_count: usize,
        definite: usize,
        maybe: usize,
    }

    let definite = findings.iter().filter(|f| f.tier == Tier::Definite).count();
    let output = JsonOutput {
        summary: JsonSummary {
            candidate_count: findings.len(),
            definite,
            maybe: findings.len() - definite,
        },
        findings,
    };
    super::to_json_pretty(&output, "visibility-tightening")
}

/// The demotion's kebab-case wire spelling, via its serde rename — one
/// source of truth for JSON and human output.
fn demotion_label(demotion: Demotion) -> String {
    serde_json::to_value(demotion)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{demotion:?}"))
}

/// Render findings in human-readable format (data → stdout).
fn render_human(findings: &[VisibilityFinding]) -> String {
    let mut buf = String::new();
    let _ = writeln!(buf, "{}", "Visibility Tightening Analysis".cyan().bold());
    let _ = writeln!(buf, "{}", "=".repeat(63).dimmed());
    let _ = writeln!(buf);

    if findings.is_empty() {
        let _ = writeln!(buf, "{}", "No tightening candidates found.".dimmed());
        return buf;
    }

    let definite = findings.iter().filter(|f| f.tier == Tier::Definite).count();
    let _ = writeln!(buf, "{}:", "Summary".white().bold());
    let _ = writeln!(buf, "  {:<28}{}", "Candidates:".dimmed(), findings.len());
    let _ = writeln!(
        buf,
        "  {:<28}{}",
        "Definite (tighten):".dimmed(),
        format!("{definite}").green()
    );
    let _ = writeln!(
        buf,
        "  {:<28}{}",
        "Maybe (verify first):".dimmed(),
        format!("{}", findings.len() - definite).yellow()
    );
    let _ = writeln!(buf);

    for finding in findings {
        let tier = match finding.tier {
            Tier::Definite => "[Definite]".green().bold(),
            Tier::Maybe => "[Maybe]   ".yellow(),
        };
        let reasons = if finding.demotions.is_empty() {
            String::new()
        } else {
            let labels: Vec<String> = finding
                .demotions
                .iter()
                .map(|&d| demotion_label(d))
                .collect();
            format!(" ({})", labels.join(", "))
        };
        let _ = writeln!(
            buf,
            "  {} {} {} — {}:{}{}",
            tier,
            finding.kind.dimmed(),
            finding.name.yellow().bold(),
            finding.file,
            finding.line,
            reasons.dimmed()
        );
    }
    buf
}
