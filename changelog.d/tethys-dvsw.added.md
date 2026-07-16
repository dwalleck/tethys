- New `tethys dead-code` command lists non-public symbols with zero
  inbound references, each tiered `Definite` (the name appears nowhere
  else in the indexed sources) or `Maybe` (it appears in text the
  indexer cannot resolve — macro arguments, format strings, comments —
  so verify before deleting).
- `--json` emits a machine-readable `{findings, summary}` report;
  `--limit N` truncates the listing while the summary keeps full counts.
- Public symbols, test code, entry points (`main`, C# `Main`), and
  trait-implementation methods are never reported; on a workspace that
  compiles warning-free the `Definite` tier is empty by construction.
