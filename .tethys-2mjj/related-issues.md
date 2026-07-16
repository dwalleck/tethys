# Related issues — tethys-2mjj probe (searched 2026-07-15)

Tracker searched for: readiness, progress, workDone, solution load, wait,
-32801, content modified, initialize.

- **tethys-2d1x** (closed) — position encoding fix (PR #27). This bug was
  discovered during that fix; the PR's fence test pins at the `LspClient`
  seam with its own readiness poll precisely because the pipeline-level
  shape can't go green until 2mjj is fixed. `discovered-from` edge exists.
- **tethys-k543** (open, blocked by 2mjj) — Pass 3 re-verifies
  speculative-band resolutions with REBIND authority. A rebind pass built
  on a racing query would inherit the race; that's why the blocks edge
  exists.
- **tethys-xpc4** (open) — nightly workflow running the #[ignore]d LSP
  tests. The pipeline-level "binds with strategy=lsp" red test this fix
  enables belongs in that job.
- **tethys-9o82** (closed) — added the gated LSP integration tests. Those
  tests pass via tree-sitter data alone (~0.07s) and never verify an
  LSP-contributed bind — the observable consequence of this bug.
- **tethys-nwwm** (closed) — original RustAnalyzerProvider implementation;
  where the "responds immediately, so no wait needed" comment entered.
- **tethys-6x7g** (closed) — CSharpLsProvider; introduced
  `wait_for_solution_load` (csharp-ls "Loading workspace" title match),
  the C#-only readiness precedent this fix generalizes.

No open duplicate of the readiness race itself. No prior ticket on
rust-analyzer progress tokens.
