//! Deprecated-callers analysis queries (tethys-jdly).
//!
//! Lists symbols carrying `#[deprecated]` and parses the attribute's
//! `since`/`note` payload out of the raw `attributes.args` text. Reference
//! sites and tiering build on this in later slices. Design and falsification
//! table: `.tethys-jdly/design.md`.

use serde::Serialize;
use tracing::trace;

use super::Index;
use crate::error::Result;

/// A symbol carrying a `#[deprecated]` attribute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeprecatedSymbol {
    /// Internal symbol id, used to join reference sites; not part of output.
    #[serde(skip)]
    pub(crate) symbol_id: i64,
    /// Symbol name as declared.
    pub name: String,
    /// Raw `symbols.kind` text (display-only; no dispatch happens on it).
    pub kind: String,
    /// Workspace-relative path of the declaring file.
    pub file: String,
    /// 1-based declaration line.
    pub line: u32,
    /// `since` value from the attribute, when present.
    pub since: Option<String>,
    /// `note` value (or the whole name-value string) from the attribute.
    pub note: Option<String>,
}

impl Index {
    /// All symbols carrying a `#[deprecated]` attribute, ordered by
    /// (file, line, name) for deterministic output.
    ///
    /// Detection is kind-agnostic: any symbol row joined by an attribute row
    /// named `deprecated` qualifies (fn, method, struct, enum variant, ...).
    pub fn get_deprecated_symbols(&self) -> Result<Vec<DeprecatedSymbol>> {
        trace!("Querying deprecated symbols");
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT s.id, s.name, s.kind, f.path, s.line, a.args
             FROM attributes a
             JOIN symbols s ON s.id = a.symbol_id
             JOIN files f ON f.id = s.file_id
             WHERE a.name = 'deprecated'
             ORDER BY f.path, s.line, s.name",
        )?;
        let rows = stmt.query_map([], |row| {
            let args: Option<String> = row.get(5)?;
            let (since, note) = parse_deprecation_args(args.as_deref());
            Ok(DeprecatedSymbol {
                symbol_id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                file: row.get(3)?,
                line: row.get(4)?,
                since,
                note,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

/// Parse a `#[deprecated]` attribute's raw args text into `(since, note)`.
///
/// The stored `attributes.args` takes one of three source shapes:
/// - `None` — bare `#[deprecated]` → `(None, None)`
/// - a bare string literal — `#[deprecated = "msg"]` name-value RHS → note
/// - a key-value list — `since = "..", note = ".."`, either order
///
/// Values lose their surrounding quotes and have `\"`, `\\`, `\n` unescaped
/// (the escapes rustc renders in its own deprecation warnings). Raw-string
/// literals (`r#".."#`) pass through verbatim — display-only degradation,
/// never wrong attribution. Total function: unrecognized shapes degrade to
/// `(None, None)`, never error.
pub(crate) fn parse_deprecation_args(args: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(raw) = args else {
        return (None, None);
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return (None, None);
    }
    if raw.starts_with('"') || raw.starts_with("r\"") || raw.starts_with("r#") {
        return (None, Some(unquote(raw)));
    }
    let mut since = None;
    let mut note = None;
    for part in split_top_level_commas(raw) {
        let part = part.trim();
        if let Some(value) = key_value(part, "since") {
            since = Some(value);
        } else if let Some(value) = key_value(part, "note") {
            note = Some(value);
        }
    }
    (since, note)
}

/// `key = "value"` → unquoted value, iff `part` starts with exactly `key`.
fn key_value(part: &str, key: &str) -> Option<String> {
    let rest = part.strip_prefix(key)?;
    let rest = rest.trim_start();
    let value = rest.strip_prefix('=')?;
    Some(unquote(value.trim()))
}

/// Split on commas outside string literals (`,` inside `"..."` is content,
/// not a separator — `note = "a, b"` is one part).
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        } else if c == ',' {
            parts.push(&s[start..i]);
            start = i + 1;
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Strip surrounding double quotes and unescape `\"`, `\\`, `\n`.
/// Non-`"..."` input (raw strings, unquoted tokens) is returned verbatim.
fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|t| t.strip_suffix('"')) else {
        return s.to_string();
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some('\\') | None => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse_deprecation_args;

    #[allow(clippy::unnecessary_wraps)] // table rows want uniform Option cells
    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn parses_all_deprecated_args_shapes() {
        // (input, expected since, expected note)
        let cases: &[(Option<&str>, Option<String>, Option<String>)] = &[
            (None, None, None),
            (Some(""), None, None),
            // bare string literal (name-value RHS, stored verbatim by S1)
            (
                Some(r#""use new_eq instead""#),
                None,
                s("use new_eq instead"),
            ),
            (
                Some(r#""uses :: and \" quote and déjà vu""#),
                None,
                s(r#"uses :: and " quote and déjà vu"#),
            ),
            // single keys
            (Some(r#"since = "1.0.0""#), s("1.0.0"), None),
            (Some(r#"note = "gone in 2.0""#), None, s("gone in 2.0")),
            (Some(r#"since="1.0""#), s("1.0"), None),
            // both keys, both orders
            (
                Some(r#"since = "4.0.0", note = "Use `message::Builder` instead""#),
                s("4.0.0"),
                s("Use `message::Builder` instead"),
            ),
            (Some(r#"note = "n", since = "2.1""#), s("2.1"), s("n")),
            // STRESS (pre-written in plan S2): comma and fake key INSIDE the
            // string must not split or leak — kills split-on-comma parsers.
            (
                Some(r#"note = "a, b, since = fake""#),
                None,
                s("a, b, since = fake"),
            ),
            // escaped quote inside a keyed value
            (Some(r#"note = "say \"hi\"""#), None, s(r#"say "hi""#)),
            // multi-line (zbus shape): keys separated by newline after comma
            (
                Some("since = \"4.0.0\",\n        note = \"Use x\""),
                s("4.0.0"),
                s("Use x"),
            ),
            // prefix key must not match: `notes` is not `note`
            (Some(r#"notes = "x""#), None, None),
            // unrecognized shape degrades, never errors
            (Some("whatever"), None, None),
        ];
        for (input, since, note) in cases {
            let got = parse_deprecation_args(*input);
            assert_eq!(
                &got.0, since,
                "since mismatch for input {input:?} (got {got:?})"
            );
            assert_eq!(
                &got.1, note,
                "note mismatch for input {input:?} (got {got:?})"
            );
        }
    }
}
