//! `tethys deprecated-callers` command implementation.

use std::fmt::Write as _;
use std::path::Path;

use colored::Colorize;
use tethys::{DeprecatedFinding, Tethys, Tier, Via};
use tracing::debug;

/// Run the deprecated-callers command.
///
/// Reports every `#[deprecated]` symbol in the index with its reference
/// sites, tiered by resolution trustworthiness (Definite/Maybe), plus a
/// clean list for deprecated symbols with no known remaining callers.
pub fn run(workspace: &Path, json: bool) -> Result<(), tethys::Error> {
    debug!(workspace = %workspace.display(), "Opening tethys index");
    let tethys = Tethys::new(workspace)?;

    let findings = tethys.get_deprecated_callers()?;
    debug!(
        finding_count = findings.len(),
        "Deprecated-callers scan complete"
    );

    let rendered = if json {
        render_json(&findings)?
    } else {
        render_human(&findings)
    };
    super::write_report(&rendered)
}

/// Render findings as pretty-printed JSON (data → stdout).
fn render_json(findings: &[DeprecatedFinding]) -> Result<String, tethys::Error> {
    #[derive(serde::Serialize)]
    struct JsonOutput<'a> {
        summary: JsonSummary,
        deprecated: &'a [DeprecatedFinding],
    }
    #[derive(serde::Serialize)]
    struct JsonSummary {
        symbol_count: usize,
        with_callers: usize,
        clean: usize,
        site_count: usize,
    }

    let with_callers = findings.iter().filter(|f| !f.sites.is_empty()).count();
    let output = JsonOutput {
        summary: JsonSummary {
            symbol_count: findings.len(),
            with_callers,
            clean: findings.len() - with_callers,
            site_count: findings.iter().map(|f| f.sites.len()).sum(),
        },
        deprecated: findings,
    };
    super::to_json_pretty(&output, "deprecated-callers")
}

/// Render a possibly multi-line note on one line (first line + ellipsis).
fn one_line(text: &str) -> String {
    match text.split_once('\n') {
        Some((first, _)) => format!("{first}…"),
        None => text.to_string(),
    }
}

/// Human-readable deprecation qualifier, e.g. `(since 4.0.0 — Use x)` for
/// Rust or `(error — Use New)` for a C# `[Obsolete("Use New", true)]`.
/// Rust output is unchanged by the `error` piece: the flag is C#-only.
fn deprecation_meta(since: Option<&str>, note: Option<&str>, error: Option<bool>) -> String {
    let mut pieces = Vec::new();
    if error == Some(true) {
        pieces.push("error".to_string());
    }
    if let Some(s) = since {
        pieces.push(format!("since {s}"));
    }
    if let Some(n) = note {
        pieces.push(one_line(n));
    }
    if pieces.is_empty() {
        String::new()
    } else {
        format!("  ({})", pieces.join(" — "))
    }
}

/// Render findings in human-readable format (data → stdout).
fn render_human(findings: &[DeprecatedFinding]) -> String {
    let mut buf = String::new();
    let _ = writeln!(buf, "{}", "Deprecated Callers Analysis".cyan().bold());
    let _ = writeln!(buf, "{}", "=".repeat(63).dimmed());
    let _ = writeln!(buf);

    if findings.is_empty() {
        let _ = writeln!(buf, "{}", "No deprecated symbols found.".dimmed());
        return buf;
    }

    let with_callers: Vec<&DeprecatedFinding> =
        findings.iter().filter(|f| !f.sites.is_empty()).collect();
    let clean: Vec<&DeprecatedFinding> = findings.iter().filter(|f| f.sites.is_empty()).collect();

    let _ = writeln!(buf, "{}:", "Summary".white().bold());
    let _ = writeln!(
        buf,
        "  {:<28}{}",
        "Deprecated symbols:".dimmed(),
        findings.len()
    );
    let _ = writeln!(
        buf,
        "  {:<28}{}",
        "With remaining callers:".dimmed(),
        format!(
            "{} ({} call sites)",
            with_callers.len(),
            with_callers.iter().map(|f| f.sites.len()).sum::<usize>()
        )
        .yellow()
    );
    let _ = writeln!(
        buf,
        "  {:<28}{}",
        "Clean (no known callers):".dimmed(),
        format!("{}", clean.len()).green()
    );

    for finding in &with_callers {
        let symbol = &finding.symbol;
        let _ = writeln!(buf);
        let _ = writeln!(buf, "{}", "-".repeat(62).dimmed());
        let _ = writeln!(
            buf,
            "{} {} — {}:{}{}",
            symbol.kind.dimmed(),
            symbol.name.yellow().bold(),
            symbol.file,
            symbol.line,
            deprecation_meta(
                symbol.since.as_deref(),
                symbol.note.as_deref(),
                symbol.error
            )
            .dimmed()
        );
        let _ = writeln!(buf, "{}", "-".repeat(62).dimmed());
        for site in &finding.sites {
            let tier = match site.tier {
                Tier::Definite => "[Definite]".red().bold(),
                Tier::Maybe => "[Maybe]   ".yellow(),
            };
            let caller = match (&site.caller, site.via) {
                (Some(name), _) => format!("in {name}"),
                (None, Via::Resolved) => "at top level".to_string(),
                (None, Via::UnresolvedQualified) => "at top level (unresolved)".to_string(),
            };
            let via_note = if site.via == Via::UnresolvedQualified && site.caller.is_some() {
                " (unresolved qualified path)"
            } else {
                ""
            };
            let _ = writeln!(
                buf,
                "  {} {}:{} {}{}",
                tier,
                site.file,
                site.line,
                caller,
                via_note.dimmed()
            );
        }
    }

    if !clean.is_empty() {
        let _ = writeln!(buf);
        let _ = writeln!(buf, "{}:", "Clean (migration done)".green().bold());
        for finding in &clean {
            let symbol = &finding.symbol;
            let _ = writeln!(
                buf,
                "  {} {} — {}:{}",
                symbol.kind.dimmed(),
                symbol.name,
                symbol.file,
                symbol.line
            );
        }
    }
    buf
}
