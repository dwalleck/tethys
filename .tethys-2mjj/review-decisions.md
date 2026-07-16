# Review-feedback decisions — PR #30 (tethys-2mjj)

Findings assessed per `gilfoyle:assessing-review-feedback`: each bug
claim verified against primary sources before any change. Commits
referenced are on `fix/tethys-2mjj-lsp-readiness-wait`.

| # | Finding (one line) | Axis | Category | Verified? | Decision | Note |
|---|---|---|---|---|---|---|
| 1 | Language dispatch in resolve.rs violates the module-resolver seam ADR | Standards | Design | Partially — the enforced seam (`tests/seam_lint.rs`) is module-resolution-scoped and passes; the replaced code was already language-branched inline. But ADR 0001's rationale ("drivers must not accrete per-language special cases") is real, and `resolve_via_lsp` already holds the provider | **Modify** | "Hard violation" overstated; fix right anyway. `LspProvider::readiness_wait()` + `LspClient::wait_until_ready(kind, timeout)`; resolve.rs now language-free for readiness. Fence relocated to provider tests (`readiness_wait_per_provider`, `any_provider_delegates_readiness_wait`). Commit `1e29b5b` |
| 2 | Protocol magic strings ("health", "ok", "experimental/serverStatus") need consts/enums per "Rust best-practices Rules 7-8" | Standards | Style | No repo-documented standard requires this; house idiom compares `$/progress` inline in the same file | **Modify (minimal)** | `SERVER_STATUS_METHOD` const colocated with the classifier — the one string split across two modules (real drift risk). Health-as-enum rejected: classifier semantics is "anything but ok" over an extension protocol's open set. Commit `2337586` |
| 3 | Vocabulary: "DB"/"database" for the index, bare "resolver" | Standards | Convention | Yes — CONTEXT.md:12-17 (Index canonical) and :102-118 (qualify resolution) confirmed | **Accept** | Test doc comment + flagged design/plan lines fixed; probe logs untouched (verbatim captured evidence). Commit `9200d69` |
| 4 | Unify the two wait drain-loops into one classification-parameterized primitive | Standards | Design (judgement) | Yes, the shape similarity is real | **Reject** | Shared mechanics already extracted (`read_message`, `ack_server_request`, commit `83de0b7`). Remaining divergence is stateful token-tracking vs stateless classification + intentionally different deadline semantics; unification would mutate `wait_for_solution_load` against the approved design's byte-for-byte-unchanged C# constraint, for two call sites. Considered rejection, not a deferral |
| 5 | probe.py / probe-noadvert.py duplicate the JSON-RPC client; use one probe with a flag | Standards | Polish (judgement) | Yes, they differ by one line (generated via `sed`, self-documenting) | **Reject** | Audit-trail artifacts are immutable evidence: probe-noadvert.py is the exact script that produced the committed probe-noadvert.log (the C1 falsifier). Rewriting one-shot falsifiers post-run falsifies provenance and buys no maintenance (they are never re-run as a pair) |
| 6 | Positive timeout can block forever if the server goes silent (no read deadline); only Duration::ZERO is tested | Spec | Bug | Yes at mechanism level — `read_message` blocks with the deadline checked between messages. But: exposure is transport-wide and pre-existing (`read_response`, `wait_for_solution_load` identical), server death IS handled (EOF → Err → warn → proceed), rust-analyzer emits serverStatus ~60ms post-initialize, and the posture is documented in the method docs and was surfaced in the plan critique | **Reject (defer)** | Real robustness gap, wrong scope: the fix is a transport-wide design change (reader thread / non-blocking IO) plus a scriptable fake-server harness for the missing positive-timeout test. Queued as entry 2 in `.tethys-2mjj/to-file.md` → files as a rivets bug at close-out (tracker checkout owned by a parallel session, per the ship-skill convention; ID will be recorded in the close-out commit) |

Verification sources: `docs/adr/0001-module-resolver-seam.md`,
`tests/seam_lint.rs` needles, `CONTEXT.md:12-17,102-118`, transport.rs
house idiom, probe artifacts. Post-decision gates: full gate green
(nextest, clippy pedantic, fmt, doctests) and the ignored LSP suite
14/14 after the dispatch refactor.

Healthy-review sanity check: 6 findings → 1 accept, 2 modify, 3 reject
(one with tracker-taxed deferral).
