- `tethys index --lsp` now waits for rust-analyzer to finish loading the
  workspace before resolving references, so LSP resolution actually
  contributes bindings on a cold workspace (previously its queries raced
  the load, silently resolved nothing, and the index looked complete).
