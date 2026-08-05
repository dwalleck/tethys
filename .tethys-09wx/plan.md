# Budgeted plan — tethys-09wx (query standing for affected-tests)

Claims C1-C16 from `.tethys-09wx/design.md`. C1 (exit-2 unclaimed) passed at
design time; its permanent fence is the exit-code asserts in slices 5-6.

## Slice 1: logs to stderr (tethys-sspl) + stdout-purity fence

**Claim:** C2 — with logs forced on, `--names-only` stdout carries only data.
**Oracle:** fixture construction — expected test names known a priori; any
extra stdout line is a failure regardless of content.
**Stress fixture:** `RUST_LOG=tethys=debug` (maximum log volume) on a fixture
workspace; bug class: subscriber writing to stdout (today's behavior — this
fence fails pre-fix, passes post-fix).
**Loop budget:** no new loops.
**Files:** `src/main.rs`, `tests/affected_tests_cli.rs` (new).

Advisory: `.with_writer(std::io::stderr)` on the `tracing_subscriber::fmt()`
builder. Bin test via `env!("CARGO_BIN_EXE_tethys")` (pattern:
`tests/deprecated_callers.rs:577`), fixture workspace in a tempdir, run
`index` then `affected-tests --names-only`, assert every stdout line is an
expected name and stderr contains the debug lines.

**Verification:** unit/bin tests pass; fence fails against pre-slice binary
(checked by inspection of today's probe transcript); oracle = probe.sh CLI
slice rerun shows WARN now on stderr; budget n/a.

## Slice 2: lexical input normalization in `relative_path` (tethys-xetb, tethys-vk3z)

**Claim:** C8 — `src/x.rs` ≡ `./src/x.rs` ≡ absolute; C10 — no spurious WARN.
**Oracle:** fixture construction (expected file-id resolution known from the
DB row the fixture created); *not* another invocation of the same lookup.
**Stress fixture:** inputs `./src/lib.rs`, `src/./lib.rs`, `src/../src/lib.rs`,
`../escape.rs`, `""` — first three must resolve identically to `src/lib.rs`;
escape and empty stay unresolved (no panic, no warn for relative forms). Bug
classes: exact-string lookup (today's xetb), ParentDir mishandling that
resolves `src/../src/lib.rs` to the wrong path or panics on `../escape.rs`.
**Loop budget:** O(components) per path, components < 10^2, paths per
invocation < 10^3 → < 10^5 cheap ops, no syscalls. Within budget.
**Files:** `src/lib.rs` (`relative_path`), `tests/test_topology.rs` (facade
tests: `./`-form returns identical affected set to plain form).

Advisory: non-absolute input → lexically normalize via `components()`:
drop `CurDir`, pop a prior `Normal` on `ParentDir`, bail to as-is when
`ParentDir` underflows (escaping path). No `warn!` for relative inputs
(the documented input form). Absolute inputs keep strip_prefix +
canonicalize fallback; downgrade the outside-workspace `warn!` to `debug!`
(outside paths are now an expected shape handled by standing).

**Verification:** unit tests pass; stress inputs produce expected forms;
oracle: `tethys affected-tests --names-only ./src/lib.rs` on the real repo
returns the same 980 names the probe measured for `src/lib.rs`; budget holds.

## Slice 3: standing types + per-file classifier

**Claim:** C4/C5/C7 facade halves — per-file standing is Unindexed for
missing rows, Stale for mtime/size divergence AND deleted-on-disk, Current
otherwise.
**Oracle:** ground truth by construction (fixture creates/touches/deletes the
files itself), same mechanism as the probe's oracle 1.
**Stress fixture:** (i) file rewritten to a different size with mtime
restored to the indexed value — must classify Stale via size alone (bug
class: mtime-only comparison); (ii) file deleted post-index — Stale, not an
error (bug class: stat error propagated as `Err` → exit 1); (iii) empty
input list → empty reasons (empty-collection path).
**Loop budget:** O(changed_files) with one row lookup + one `stat` each;
CI-scale changed_files ≈ 10^2 → ≈ 2×10^2 syscalls, one-shot CLI phase.
Within budget.
**Files:** `src/types.rs` (`QueryStanding`, `StandingReason`,
`StandingReasonKind`, `AffectedTestsReport`), `src/reindex.rs`
(`pub(crate) fn classify_changed_files`, reusing private
`classify_indexed_file`; unit tests in the existing `#[cfg(test)]` mod).

**Verification:** unit tests pass; stress fixtures produce written-down
outcomes; probe.py agrees with classifier on the real repo (CURRENT/STALE/
UNINDEXED for the same three files); budget holds.

## Slice 4: facade `get_affected_tests_with_standing` + delegating wrapper + trigger (c)

**Claim:** C13 — `get_affected_tests` behavior unchanged; C16 facade half —
standing includes `StaleIndex` when `needs_update()` is true.
**Oracle:** existing `tests/test_topology.rs` suite (written against the old
model, untouched) for C13; fixture construction for C16.
**Stress fixture:** changed file current + an unrelated file created
post-index → standing Indeterminate(StaleIndex) while tests list is the
normal traversal result (bug class: early-return skipping traversal on
indeterminate; also the inverse — pristine workspace must NOT report
StaleIndex).
**Loop budget:** traversal unchanged (existing O(V+E) reverse walk);
`needs_update()` adds an early-exit walk, O(workspace files) stats — ≈10^2
here, ≈5×10^4 at a 50k-file production workspace. Exceeds the 10^3-syscall
guideline; justified in writing: this is a one-shot CLI query (not an
always-on phase), the walk is the same cost class as the `tethys index`
discovery the documented recipe runs immediately before, and it early-exits
on the first divergence in the non-pristine case.
**Files:** `src/lib.rs`, `tests/test_topology.rs` (standing-path tests).

**Verification:** full existing suite passes untouched (C13); stress fixture
outcomes as written; oracle agreement; budget justification recorded above.

## Slice 5: CLI exit mapping + stderr reason lines

**Claim:** CLI halves of C3 (confirmed → 0/empty), C4/C5 (reason lines +
exit 2), C6 (stdout still carries found tests), C9 (outside → unindexed),
C11 (dedup), C12 (empty input → 0); C14 discipline (no DB/fs in CLI).
**Oracle:** shell `$?` + stderr line grep in the slice-6 fences; during this
slice, manual runs against the real repo mirroring the probe's CLI table.
**Stress fixture:** same file passed as `src/lib.rs` AND `./src/lib.rs` →
exactly one reason line if stale, zero if current (bug class: dedup on the
raw input string instead of the normalized path); reasons ordered
first-occurrence, `stale-index` line last.
**Loop budget:** O(files) HashSet dedup + O(reasons) prints; ≪ 10^6. Within
budget.
**Files:** `src/cli/affected_tests.rs` (run returns `Result<ExitCode>`),
`src/main.rs` (AffectedTests dispatch arm maps it; all other arms unchanged).

Output streams: test names + human report = **data** → stdout. Reason lines =
**diagnostic with a documented contract** → stderr via `eprintln!` —
deliberate (design D4): stdout stays pure for `| xargs`, and `^indeterminate: `
is the grep anchor. Existing "no files specified" warning stays on stderr.

**Verification:** manual CLI matrix matches the design's contract table;
existing suite still passes; budget holds; grep confirms no
rusqlite/fs-metadata use in the CLI file.

## Slice 6: bin-level fence suite

**Claim:** deterministic CI fences for C2, C3, C4, C5, C6, C8, C9, C11, C12,
C15, C16 — one `#[test]` per claim (distinct failure localization).
**Oracle:** fixture construction per test; determinism fence (C15) uses
byte-diff of two identical invocations.
**Stress fixture:** the shared fixture workspace gains: a lib file with a
dependent test file, an isolated leaf file (no dependents), and per-test
mutations (touch / delete / create-after-index). Adversarial cases baked in:
`./`-spelling duplicate (C11), unrelated-file creation (C16 vs C3 inverse
assert), stale file that still has dependents (C6 — stdout must not be
suppressed by indeterminacy).
**Loop budget:** test-only; each test indexes a ≤6-file fixture → trivial.
**Files:** `tests/affected_tests_cli.rs` (extend), `tests/common/mod.rs`
(fixture helper additions if needed).

**Verification:** every fence fails against a deliberately-broken local
mutation of its target (spot-check C4 by re-introducing silent-skip, C15 by
injecting HashMap-ordered reason emission — TDD-inversion, not committed);
all pass against the built binary.

## Slice 7: architecture fence for CLI purity

**Claim:** C14 — `src/cli/affected_tests.rs` never touches rusqlite /
`std::fs` metadata directly (standing comes from the facade).
**Oracle:** source text scan (mechanical), independent of runtime behavior.
**Stress fixture:** the assert itself: scan the file for
`rusqlite|fs::metadata|fs::symlink_metadata|Connection::` — bug class: a
future "quick fix" stat-ing files in the CLI layer, silently forking the
staleness semantics from `classify_indexed_file`.
**Loop budget:** one file read at test time. Trivial.
**Files:** `tests/architecture.rs`.

**Verification:** fence passes; verified it FAILS when a `use rusqlite;` line
is temporarily added to the CLI file (TDD-inversion, not committed).

## Slice 8: README CI recipe + changelog fragment

**Claim:** issue AC — README documents the recipe (exit 0 + empty stdout =
confirmed skip; exit 2 = run full suite, fail-open with signal; exit 1 =
tooling error); mechanism in the tool, policy in the consumer.
**Oracle:** `tests/changelog_lint.rs` (existing) fences the fragment format;
README reviewed against the design's contract table at code review.
**Stress fixture:** n/a — docs slice; the changelog lint is the mechanical
check. (Per budgeted-plan rules a no-fixture slice must justify itself: the
only executable artifact here is the fragment, and the lint is its fence.)
**Loop budget:** none.
**Files:** `README.md`, `changelog.d/tethys-09wx.changed.md`.

**Verification:** changelog lint passes; README section names the three exit
codes and the `^indeterminate: ` anchor; full gate green.

## Plan self-review

1. **Loops:** slice 2 O(components×paths) < 10^5 ops; slice 3
   O(changed_files) ≈ 2×10^2 syscalls; slice 4 needs_update O(workspace
   files) — over the 10^3-syscall guideline at 50k-file scale, justified
   in-slice (one-shot CLI, same cost class as the adjacent `tethys index`
   run, early-exit); slice 5 O(files) dedup. No unbounded or unstated loops.
2. **Fixtures:** every logic slice names its bug class (subscriber-on-stdout;
   exact-string lookup / ParentDir mishandling; mtime-only staleness /
   stat-error-as-Err; skip-traversal-on-indeterminate / false StaleIndex;
   dedup-on-raw-string; HashMap-order nondeterminism; CLI-stats-files).
   Slice 8 is docs-only with the changelog lint as its mechanical check.
3. **Doc-comment preconditions:** the new facade method documents "paths may
   be workspace-relative (any lexical spelling) or absolute" — not a
   precondition but a guarantee, enforced by slice 2's normalization; no
   load-bearing caller-must-X preconditions are introduced (violating input
   shapes degrade to deterministic `unindexed`, never silent wrong output).
4. **Write targets:** stdout = data (test names, human report, unchanged);
   stderr = diagnostics + the documented `indeterminate:` contract lines
   (justified in slice 5); no other writes.
5. **Tracker references:** tethys-sspl (slice 1), tethys-xetb + tethys-vk3z
   (slice 2) — all verified to exist (filed by this run's probe);
   resolution-quality triggers deferred per PRD tethys-l6nt (verified, open);
   JSON envelope deferred to tethys-zwaz (verified, open). No uncited
   deferrals.

Claim coverage: C1 design-time (fenced by C3/C4 exit asserts) • C2 s1/s6 •
C3 s5/s6 • C4 s3/s5/s6 • C5 s3/s6 • C6 s5/s6 • C7 s3 • C8 s2/s6 • C9
s2/s5/s6 • C10 s1/s2 (assert in s6's C2 fence) • C11 s5/s6 • C12 s5/s6 •
C13 s4 • C14 s5/s7 • C15 s6 • C16 s4/s6. Complete.
