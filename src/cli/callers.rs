//! `tethys callers` command implementation.

use std::collections::HashSet;
use std::path::Path;

use colored::Colorize;
use tethys::{CallEdgeSelection, CallerMode, Tethys};

use super::display::{print_callers_by_file, print_symbol_impact_callers_by_file};
use super::ensure_lsp_if_requested;

/// Run the callers command.
pub fn run(
    workspace: &Path,
    symbol: &str,
    transitive: bool,
    depth: Option<usize>,
    lsp: bool,
    exclude_speculative: bool,
) -> Result<(), tethys::Error> {
    ensure_lsp_if_requested(lsp)?;

    let tethys = Tethys::new(workspace)?;

    if transitive {
        let impact = tethys.get_symbol_impact(symbol, depth, exclude_speculative)?;

        if impact.callers().is_empty() {
            println!("No callers found for \"{}\"", symbol.cyan());
            return Ok(());
        }

        println!(
            "Callers of \"{}\" (including transitive):",
            symbol.cyan().bold()
        );
        println!();

        let callers = impact.callers();
        let unique_files: HashSet<_> = callers.iter().map(|entry| &entry.file).collect();
        print_symbol_impact_callers_by_file(callers);

        let total = callers.len();
        println!();
        println!(
            "{}: {} direct, {} transitive",
            "Total".dimmed(),
            impact.direct_callers().len().to_string().green(),
            impact.transitive_callers().len().to_string().yellow()
        );
        println!(
            "{}: {} callers across {} files",
            "Summary".dimmed(),
            total,
            unique_files.len()
        );
    } else {
        let mode = if lsp {
            CallerMode::LspRefined
        } else {
            CallerMode::Indexed {
                call_edges: if exclude_speculative {
                    CallEdgeSelection::ExcludeSpeculative
                } else {
                    CallEdgeSelection::All
                },
            }
        };
        let callers = tethys.get_callers(symbol, mode)?;

        if callers.is_empty() {
            println!("No callers found for \"{}\"", symbol.cyan());
            return Ok(());
        }

        let mode_suffix = if lsp { " (with LSP)" } else { "" };
        println!("Callers of \"{}\"{}:", symbol.cyan().bold(), mode_suffix);
        println!();

        let unique_files: HashSet<_> = callers.iter().map(|c| &c.file).collect();
        print_callers_by_file(&callers);

        println!();
        println!(
            "{}: {} direct callers across {} files",
            "Total".dimmed(),
            callers.len().to_string().green(),
            unique_files.len()
        );
    }

    Ok(())
}
