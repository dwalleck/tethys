# Probe findings — tethys-epmj (discriminated NotFound variants)

## Smallest questions asked

1. What does the user actually observe today when `tethys coupling
   --package <missing>` runs against a real index? (Slices A/A2/A3)
2. What is the full inventory of `Error::NotFound` construction sites and
   payload formats — the blast radius of adding a discriminated variant?
   (Slice B)
3. Who pattern-matches on `Error::NotFound`, and which tests fence the
   current message format? (Slice C, static)

## Oracle

Slice A oracle: static derivation from source (thiserror `#[error]` attrs +
`main.rs:323-335` rendering + `run_detail_to` control flow), pre-registered
in `oracle-expectations.md` BEFORE the probe ran. Probe: the real
`target/debug/tethys` binary against the real repo index.

**Agreement: exact on all four slices** — exit 1 / stdout empty (text) /
stdout `null` (json) / stderr `error: not found: package '<name>'` /
suggestion block precedes the error line on stderr / control success case.

Slice B probe: `tethys callers "Error::NotFound"` (AST index). Oracle:
grep pipeline over `src/`. **They disagreed** — grep 28 sites in 5 files
incl. `cli/coupling.rs:101`; tethys 23 callers in 5 files, `cli/` absent.

## The disagreement, resolved

Cause 1 (underlying system broken): the ref for
`tethys::Error::NotFound(...)` at `cli/coupling.rs:101` is emitted but
never resolves (`symbol_id` NULL) — the Rust module resolver does not alias
the package's own crate name to the lib crate root for bin/test targets.
Method calls cross the boundary fine (receiver-type resolution: `tethys
callers Tethys::get_package_coupling` correctly shows `run_detail_to`).

- Verified NOT a duplicate of tethys-i09d (value position) or tethys-ewa7
  (macro token trees). **Filed: tethys-staf.**
- Scope-around for this feature: **grep is the source of truth for epmj's
  impact analysis** (the affected site is exactly the one this issue edits).
  The feature itself does not depend on the broken part.

## What I learned (that I didn't know before)

**The issue text is wrong on both of its load-bearing claims: the payload
format is `package 'name'` (not `package: name`), and `get_package_coupling`
never constructs `NotFound` — the only package-flavored construction lives
in the CLI layer (`run_detail_to`), because the library API returns
`Result<Option<CouplingDetail>>`.**

Supporting inventory (design inputs):

| fact | source |
|---|---|
| 28 `NotFound(format!)` sites: `file:` 8, `symbol:` 7, `file id:` 8 (one multiline), `symbol id:` 3, `type:` 1, `package '` 1 | grep, slice B |
| Only ONE package-flavored site — the change surface is tiny | slice B |
| `From<&Error> for IndexErrorKind` (error.rs:147) matches exhaustively → compiler forces categorization of any new variant | slice C |
| `Error` is NOT `#[non_exhaustive]` → new variant breaks exhaustive matchers in external consumers (none exist yet; rivets-mcp is future) | error.rs:32 |
| 3 `matches!(err, tethys::Error::NotFound(_))` in tests/indexing.rs — all file/symbol flavored, unaffected by a package-only variant | slice C |
| Message fence `msg.contains("not found") && msg.contains("no-such-pkg")` (cli/coupling.rs:718) survives any Display containing both | slice C |
| Suggestions print to stderr BEFORE main.rs's error line; JSON mode prints `null` to stdout AND still errors on stderr | slice A |
| tethys callers misses `tethys::`-qualified refs from bin target → tethys-staf; grep is epmj's impact oracle | slice B |

## Hard-gate checklist

- [x] Probe written and runs against the real codebase (`probe.sh`)
- [x] Oracle defined and produces output (pre-registered static derivation + grep)
- [x] Probe and oracle agree on non-trivial slices (A, A2, A3, control; B after resolution)
- [x] Learned something new (see above — two issue-text claims falsified)
