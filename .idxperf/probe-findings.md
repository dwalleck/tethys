# idxperf probe findings

Date: 2026-06-09. Baseline commit: ceabc19 (branch index-correctness).

## Probes run

1. **Canonical dump tool** (`probe-dump.py`, 110 lines): all 10 tables,
   ids → natural keys, `indexed_at`/`mtime_ns` excluded, newlines/pipes
   escaped in free-text fields, sorted, duplicates preserved.
2. **Determinism baseline**: index → dump → index → dump → diff.
3. **fsync-bound hypothesis**: identical workload on disk vs tmpfs.
4. **Pass 2 read-after-write**: static scan of `FROM refs` reads in the DB
   layer vs the call set reachable from `resolve_cross_file_references`.
5. **Perf baselines**: hyperfine median-of-5 self-index; criterion
   `--save-baseline pre-idxperf`.

## Oracles and agreement

- **Dump faithfulness**: sqlite3 CLI row counts per table vs dump line
  counts per prefix — all 10 match exactly (80 files / 1463 sym / 14719
  ref / 842 imp / 250 dep / 1731 edge / 587 attr / 1 pkg / 80 fpkg /
  0 pdep). Hand-checked row: `index_file_atomic` → `sym|src/db/files.rs
  |94|5|...|method|220|6|...` matches the source (files.rs:94).
- **fsync hypothesis**: tmpfs differential (15.74s disk vs 0.62s tmpfs,
  25.4×) agrees with independent arithmetic: ~18k expected commits
  (14,719 refs + 842 imports + 250 deps + resolve updates + 80 file tx)
  × ~0.85ms/fdatasync ≈ 15.3s. Two mechanisms, same answer.

## Results

| Question | Answer | Evidence |
|---|---|---|
| Is canonical content deterministic (batch)? | YES — byte-identical across fresh runs | dump-batch-run{1,2}.txt diff empty (20,416 lines) |
| Is canonical content deterministic (streaming)? | YES | dump-stream-run{1,2}.txt diff empty |
| Is canonical content deterministic (C#)? | YES | fixtures/csharp-ws, 30 rows, diff empty |
| Is the write path fsync-bound? | YES, ~96% of wall time | 15.74s disk vs 0.62s tmpfs (25.4×) |
| Does any Pass 2 lookup read refs mid-pass? | NO | only `get_unresolved_references` (before loop), Pass 3 LSP + `populate_call_edges` (after) read refs; no symbol-search touches refs |
| Disk baseline (gate denominator) | **15.740 s ± 0.262 s** (hyperfine, 5 runs, warmup 1) | baseline-selfindex.json |
| Criterion baseline | saved as `pre-idxperf` | target/criterion |

## What I learned (that I did not know before)

The write path is not merely "slow from per-row inserts" — it is ~96%
fdatasync-bound (25× tmpfs differential), meaning SQL parse overhead and
mutex churn are noise next to per-statement commit syncs; the ≥2× gate is
extremely conservative and the transaction change alone should dominate.

Secondary learnings: (a) canonical index content is deterministic in both
write modes — parallel parse order does not leak, so the dump-equality
oracle is valid as specified; (b) batch-vs-streaming dumps differ by 2,298
lines on this repo (known module_path divergence + dep-count differences),
confirming the spec's per-mode comparison rule; (c) all comparison dumps
must be taken at an identical source tree (adding the probe's own example
file changed the dump).

## Probe-harness gotchas (so the build phase doesn't trip on them)

- Piping the indexer's stdout to `head` SIGPIPE-kills it mid-index; an
  empty DB dump follows. Use full consumption or `>/dev/null`.
- Symbol signatures are multi-line; dump fields must escape `\n` and `|`.

## Hard gate

- [x] Probe written and runs against the real codebase
- [x] Oracle defined and produces output
- [x] Probe and oracle agree on a non-trivial slice (10/10 table counts;
      25.4× differential vs arithmetic estimate)
- [x] One-sentence learning recorded (above)

Cheapest falsifier for the design phase: a ~20-line prototype patch
wrapping one file's refs+imports inside the existing `index_file_atomic`
transaction, measured on this repo — predicted to cut wall time by >5×
on its own; if it doesn't, the design's premise is wrong.
