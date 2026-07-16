//! Dead-code analysis (tethys-dvsw), facade layer: the textual tier
//! decision and report assembly over the db-layer evidence funnel
//! (`db::dead_code`).
//!
//! # Semantics
//!
//! A finding is a non-public, non-test symbol with **zero inbound
//! evidence** — no resolved reference from outside itself (any confidence
//! band, speculative included), no unresolved reference matching its name,
//! no trait-impl `inherit` marker, no live descendant (containers), not an
//! entry point. The tier is a textual verdict:
//!
//! - [`Tier::Definite`]: the name has **zero word-boundary occurrences**
//!   anywhere in the indexed corpus outside the symbol's own definition
//!   span. On a warning-free workspace this tier is empty by construction
//!   (`rustc`'s `dead_code` lint would have flagged the symbol first) —
//!   fenced by the self-index CI test.
//! - [`Tier::Maybe`]: the name appears somewhere the reference extractor
//!   cannot see. Deleting these requires human verification.
//!
//! Errors are suppressions, not accusations: every known gap in reference
//! extraction demotes a finding to Maybe rather than letting it accuse.
//!
//! # Known false-positive sources (documented, deliberately not filtered)
//!
//! All of these produce zero-evidence candidates whose names usually still
//! appear textually, so they surface as Maybe, not Definite:
//!
//! - Method-shape calls inside macro arguments (`assert!(x.is_valid())`)
//!   emit no reference (tethys-9l27).
//! - Bare non-call identifiers in macro token trees
//!   (`criterion_group!(benches, bench_fn)`) emit no reference — only
//!   call-shaped tokens do (tethys-8ym0's `macro_call` covers those;
//!   nested/path shapes are tethys-7dqj and tethys-ewa7).
//! - Format-string captures (`format!("{NAME}")`) are string *content*,
//!   invisible to any token walk — only the textual scan sees them.
//! - `Type::assoc_fn` used as a value (`.map(Self::row_to_thing)`) emits
//!   no reference (tethys-i09d); plain fn-as-value gaps in struct-init /
//!   assignment / tuple positions are tethys-wbrh.
//! - Same-file name twins: the Pass-1 last-wins map binds every same-file
//!   call to ONE of N same-named symbols, starving the others
//!   (tethys-0aqj class; measured on the self-index's triple
//!   `seeded_index`).
//! - Functions defined inside macro invocations (`proptest!`) are not
//!   indexed at all (tethys-0nar) — their helpers' only callers are
//!   invisible symbols.
//!
//! A symbol referenced ONLY by tests reads alive (reference evidence is
//! reference evidence); the policy question of test-only liveness is
//! tethys-m7zm. C# method-level interface-implementation suppression
//! needs override resolution (tethys-3b06) — C# gets type-level `inherit`
//! edges only, and interface-impl method names textually match their
//! interface declaration, landing in Maybe.
//!
//! # Scan semantics
//!
//! Word-boundary matching with the identifier class `[A-Za-z0-9_]` — the
//! same boundary rule as `unused_imports::count_word_occurrences`,
//! reimplemented line-wise here because span exclusion needs line
//! attribution the whole-content counter cannot provide. Unicode
//! identifiers are out of scope (non-ASCII characters act as separators;
//! a boundary miss shifts a finding toward Maybe — the safe direction).
//! Comments and strings COUNT as mentions: textual means textual.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use tracing::{debug, warn};

use crate::db::Tier;
use crate::db::dead_code::ZeroEvidenceCandidate;
use crate::types::IndexedFile;

/// One dead-code candidate, tiered.
#[derive(Debug, Clone, Serialize)]
pub struct DeadCodeFinding {
    /// Symbol name as declared.
    pub name: String,
    /// Stored qualified name (`module::Name`), for disambiguation.
    pub qualified_name: String,
    /// Raw `symbols.kind` text (display-only).
    pub kind: String,
    /// Raw `symbols.visibility` text (`private`, `crate`, `module`).
    pub visibility: String,
    /// Workspace-relative declaring file path.
    pub file: String,
    /// 1-based declaration line.
    pub line: u32,
    /// Textual verdict: [`Tier::Definite`] = safe to treat as dead,
    /// [`Tier::Maybe`] = name appears somewhere; verify by hand.
    pub tier: Tier,
}

/// Counts a consumer needs to interpret the findings. Counts are always
/// the FULL population — a `limit` truncates [`DeadCodeReport::findings`],
/// never the summary.
#[derive(Debug, Clone, Serialize)]
pub struct DeadCodeSummary {
    /// Zero-evidence candidates evaluated (= definite + maybe).
    pub candidates: usize,
    /// Findings with zero textual occurrences.
    pub definite: usize,
    /// Findings whose name appears textually somewhere.
    pub maybe: usize,
}

/// Output of [`crate::Tethys::find_dead_code`]: findings sorted by
/// (file, line, name), plus full-population counts. Zero candidates is a
/// legitimately clean report (empty findings, zeroed summary) — unlike
/// untested-code there is no root-set precondition to make emptiness
/// indeterminate; the candidate population itself is the subject.
#[derive(Debug, Clone, Serialize)]
pub struct DeadCodeReport {
    /// Tiered findings, truncated to `limit` when one was given.
    pub findings: Vec<DeadCodeFinding>,
    /// Full-population counts (never truncated).
    pub summary: DeadCodeSummary,
}

/// Assemble the report: run the textual scan over the indexed corpus,
/// tier each candidate, truncate to `limit`.
///
/// Cost: one pass over the corpus (`O(total source bytes)` tokenization,
/// `O(tokens)` hash lookups) — the same cost class as parsing the corpus
/// at index time; this is a one-shot analysis command, not an always-on
/// phase. Early-exits once every candidate has a hit.
pub(crate) fn build_report(
    workspace_root: &Path,
    files: &[IndexedFile],
    candidates: Vec<ZeroEvidenceCandidate>,
    limit: Option<usize>,
) -> DeadCodeReport {
    let mentioned = textually_mentioned(workspace_root, files, &candidates);

    let mut definite = 0usize;
    let mut maybe = 0usize;
    let findings: Vec<DeadCodeFinding> = candidates
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let tier = if mentioned.contains(&i) {
                maybe += 1;
                Tier::Maybe
            } else {
                definite += 1;
                Tier::Definite
            };
            DeadCodeFinding {
                name: c.name,
                qualified_name: c.qualified_name,
                kind: c.kind,
                visibility: c.visibility,
                file: c.file,
                line: c.line,
                tier,
            }
        })
        .collect();

    let summary = DeadCodeSummary {
        candidates: findings.len(),
        definite,
        maybe,
    };
    let mut findings = findings;
    if let Some(limit) = limit {
        findings.truncate(limit);
    }
    debug!(
        candidates = summary.candidates,
        definite = summary.definite,
        maybe = summary.maybe,
        "Dead-code report assembled"
    );
    DeadCodeReport { findings, summary }
}

/// Indexes (into `candidates`) of every candidate whose name occurs, with
/// word boundaries, on any corpus line outside that candidate's own
/// definition span. A candidate with a `NULL` end line degrades to
/// line-only exclusion — occurrences inside its own body then count as
/// mentions, shifting it toward Maybe (safe direction; documented in the
/// module docs).
fn textually_mentioned(
    workspace_root: &Path,
    files: &[IndexedFile],
    candidates: &[ZeroEvidenceCandidate],
) -> std::collections::HashSet<usize> {
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, c) in candidates.iter().enumerate() {
        by_name.entry(c.name.as_str()).or_default().push(i);
    }
    let mut mentioned = std::collections::HashSet::new();

    'files: for file in files {
        let rel_path = file.path.to_string_lossy();
        let content = match std::fs::read_to_string(workspace_root.join(&file.path)) {
            Ok(content) => content,
            Err(err) => {
                // Diagnostic, not data: an unreadable file only weakens
                // the scan toward Definite, so surface it on stderr.
                warn!(path = %rel_path, %err, "dead-code scan cannot read file; skipping");
                continue;
            }
        };
        for (line_idx, line) in content.lines().enumerate() {
            let line_no = u32::try_from(line_idx + 1).unwrap_or(u32::MAX);
            for token in line
                .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .filter(|t| !t.is_empty())
            {
                let Some(hits) = by_name.get(token) else {
                    continue;
                };
                for &ci in hits {
                    let c = &candidates[ci];
                    let own_span = c.file == rel_path
                        && c.line <= line_no
                        && line_no <= c.end_line.unwrap_or(c.line);
                    if !own_span {
                        mentioned.insert(ci);
                    }
                }
            }
            if mentioned.len() == candidates.len() {
                break 'files;
            }
        }
    }
    mentioned
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;
    use crate::types::{FileId, Language, SymbolId};

    fn candidate(
        name: &str,
        file: &str,
        line: u32,
        end_line: Option<u32>,
    ) -> ZeroEvidenceCandidate {
        ZeroEvidenceCandidate {
            id: SymbolId::from(1),
            name: name.to_string(),
            qualified_name: name.to_string(),
            kind: "function".to_string(),
            visibility: "private".to_string(),
            file: file.to_string(),
            line,
            end_line,
        }
    }

    fn indexed(path: &str) -> IndexedFile {
        IndexedFile {
            id: FileId::from(1),
            path: PathBuf::from(path),
            language: Language::Rust,
            mtime_ns: 0,
            size_bytes: 0,
            content_hash: None,
            indexed_at: 0,
        }
    }

    /// Write fixture files under a temp workspace root, return the root.
    fn workspace(files: &[(&str, &str)]) -> TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        for (path, content) in files {
            let full = dir.path().join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("mkdir");
            }
            std::fs::write(&full, content).expect("write fixture");
        }
        dir
    }

    /// C6: a name whose only occurrence is inside ANOTHER file's macro
    /// argument tiers Maybe — the scan is cross-file. Kills: scanning
    /// only the candidate's defining file.
    #[test]
    fn macro_only_mention_is_maybe() {
        let dir = workspace(&[
            ("src/a.rs", "fn hidden() {}\n"),
            ("benches/b.rs", "criterion_group!(benches, hidden);\n"),
        ]);
        let files = [indexed("src/a.rs"), indexed("benches/b.rs")];
        let report = build_report(
            dir.path(),
            &files,
            vec![candidate("hidden", "src/a.rs", 1, Some(1))],
            None,
        );
        assert_eq!(report.findings[0].tier, Tier::Maybe);
        assert_eq!((report.summary.definite, report.summary.maybe), (0, 1));
    }

    /// C6 span exclusion: a recursive fn whose name appears ONLY inside
    /// its own definition span is Definite — rustc flags recursive dead
    /// fns and so do we. Kills: excluding only the declaration line.
    #[test]
    fn recursion_inside_own_span_is_definite() {
        let dir = workspace(&[(
            "src/a.rs",
            "fn rec(n: u8) -> u8 {\n    if n == 0 { 0 } else { rec(n - 1) }\n}\n",
        )]);
        let files = [indexed("src/a.rs")];
        let report = build_report(
            dir.path(),
            &files,
            vec![candidate("rec", "src/a.rs", 1, Some(3))],
            None,
        );
        assert_eq!(report.findings[0].tier, Tier::Definite);
    }

    /// C6 word boundary: `foobar` must not count as a mention of `foo`.
    /// Kills: substring matching.
    #[test]
    fn substring_is_not_a_mention() {
        let dir = workspace(&[
            ("src/a.rs", "fn foo() {}\n"),
            ("src/b.rs", "fn foobar() { foobar(); }\n"),
        ]);
        let files = [indexed("src/a.rs"), indexed("src/b.rs")];
        let report = build_report(
            dir.path(),
            &files,
            vec![candidate("foo", "src/a.rs", 1, Some(1))],
            None,
        );
        assert_eq!(report.findings[0].tier, Tier::Definite);
    }

    /// C6 NULL `end_line` degrade: without a recorded span end, exclusion
    /// shrinks to the declaration line and an in-body self-mention counts
    /// — the candidate shifts toward Maybe, never toward Definite.
    #[test]
    fn null_end_line_degrades_to_maybe() {
        let dir = workspace(&[("src/a.rs", "fn rec(n: u8) -> u8 {\n    rec(n)\n}\n")]);
        let files = [indexed("src/a.rs")];
        let report = build_report(
            dir.path(),
            &files,
            vec![candidate("rec", "src/a.rs", 1, None)],
            None,
        );
        assert_eq!(report.findings[0].tier, Tier::Maybe);
    }

    /// C6: comments are text — a comment mention demotes to Maybe
    /// (textual means textual; documented posture).
    #[test]
    fn comment_mention_counts() {
        let dir = workspace(&[
            ("src/a.rs", "fn helper() {}\n"),
            ("src/b.rs", "// helper is kept for the migration\n"),
        ]);
        let files = [indexed("src/a.rs"), indexed("src/b.rs")];
        let report = build_report(
            dir.path(),
            &files,
            vec![candidate("helper", "src/a.rs", 1, Some(1))],
            None,
        );
        assert_eq!(report.findings[0].tier, Tier::Maybe);
    }

    /// Same-file name twins see each OTHER's lines as mentions (outside
    /// their own spans) — both tier Maybe, matching probe3 on the
    /// self-index's `seeded_index` triple.
    #[test]
    fn same_name_twins_demote_each_other() {
        let dir = workspace(&[("src/a.rs", "fn twin() {}\nfn other() {}\nfn twin() {}\n")]);
        let files = [indexed("src/a.rs")];
        let report = build_report(
            dir.path(),
            &files,
            vec![
                candidate("twin", "src/a.rs", 1, Some(1)),
                candidate("twin", "src/a.rs", 3, Some(3)),
            ],
            None,
        );
        assert!(report.findings.iter().all(|f| f.tier == Tier::Maybe));
    }

    /// C11 substrate: `limit` truncates findings AFTER assembly; the
    /// summary keeps full-population counts. Kills: counting after
    /// truncation.
    #[test]
    fn limit_truncates_findings_not_summary() {
        let dir = workspace(&[("src/a.rs", "fn a() {}\nfn b() {}\nfn c() {}\n")]);
        let files = [indexed("src/a.rs")];
        let report = build_report(
            dir.path(),
            &files,
            vec![
                candidate("a", "src/a.rs", 1, Some(1)),
                candidate("b", "src/a.rs", 2, Some(2)),
                candidate("c", "src/a.rs", 3, Some(3)),
            ],
            Some(1),
        );
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.summary.candidates, 3);
        assert_eq!(report.summary.definite, 3);
    }

    /// An unreadable corpus file is skipped with a warning, not an error
    /// — the scan degrades and the report still assembles.
    #[test]
    fn unreadable_file_skipped() {
        let dir = workspace(&[("src/a.rs", "fn lonely() {}\n")]);
        let files = [indexed("src/a.rs"), indexed("src/missing.rs")];
        let report = build_report(
            dir.path(),
            &files,
            vec![candidate("lonely", "src/a.rs", 1, Some(1))],
            None,
        );
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].tier, Tier::Definite);
    }
}
