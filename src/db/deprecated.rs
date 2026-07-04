//! Deprecated-callers analysis queries (tethys-jdly).
//!
//! Lists symbols carrying `#[deprecated]` with `since`/`note` parsed from
//! the raw `attributes.args` text, and joins each to its reference sites,
//! tiered by resolution trustworthiness. Sites are refs (calls, type uses) —
//! `use` statements importing a deprecated item are excluded by definition:
//! they vanish with their call sites during migration, and a call-less
//! deprecated import is already flagged by unused-imports. Design and
//! falsification table: `.tethys-jdly/design.md`.

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

/// Confidence tier for a reported deprecated-use site (design C5).
///
/// Tiering exists because Pass-2 name-only resolution fabricates edges for
/// ambiguous names (tethys-53iv): on real data (zbus 4.4.0), every
/// unique-name resolution matched rustc while every ambiguous one was a
/// phantom. Errors are suppressions, not accusations — Maybe means "verify
/// by hand", never "definitely calls deprecated code".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Tier {
    /// Every same-named symbol in the index is deprecated (uniqueness is the
    /// n=1 case): whichever candidate the ref really binds to, the site uses
    /// deprecated code.
    Definite,
    /// A same-named non-deprecated symbol exists, so name-only resolution
    /// could have misattributed this ref — or the ref is unresolved.
    Maybe,
}

/// How a site was associated with the deprecated symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Via {
    /// Pass-2-resolved reference (`refs.symbol_id` points at the symbol).
    Resolved,
    /// Unresolved reference whose qualified name ends in `::<symbol name>` —
    /// the `crate::`/`super::` shape Pass 2 declines (tethys-3i35). Always
    /// [`Tier::Maybe`].
    UnresolvedQualified,
}

/// One reference site of a deprecated symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallSite {
    /// Workspace-relative path of the referencing file.
    pub file: String,
    /// 1-based reference line.
    pub line: u32,
    /// 1-based reference column (tie-break for same-line sites).
    pub column: u32,
    /// Enclosing symbol name; `None` for top-level references (e.g. calls
    /// inside `#[cfg(test)] mod tests` items the extractor doesn't nest).
    pub caller: Option<String>,
    /// Confidence tier (see [`Tier`]).
    pub tier: Tier,
    /// Association mechanism (see [`Via`]).
    pub via: Via,
}

/// A deprecated symbol together with its (possibly empty) reference sites.
///
/// An empty `sites` vec is meaningful output, not absence: it is the
/// "clean — migration done" verdict (design C6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeprecatedFinding {
    /// The symbol carrying `#[deprecated]`.
    pub symbol: DeprecatedSymbol,
    /// Reference sites, ordered by (file, line, column).
    pub sites: Vec<CallSite>,
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

    /// Full deprecated-callers report: every deprecated symbol with its
    /// reference sites, tiered per [`Tier`].
    ///
    /// Two association paths:
    /// - resolved refs (`refs.symbol_id` = the symbol) — read from `refs`
    ///   directly, not `call_edges`, so top-level references
    ///   (`in_symbol_id NULL`, e.g. calls inside `#[cfg(test)] mod tests`)
    ///   are included; `populate_call_edges` skips those;
    /// - unresolved refs whose qualified `reference_name` ends with
    ///   `::<symbol name>` — the cross-file `crate::`/`super::` shape Pass 2
    ///   declines (tethys-3i35 / tethys-z9mr). Qualified-only by
    ///   measurement: on zbus 4.4.0 every bare unresolved name-match was
    ///   noise (36/36 refuted by rustc). A qualified match whose last
    ///   segment fits several deprecated symbols is attached to each —
    ///   honest under Maybe semantics ("possibly calls this one").
    pub fn get_deprecated_callers(&self) -> Result<Vec<DeprecatedFinding>> {
        let symbols = self.get_deprecated_symbols()?;
        let conn = self.connection()?;

        // Names shared with at least one NON-deprecated symbol: sites on
        // these names tier Maybe (a phantom binding is possible). One
        // statement, no per-symbol round-trips.
        let mut ambiguous_stmt = conn.prepare(
            "WITH deprecated_ids AS (SELECT symbol_id FROM attributes
                                     WHERE name = 'deprecated')
             SELECT DISTINCT s2.name
             FROM symbols s2
             WHERE s2.name IN (SELECT s.name
                               FROM symbols s
                               JOIN deprecated_ids d ON d.symbol_id = s.id)
               AND s2.id NOT IN (SELECT symbol_id FROM deprecated_ids)",
        )?;
        let ambiguous_names: std::collections::HashSet<String> = ambiguous_stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<_, _>>()?;

        let mut sites_stmt = conn.prepare(
            "SELECT f.path, r.line, r.column, cs.name
             FROM refs r
             JOIN files f ON f.id = r.file_id
             LEFT JOIN symbols cs ON cs.id = r.in_symbol_id
             WHERE r.symbol_id = ?1
             ORDER BY f.path, r.line, r.column",
        )?;

        // Path B: one pass over unresolved refs (partial index
        // idx_refs_unresolved), matching the qualified name's last segment
        // against deprecated symbol names in a hash map — O(u + d), never
        // the O(d × u) nested LIKE join.
        let mut by_name: std::collections::HashMap<&str, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, symbol) in symbols.iter().enumerate() {
            by_name.entry(symbol.name.as_str()).or_default().push(i);
        }
        let mut unresolved_stmt = conn.prepare(
            "SELECT r.reference_name, f.path, r.line, r.column, cs.name
             FROM refs r
             JOIN files f ON f.id = r.file_id
             LEFT JOIN symbols cs ON cs.id = r.in_symbol_id
             WHERE r.symbol_id IS NULL
               AND r.reference_name LIKE '%::%'
             ORDER BY f.path, r.line, r.column",
        )?;
        let mut recovered: Vec<Vec<CallSite>> = vec![Vec::new(); symbols.len()];
        let unresolved_rows = unresolved_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        for row in unresolved_rows {
            let (reference_name, file, line, column, caller) = row?;
            let Some(last_segment) = reference_name.rsplit("::").next() else {
                continue;
            };
            let Some(indices) = by_name.get(last_segment) else {
                continue;
            };
            for &i in indices {
                recovered[i].push(CallSite {
                    file: file.clone(),
                    line,
                    column,
                    caller: caller.clone(),
                    tier: Tier::Maybe,
                    via: Via::UnresolvedQualified,
                });
            }
        }

        let mut findings = Vec::with_capacity(symbols.len());
        for (symbol, path_b_sites) in symbols.into_iter().zip(recovered) {
            let tier = if ambiguous_names.contains(&symbol.name) {
                Tier::Maybe
            } else {
                Tier::Definite
            };
            let mut sites = sites_stmt
                .query_map([symbol.symbol_id], |row| {
                    Ok(CallSite {
                        file: row.get(0)?,
                        line: row.get(1)?,
                        column: row.get(2)?,
                        caller: row.get(3)?,
                        tier,
                        via: Via::Resolved,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            sites.extend(path_b_sites);
            sites.sort_by(|a, b| {
                (&a.file, a.line, a.column, a.via).cmp(&(&b.file, b.line, b.column, b.via))
            });
            findings.push(DeprecatedFinding { symbol, sites });
        }
        Ok(findings)
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
