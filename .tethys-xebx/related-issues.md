# tethys-xebx — prior art (tracker sweep, 2026-07-05)

- **tethys-haw5** (closed) — parent probe that discovered this gap; shipped C#
  attribute extraction + deprecated-callers C# support for methods. Its
  `.tethys-haw5/findings.md` documents the substrate facts this issue stands on
  (attribute storage shape, conservative C# resolution, `function` = static
  method vocabulary).
- **tethys-53iv** (open, P2) — Rust method calls resolve by name only;
  misattribution on external types. The C# side's unique-or-decline rules are
  the intended contrast; member-read resolution must not loosen them. Same-file
  Pass-1 name matching carries the same theoretical misattribution shape for
  properties (local `Data` property + external `x.Data` read in one file) —
  accepted as consistent with existing method behavior, not new exposure.
- **tethys-xov3** (closed) — C# nested-type extraction; substrate for member
  declarations inside nested classes/records/structs.
- **tethys-itz7** (closed) — imports stored for Rust + C#; feeds the
  `import_union` arm that member reads rely on for cross-file resolution.
- **tethys-jdly** (closed) — Rust deprecated-callers analysis (PR #9); the
  analysis is kind-agnostic and needs no change for property symbols.
- **tethys-ygjx** (closed, PR #19) — fn-as-value refs: closest precedent for
  adding a new reference kind end to end (extractor emission, Pass-2
  resolution, `drop_unresolved_value_refs`, call-edge exclusion, determinism
  fences in `tests/value_refs.rs`).
- **tethys-l6nt** (open, P1) — PRD; C# parity user story this serves.

No open issue duplicates xebx's scope (properties/fields/events/delegates as
symbols; member-access reads as refs).
