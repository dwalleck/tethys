//! Common display utilities for CLI commands.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use colored::Colorize;
use tethys::{Caller, FileImpactDependent, SymbolImpactCaller};

const MAX_DISPLAY_ITEMS: usize = 10;

/// Display a list of dependent files with optional truncation.
///
/// Shows up to `MAX_DISPLAY_ITEMS` files with bullet points. If there are more,
/// shows "... and N more". If empty, shows the provided `empty_message`.
pub fn print_dependents(dependents: &[FileImpactDependent], empty_message: &str) {
    if dependents.is_empty() {
        println!("    {}", empty_message.dimmed());
        return;
    }

    for dep in dependents.iter().take(MAX_DISPLAY_ITEMS) {
        println!("    {} {}", "•".dimmed(), dep.file.display());
    }

    if dependents.len() > MAX_DISPLAY_ITEMS {
        println!(
            "    {} ... and {} more",
            "•".dimmed(),
            dependents.len() - MAX_DISPLAY_ITEMS
        );
    }
}

/// Group direct callers by indexed file and display their symbol names.
pub fn print_callers_by_file(callers: &[Caller]) {
    let grouped = group_callers(
        callers
            .iter()
            .map(|caller| (caller.file.as_path(), caller.symbol.qualified_name.as_str())),
    );
    print_grouped_callers(grouped);
}

/// Group symbol-impact callers by indexed file and display their symbol names.
pub fn print_symbol_impact_callers_by_file(callers: &[SymbolImpactCaller]) {
    let grouped = group_callers(
        callers
            .iter()
            .map(|entry| (entry.file.as_path(), entry.symbol.qualified_name.as_str())),
    );
    print_grouped_callers(grouped);
}

fn group_callers<'a>(
    callers: impl IntoIterator<Item = (&'a Path, &'a str)>,
) -> Vec<(&'a Path, Vec<&'a str>)> {
    let mut by_file = HashMap::<_, HashSet<_>>::new();
    for (file, symbol) in callers {
        by_file.entry(file).or_default().insert(symbol);
    }

    let mut grouped: Vec<_> = by_file
        .into_iter()
        .map(|(file, symbols)| {
            let mut symbols: Vec<_> = symbols.into_iter().collect();
            symbols.sort_unstable();
            (file, symbols)
        })
        .collect();
    grouped.sort_by(|(left, _), (right, _)| left.cmp(right));
    grouped
}

fn print_grouped_callers(grouped: Vec<(&Path, Vec<&str>)>) {
    for (file, symbols) in grouped {
        println!("  {}:", file.display().to_string().white().bold());
        for symbol in symbols {
            println!("    {} {}", "•".dimmed(), symbol);
        }
    }
}

#[cfg(test)]
fn format_dependents(dependents: &[FileImpactDependent], empty_message: &str) -> Vec<String> {
    if dependents.is_empty() {
        return vec![format!("    {empty_message}")];
    }

    let mut lines = Vec::new();
    for dep in dependents.iter().take(MAX_DISPLAY_ITEMS) {
        lines.push(format!("    • {}", dep.file.display()));
    }

    if dependents.len() > MAX_DISPLAY_ITEMS {
        lines.push(format!(
            "    • ... and {} more",
            dependents.len() - MAX_DISPLAY_ITEMS
        ));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_dependent(path: &str) -> FileImpactDependent {
        FileImpactDependent {
            file: PathBuf::from(path),
            depth: 1,
        }
    }

    #[test]
    fn format_dependents_empty_shows_message() {
        let lines = format_dependents(&[], "No dependents found");
        assert_eq!(lines.len(), 1, "empty dependents should produce one line");
        assert!(
            lines[0].contains("No dependents found"),
            "should contain the empty message"
        );
    }

    #[test]
    fn format_dependents_single_item() {
        let deps = vec![make_dependent("src/main.rs")];
        let lines = format_dependents(&deps, "none");
        assert_eq!(lines.len(), 1, "single dependent should produce one line");
        assert!(
            lines[0].contains("src/main.rs"),
            "should contain the file path"
        );
    }

    #[test]
    fn format_dependents_at_max_shows_no_overflow() {
        let deps: Vec<_> = (0..MAX_DISPLAY_ITEMS)
            .map(|i| make_dependent(&format!("src/file_{i}.rs")))
            .collect();
        let lines = format_dependents(&deps, "none");
        assert_eq!(
            lines.len(),
            MAX_DISPLAY_ITEMS,
            "exactly MAX_DISPLAY_ITEMS should show no overflow line"
        );
        assert!(
            !lines.last().expect("should have lines").contains("more"),
            "should not contain overflow indicator"
        );
    }

    #[test]
    fn format_dependents_over_max_shows_overflow() {
        let count = MAX_DISPLAY_ITEMS + 5;
        let deps: Vec<_> = (0..count)
            .map(|i| make_dependent(&format!("src/file_{i}.rs")))
            .collect();
        let lines = format_dependents(&deps, "none");
        assert_eq!(
            lines.len(),
            MAX_DISPLAY_ITEMS + 1,
            "should have MAX items plus one overflow line"
        );
        let last = lines.last().expect("should have lines");
        assert!(
            last.contains("5 more"),
            "overflow line should show correct remaining count, got: {last}"
        );
    }
}
