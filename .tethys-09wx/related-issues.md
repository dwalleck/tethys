# Related issues — tethys-09wx (tracker prior-art search)

Searched: `affected|staleness|stale|exit code|envelope|indeterminate`,
`stdout|tracing|names-only|log.*pollut|subscriber|stderr`.

- **tethys-gkt2** (in_progress, P2) — "Implement proper staleness check".
  Filed when `needs_update()` was a stub; `reindex.rs` now carries the real
  machinery (`get_stale_files`, `classify_indexed_file`). gkt2 is about
  skip-if-fresh at *index* time; 09wx reuses the same classifier per changed
  file at *query* time. Not blocking, no overlap in deliverable.
- **tethys-zwaz** (open, P3) — analysis-command output-envelope convergence.
  affected-tests is not one of zwaz's three commands; 09wx must compose (keep
  stdout pure data, stderr for reasons) rather than grow another envelope.
- **tethys-l6nt** (PRD) — user stories 11-12 (CI affected-tests, stable JSON /
  exit contracts); resolution-quality triggers explicitly deferred to Act 1
  resolver work.
- No existing ticket covers logs-on-stdout, the spurious relative-path WARN,
  or the `./`-prefix lookup miss — all three filed from this probe (IDs in
  findings.md).
