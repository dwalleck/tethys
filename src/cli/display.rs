//! Common display utilities for CLI commands.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use colored::Colorize;
use tethys::{Caller, Dependent, SymbolImpactCaller};

const MAX_DISPLAY_ITEMS: usize = 10;

/// Display a list of dependent files with optional truncation.
///
/// Shows up to `MAX_DISPLAY_ITEMS` files with bullet points. If there are more,
/// shows "... and N more". If empty, shows the provided `empty_message`.
pub fn print_dependents(dependents: &[Dependent], empty_message: &str) {
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
fn format_dependents(dependents: &[Dependent], empty_message: &str) -> Vec<String> {
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
fn format_callers_by_file(callers: &[Dependent]) -> Vec<String> {
    let grouped = group_callers(callers.iter().flat_map(|caller| {
        caller
            .symbols_used
            .iter()
            .map(move |symbol| (caller.file.as_path(), symbol.as_str()))
    }));

    let mut lines = Vec::new();
    for (file, symbols) in grouped {
        lines.push(format!("  {}:", file.display()));
        for symbol in symbols {
            lines.push(format!("    • {symbol}"));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_dependent(path: &str, symbols: &[&str]) -> Dependent {
        Dependent {
            file: PathBuf::from(path),
            symbols_used: symbols.iter().copied().map(str::to_string).collect(),
            line_count: 1,
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
        let deps = vec![make_dependent("src/main.rs", &["foo"])];
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
            .map(|i| make_dependent(&format!("src/file_{i}.rs"), &[]))
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
            .map(|i| make_dependent(&format!("src/file_{i}.rs"), &[]))
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

    #[test]
    fn format_callers_by_file_empty() {
        let lines = format_callers_by_file(&[]);
        assert!(lines.is_empty(), "no callers should produce no lines");
    }

    #[test]
    fn format_callers_by_file_groups_and_sorts() {
        let callers = vec![
            make_dependent("src/b.rs", &["zeta", "alpha"]),
            make_dependent("src/a.rs", &["gamma"]),
            make_dependent("src/b.rs", &["alpha", "beta"]),
        ];
        let lines = format_callers_by_file(&callers);

        // src/a.rs should come first (sorted)
        assert!(
            lines[0].contains("src/a.rs"),
            "first file should be src/a.rs, got: {}",
            lines[0]
        );
        assert!(
            lines[1].contains("gamma"),
            "src/a.rs should have gamma symbol"
        );

        // src/b.rs should come second with deduplicated, sorted symbols
        assert!(
            lines[2].contains("src/b.rs"),
            "second file should be src/b.rs, got: {}",
            lines[2]
        );
        // Symbols should be alpha, beta, zeta (sorted, deduplicated)
        assert!(
            lines[3].contains("alpha"),
            "first symbol of src/b.rs should be alpha"
        );
        assert!(
            lines[4].contains("beta"),
            "second symbol of src/b.rs should be beta"
        );
        assert!(
            lines[5].contains("zeta"),
            "third symbol of src/b.rs should be zeta"
        );
        assert_eq!(lines.len(), 6, "should have 2 file headers + 4 symbols");
    }

    #[test]
    fn format_callers_deduplicates_symbols() {
        let callers = vec![
            make_dependent("src/a.rs", &["foo", "foo", "bar"]),
            make_dependent("src/a.rs", &["foo"]),
        ];
        let lines = format_callers_by_file(&callers);
        // Header + 2 unique symbols (bar, foo)
        assert_eq!(
            lines.len(),
            3,
            "should have 1 file header + 2 unique symbols, got: {lines:?}"
        );
    }
}
