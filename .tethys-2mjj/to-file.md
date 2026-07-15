# Issues to file at close-out (tracker checkout owned by parallel session)

1. **test(lsp): existing ignored LSP integration tests never assert an
   LSP-contributed bind** — they pass in ~0.07s via tree-sitter-resolved
   data alone (observed during tethys-2d1x and tethys-2mjj). Once 2mjj's
   readiness wait merges, they can cheaply assert `strategy=lsp` on at
   least one ref each, turning them into real LSP fences for the nightly
   job (tethys-xpc4). Type: task. Priority: P3. Relates: tethys-xpc4,
   discovered-from tethys-2mjj.
