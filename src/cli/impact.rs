//! `tethys impact` command implementation.

use std::path::Path;

use colored::Colorize;
use tethys::{CallEdgeSelection, Impact, SymbolImpact, Tethys};

use super::display::{print_dependents, print_symbol_impact_callers_by_file};
use super::ensure_lsp_if_requested;

/// Run the impact command.
pub fn run(
    workspace: &Path,
    target: &str,
    is_symbol: bool,
    depth: Option<usize>,
    lsp: bool,
) -> Result<(), tethys::Error> {
    ensure_lsp_if_requested(lsp)?;

    let tethys = Tethys::new(workspace)?;

    if is_symbol {
        let impact = tethys.get_symbol_impact(target, depth, CallEdgeSelection::All)?;
        println!("Impact analysis for symbol \"{}\":", target.cyan().bold());
        print_symbol_impact_analysis(&impact);
    } else {
        let target_path = Path::new(target);
        let impact = tethys.get_impact(target_path, depth)?;
        println!("Impact analysis for {}:", target.cyan().bold());
        print_impact_analysis(&impact);
    }

    Ok(())
}

/// Display impact analysis results.
fn print_impact_analysis(impact: &Impact) {
    println!();

    // Direct dependents
    println!(
        "  {} ({} files):",
        "Direct dependents".white().bold(),
        impact.direct_dependents.len().to_string().green()
    );
    print_dependents(&impact.direct_dependents, "(none)");

    println!();

    // Transitive dependents
    println!(
        "  {} ({} files total):",
        "Transitive dependents".white().bold(),
        impact.transitive_dependents.len().to_string().yellow()
    );
    print_dependents(&impact.transitive_dependents, "(none beyond direct)");
}

/// Display symbol impact using caller-specific, depth-accurate results.
fn print_symbol_impact_analysis(impact: &SymbolImpact) {
    println!();

    let direct = impact.direct_callers();
    println!(
        "  {} ({} symbols):",
        "Direct callers".white().bold(),
        direct.len().to_string().green()
    );
    if direct.is_empty() {
        println!("    {}", "(none)".dimmed());
    } else {
        print_symbol_impact_callers_by_file(direct);
    }

    println!();

    let transitive = impact.transitive_callers();
    println!(
        "  {} ({} symbols beyond direct):",
        "Transitive callers".white().bold(),
        transitive.len().to_string().yellow()
    );
    if transitive.is_empty() {
        println!("    {}", "(none beyond direct)".dimmed());
    } else {
        print_symbol_impact_callers_by_file(transitive);
    }
}
