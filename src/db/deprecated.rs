//! Deprecated-callers analysis queries (tethys-jdly; C# parity tethys-haw5).
//!
//! Lists symbols carrying Rust `#[deprecated]` (`since`/`note` parsed from
//! the raw `attributes.args` text) or C# `[Obsolete]` (message/error flag),
//! and joins each to its reference sites, tiered by resolution
//! trustworthiness. Sites are refs (calls, type uses) — `use` statements
//! importing a deprecated item are excluded by definition: they vanish with
//! their call sites during migration, and a call-less deprecated import is
//! already flagged by unused-imports. Design and falsification tables:
//! `.tethys-jdly/design.md`, `.tethys-haw5/design.md`.

use serde::Serialize;
use tracing::trace;

use super::Index;
use crate::error::Result;

/// A symbol carrying a Rust `#[deprecated]` or C# `[Obsolete]` attribute.
///
/// The JSON key set is identical across languages (design C10): `since` is
/// always null for C#, `error` always null for Rust — both serialize as
/// explicit nulls rather than vanishing, so downstream consumers see one
/// stable shape.
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
    /// `since` value from the attribute, when present (Rust only).
    pub since: Option<String>,
    /// `note` value from the attribute — Rust `note = ".."` / name-value
    /// string, or the C# `[Obsolete]` message.
    pub note: Option<String>,
    /// C# `[Obsolete]` error flag: `Some(true)` means use sites are compile
    /// errors, not warnings. `None` for Rust and for `[Obsolete]` without a
    /// bool argument.
    pub error: Option<bool>,
    /// Declaring file's `files.language` value; drives the same-language
    /// guards on Path B and ambiguity tiering (design C9). Not output.
    #[serde(skip)]
    pub(crate) language: String,
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
pub struct ReferenceSite {
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
    pub sites: Vec<ReferenceSite>,
}

/// SQL literal listing every attribute name that marks a symbol deprecated:
/// Rust `#[deprecated]` plus the four C# `[Obsolete]` spellings. Attribute
/// names are stored as written by the extractors; spelling variants are
/// matched here at query time (design C5 — exact names, never substrings,
/// so a custom `NotObsolete` attribute can't false-positive).
const DEPRECATION_ATTR_NAMES_SQL: &str = "('deprecated', 'Obsolete', 'ObsoleteAttribute', \
     'System.Obsolete', 'System.ObsoleteAttribute')";

impl Index {
    /// All symbols carrying a Rust `#[deprecated]` or C# `[Obsolete]`
    /// attribute, ordered by (file, line, name) for deterministic output.
    ///
    /// Detection is kind-agnostic: any symbol row joined by a matching
    /// attribute row qualifies (fn, method, struct, enum variant, class, ...).
    /// Args parsing dispatches on the attribute name — `deprecated` uses the
    /// Rust key-value grammar, the `Obsolete` spellings use the C# positional/
    /// named grammar. Both parsers are total, so a mis-dispatch degrades to
    /// nulls, never to wrong attribution.
    pub fn get_deprecated_symbols(&self) -> Result<Vec<DeprecatedSymbol>> {
        trace!("Querying deprecated symbols");
        let conn = self.connection()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT s.id, s.name, s.kind, f.path, s.line, a.args, a.name, f.language
             FROM attributes a
             JOIN symbols s ON s.id = a.symbol_id
             JOIN files f ON f.id = s.file_id
             WHERE a.name IN {DEPRECATION_ATTR_NAMES_SQL}
             ORDER BY f.path, s.line, s.name",
        ))?;
        let rows = stmt.query_map([], |row| {
            let args: Option<String> = row.get(5)?;
            let attr_name: String = row.get(6)?;
            let (since, note, error) = if attr_name == "deprecated" {
                let (since, note) = parse_deprecation_args(args.as_deref());
                (since, note, None)
            } else {
                let (note, error) = parse_obsolete_args(args.as_deref());
                (None, note, error)
            };
            Ok(DeprecatedSymbol {
                symbol_id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                file: row.get(3)?,
                line: row.get(4)?,
                since,
                note,
                error,
                language: row.get(7)?,
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

        // (name, language) pairs shared with at least one NON-deprecated
        // symbol OF THE SAME LANGUAGE: sites on these tier Maybe (a phantom
        // binding is possible). Language-scoped per design C9 — a C# method
        // named like a Rust fn can't be what a Rust ref binds to, so it must
        // not demote the Rust finding (and vice versa). One statement, no
        // per-symbol round-trips.
        let mut ambiguous_stmt = conn.prepare(&format!(
            "WITH deprecated_ids AS (SELECT symbol_id FROM attributes
                                     WHERE name IN {DEPRECATION_ATTR_NAMES_SQL})
             SELECT DISTINCT s2.name, f2.language
             FROM symbols s2
             JOIN files f2 ON f2.id = s2.file_id
             WHERE s2.name IN (SELECT s.name
                               FROM symbols s
                               JOIN deprecated_ids d ON d.symbol_id = s.id)
               AND s2.id NOT IN (SELECT symbol_id FROM deprecated_ids)",
        ))?;
        let ambiguous_names: std::collections::HashSet<(String, String)> = ambiguous_stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
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
            "SELECT r.reference_name, f.path, r.line, r.column, cs.name, f.language
             FROM refs r
             JOIN files f ON f.id = r.file_id
             LEFT JOIN symbols cs ON cs.id = r.in_symbol_id
             WHERE r.symbol_id IS NULL
               AND r.reference_name LIKE '%::%'
             ORDER BY f.path, r.line, r.column",
        )?;
        let mut recovered: Vec<Vec<ReferenceSite>> = vec![Vec::new(); symbols.len()];
        let unresolved_rows = unresolved_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        for row in unresolved_rows {
            let (reference_name, file, line, column, caller, ref_language) = row?;
            let Some(last_segment) = reference_name.rsplit("::").next() else {
                continue;
            };
            let Some(indices) = by_name.get(last_segment) else {
                continue;
            };
            for &i in indices {
                // Same-language guard (design C9): a ref can only bind a
                // symbol its own language could reach — a Rust `crate::Run`
                // must not attach to a C# `Run` and vice versa.
                if symbols[i].language != ref_language {
                    continue;
                }
                recovered[i].push(ReferenceSite {
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
            // Tuple lookup, not clone-and-build: (name, language) borrows
            // suffice via the set's Borrow impl only for owned tuples, so
            // build the key once per symbol (d is small — tens, not 10^5).
            let key = (symbol.name.clone(), symbol.language.clone());
            let tier = if ambiguous_names.contains(&key) {
                Tier::Maybe
            } else {
                Tier::Definite
            };
            let mut sites = sites_stmt
                .query_map([symbol.symbol_id], |row| {
                    Ok(ReferenceSite {
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

/// Parse a C# `[Obsolete]` attribute's raw args text into `(note, error)`.
///
/// The stored `attributes.args` takes these source shapes:
/// - `None` — bare `[Obsolete]` → `(None, None)`
/// - positional: `"msg"` / `"msg", true` / `"msg", false`
/// - named C# attribute arguments: `message: "msg"`, `error: true`
/// - newer property-assign args (`DiagnosticId = ".."`, `UrlFormat = ".."`)
///   stay preserved in the raw args column but are never surfaced here
///
/// The first string literal (positional or `message:`) becomes the note —
/// quotes stripped, `\"`/`\\`/`\n` unescaped; verbatim strings (`@".."`)
/// pass through raw, display-only degradation, never wrong attribution
/// (same posture as Rust `r#".."#` in [`parse_deprecation_args`]). The
/// first bool literal (positional or `error:`) becomes the error flag.
/// Total function: unrecognized shapes degrade to `(None, None)`.
pub(crate) fn parse_obsolete_args(args: Option<&str>) -> (Option<String>, Option<bool>) {
    let Some(raw) = args else {
        return (None, None);
    };
    let mut note = None;
    let mut error = None;
    for part in split_top_level_commas(raw.trim()) {
        let mut part = part.trim();
        if let Some(rest) = part.strip_prefix("message:") {
            part = rest.trim_start();
        } else if let Some(rest) = part.strip_prefix("error:") {
            part = rest.trim_start();
        }
        if note.is_none() && (part.starts_with('"') || part.starts_with("@\"")) {
            note = Some(unquote(part));
        } else if error.is_none() && (part == "true" || part == "false") {
            error = Some(part == "true");
        }
    }
    (note, error)
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
    use super::parse_obsolete_args;

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

    /// haw5 plan S3 stress fixture — attribute rows inserted DIRECTLY (no C#
    /// parsing), so this test is independent of the extractor. Kills:
    /// substring matching (`NotObsolete` decoy), exact-'Obsolete'-only
    /// matching (three variant spellings), parser dispatch corrupting Rust
    /// since/note, serde skipping the error key.
    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "the fixture IS the test: every spelling and decoy asserted \
                  against one directly-inserted attribute set"
    )]
    fn detects_obsolete_spellings_and_decoys() {
        use crate::db::{Index, SymbolData};
        use crate::languages::common::ExtractedAttribute;
        use crate::types::{Language, SymbolKind, Visibility};
        use std::path::Path;

        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open index");

        let attr = |name: &str, args: Option<&str>, line: u32| {
            vec![ExtractedAttribute {
                name: name.to_string(),
                args: args.map(String::from),
                line,
            }]
        };
        // (symbol name, attribute rows) — one symbol per spelling + decoys.
        let rows = [
            ("cs_bare", attr("Obsolete", None, 1)),
            (
                "cs_attr",
                attr("ObsoleteAttribute", Some(r#""m", true"#), 2),
            ),
            ("cs_sys", attr("System.Obsolete", Some(r#""x""#), 3)),
            (
                "cs_sysattr",
                attr("System.ObsoleteAttribute", Some("error: true"), 4),
            ),
            ("decoy_custom", attr("NotObsolete", Some(r#""boom""#), 5)),
            ("decoy_marker", attr("Serializable", None, 6)),
        ];
        let symbols: Vec<SymbolData<'_>> = rows
            .iter()
            .enumerate()
            .map(|(i, (name, attrs))| SymbolData {
                name,
                module_path: "",
                qualified_name: name,
                kind: SymbolKind::Function,
                line: u32::try_from(i).expect("small") + 1,
                column: 1,
                span: None,
                signature: None,
                visibility: Visibility::Public,
                parent_symbol_id: None,
                is_test: false,
                attributes: attrs,
            })
            .collect();
        index
            .index_parsed_file_atomic(
                Path::new("src/Legacy.cs"),
                Language::CSharp,
                1,
                1,
                None,
                &symbols,
                &[],
                &[],
            )
            .expect("write C# file");

        let rust_attrs = attr("deprecated", Some(r#"since = "1.0", note = "n""#), 1);
        let rust_sym = SymbolData {
            name: "rs_old",
            module_path: "",
            qualified_name: "rs_old",
            kind: SymbolKind::Function,
            line: 1,
            column: 1,
            span: None,
            signature: None,
            visibility: Visibility::Public,
            parent_symbol_id: None,
            is_test: false,
            attributes: &rust_attrs,
        };
        index
            .index_parsed_file_atomic(
                Path::new("src/old.rs"),
                Language::Rust,
                1,
                1,
                None,
                &[rust_sym],
                &[],
                &[],
            )
            .expect("write Rust file");

        let found = index.get_deprecated_symbols().expect("query");
        let names: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
        // Ordered by (file, line, name): Legacy.cs rows then old.rs.
        assert_eq!(
            names,
            ["cs_bare", "cs_attr", "cs_sys", "cs_sysattr", "rs_old"],
            "exactly the four Obsolete spellings + Rust deprecated; decoys never"
        );

        let by_name = |n: &str| found.iter().find(|s| s.name == n).expect("present");
        let cs_bare = by_name("cs_bare");
        assert_eq!(
            (
                cs_bare.since.as_deref(),
                cs_bare.note.as_deref(),
                cs_bare.error
            ),
            (None, None, None)
        );
        let cs_attr = by_name("cs_attr");
        assert_eq!(
            (
                cs_attr.since.as_deref(),
                cs_attr.note.as_deref(),
                cs_attr.error
            ),
            (None, Some("m"), Some(true))
        );
        let cs_sys = by_name("cs_sys");
        assert_eq!(
            (
                cs_sys.since.as_deref(),
                cs_sys.note.as_deref(),
                cs_sys.error
            ),
            (None, Some("x"), None)
        );
        let cs_sysattr = by_name("cs_sysattr");
        assert_eq!(
            (
                cs_sysattr.since.as_deref(),
                cs_sysattr.note.as_deref(),
                cs_sysattr.error
            ),
            (None, None, Some(true))
        );
        // Rust dispatch untouched by the C# parser (kills dispatch corruption).
        let rs_old = by_name("rs_old");
        assert_eq!(
            (
                rs_old.since.as_deref(),
                rs_old.note.as_deref(),
                rs_old.error
            ),
            (Some("1.0"), Some("n"), None)
        );

        // Design C10: identical key set across languages, error serialized
        // even when null.
        for symbol in &found {
            let value = serde_json::to_value(symbol).expect("serialize");
            let mut keys: Vec<&str> = value
                .as_object()
                .expect("object")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            assert_eq!(
                keys,
                ["error", "file", "kind", "line", "name", "note", "since"]
            );
        }
    }

    /// haw5 plan S2 stress table — expected values pre-written in the plan.
    /// Kills: naive comma split (comma+bool inside string), any-second-arg
    /// treated as true (explicit false row), named-only or positional-only
    /// parsing, escape mangling.
    #[test]
    fn parses_all_obsolete_args_shapes() {
        #[allow(clippy::unnecessary_wraps)] // table rows want uniform cells
        fn s(v: &str) -> Option<String> {
            Some(v.to_string())
        }
        // (input, expected note, expected error flag)
        let cases: &[(Option<&str>, Option<String>, Option<bool>)] = &[
            (None, None, None),
            (Some(""), None, None),
            (Some(r#""m""#), s("m"), None),
            (Some(r#""m", true"#), s("m"), Some(true)),
            // explicit false must surface as false, not collapse to None
            (Some(r#""m", false"#), s("m"), Some(false)),
            (Some(r#"message: "m", error: true"#), s("m"), Some(true)),
            // comma AND bool inside the string must not split or leak
            (Some(r#""a, true, b""#), s("a, true, b"), None),
            (Some(r#""say \"hi\"""#), s(r#"say "hi""#), None),
            // verbatim strings pass through raw (display-only degradation)
            (Some(r#"@"C:\path""#), s(r#"@"C:\path""#), None),
            // bool without a message is legal to parse
            (Some("true"), None, Some(true)),
            (Some(r#""déjà vu 🦀""#), s("déjà vu 🦀"), None),
            // newer named args are preserved-but-ignored, never note
            (Some(r#""m", DiagnosticId = "X123""#), s("m"), None),
            // unrecognized shape degrades, never errors
            (Some("whatever"), None, None),
        ];
        for (input, note, error) in cases {
            let got = parse_obsolete_args(*input);
            assert_eq!(
                &got.0, note,
                "note mismatch for input {input:?} (got {got:?})"
            );
            assert_eq!(
                &got.1, error,
                "error-flag mismatch for input {input:?} (got {got:?})"
            );
        }
    }
}
