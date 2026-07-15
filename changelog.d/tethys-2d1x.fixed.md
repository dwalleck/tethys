- `tethys index --lsp` and `tethys callers --lsp` now negotiate the LSP
  position encoding, so references on lines containing non-ASCII text
  (CJK strings, em-dashes, emoji) resolve to the right symbol instead of
  missing or hitting the wrong one.
