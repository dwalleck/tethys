# Issues to file at close-out (tracker checkout owned by parallel session)

2. **lsp: transport reads have no deadline — a live-but-silent server can
   stall any wait or request past its timeout** — all transport reads
   (`read_response`, `wait_for_solution_load`, `wait_for_quiescence` via
   `read_message`) block on the pipe with the deadline checked only
   BETWEEN messages. Server death is handled (EOF → Err → warn →
   proceed), and rust-analyzer emits serverStatus ~60ms after initialize,
   so the exposure needs a pathological alive-but-silent server — but the
   posture is transport-wide and predates tethys-2mjj (documented in
   `wait_for_quiescence`'s docs and the 2mjj plan critique). A real fix
   is a design decision: deadline-aware reads via a reader thread or
   non-blocking pipe IO, applied to every transport path, plus a
   scriptable fake-server harness to test positive-timeout/no-message
   (rust-analyzer cannot be made silent on demand). Surfaced by PR #30
   spec review. Type: bug. Priority: P3. Relates: discovered-from
   tethys-2mjj.

1. **test(lsp): existing ignored LSP integration tests never assert an
   LSP-contributed bind** — they pass in ~0.07s via tree-sitter-resolved
   data alone (observed during tethys-2d1x and tethys-2mjj). Once 2mjj's
   readiness wait merges, they can cheaply assert `strategy=lsp` on at
   least one ref each, turning them into real LSP fences for the nightly
   job (tethys-xpc4). Type: task. Priority: P3. Relates: tethys-xpc4,
   discovered-from tethys-2mjj.
