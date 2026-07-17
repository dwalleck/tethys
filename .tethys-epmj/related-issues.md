# Related issues — tethys-epmj (discriminated NotFound variants)

Tracker searched via python scan of `.rivets/issues.jsonl` for keywords
`notfound`, `packagenotfound`, `error variant`, `error::` (rivets has no
`search` subcommand). Matches reviewed:

- **tethys-byie** (open, P2) — parent epic: architecture analysis / coupling
  metrics. epmj is a child polish item on its error surface.
- **tethys-o4re** (open, P2) — MCP server for tethys tools. The *motivating
  consumer*: a future `tethys_coupling` MCP tool would need to distinguish
  package-not-found from file-not-found without string parsing. Not blocked
  by epmj, but epmj is sequenced "just before" it per the issue text.
- **tethys-l8ur** (open, P4) — PR #60 polish backlog (test-quality items on
  the same coupling code). Item 3 touches error-variant assertions on
  `get_package_coupling` corrupt-source tests (`Error::Internal`), adjacent
  but not overlapping with epmj.
- **tethys-zwaz** (open, P3) — CLI output convergence (envelope fences,
  BrokenPipe). Touches `Error::Internal` rendering in JSON envelopes, not
  NotFound discrimination. No overlap.

**No existing ticket already implements or supersedes the epmj fix.**

## Drift between issue text and current code (found during step 0)

1. Issue says the payload format is `'package: foo'`; the actual site
   (`src/cli/coupling.rs:101`) writes `package '{name}'`.
2. Issue says "Update get_package_coupling … to use PackageNotFound", but
   `get_package_coupling` (lib + db layers) returns `Result<Option<_>>` and
   never constructs `NotFound`; the only package-flavored construction is in
   the CLI layer (`run_detail_to`).

Both matter for the design: the variant's construction site is the CLI (and
the future MCP layer), unless the design deliberately moves the None→error
mapping into the library.
