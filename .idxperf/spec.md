# Feature: Index write-path and Pass 2 performance, behavior-preserving

## What this is
Three performance changes to `tethys index`: (1) each file's complete write
(file row + symbols + refs + imports) becomes one SQLite transaction using
cached prepared statements, in both batch and streaming write paths; (2) Pass 2
cross-file resolution memoizes per-file outcomes by reference name and applies
resolution UPDATEs in one transaction; (3) `build_file_crate_map` reuses the
fast ancestor-walk crate index already used by `run_architecture_phase`.
Index *content* is unchanged; only speed and crash semantics change.

## Users

- **Local developer (dwalleck)**: runs `tethys index` on workspaces up to
  tethys-size (~80 files) and larger; sees wall-clock indexing time drop ≥2×.
- **CI pipeline**: runs `tethys index` fresh per job; same gate applies.
  Neither sees any change in query results, stats, or index contents.

## Behavior

### File write is atomic and fast
- **Given**: a parsed file with N symbols, M refs, K imports
- **When**: the indexer writes it (batch mode `write_parsed_file` or streaming
  `BatchWriter::write_single_file`)
- **Then**: exactly one transaction commits all 1+N+M+K rows (observable via
  SQLite trace / WAL commit count); on crash mid-write, zero rows for that
  file are present after reopen.

### Pass 2 memoization
- **Given**: a file whose unresolved refs contain the same `reference_name` R
  twice or more
- **When**: `resolve_refs_for_file` runs
- **Then**: the resolution strategy cascade executes once for R; all refs
  named R in that file receive the same `symbol_id` outcome; total resolved
  count equals main's count for the same workspace.

### Pass 2 batched updates
- **Given**: P resolutions succeed for the workspace
- **When**: Pass 2 completes
- **Then**: all P `UPDATE refs` statements commit inside one transaction
  (per resolve run), and the post-Pass-2 refs table is row-identical to
  main's (per canonical dump).

### Fast crate map
- **Given**: an indexed workspace with F files and C crates
- **When**: `build_file_crate_map` runs
- **Then**: it performs zero `canonicalize()` syscalls per file (ancestor-walk
  against the pre-canonicalized crate index) and returns a map equal to the
  one main produces.

## Success criteria

- **Speedup**: median of 5 fresh-DB `tethys index` runs on the tethys repo
  (release build, this machine) improves **≥2.0×** vs main (baseline ≈15.9s →
  ≤ half of measured main baseline), measured by hyperfine (or `time` median).
- **No bench regression**: no criterion case in `benches/indexing.rs` or
  `benches/queries.rs` slower than main by >10% (criterion comparison).
- **Behavior preservation**: canonical dump (all tables; `indexed_at`
  excluded; integer ids replaced by natural keys; sorted; duplicates
  preserved) is byte-identical main-vs-branch for: (a) the tethys repo in
  batch mode, (b) the same repo in streaming mode (compared against main's
  streaming dump), (c) at least one C#-containing fixture workspace.
- **Determinism baseline**: main-vs-main dump diff is empty across 2 runs
  (precondition for the oracle; if violated, the oracle is refined before any
  optimization is judged).
- **Suite**: all existing tests pass; `cargo clippy --all-targets` zero
  warnings.

## Edge cases and decisions

| Edge | Decision | Rationale |
|---|---|---|
| Crash mid-file-write | Whole file absent (all-or-nothing) | Requester accepted; partial data (symbols w/o refs) silently corrupts usage evidence |
| Empty workspace (0 files) | Dump equality trivially holds; no special-casing | Both paths already handle |
| File fails to parse | Error counts and indexed-file set match main exactly | Errors are part of observable behavior |
| Streaming mode | In scope — both write paths converted; equality checked per-mode (main-batch vs branch-batch, main-streaming vs branch-streaming) | Streaming and batch already diverge on main (module_path); cross-mode equality is NOT claimed |
| Same reference_name, same file, different refs | One memo entry; all get same outcome | Resolution today depends only on (file imports, reference_name); memo key = reference_name per file |
| Memo across files | Forbidden — memo is per-file | Import context differs per file |
| Pass 2 read-after-write | Must verify no resolution lookup reads `refs.symbol_id` mid-pass (else batching changes outcomes) | Probe/design concern; if found, batching is restructured or dropped |
| Duplicate rows in dump | Preserved (sorted multiset, not set) | A lost+gained pair must not cancel |
| BatchWriter holding write lock longer per batch | Acceptable; no concurrent writer exists during indexing | Single-writer architecture |

## Out of scope

This change does NOT include:
- Incremental `update()` (tracked: tethys-q8qw)
- Pass 2 layer (c): in-memory name→symbols multimap replacing SELECTs
- Connection pooling / replacing `Mutex<Connection>`
- Any schema change
- LSP pass performance
- Any change to resolution *logic* (strategies, order, outcomes)

## Constraints

| Dimension | Limit | How measured |
|---|---|---|
| Self-index wall time | ≥2.0× faster than main | hyperfine median of 5, release, fresh DB |
| Criterion benches | no case >10% slower | criterion compare vs main baseline |
| Index content | canonical dump byte-identical per-mode | dump tool + diff |
| Tests | all pass | cargo test |
| Lints | zero warnings | cargo clippy --all-targets |
| API | `index_file_atomic` signature may change (pub-for-tests convention); no CLI surface change | review |

## Decisions log

| # | Question | Decision | Why |
|---|---|---|---|
| 1 | What does "no unintended side effects" mean operationally? | Canonical dump equality: all tables, volatile cols excluded, ids→natural keys, sorted, duplicates preserved; byte-identical main vs branch | Strongest practical oracle; aggregate counts can hide compensating errors |
| 2 | Speedup pass/fail gate? | ≥2.0× median-of-5 self-index AND no criterion case >10% slower | Falsifiable floor without overpromising; write fix may yield more but parsing/Pass 2 cap end-to-end |
| 3 | Accept all-or-nothing crash semantics per file? | Yes | Partial file data is the worse failure mode |
| 4 | Streaming mode in scope? | Yes, both write paths; equality judged per-mode | Both share the per-row insert pattern; cross-mode equality already false on main |
| 5 | Oracle precondition | Main must dump-equal itself across runs first | Parallel parse order must not leak into canonical content; if it does, refine oracle before judging |

## Sign-off

The requester typed, verbatim: "We wrap our writes in a single transaction,
we use memoization in pass 2, and build_file_crate_map uses the fast
ancestor-walk crate index."

(Gates — ≥2.0× self-index, no criterion regression >10%, canonical dump
equality per mode, all-or-nothing crash semantics — were each selected
explicitly by the requester via closed-form questions; see decisions log
#1–#5.)

Date: 2026-06-09
