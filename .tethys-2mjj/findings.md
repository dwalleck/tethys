# Probe findings — tethys-2mjj (rust-analyzer readiness before Pass 3)

Probes: `probe.py` (full timeline: signal channel vs query-success oracle,
run cold and warm against the real tethys workspace), `probe-edge.py`
(broken-workspace signal behavior). Logs: `probe-cold.log`,
`probe-warm.log`. rust-analyzer 1.94.0 (4a4ef49 2026-03-02).

## Smallest question

Does rust-analyzer emit a discoverable end-of-load signal over LSP, and
does that signal coincide with `goto_definition` starting to answer?

## Probe (signal channel)

Raw JSON-RPC client advertising `window.workDoneProgress`,
`general.positionEncodings: ["utf-8","utf-16"]`, and
`experimental.serverStatusNotification: true`; logs every `$/progress`
and `experimental/serverStatus` notification with timestamps.

## Oracle (independent channel)

Poll `textDocument/definition` every 500ms from t=0 at the call site of
`from_initialize_result` (src/lsp/transport.rs:165 → expected target
src/lsp/encoding.rs:31), recording each outcome. Readiness per the oracle
= first HIT from a request *sent after* the signal. The oracle never
reads the progress/status channel; the probe signal never looks at query
results. Different mechanisms, same question.

## Agreement

| Run  | Signal says ready (quiescent=true / cachePriming end) | Oracle: polls before signal | Oracle: first fresh post-signal query |
|------|------|------|------|
| cold (no target/) | 8.70s | EMPTY `[]` (0.10s, 0.65s), `-32801` (5.66s) | HIT encoding.rs:31 (~9.5s; ~0.4s response), stable ×3 |
| warm | 4.88s | EMPTY `[]` (0.10s, 0.65s) | HIT encoding.rs:31 (5.51s), stable ×3 |

Agreement on both runs: every query resolved before the signal fails
(empty or cancelled); every fresh query after the signal hits the correct
target. One caveat observed on BOTH runs: a request already in flight
when quiescence flips gets cancelled with `-32801 content modified` —
the gate must complete before requests are sent (Pass 3's shape already
does this: wait once, then loop).

## What I learned (didn't know before)

1. **The silent failure is EMPTY, not an error.** For the first ~1s of
   load, `goto_definition` returns `[]` — indistinguishable from "no
   definition exists". Pass 3 records it as unresolved and moves on; no
   retry, no error count. Mid-load, queries instead fail with `-32801`.
   Both shapes observed cold and warm.
2. **rust-analyzer has a designed readiness signal tethys never asks
   for**: `experimental/serverStatus {health, quiescent}` — sent only if
   the client advertises `experimental.serverStatusNotification: true`
   (tethys today does not). It fires immediately (`quiescent: false` at
   0.06s) and flips `quiescent: true` exactly at load-end, coinciding
   with `rustAnalyzer/cachePriming` end.
3. **The existing csharp-ls matcher can never fire for Rust**:
   rust-analyzer's progress titles are `Fetching`, `Building CrateGraph`,
   `Roots Scanned`, `Loading proc-macros`, `Building compile-time-deps`,
   `Indexing` (token `rustAnalyzer/cachePriming`) — none starts with
   "Loading workspace". Also progress phases interleave and repeat
   (Fetching begins three times cold), so "first end" heuristics on
   progress tokens are unreliable; quiescent is the aggregate signal.
4. **Broken workspace cannot hang a quiescence-keyed wait**: with no
   Cargo.toml, rust-analyzer reports `quiescent: true` within 0.05s with
   `health: warning` then `error` ("Failed to discover workspace"). The
   wait returns immediately; `health` discriminates real readiness from
   "ready because there is nothing to do".

## Residual risks for the design to address

- Theoretical window where the very first serverStatus on a healthy
  workspace could read `quiescent: true` before load work is enqueued
  (never observed — both runs opened with `quiescent: false` at 0.06s).
  Cheap insurance: a short post-quiescence settle or a `-32801`-retry on
  the first queries.
- `-32801` on requests straddling the quiescence boundary (observed both
  runs) — same insurance applies; Pass 3's wait-then-loop shape avoids
  it structurally.
- Timings are per-machine: 8.7s cold / 4.9s warm on the tethys workspace
  here; larger workspaces scale up. The wait needs the same timeout
  posture as the csharp-ls path (proceed with a warning on timeout).

## Post-implementation observation (2026-07-15, slice 4 oracle run)

Self-indexing tethys with the fix: strategy=lsp bind count went 0 → 396.
One nuance the 3-poll probes could not see: on a self-index run started
seconds after a `cargo build` had mutated `target/`, a burst of
post-quiescence queries failed with transient `-32801` (rust-analyzer
re-fetching the changed build directory mid-session); an immediately
following steady-state run had zero such errors. The affected refs were
std-method names (`iter`, `map`, `collect`) that cannot match back to
in-workspace definitions regardless, the error path is the existing
counted-and-logged one, and the terminal state (ref stays unresolved)
equals pre-fix behavior — so the no-retry negative-space decision stands,
now with its boundary documented rather than assumed universal.
