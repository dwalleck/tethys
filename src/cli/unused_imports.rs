//! `tethys unused-imports` command implementation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use colored::Colorize;
use tethys::{Tethys, UnusedImport, UnusedImportConfidence};
use tracing::debug;

/// Run the unused-imports command.
///
/// Re-parses workspace Rust files and reports explicit imports whose bound
/// name is never referenced. The index database (if present) is used to
/// upgrade confidence for names that resolve to workspace symbols.
///
/// By default only [`UnusedImportConfidence::Definite`] findings are shown;
/// `all` additionally shows possible-trait findings, which are usually
/// false positives (a trait imported for method-call syntax leaves no
/// reference to its own name).
pub fn run(workspace: &Path, json: bool, all: bool) -> Result<(), tethys::Error> {
    debug!(workspace = %workspace.display(), "Opening tethys database");
    let tethys = Tethys::new(workspace)?;

    let findings = tethys.find_unused_imports()?;
    debug!(
        finding_count = findings.len(),
        "Unused import scan complete"
    );

    let definite_count = findings
        .iter()
        .filter(|f| f.confidence == UnusedImportConfidence::Definite)
        .count();
    let maybe_trait_count = findings.len() - definite_count;

    let shown: Vec<&UnusedImport> = findings
        .iter()
        .filter(|f| all || f.confidence == UnusedImportConfidence::Definite)
        .collect();

    if json {
        output_json(&shown, definite_count, maybe_trait_count)?;
    } else {
        output_human(&shown, definite_count, maybe_trait_count, all);
    }

    Ok(())
}

/// Output findings in JSON format.
fn output_json(
    shown: &[&UnusedImport],
    definite_count: usize,
    maybe_trait_count: usize,
) -> Result<(), tethys::Error> {
    #[derive(serde::Serialize)]
    struct JsonOutput<'a> {
        summary: JsonSummary,
        unused_imports: Vec<&'a UnusedImport>,
    }

    #[derive(serde::Serialize)]
    struct JsonSummary {
        definite: usize,
        maybe_trait: usize,
    }

    let output = JsonOutput {
        summary: JsonSummary {
            definite: definite_count,
            maybe_trait: maybe_trait_count,
        },
        unused_imports: shown.to_vec(),
    };

    let json = serde_json::to_string_pretty(&output).map_err(|e| {
        tethys::Error::Internal(format!("Failed to serialize unused imports to JSON: {e}"))
    })?;
    println!("{json}");

    Ok(())
}

/// Output findings in human-readable format, grouped by file.
fn output_human(
    shown: &[&UnusedImport],
    definite_count: usize,
    maybe_trait_count: usize,
    all: bool,
) {
    println!("{}", "Unused Imports".cyan().bold());
    println!("{}", "=".repeat(63).dimmed());
    println!();

    println!("{}:", "Summary".white().bold());
    println!(
        "  {:<20}{}",
        "Definite:".dimmed(),
        definite_count.to_string().yellow()
    );
    if all {
        println!(
            "  {:<20}{}",
            "Possible traits:".dimmed(),
            maybe_trait_count.to_string().dimmed()
        );
    } else {
        println!(
            "  {:<20}{}",
            "Possible traits:".dimmed(),
            format!("{maybe_trait_count} (use --all to show)").dimmed()
        );
    }
    println!();

    if shown.is_empty() {
        println!("{}", "No unused imports found.".dimmed());
        return;
    }

    let mut by_file: BTreeMap<&PathBuf, Vec<&UnusedImport>> = BTreeMap::new();
    for finding in shown {
        by_file.entry(&finding.file).or_default().push(finding);
    }

    for (file, imports) in &by_file {
        println!("{}", file.display().to_string().white().bold());
        for imp in imports {
            let marker = match imp.confidence {
                UnusedImportConfidence::Definite => String::new(),
                UnusedImportConfidence::MaybeTrait => " [may be a trait used via method calls]"
                    .dimmed()
                    .to_string(),
            };
            println!(
                "  {}:{}  {} {} {}{}",
                file.display().to_string().dimmed(),
                imp.line.to_string().cyan(),
                "use".dimmed(),
                imp.source_module.dimmed(),
                imp.name.yellow(),
                marker
            );
        }
        println!();
    }
}
