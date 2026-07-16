//! `tethys hierarchy` command implementation.

use std::fmt::Write as _;
use std::path::Path;

use colored::Colorize;
use tethys::{HierarchyDirection, HierarchyNode, Tethys, TypeHierarchy};
use tracing::debug;

/// Run the hierarchy command: walk `inherit` edges up (supertypes) and/or
/// down (subtypes) from the named type. External supertypes (std traits)
/// appear as name-only entries.
pub fn run(
    workspace: &Path,
    symbol: &str,
    direction: &str,
    json: bool,
) -> Result<(), tethys::Error> {
    let dir = match direction {
        "up" => HierarchyDirection::Up,
        "down" => HierarchyDirection::Down,
        _ => HierarchyDirection::Both,
    };
    debug!(workspace = %workspace.display(), symbol, "Opening tethys index");
    let tethys = Tethys::new(workspace)?;
    let hierarchy = tethys.get_type_hierarchy(symbol, dir)?;

    let rendered = if json {
        render_json(&hierarchy)?
    } else {
        render_human(&hierarchy, dir)
    };
    super::write_report(&rendered)
}

/// Render as pretty-printed JSON (data → stdout).
fn render_json(hierarchy: &TypeHierarchy) -> Result<String, tethys::Error> {
    #[derive(serde::Serialize)]
    struct JsonOutput<'a> {
        summary: JsonSummary,
        hierarchy: &'a TypeHierarchy,
    }
    #[derive(serde::Serialize)]
    struct JsonSummary {
        supertypes: usize,
        subtypes: usize,
    }
    let output = JsonOutput {
        summary: JsonSummary {
            supertypes: hierarchy.up.len(),
            subtypes: hierarchy.down.len(),
        },
        hierarchy,
    };
    super::to_json_pretty(&output, "hierarchy")
}

fn write_nodes(buf: &mut String, nodes: &[HierarchyNode]) {
    for node in nodes {
        let indent = "  ".repeat(node.depth as usize);
        let loc = match (&node.file, node.line) {
            (Some(f), Some(l)) => format!(" — {f}:{l}"),
            _ => " — external".to_string(),
        };
        let kind = node.kind.as_deref().unwrap_or("?");
        let _ = writeln!(
            buf,
            "{indent}{} {}{}",
            kind.dimmed(),
            node.name.yellow(),
            loc.dimmed()
        );
    }
}

/// Render in human-readable format (data → stdout).
fn render_human(hierarchy: &TypeHierarchy, dir: HierarchyDirection) -> String {
    let mut buf = String::new();
    let _ = writeln!(buf, "{}", "Type Hierarchy".cyan().bold());
    let _ = writeln!(buf, "{}", "=".repeat(63).dimmed());
    let _ = writeln!(buf);
    let _ = writeln!(buf, "{}", hierarchy.name.white().bold());

    if matches!(dir, HierarchyDirection::Up | HierarchyDirection::Both) {
        let _ = writeln!(buf);
        let _ = writeln!(
            buf,
            "{} ({}):",
            "Supertypes".white().bold(),
            hierarchy.up.len()
        );
        if hierarchy.up.is_empty() {
            let _ = writeln!(buf, "  {}", "none".dimmed());
        }
        write_nodes(&mut buf, &hierarchy.up);
    }
    if matches!(dir, HierarchyDirection::Down | HierarchyDirection::Both) {
        let _ = writeln!(buf);
        let _ = writeln!(
            buf,
            "{} ({}):",
            "Subtypes".white().bold(),
            hierarchy.down.len()
        );
        if hierarchy.down.is_empty() {
            let _ = writeln!(buf, "  {}", "none".dimmed());
        }
        write_nodes(&mut buf, &hierarchy.down);
    }
    buf
}
