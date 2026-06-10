# idxperf plan (from approved design .idxperf/design.md)

Global oracle (all slices): `probe-dump.py` canonical-dump equality on the
frozen tree (`git archive HEAD` → /tmp/tethys-frozen, baseline binary
`/tmp/tethys-baseline-bin` already built from the pre-change commit).
Baselines: hyperfine 15.740s ± 0.262s; criterion `pre-idxperf` (34 cases).

Production scale for budgets: files ≈ 50k, symbols ≈ 1M, refs ≈ 10M,
crates ≈ 500, path depth ≈ 12, unresolved refs/file ≈ 200.

## Slice 1: `index_parsed_file_atomic` — whole-file write in one transaction

**Claim:** C4 (exactly one commit per file write) + C5 (mid-file failure
rolls back the whole file).
**Oracle:** SQLite's own `commit_hook` (engine-level, independent of tethys
code) counts commits; sqlite3 row counts after induced failure.
**Stress fixture:** one file whose data hits every shape at once: duplicate
symbol names (last-wins map), refs that are same-file-resolved / unresolved /
top-level (`in_symbol_id` NULL) / duplicates, glob + aliased + multi-name
imports, symbols with multiple attributes. Expected: 1 commit, all rows
present, name-collision ref attributes to the LAST duplicate symbol
(current behavior, preserved). Rollback fixture: a symbol with
`parent_symbol_id` pointing at a nonexistent id → FK error → expected ZERO
rows in files/symbols/refs/imports/attributes for that path.
**Loop budget:** O(symbols + refs + imports) per file with prepare_cached
statements — identical asymptotics to today, minus per-row commits. At
production scale: 10M total statement executions, ~50k commits (was ~11M).
**Wall budget:** n/a (covered by final gate).
**Files:** `src/db/files.rs` (new method + unit tests), `src/db/mod.rs`
(export + shared `build_qualified_name` helper if moved here).

Notes: method signature
`index_parsed_file_atomic(path, language, mtime_ns, size_bytes, content_hash, symbols: &[SymbolData], references: &[ExtractedReference], imports: &[ImportStatement]) -> Result<(FileId, Vec<SymbolId>, usize)`
(usize = refs stored). Internally: existing upsert+DELETEs (refs/symbols/
imports — preserving the d4d87f1 refs fix), symbol+attribute inserts,
name/span maps from inserted data, ref inserts with same-file resolution,
import inserts via the language's `join_import`. All statements
`prepare_cached`. The update-path DELETE set is inside the same tx.

**Verification:**
- [ ] Unit tests pass (commit-count test `one_commit_per_file_write`,
      rollback test `failed_file_write_leaves_no_rows`, shape fixture)
- [ ] Stress fixture produces expected outcome (written above, before code)
- [ ] prove-it-prototype oracle still agrees (no call sites converted yet —
      dump unchanged by construction; run anyway)
- [ ] Loop budgets hold

## Slice 2: convert the batch call site

**Claim:** C1 (batch-mode canonical dump byte-identical on frozen tree) +
C3 batch arm (C# fixture dump identical).
**Oracle:** probe-dump.py diff, frozen tree + C# fixture, baseline binary vs
slice binary.
**Stress fixture:** the frozen tethys tree itself (19,770 canonical rows —
covers every shape the repo contains) + `.idxperf/fixtures/csharp-ws`
(30 rows). Expected: both diffs empty.
**Loop budget:** no new loops (deletes store_references/store_imports from
indexing.rs; write_parsed_file becomes conversion + one call).
**Files:** `src/indexing.rs`.

**Verification:**
- [ ] Unit tests pass (existing suite)
- [ ] Stress fixture: frozen-tree batch dump diff empty; C# fixture diff empty
- [ ] Oracle agrees (same thing here)
- [ ] Budgets hold

## Slice 3: convert the streaming call site, delete duplicated helpers

**Claim:** C2 (streaming-mode dump identical per-mode).
**Oracle:** probe-dump.py diff on frozen tree via `idxperf_stream` example,
baseline vs slice binaries.
**Stress fixture:** frozen tree streamed with batch_size 1 and 100 (boundary
shapes: batch smaller than/larger than file count). Expected: dumps for both
batch sizes identical to baseline streaming dump. Plus: one file in the
stream fails (unparseable file injected in a temp-copy run) — expected: other
files' rows complete, failed file absent, files_failed=1.
**Loop budget:** no new loops; batch_writer loses ~120 duplicated lines.
**Files:** `src/batch_writer.rs`.

**Verification:**
- [ ] Unit tests pass (batch_writer tests updated)
- [ ] Stress fixture: streaming dumps equal at batch_size 1 and 100;
      bad-file case isolates failure
- [ ] Oracle agrees
- [ ] Budgets hold

## Slice 4: golden-content regression fence

**Claim:** regression fence for C1/C2/C3/C7 (the one-shot dump measurements
become a deterministic CI test).
**Oracle:** hand-written expected canonical rows (literal in the test,
written from the fixture's source code BEFORE running the test — then
cross-checked against probe-dump.py output of the same fixture).
**Stress fixture:** a mixed-language fixture workspace (inline tempdir):
2 Rust files with a cross-file call + duplicate fn names across files +
a top-level type-alias ref, 2 C# files with namespace + using + static call.
Expected rows enumerated in the test. Batch and streaming arms asserted
separately (streaming's empty module_path is part of its expected set).
**Loop budget:** test-only.
**Files:** `tests/idxperf_golden.rs` (new; in-test canonical dump helper
reads via rusqlite — duplication of probe-dump.py is deliberate: the fence
must not depend on python in CI).

**Verification:**
- [ ] Test passes pre-conversion baseline? N/A — lands AFTER slices 2–3;
      asserts post-change behavior and fences it permanently
- [ ] Expected rows cross-checked against probe-dump.py on the same fixture
- [ ] Oracle agrees
- [ ] Budgets hold

## Slice 5: Pass 2 memo + batched resolution updates

**Claim:** C6 (memo preserves outcomes; same-name refs share outcome) +
C7 (updates apply in one transaction, ordering before call-edges/Pass 3
preserved).
**Oracle:** frozen-tree resolved_count log line (must equal baseline 1,847)
+ probe-dump.py diff (edge rows prove call_edges still sees resolutions);
fixture test asserts per-ref outcomes via direct DB reads.
**Stress fixture:** one file containing (a) three call refs to the same
unresolved name `shared_fn` defined in another file — expected: all three
resolve to the same symbol_id, resolution cascade consulted once (observable:
outcomes equal; effectiveness is fenced by slice 1's commit counting and the
final wall gate); (b) refs `foo` AND `Bar::foo` (distinct reference_names —
memo MUST NOT collapse them; expected: `foo` → free fn, `Bar::foo` → method);
(c) three refs to an unknown name `no_such_thing` — expected: all remain
unresolved (negative outcome cached, count unchanged).
**Loop budget:** memo HashMap O(unresolved refs per file) inserts/lookups —
200/file production scale, trivially under budget; collected resolutions
Vec O(total resolved) ≈ 10^5–10^6 entries, one pass to apply: in budget
(10^6 ops, single tx).
**Files:** `src/resolve.rs` (memo + collect), `src/db/references.rs`
(`apply_resolutions(&[(i64, SymbolId)])` in one tx + unit test).

Doc-contract: `apply_resolutions` precondition "ref ids come from the same
index" — sanity hint (a stale id makes the UPDATE a no-op, caught upstream
by resolved-count equality), `debug_assert!` on non-empty pairs only.

**Verification:**
- [ ] Unit tests pass (fixture a/b/c, apply_resolutions tx test)
- [ ] Stress fixture expected outcomes (written above) hold
- [ ] Oracle: frozen-tree dump diff empty AND resolved_count == 1847
- [ ] Budgets hold

## Slice 6: shared ancestor-walk crate map

**Claim:** C8 (new `build_file_crate_map` ≡ old implementation).
**Oracle:** hand-computed expected map for the fixture (written before
implementation); plus frozen-tree dump diff (file_deps rows unchanged).
**Stress fixture:** workspace with crates `foo` and `foo-utils` (overlapping
name prefixes — defeats first-prefix-match bugs), a file nested two dirs deep
in `foo`, an orphan at `tools/helper.rs` (expected `orphan:tools`), an orphan
at workspace root `loose.rs` (expected `orphan:loose.rs`). Expected map
enumerated per file in the test.
**Loop budget:** O(files × path-depth) ancestor walk = 50k × 12 = 6×10^5
HashMap probes, zero syscalls (was: 50k canonicalize syscalls + 50k × 500
scans = 2.5×10^7). In budget.
**Files:** `src/indexing.rs` (extract shared helper used by
`run_architecture_phase` + rewrite `build_file_crate_map`; unit test).

**Verification:**
- [ ] Unit tests pass (`fast_crate_map_matches_expected`)
- [ ] Stress fixture expected map holds
- [ ] Oracle: frozen-tree dump diff empty
- [ ] Budgets hold

## Slice 7: re-index ≡ rebuild fence

**Claim:** C9 (second index run without rebuild yields content identical to
fresh rebuild, under the new tx structure).
**Oracle:** in-test canonical rows (slice 4's helper) compared between
rebuild and re-index runs of the same fixture; independent of refs-count-only
assertions.
**Stress fixture:** fixture from slice 4 PLUS a top-level unresolved type
alias (the exact accumulating shape from the d4d87f1 bug). Expected:
canonical row multisets identical across rebuild → index → index.
**Loop budget:** test-only.
**Files:** `tests/idxperf_golden.rs` (add case).

**Verification:**
- [ ] Test passes; intentionally re-breaking the refs DELETE makes it fail
      (vacuity check, one-off)
- [ ] Oracle agrees
- [ ] Budgets hold

## Slice 8: acceptance ceremony (final gates)

**Claim:** C10 (≥2.0× self-index) + C11 (no criterion case >10% slower).
**Oracle:** hyperfine vs recorded 15.740s; criterion compare vs
`pre-idxperf`. Both manual fences per approved design; mechanism fence is
slice 1's commit-count test.
**Stress fixture:** the real repo (current tree) + frozen tree, all three
dump arms re-run with the FINAL binary.
**Loop budget:** n/a.
**Files:** `.idxperf/acceptance.md` (results), `BENCHMARKS.md` (updated
numbers), commit + PR.

**Verification:**
- [ ] hyperfine median-of-5 ≥2.0× vs 15.740s
- [ ] criterion: no case >10% regression
- [ ] frozen-tree dumps: batch, streaming, C# — all diffs empty
- [ ] cargo test (full) + clippy --all-targets clean
- [ ] acceptance recorded; tickets tethys-rex0 / tethys-8ya3 referenced

## Plan Self-Review

1. **Loops:** slice 1 O(rows)/file (unchanged asymptotics, commits 11M→50k);
   slice 5 memo O(200)/file + apply Vec ≤10^6 single-tx; slice 6 6×10^5
   probes, 0 syscalls. No loop without a statement. No violations.
2. **Fixtures:** slice 1 = shape-complete file + FK rollback (bug class:
   partial writes, name collisions); slice 3 = batch-size boundaries + bad
   file (bug class: batch boundary loss, failure isolation); slice 4 =
   cross-file dup names + top-level ref + mixed language (bug class:
   content drift); slice 5 = same-name triple, qualified/simple collision,
   negative cache (bug class: memo key collapse, uncached negatives);
   slice 6 = overlapping crate prefixes + two orphan shapes (bug class:
   prefix match, orphan naming); slice 7 = the d4d87f1 accumulating shape.
   None happy-path-only.
3. **Doc-comment preconditions:** `apply_resolutions` — sanity hint,
   debug_assert (silently-no-op upstream-caught); `index_parsed_file_atomic`
   symbol/id alignment is internal (constructed inside the method, no
   caller-facing precondition). No undocumented load-bearing contracts.
4. **Write targets:** no new user-facing output; all new logging is
   tracing → stderr (diagnostic). Test asserts write to test harness only.
5. **Tracker references:** tethys-q8qw (verified, incremental update),
   tethys-rex0 (filed, layer-c multimap), tethys-8ya3 (filed, file_deps
   batching). No uncited deferrals.

Claim coverage: C1→S2, C2→S3, C3→S2/S3, C4→S1, C5→S1, C6→S5, C7→S5,
C8→S6, C9→S7, C10→S8, C11→S8. Complete.
