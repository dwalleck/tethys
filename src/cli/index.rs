//! `tethys index` command implementation.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use colored::Colorize;
use tethys::discovery::{
    DiscoveryCachePolicy, DiscoveryFailure, DiscoveryOptions, DiscoverySnapshot, DiscoveryStanding,
    EvaluationContext, ImportToleranceProfile,
};
use tethys::{ArchPhaseResult, IndexOptions, Tethys};

use super::ensure_lsp_if_requested;

/// Parse an explicit global property, trimming its name without losing empty values or embedded `=`.
pub(crate) fn parse_property(value: &str) -> Result<(String, String), String> {
    let (name, value) = value
        .split_once('=')
        .filter(|(name, _)| !name.trim().is_empty() && !name.contains('\0'))
        .ok_or_else(|| "expected NAME=VALUE with a nonempty property name".to_owned())?;
    Ok((name.trim().to_owned(), value.to_owned()))
}

#[cfg(test)]
mod parse_property_tests {
    use super::parse_property;

    #[test]
    fn trims_padded_property_names() {
        assert_eq!(
            parse_property(" Configuration =Release").expect("valid property"),
            ("Configuration".to_owned(), "Release".to_owned())
        );
        assert_eq!(
            parse_property(" TargetFramework =net8.0").expect("valid property"),
            ("TargetFramework".to_owned(), "net8.0".to_owned())
        );
    }

    #[test]
    fn preserves_value_spacing_and_embedded_equals() {
        assert_eq!(
            parse_property("Configuration=  Release  ").expect("valid property"),
            ("Configuration".to_owned(), "  Release  ".to_owned())
        );
        assert_eq!(
            parse_property("Configuration=").expect("empty value is valid"),
            ("Configuration".to_owned(), String::new())
        );
        assert_eq!(
            parse_property("A=b=c").expect("embedded equals are valid"),
            ("A".to_owned(), "b=c".to_owned())
        );
    }

    #[test]
    fn rejects_missing_or_invalid_property_names() {
        assert!(parse_property("").is_err());
        assert!(parse_property("=x").is_err());
        assert!(parse_property("A\0=x").is_err());
    }
}

/// Run the index command.
pub(crate) fn run(
    workspace: &Path,
    rebuild: bool,
    lsp: bool,
    lsp_timeout: Option<u64>,
    discovery: crate::DiscoveryArgs,
) -> Result<ExitCode, tethys::Error> {
    ensure_lsp_if_requested(lsp)?;

    println!("{} {}...", "Indexing".cyan().bold(), workspace.display());

    // Build options - with_lsp() reads TETHYS_LSP_TIMEOUT env var by default,
    // but CLI arg takes precedence if provided
    let options = if lsp {
        let mut opts = IndexOptions::with_lsp();
        if let Some(timeout) = lsp_timeout {
            opts = opts.lsp_timeout(timeout);
        }
        opts
    } else {
        IndexOptions::default()
    }
    .with_discovery(DiscoveryOptions {
        trust_msbuild: discovery.trust_msbuild,
        allow_restore: discovery.allow_restore,
        context: EvaluationContext {
            configuration: discovery.configuration,
            platform: discovery.platform,
            runtime_identifier: discovery.runtime_identifier,
            global_properties: discovery.property.into_iter().collect(),
            import_profile: match discovery.import_profile {
                crate::ImportProfile::Strict => ImportToleranceProfile::Strict,
                crate::ImportProfile::BlankVsToolsPath => ImportToleranceProfile::BlankVsToolsPath,
            },
        },
        msbuild_path: discovery.msbuild_path,
        cache_policy: if discovery.no_discovery_cache {
            DiscoveryCachePolicy::Disabled
        } else {
            DiscoveryCachePolicy::Enabled
        },
        ..DiscoveryOptions::default()
    });

    let stats = if rebuild {
        println!("{}", "Rebuilding index from scratch".yellow());
        Tethys::rebuild_workspace(workspace, options)?
    } else {
        Tethys::new(workspace)?.index_with_options(options)?
    };

    // Display results
    println!();
    println!(
        "{} {} files, found {} symbols, {} references",
        "Indexed".green().bold(),
        stats.files_indexed,
        stats.symbols_found,
        stats.references_found
    );
    println!("{}: {:.2?}", "Duration".dimmed(), stats.duration);

    if stats.files_skipped > 0 {
        println!(
            "{}: {} files (unsupported language)",
            "Skipped".yellow(),
            stats.files_skipped
        );
    }

    if !stats.directories_skipped.is_empty() {
        println!(
            "{}: {} directories (permission denied)",
            "Skipped".yellow(),
            stats.directories_skipped.len()
        );
    }

    if !stats.errors.is_empty() {
        println!();
        println!("{} ({}):", "Errors".red().bold(), stats.errors.len());
        for err in stats.errors.iter().take(5) {
            println!("  {} {}: {}", "•".red(), err.path.display(), err.message);
        }
        if stats.errors.len() > 5 {
            println!("  ... and {} more", stats.errors.len() - 5);
        }
    }

    if !stats.unresolved_dependencies.is_empty() {
        println!();
        println!(
            "{}: {} (likely external crates)",
            "Unresolved dependencies".dimmed(),
            stats.unresolved_dependencies.len()
        );
    }

    let total_lsp_resolved = stats.total_lsp_resolved();
    if total_lsp_resolved > 0 {
        println!(
            "{}: {total_lsp_resolved} references via LSP",
            "LSP resolved".cyan(),
        );
    }

    // Indexing already succeeded by this point. The arch-phase warning is a
    // diagnostic, not data — send it to stderr so callers piping `tethys index`
    // stdout into another tool don't see warnings mixed into the data stream.
    // Also swallow BrokenPipe so a closed downstream pipe doesn't fail the
    // command (the primary work is already done).
    print_arch_phase_result(&mut io::stderr().lock(), stats.arch_phase.as_ref())
        .or_else(super::ignore_broken_pipe)
        .map_err(tethys::Error::Io)?;
    print_lsp_session_errors(&stats.lsp_sessions);

    print_discovery_failures(&stats.discovery);
    Ok(if stats.discovery.is_complete() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Render typed incomplete coverage only after the source revision is published.
fn print_discovery_failures(snapshot: &DiscoverySnapshot) {
    for issue in &snapshot.issues {
        print_discovery_failure("candidate", &issue.path.display(), &issue.failure);
    }
    for project in &snapshot.projects {
        if let DiscoveryStanding::Indeterminate(failure) = &project.standing {
            print_discovery_failure("project", &project.key.as_str(), failure);
        }
    }
    for unit in &snapshot.units {
        if let DiscoveryStanding::Indeterminate(failure) = &unit.standing {
            print_discovery_failure("unit", &unit.key.as_str(), failure);
        }
    }
}

fn print_discovery_failure(
    kind: &str,
    identity: &dyn std::fmt::Display,
    failure: &DiscoveryFailure,
) {
    eprintln!("discovery {kind} {identity}: {:?}", failure.reason);
    for diagnostic in &failure.diagnostics {
        eprintln!("  {:?}: {}", diagnostic.severity, diagnostic.message);
        if let Some(code) = &diagnostic.code {
            eprintln!("    code: {code}");
        }
        if let Some(file) = &diagnostic.file {
            eprintln!(
                "    {}:{}:{}",
                file.display(),
                diagnostic.line,
                diagnostic.column
            );
        }
    }
}

/// Print architecture-phase outcome to `out`, if any. Success path is silent.
///
/// Takes a `Write` sink so callers can unit-test all three output paths
/// (Failed → warning, Completed → silent, None → silent) without capturing
/// stdout.
fn print_arch_phase_result<W: Write>(
    out: &mut W,
    arch_phase: Option<&ArchPhaseResult>,
) -> io::Result<()> {
    match arch_phase {
        Some(ArchPhaseResult::Completed(arch)) => {
            // Keep the success case silent — rivets-tuph tracks surfacing
            // the package count in `tethys index` output. We don't want to
            // drop the no-output behavior callers may scrape.
            tracing::debug!(
                packages = arch.packages_recorded,
                files = arch.files_assigned,
                "architecture phase summary"
            );
        }
        Some(ArchPhaseResult::Failed(err)) => {
            writeln!(out)?;
            writeln!(
                out,
                "  {}: architecture phase failed — coupling metrics unavailable",
                "Warning".yellow().bold()
            )?;
            writeln!(out, "  {}", err.dimmed())?;
        }
        None => {
            // Phase didn't run (e.g., default state) — nothing to print.
        }
    }
    Ok(())
}

/// Print LSP session errors, if any. Diagnostics go to stderr so they don't
/// pollute stdout for callers that pipe `tethys index` output into another tool.
fn print_lsp_session_errors(sessions: &[tethys::LspSessionResult]) {
    for session in sessions {
        if session.has_errors() {
            eprintln!();
            match &session.outcome {
                tethys::LspOutcome::ServerUnavailable {
                    reason,
                    install_hint,
                } => {
                    eprintln!(
                        "{}: {} - {reason}",
                        "LSP error".red(),
                        session.language.as_str()
                    );
                    eprintln!("  {}: {install_hint}", "hint".dimmed());
                }
                tethys::LspOutcome::Completed(s) => {
                    for err in &s.errors {
                        eprintln!(
                            "{}: {} - {err}",
                            "LSP error".red(),
                            session.language.as_str()
                        );
                    }
                }
                tethys::LspOutcome::NothingToResolve => {}
            }
        }
    }
}

#[cfg(test)]
mod arch_phase_print_tests {
    use super::*;
    use tethys::{ArchPhaseResult, ArchStats};

    #[test]
    fn failed_path_writes_warning_with_error_text() {
        // No color override needed: the sink is a Vec<u8>, not a TTY.
        // colored strips ANSI codes when stdout is not a terminal, so
        // `.contains("Warning")` matches the plain-text output regardless.
        let mut buf: Vec<u8> = Vec::new();
        let result = ArchPhaseResult::Failed("simulated db corruption".into());
        print_arch_phase_result(&mut buf, Some(&result)).expect("write");
        let out = String::from_utf8(buf).expect("utf-8");
        assert!(out.contains("Warning"), "should include the Warning label");
        assert!(
            out.contains("simulated db corruption"),
            "should include the error Display form"
        );
    }

    #[test]
    fn completed_path_writes_nothing() {
        let mut buf: Vec<u8> = Vec::new();
        let result = ArchPhaseResult::Completed(ArchStats::default());
        print_arch_phase_result(&mut buf, Some(&result)).expect("write");
        assert!(
            buf.is_empty(),
            "completed path should be silent (rivets-tuph tracks surfacing the package count)"
        );
    }

    #[test]
    fn none_writes_nothing() {
        let mut buf: Vec<u8> = Vec::new();
        print_arch_phase_result(&mut buf, None).expect("write");
        assert!(buf.is_empty());
    }
}
