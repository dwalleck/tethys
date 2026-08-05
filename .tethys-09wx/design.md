# Design — query standing for affected-tests (tethys-09wx)

## Purpose

`tethys affected-tests` is the flagship CI command, and today an empty result
the index cannot stand behind is indistinguishable from confirmed-clean
(exit 0, empty stdout, silently-skipped unknown files). This change adopts the
affirmative-clean posture: **confirmed** results keep today's shape (exit 0,
stdout data); **indeterminate** results exit 2 (distinct from error exit 1)
with machine-readable reasons on stderr, while stdout still carries whatever
tests were found (fail-open with signal).

Vocabulary is the existing CONTEXT.md "Query standing" entry
(confirmed / indeterminate) — no new terms coined.

## Architecture / placement (step 2c)

| Capability | Owner | Forbidden |
|---|---|---|
| Per-changed-file standing classifier (Unindexed / Stale / Current) | `impl Tethys` block in `src/reindex.rs`, colocated with and reusing private `classify_indexed_file` | `classify_indexed_file` stays private to reindex.rs; no second mtime-comparison implementation anywhere |
| Standing-aware facade method `get_affected_tests_with_standing` → `AffectedTestsReport` | `src/lib.rs`, beside `get_affected_tests` (which becomes a delegating wrapper — one traversal implementation) | CLI must not open SQLite or stat files itself; MCP (Act 2) wraps the same method |
| Result types `AffectedTestsReport`, `QueryStanding`, `StandingReason(Kind)` | `src/types.rs` (beside `StalenessReport`) | — |
| Lexical input-path normalization (`./`, intra-path `..`) | `Tethys::relative_path` in `src/lib.rs` — fixes tethys-xetb and removes the tethys-vk3z spurious WARN for every path-taking command at once | no per-command path munging in `src/cli/` |
| Exit-code mapping (standing → `ExitCode`) | `src/cli/affected_tests.rs::run` returns `Result<ExitCode>`; only the affected-tests arm of `main()`'s dispatch changes | no `process::exit()` (skips destructors); no exit-code knowledge in lib.rs — facade reports standing, CLI maps it, README states policy |
| stderr reason lines | `src/cli/affected_tests.rs`, plain `eprintln!` | reasons are contract output, never routed through `tracing` |
| Logs to stderr (tethys-sspl prerequisite) | `src/main.rs` subscriber: `.with_writer(std::io::stderr)` | — |

Extend-existing throughout — no new seam, so no `design-an-interface` pass.

## Contract (from the issue, plus resolved details)

- Confirmed: exit 0; stdout exactly as today. Empty stdout + exit 0 ⟺
  confirmed no affected tests.
- Indeterminate: exit 2; stdout still carries found tests; one stderr line
  per unique offending file, first-occurrence input order, normalized
  workspace-relative path:
  `indeterminate: unindexed: <path>` | `indeterminate: stale: <path>`
  (grep-anchor `^indeterminate: ` — tracing lines start with a timestamp and
  can never collide).
- Hard errors keep exit 1.
- Both output modes (`--names-only` and human) carry identical exit and
  stderr-reason semantics; human stdout shape is unchanged.
- v1 triggers: (a) changed file has no row; (b) changed file's mtime_ns OR
  size_bytes differ from its row (`classify_indexed_file` semantics — "differ",
  not just "newer": any divergence means the index cannot vouch);
  (c) whole-index staleness via early-exit `needs_update()` — any indexed file
  added/modified/deleted on disk emits one fixed line
  `indeterminate: stale-index: workspace changed since last index`, after the
  per-file reasons. (c) is deliberately a superset signal: it also fires
  whenever (b) fires or a new-on-disk file triggers (a) — all applicable
  reasons are emitted, complete and deterministic.
  Deleted-on-disk maps to `stale` (row exists, disk disagrees).
  Outside-workspace inputs report `unindexed` (unindexable ⇒ cannot vouch).
- Resolution-quality triggers stay out per the issue, gated on Act 1 resolver
  work (PRD tethys-l6nt).

## Input shapes (step 2)

Changed-files list: empty • single current • single stale-mtime • single
stale-size (mtime preserved) • single deleted-on-disk (row exists) • single
unindexed-new-on-disk • single unindexed-never-existed • multi all-current •
multi mixed • duplicates (incl. same file in two spellings).
Path forms: plain relative • `./`-prefixed • absolute-inside • outside
workspace (`../x.rs` or absolute) • intra-path dots (`src/../src/x.rs`) •
empty string (no row ⇒ deterministic `unindexed`).
Flags: `--names-only` on/off. Workspace state: never-indexed (empty DB).
Out of scope: Windows backslash forms (`normalize_path` handles at the DB
seam; no Windows CI here); Unicode paths (lookup is byte-exact on both sides;
no case-folding attempted or promised).

## Removed-invariant sweep (step 2b)

Subtractive in two places:

1. **"affected-tests exits 0 unless hard error"** — removed deliberately (the
   point of the issue). No in-repo consumer: zero hits for affected-tests in
   `.github/workflows/` and `scripts/`. README documents the new recipe.
2. **"logs appear on stdout"** (sspl fix moves every command's logs to
   stderr) — no test asserts on stdout log text (grep: zero hits for
   WARN/log-text expectations in `tests/`); full gate re-verifies.
3. `get_affected_tests` (public facade) keeps its exact contract — it becomes
   a delegating wrapper; `tests/test_topology.rs` passes unchanged (C13).

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| C1 | Exit 2 is unclaimed today (Ok→0, Err→1) | run current CLI success + hard-error; grep src for exit-2 | shell `$?` + grep | 2m | **passed** (0/1, zero grep hits) | C3/C4 fences pin 0-vs-2 forever |
| C2 | With RUST_LOG forcing logs, `--names-only` stdout = data lines only; logs on stderr | bin test: fixture ws, RUST_LOG=tethys=debug, assert stdout set == expected names exactly | fixture construction | 15m | pending | `affected_tests_cli::stdout_carries_only_data` |
| C3 | All-current changed files, no dependents → exit 0 + empty stdout | bin test: leaf file with no test deps | fixture construction | 10m | pending | `affected_tests_cli::confirmed_empty_exits_zero` |
| C4 | Unindexed file (new-on-disk AND never-existed) → exit 2 + `indeterminate: unindexed:` line | bin test: create file post-index / pass bogus path | fixture construction | 10m | pending | `affected_tests_cli::unindexed_is_indeterminate` |
| C5 | Stale file → exit 2 + `indeterminate: stale:` (mtime-only bump; size-change-mtime-restored variant at facade level) | bin test: touch file post-index; unit test: rewrite + restore mtime | `find -newer` logic by construction | 15m | pending | `affected_tests_cli::stale_is_indeterminate`; facade unit `standing_detects_size_change` |
| C6 | Stale file with dependents → stdout carries found tests AND exit 2 | bin test: touch a file that has a dependent test | fixture construction | 10m | pending | `affected_tests_cli::indeterminate_still_reports_found_tests` |
| C7 | Deleted-on-disk (row exists) → exit 2, kind `stale` | facade unit + bin assert: delete post-index | fixture construction | 10m | pending | `standing_deleted_file_is_stale` |
| C8 | `src/x.rs` ≡ `./src/x.rs` ≡ absolute: identical stdout, exit, reasons | bin test: run all three, pairwise diff vs constructed expected list | fixture construction (expected list known a priori) | 15m | pending | `affected_tests_cli::path_forms_equivalent` |
| C9 | Outside-workspace path → exit 2, `unindexed`, no panic/exit-1 | bin test: pass `../outside.rs` | fixture construction | 10m | pending | `affected_tests_cli::outside_workspace_is_indeterminate` |
| C10 | Valid relative path fires zero "outside workspace root" WARNs | assert within C2's bin test (distinct assert, distinct message) | stderr grep | 5m | pending | same test, distinct assert |
| C11 | Duplicate inputs (incl. two spellings of one file) → one reason line | bin test: pass dup; count `^indeterminate:` lines == 1 | line count | 5m | pending | `affected_tests_cli::reasons_deduped` |
| C12 | Empty input → exit 0, empty stdout (existing warning preserved) | bin test: no file args | shell `$?` | 5m | pending | `affected_tests_cli::empty_input_confirmed` |
| C13 | `get_affected_tests` public behavior unchanged | run existing suite untouched | `tests/test_topology.rs` (pre-existing, written against old model) | 5m | pending | existing suite |
| C14 | CLI layer contains no direct DB/fs-metadata access | grep `rusqlite\|fs::metadata\|sqlite` in `src/cli/affected_tests.rs` == 0 | grep (mechanical) | 5m | pending | assert in `tests/architecture.rs` |
| C15 | Determinism: same invocation twice → byte-identical stdout + reason lines + exit | bin test: run twice, compare | byte diff | 5m | pending | `affected_tests_cli::deterministic_output` |
| C16 | Current changed file + any OTHER indexed file changed on disk → exit 2 + `stale-index` line; pristine workspace → no such line | bin test: create unrelated file post-index, query current file; inverse assert lives in C3's confirmed test | fixture construction | 10m | pending | `affected_tests_cli::stale_index_is_indeterminate` |

Non-vacuity (buggy implementation each falsifier catches): C2 subscriber left
on stdout; C3 current misread as stale; C4 silently-skip retained; C5
size-only staleness check; C6 early-return before traversal on indeterminate;
C7 stat-error propagated to exit 1; C8 exact-string lookup retained; C9
panic/error on strip_prefix failure; C10 vk3z warn retained; C11 per-occurrence
reason emission; C12 empty list treated as indeterminate; C13 wrapper drift;
C14 CLI stat-ing files directly; C15 HashMap order leaking into output.

## Negative space

1. **No resolution-quality triggers** (unresolved/speculative ratios) — per
   the issue, gated behind Act 1 resolver work (PRD tethys-l6nt).
2. **No JSON output mode / envelope** for affected-tests — that surface
   belongs to the tethys-zwaz convergence; the `^indeterminate: ` stderr
   anchor and pure-data stdout are designed to compose with it.
3. **No per-file detail for trigger (c)** — the `stale-index` line is a
   fixed string (no counts, no file list): enumerating offenders costs a full
   `get_stale_files` walk and the CI reaction is identical either way; a
   consumer wanting the list runs `tethys index` which reports it.
4. **No auto-reindex on indeterminate** — mechanism in the tool, policy in
   the consumer (the CI recipe decides to re-run or re-index).
5. **No human-mode stdout change** — indeterminacy is exit code + stderr.
6. **TOCTOU accepted** between stat and answer, same documented posture as
   `get_stale_files`.

## Decisions (resolved at design pause, 2026-08-05)

- **D1 — trigger (c) whole-index staleness: INCLUDED** (closes the
  false-confirm hole where an edited-but-unlisted file hides graph edges;
  claim C16). The `needs_update()` early-exit walk runs once per query.
- **D2 — deleted-on-disk maps to `stale`** (two reason kinds for per-file
  triggers stay two).
- **D3 — in-branch scope approved**: tethys-sspl (logs→stderr) and
  tethys-vk3z (spurious WARN) fixed here as prerequisites; tethys-xetb
  inherently in-branch.
- **D4 — reason-line format approved**: `indeterminate: <kind>: <path>`,
  normalized workspace-relative path, first-occurrence order, deduped;
  `stale-index` line is fixed-string, emitted last.
- **D5 — outside-workspace inputs report `unindexed`** (fail-open with
  signal, not exit 1).
