//! `tethys affected-tests` command implementation.
//!
//! Exit-code contract (tethys-09wx, CONTEXT.md "Query standing"):
//! - **0** — confirmed: the index stands behind the result; empty stdout
//!   means confirmed no affected tests.
//! - **2** — indeterminate: the result may be under-complete; stdout still
//!   carries whatever tests were found, and one machine-readable
//!   `indeterminate: <kind>: <detail>` line per reason goes to stderr
//!   (grep anchor `^indeterminate: ` — tracing lines start with a
//!   timestamp, so the anchor never collides).
//! - **1** — hard error (unchanged).
//!
//! Standing itself is computed by the facade
//! (`get_affected_tests_with_standing`); this layer only maps it onto the
//! process contract.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use colored::Colorize;
use tethys::{QueryStanding, StandingReason, Tethys};
use tracing::{debug, warn};

/// Exit code for an indeterminate query standing — distinct from `1`
/// (hard error) so CI recipes can fail open with signal.
const EXIT_INDETERMINATE: u8 = 2;

/// Run the affected-tests command, returning the process exit code.
pub fn run(
    workspace: &Path,
    files: &[String],
    names_only: bool,
) -> Result<ExitCode, tethys::Error> {
    debug!(workspace = %workspace.display(), "Opening tethys database");
    let tethys = Tethys::new(workspace)?;

    // Convert file strings to PathBuf
    let changed_files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();

    if changed_files.is_empty() {
        warn!("No files specified for affected-tests query");
        eprintln!("{}: no files specified", "warning".yellow());
        return Ok(ExitCode::SUCCESS);
    }

    debug!(
        file_count = changed_files.len(),
        files = ?changed_files,
        "Querying affected tests"
    );
    let report = tethys.get_affected_tests_with_standing(&changed_files)?;
    let affected = report.tests;
    debug!(affected_count = affected.len(), "Found affected tests");

    if affected.is_empty() {
        if !names_only {
            println!("No tests affected by changes to the specified files.");
        }
        return Ok(exit_for_standing(&report.standing));
    }

    if names_only {
        // Machine-readable output: one test name per line
        for test in &affected {
            println!("{}", test.qualified_name);
        }
    } else {
        // Human-readable output
        println!(
            "Tests affected by changes to {} file(s):",
            changed_files.len().to_string().cyan()
        );
        println!();

        // Group tests by file for nicer display
        let mut tests_by_file: std::collections::HashMap<tethys::FileId, Vec<&tethys::Symbol>> =
            std::collections::HashMap::new();
        for test in &affected {
            tests_by_file.entry(test.file_id).or_default().push(test);
        }

        let file_count = tests_by_file.len();

        // Get file paths for display
        for (file_id, tests) in &tests_by_file {
            let file_display = match tethys.get_file_by_id(*file_id) {
                Ok(Some(file)) => file.path.display().to_string(),
                Ok(None) => {
                    warn!(file_id = %file_id, "File not found in database");
                    format!("(unknown file_id: {file_id})")
                }
                Err(e) => {
                    warn!(file_id = %file_id, error = %e, "Failed to look up file");
                    format!("(file_id: {file_id})")
                }
            };
            println!("  {}:", file_display.white().bold());
            for test in tests {
                println!("    {} {}", "-".dimmed(), test.qualified_name.green());
            }
            println!();
        }

        println!(
            "{}: {} test(s) across {} file(s)",
            "Total".dimmed(),
            affected.len().to_string().green(),
            file_count
        );
    }

    Ok(exit_for_standing(&report.standing))
}

/// Map query standing onto the process contract: emit one machine-readable
/// reason line per trigger on stderr, then pick the exit code.
///
/// Reason lines are contract output, not logs — they never route through
/// `tracing` (whose lines are timestamped and level-tagged), so the
/// `^indeterminate: ` grep anchor stays collision-free on stderr.
fn exit_for_standing(standing: &QueryStanding) -> ExitCode {
    match standing {
        QueryStanding::Confirmed => ExitCode::SUCCESS,
        QueryStanding::Indeterminate(reasons) => {
            for StandingReason { kind, path } in reasons {
                match path {
                    Some(p) => eprintln!("indeterminate: {kind}: {}", p.display()),
                    None => eprintln!("indeterminate: {kind}: workspace changed since last index"),
                }
            }
            ExitCode::from(EXIT_INDETERMINATE)
        }
    }
}
