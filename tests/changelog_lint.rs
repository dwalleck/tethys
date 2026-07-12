//! Format fences for `changelog.d/` release-note fragments.
//!
//! Every PR ships a fragment named `<slug>.<category>.md`; the `changelog`
//! CI job enforces *presence* (only CI can see the PR diff), this test
//! enforces *shape* — filenames parse, the category is a real
//! Keep-a-Changelog section, and bodies stay short, bullet-only, and
//! user-facing. `scripts/changelog-release.sh` consumes everything
//! validated here into `CHANGELOG.md` at release time, so between releases
//! `changelog.d/` is the unreleased changelog. See `changelog.d/README.md`
//! for the format spec.

use std::fs;
use std::path::PathBuf;

/// Keep-a-Changelog section names, lowercase, as used in fragment filenames.
const CATEGORIES: [&str; 6] = [
    "added",
    "changed",
    "deprecated",
    "removed",
    "fixed",
    "security",
];

/// Fragments hold at most this many non-blank lines — they are release
/// notes, not commit narration (the PR body carries the long story).
const MAX_LINES: usize = 10;

/// Every fragment in `changelog.d/` — everything except `README.md` and
/// hidden entries (untracked filesystem noise like `.DS_Store` or editor
/// swap files must not fail the fence) — failing on any other non-file
/// entry: the directory stays flat.
fn fragment_paths() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("changelog.d");
    let mut paths = Vec::new();
    for entry in fs::read_dir(&dir).expect("changelog.d/ must exist at the repo root") {
        let path = entry.expect("readable changelog.d/ entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("UTF-8 filename in changelog.d/");
        if name.starts_with('.') || name == "README.md" {
            continue;
        }
        assert!(
            path.is_file(),
            "changelog.d/ must stay flat — unexpected non-file entry: {}",
            path.display()
        );
        paths.push(path);
    }
    paths
}

/// Filenames are `<slug>.<category>.md`: the slug is lowercase
/// alphanumeric-plus-hyphen (normally the rivets issue ID, e.g.
/// `tethys-53iv`), and the category is one of the six Keep-a-Changelog
/// sections.
#[test]
fn fragment_filenames_are_slug_category_md() {
    for path in fragment_paths() {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("UTF-8 filename");
        let parts: Vec<&str> = name.split('.').collect();
        assert!(
            parts.len() == 3 && parts[2] == "md",
            "changelog.d/{name}: expected <slug>.<category>.md"
        );
        let (slug, category) = (parts[0], parts[1]);
        let slug_ok = !slug.is_empty()
            && !slug.starts_with('-')
            && slug
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        assert!(
            slug_ok,
            "changelog.d/{name}: slug '{slug}' must be lowercase \
             alphanumeric/hyphen (use the rivets ID, e.g. tethys-53iv)"
        );
        assert!(
            CATEGORIES.contains(&category),
            "changelog.d/{name}: category '{category}' is not one of {CATEGORIES:?}"
        );
    }
}

/// Bodies are 1-5 markdown bullets (`- `), each optionally continued on
/// two-space-indented lines, and at most `MAX_LINES` non-blank lines total.
#[test]
fn fragment_bodies_are_short_bullet_lists() {
    for path in fragment_paths() {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("UTF-8 filename");
        let body = fs::read_to_string(&path).expect("readable fragment");
        let lines: Vec<&str> = body
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        assert!(!lines.is_empty(), "changelog.d/{name}: fragment is empty");
        assert!(
            lines.len() <= MAX_LINES,
            "changelog.d/{name}: {} non-blank lines — fragments are short \
             release notes; put the long story in the PR body",
            lines.len()
        );
        let bullets = lines.iter().filter(|line| line.starts_with("- ")).count();
        assert!(
            (1..=5).contains(&bullets),
            "changelog.d/{name}: {bullets} bullets — write 1-5 lines starting with '- '"
        );
        for line in &lines {
            assert!(
                line.starts_with("- ") || line.starts_with("  "),
                "changelog.d/{name}: line {line:?} is neither a bullet ('- ') \
                 nor a two-space-indented continuation"
            );
        }
    }
}
