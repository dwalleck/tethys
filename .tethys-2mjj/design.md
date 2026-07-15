# Design — tethys-2mjj: gate Pass 3 on rust-analyzer readiness

Inputs: `findings.md` (probe/oracle agreement, cold + warm + broken
workspace), `probe*.log` (raw timelines), rust-analyzer 1.94.0.

## Purpose

Pass 3 LSP resolution queries rust-analyzer immediately after
`initialize`, while its asynchronous workspace load is still running.
Probed consequence: queries return `[]` (indistinguishable from "no
definition") or `-32801 content modified`, so `--lsp` runs on cold Rust
workspaces silently resolve nothing. The existing readiness wait
(`wait_for_solution_load`) matches csharp-ls's "Loading workspace"
progress title — which rust-analyzer never emits — and is gated to C#.

## Core rule

After file pre-opening and BEFORE the per-ref query loop, Pass 3 waits
for server readiness, dispatched by language:

- **CSharp** → existing `wait_for_solution_load` (byte-for-byte
  unchanged).
- **Rust** → new `wait_for_quiescence(timeout)`: drain server messages —
  acking server→client requests with `null`, same as the existing loop —
  until an `experimental/serverStatus` notification classifies **Ready**
  (`quiescent == true`, any `health`; `health != "ok"` logged as a
  warning). Timeout → `Ok(false)`, warn, proceed (same posture as the
  C# path).

Enabled by advertising `experimental.serverStatusNotification: true` in
`ClientCapabilities` — unconditional for both providers (csharp-ls
handshake verified tolerant, 0.60s INITIALIZE OK with the advert).
Timeout reuses `lsp_timeout_secs` (default 60s; observed need 8.7s cold
on the tethys workspace itself).

The false comment at `src/resolve.rs:807-809` ("responds immediately, so
no wait needed") is replaced by one citing the probed behavior.

## Components

- `src/lsp/transport.rs` — capability advert in `initialize`; pure
  classifier for serverStatus params (unit-testable without a process);
  `wait_for_quiescence` drain loop (same shape as the proven
  `wait_for_solution_load`).
- `src/resolve.rs` — two-arm language dispatch replacing the C#-only
  `if`; corrected comment.
- `tests/lsp_resolution.rs` — new `#[ignore]`d pipeline-level test: cold
  fixture, full index with LSP, assert ≥1 ref binds with `strategy=lsp`
  (red today; runs in nightly, tethys-xpc4).

## Input shapes

- **`Language`**: `Rust` (new wait; C4/C6/C8), `CSharp` (existing wait;
  C7). Exhaustive — enum has two variants.
- **serverStatus param shapes**: `{quiescent:true, health:"ok"}` (C2,
  C4), `{quiescent:true, health:"warning"/"error"}` (C4, C5 — broken
  workspace), `{quiescent:false}` (C4 — keep draining), malformed /
  missing fields (C4 — keep draining, never crash), **never arrives**
  (C6 — old rust-analyzer or non-supporting server → timeout path).
- **Interleaved drain traffic**: `$/progress` storms (~1900
  notifications on the cold run), repeated/interleaved progress phases,
  server→client requests needing null-acks (`workspace/configuration`,
  `client/registerCapability` observed) — C8 exercises the full mix.
- **Timeout**: zero (C6 immediate-false variant), default 60s (C8),
  elapsed-mid-drain (C6).
- **Workspace**: cold (C8), warm (C2), broken/no-Cargo-project (C5),
  larger-than-timeout (C6 posture; not separately fixtured — same code
  path as never-arrives).

Out of scope shape: languages beyond the two enum variants (none exist);
non-UTF-8 position encodings are already handled by tethys-2d1x's module
and unaffected by when queries are sent.

## Subtractive sweep

Purely additive: the change adds a bounded wait before existing queries
and narrows nothing — the C#-only conditional becomes a two-arm dispatch
with both arms preserved, and no lock, ordering, guard, or uniqueness
property is removed. (Observable side effect, intended: `--lsp` runs
gain up-front latency equal to server load time — bounded by the
timeout — in exchange for queries that can actually answer.)

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| C1 | rust-analyzer sends `experimental/serverStatus` iff the client advertises `experimental.serverStatusNotification` | Run probe with and without the advert; any SERVERSTATUS line in the no-advert log falsifies (as would zero lines in the advert log) | Raw JSON-RPC logs (`probe-warm.log` vs `probe-noadvert.log`), no tethys code involved | 3m | **passed** (stream vs 0 lines) | unit test: capabilities JSON includes the advert (same pattern as 2d1x's capability test) |
| C2 | A fresh `goto_definition` sent after `quiescent:true` resolves to the correct target | Poll from t=0; a post-quiescence fresh request returning empty/wrong target falsifies | Independent poll channel vs signal channel (`probe-cold.log`, `probe-warm.log`) | run | **passed** (6/6 hits at encoding.rs:31, cold+warm) | `#[ignore]`d pipeline test (C8's), nightly |
| C3 | Pre-quiescence queries return only `[]` or `-32801` — never a wrong non-empty binding (bug is silent miss, not corruption) | Any pre-quiescence poll returning a non-empty wrong location falsifies | Same logs, pre-signal section | run | **passed** (6/6: 4×EMPTY, 2×-32801) | **manual** (audit logs; post-fix the product path cannot issue pre-quiescence queries) — needs explicit approval |
| C4 | Classifier: `quiescent:true` (any health) → Ready; `quiescent:false` or malformed → keep waiting | Unit tests over the four param shapes; wrong classification falsifies | Hand-written expected values from the LSP extension contract observed in probes | with impl | pending | unit tests `ready_classifier_{quiescent_ok,quiescent_degraded,not_quiescent,malformed}` |
| C5 | Broken workspace (no Cargo project) cannot hang the wait: quiescent:true arrives immediately with degraded health | Point probe-edge at a Cargo-less dir; no quiescent:true within seconds would falsify | `probe-edge.py` output (0.05s, health warning→error) | run | **passed** | classifier unit test (`quiescent_degraded` → Ready) |
| C6 | No serverStatus within timeout → `Ok(false)`, warn, proceed — no hang, no error | Zero-timeout wait against live rust-analyzer must return false immediately; hang or `Err` falsifies | Wall clock + return value, vs the documented posture of the C# path | rides nightly | pending | `#[ignore]`d test `readiness_wait_returns_false_on_zero_timeout` |
| C7 | C# path unchanged and unbroken by the advert | csharp-ls initialize with the new capabilities; handshake failure falsifies. Dispatch routes CSharp to `wait_for_solution_load`; unit test on the dispatch falsifies rerouting | Live csharp-ls handshake (ran 2026-07-15, INITIALIZE OK 0.60s); git diff of the C# arm | 5m | **passed** (handshake); dispatch test with impl | dispatch unit test + existing suite |
| C8 | **Headline**: cold Rust fixture, full pipeline with LSP binds ≥1 ref with `strategy=lsp` (today: 0) | Run the new pipeline test against UNFIXED code — must fail (red); against fixed code — must pass; wrong direction falsifies design or test | The DB `refs` table strategy column, written by the pipeline, read by the test — independent of the wait's own logging | 30m write, ~30s run | pending — **red-first during build** | the same test, nightly (tethys-xpc4, verified open 2026-07-15) |

Cheapest-falsifier gate: C1 ran and passed before this doc was presented
(alongside C2/C3/C5/C7 carried by probe evidence). Pending claims C4, C6,
C8 are implementation-coupled; C8 runs red-first as the first build
slice.

Non-vacuity (buggy implementations each fence catches): C1 — advert line
dropped from `ClientCapabilities` (wait then always times out); C4 —
returning Ready on the first serverStatus regardless of flag
(`not_quiescent` test fails) or panicking on malformed params
(`malformed` test fails); C6 — timeout check missing from the drain loop
(zero-timeout test hangs/fails); C7 — "simplifying" the dispatch to send
C# through the quiescence wait (dispatch test fails; csharp-ls never
sends serverStatus, so C# LSP resolution would silently regress to
today's Rust behavior); C8 — any future change that reorders the wait
after the query loop or drops the advert (test observes zero lsp-strategy
binds and fails).

## Negative space (deliberately not doing)

1. **No `-32801` retry logic.** Settled rationale: 9/9 fresh
   post-quiescence requests hit across cold/warm/no-advert runs;
   cancellations only struck requests already in flight when state
   changed, which the wait-then-loop shape structurally avoids. A retry
   layer would mask real cancellation bugs.
2. **No CLI/stats surfacing of readiness** — timeout and degraded-health
   outcomes go to logs with the same posture as the C# path today.
3. **No new timeout flag** — the wait reuses `lsp_timeout_secs` (60s
   default) rather than growing config surface.
4. **Default (non-`--lsp`) path untouched** — zero behavior change when
   LSP is off.
5. **Existing ignored LSP tests are not rewritten** to assert
   `strategy=lsp` in this PR — queued in `.tethys-2mjj/to-file.md`
   (tracker checkout owned by a parallel session; filed at close-out per
   ship-skill convention).

## Deferrals / tracker references

- Nightly execution of the new ignored tests: **tethys-xpc4** (verified
  open, 2026-07-15).
- Pass 3 speculative re-verification that this bug blocks: **tethys-k543**
  (blocks edge verified — unblocks when this merges).
- Existing-LSP-test strengthening: queued in `.tethys-2mjj/to-file.md`,
  files at close-out.

## Open decisions for design approval

1. **Advert scope**: unconditional for both providers (recommended —
   csharp-ls verified tolerant; simpler) vs rust-analyzer-only.
2. **C3's regression fence is `manual`** (audit-trail logs only) — the
   skill requires explicit approval for a manual fence. Rationale: the
   fixed product path cannot reach pre-quiescence queries, so no CI test
   can exercise the claim without deleting the fix.
3. **Timeout reuse**: `lsp_timeout_secs` (60s) doubles as the readiness
   budget (recommended) vs a separate readiness timeout.
4. **Method name**: `wait_for_quiescence` (rust-analyzer-specific,
   recommended) vs generalizing `wait_for_ready(language)` inside the
   client.
