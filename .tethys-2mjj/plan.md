# Budgeted plan — tethys-2mjj: readiness gate for Pass 3 LSP (Rust)

Design: `design.md` (approved 2026-07-15, all decisions recorded).
Claim coverage: C1→S3, C2→S1+S4, C3→manual (approved), C4→S2, C5→S2,
C6→S4, C7→S4, C8→S1 (red) + S4 (green).

## Slice 1: Red-first pipeline fence — prove the race at the product seam

**Claim:** C8 — cold Rust fixture, full pipeline with LSP binds ≥1 ref
with `strategy=lsp`; today it binds 0 because Pass 3 races the workspace
load. (Also carries C2's fence.)

**Oracle:** the `refs.strategy` column in the persisted SQLite DB,
queried directly by the test — independent of the resolver's own
`LspCompletedSession` counters and of the wait's logging. Cross-check at
S4: `probe.py`'s poll channel proved the fixture class resolvable
post-quiescence.

**Stress fixture:** `TempDir` cargo crate (existing `create_cargo_toml`
pattern) with a trait-object method call (`dyn` dispatch) — the exact
shape Pass 2 declines, so the ref lands in Pass 3's unresolved set. No
`target/` dir → cold workspace, the racing condition. Expected output
written BEFORE implementation: **red today** with lsp-strategy count = 0
(the assert message must show the count so red is diagnosable); green
post-S4 with count ≥ 1.

**Loop budget:** test-only DB scan, O(refs in fixture) ≈ 10¹ rows. No
production loops.

**Wall budget:** n/a (ignored test, nightly job; observed class ~15-30s
with rust-analyzer load).

**Files:** `tests/lsp_resolution.rs`.

**Code (advisory):** new `#[ignore = "requires rust-analyzer installed"]`
test `lsp_pipeline_binds_refs_on_cold_workspace`; fixture crate with
`trait Processor` / `impl` / `dyn` call; `index_with_options(IndexOptions::with_lsp())`;
then `SELECT COUNT(*) FROM refs WHERE strategy = 'lsp'` ≥ 1 via the
test's own rusqlite connection to the workspace DB.

**Verification:**
- [ ] Test compiles; runs RED against unfixed code with count = 0 (record output in `.tethys-2mjj/slice1-red.txt`)
- [ ] Red is for the right reason (0 lsp binds, not a panic/fixture error)
- [ ] Full gate on the branch (test is `#[ignore]`d so CI stays green)

## Slice 2: serverStatus classifier — pure, unit-fenced

**Claim:** C4 — `{quiescent:true}` (any health) classifies Ready
(degraded health flagged for logging); `{quiescent:false}` or malformed
params classify keep-waiting; never panics. (Carries C5's fence:
`quiescent:true + health:error` → Ready is exactly the broken-workspace
quick-return.)

**Oracle:** hand-written expected values from the rust-analyzer LSP
extension contract as OBSERVED in probe logs — including the verbatim
captured JSON lines from `probe-cold.log` (`{"health":"ok","quiescent":false}`
at t=0.06s, `{"health":"ok","quiescent":true}` at t=8.70s) and
`probe-edge.py` output (`health:"warning"/"error"` with `quiescent:true`)
as test inputs. Production-shape data, not invented fixtures.

**Stress fixture:** malformed shapes designed to fail plausible bugs:
missing `quiescent` field (bug: unwrap panic), `quiescent` as string
`"true"` (bug: loose truthiness), missing `health` (bug: assuming it's
always present — classify Ready/not-degraded, quiescent governs), `null`
params (bug: panic), first-notification-is-ready bug (input
`quiescent:false` must NOT classify Ready).

**Loop budget:** none — pure function over a tiny JSON value.

**Wall budget:** n/a.

**Files:** `src/lsp/status.rs` (new), `src/lsp/mod.rs` (wiring).

**Code (advisory):** `pub(crate) enum ReadyState { Ready { degraded: bool }, NotReady }`
+ `pub(crate) fn classify_server_status(params: &Value) -> ReadyState`
(serde-free manual field reads, mirroring the existing progress parsing
style). Unit tests: `ready_classifier_quiescent_ok`,
`ready_classifier_quiescent_degraded`, `ready_classifier_not_quiescent`,
`ready_classifier_malformed_params`, plus the captured-JSON cases.

**Verification:**
- [ ] Unit tests pass (each shape a distinct test name — failure localizes)
- [ ] Stress shapes produce written-down expected outcomes
- [ ] Full gate

## Slice 3: capability advert + `wait_for_quiescence` drain loop

**Claim:** C1 — the advert (`experimental.serverStatusNotification: true`)
is what makes rust-analyzer send serverStatus (probe-proved: 0
notifications without it); C6's loop shape — drain until Ready, timeout
→ `Ok(false)`.

**Oracle:** for the advert: the serialized `ClientCapabilities` JSON
asserted at the exact path `capabilities.experimental.serverStatusNotification == true`
— independent of the drain loop; validated against the raw probe request
that provably elicited notifications. For the loop: shape mirrors the
production-proven `wait_for_solution_load` (same timeout-at-loop-top,
same null-ack of server→client requests); behavioral verification lands
in S4's live tests.

**Stress fixture:** the advert unit test is designed to fail the
plausible nesting bug — advert placed under `general` (where 2d1x's
encodings went) instead of `experimental`, or wrong casing
(`server_status_notification`); asserting the full JSON path defeats
both. Loop stress (live) is S4's zero-timeout test + S1's message storm
(~1.9k notifications, interleaved server→client requests observed in
probes).

**Loop budget:** drain loop is O(messages until quiescence): observed
~2×10³ messages on the cold tethys workspace, hard-bounded by
`lsp_timeout_secs` (60s) at loop-top; runs ONCE per opt-in `--lsp`
session, not per ref. Well under 10⁶ ops / 10³ syscalls-per-always-on
budget (and not always-on).

**Wall budget:** ≤ `lsp_timeout_secs` (60s default) worst case, observed
8.7s cold / 4.9s warm — this latency IS the fix (queries answerable
afterward), only on `--lsp` runs.

**Files:** `src/lsp/transport.rs`.

**Code (advisory):** advert in `initialize()`'s `ClientCapabilities`
(`experimental: Some(json!({"serverStatusNotification": true}))`);
`pub fn wait_for_quiescence(&mut self, timeout: Duration) -> Result<bool>`
— clone of `wait_for_solution_load`'s loop with the progress-title match
replaced by `classify_server_status` on `experimental/serverStatus`
notifications; `Ready{degraded:true}` logs a warning, both Ready arms
return `Ok(true)`. Doc comment: no load-bearing preconditions — safe to
call at any point after `initialize` (if the server is already quiescent,
the initial status notification arrives immediately: probe-edge observed
0.05s); timeout returns `Ok(false)` and callers proceed degraded
(documented, matches C# posture).

**Verification:**
- [ ] Advert unit test passes (JSON-path assert)
- [ ] Unit tests + full gate
- [ ] No new stdout writes (all diagnostics via `tracing` → stderr)

## Slice 4: dispatch in Pass 3, zero-timeout fence, C8 goes green

**Claim:** C7 — CSharp routes to `wait_for_solution_load` (unchanged),
Rust routes to `wait_for_quiescence`; C6 — zero timeout returns false
immediately (no hang); C8 — S1's test goes green.

**Oracle:** C8: S1's DB-column oracle, now expected ≥1. Self-index
cross-check: `tethys index --lsp` on the tethys workspace itself, then
count `strategy='lsp'` refs — the same workspace where `probe.py`'s
independent poll channel proved post-quiescence resolution; pre-fix
count is 0 (2d1x subagent observation + this design's C8 premise), post-fix > 0.
C6: wall clock — the zero-timeout call must return in ≪1s with `Ok(false)`.

**Stress fixture:** (a) zero-timeout ignored test
`readiness_wait_returns_false_on_zero_timeout` — designed to fail the
plausible bug of checking timeout AFTER a blocking read (test hangs/fails);
(b) S1's cold-fixture test — designed to fail dispatch inversion (Rust
sent to the C# title-matcher can never see readiness → 0 binds → red) and
wait-after-queries ordering; (c) dispatch unit test
`readiness_dispatch_per_language` on a pure `fn` mapping
`Language → WaitKind` — fails the "simplify the match" rerouting bug.

**Loop budget:** no new loops (dispatch is a two-arm match; comment fix
is prose).

**Wall budget:** adds the S3 wait once per Rust `--lsp` session (≤60s,
observed ≤8.7s); per-ref query loop unchanged.

**Files:** `src/resolve.rs`, `tests/lsp_resolution.rs`.

**Code (advisory):** replace the `if language == Language::CSharp` block
with a match calling the per-language wait; both arms keep the existing
`Ok(true)/Ok(false)/Err` logging posture; replace the false comment with
one citing probed behavior ("rust-analyzer loads the workspace
asynchronously; pre-quiescence queries return empty or -32801 — see
.tethys-2mjj/findings.md").

**Verification:**
- [ ] S1's test now GREEN (record output in `.tethys-2mjj/slice4-green.txt`), 3 consecutive runs stable
- [ ] Zero-timeout test green; dispatch unit test green
- [ ] Self-index oracle: strategy='lsp' count on tethys workspace 0 → >0 (record numbers)
- [ ] Full gate (`cargo nextest run`, clippy pedantic `-D warnings`, `cargo fmt --check`, doctests — real exit codes)
- [ ] Impact analysis: `wait_for_solution_load` callers unchanged (grep — this slice edits tethys's own resolver path, so grep is the source of truth, not `tethys callers`)

## Plan self-review

1. **Loops:** S3 drain loop — O(messages-to-quiescence), ~2×10³ observed,
   timeout-bounded, once per opt-in session: within budget. S1/S4 test
   DB scans O(10¹). No other new loops. No gaps.
2. **Fixtures:** S1 — dyn-dispatch ref on a cold workspace (fails
   readiness race and dispatch inversion); S2 — malformed/loose-typed
   params + verbatim captured production JSON (fails panic and
   truthiness bugs); S3 — JSON-path advert assert (fails wrong-nesting
   bug); S4 — zero-timeout (fails timeout-after-read bug), dispatch unit
   (fails rerouting), S1-rerun (fails ordering). All adversarial, none
   happy-path-only. No gaps.
3. **Doc-comment preconditions:** `wait_for_quiescence` has no
   load-bearing precondition (safe at any post-initialize point; timeout
   → documented `Ok(false)` degraded path — runtime-handled, not
   assert-dependent). Classifier: total function, no preconditions. No
   enforcement gaps.
4. **Write targets:** all new output is `tracing` diagnostics → stderr.
   No stdout writes. Test artifacts (`slice1-red.txt`,
   `slice4-green.txt`) are audit files in `.tethys-2mjj/`. No gaps.
5. **Tracker references:** tethys-xpc4 (nightly, verified open),
   tethys-k543 (blocked-by edge, verified), existing-LSP-test
   strengthening queued in `.tethys-2mjj/to-file.md` per ship
   parallel-work rule (files at close-out). No unfiled deferrals.
