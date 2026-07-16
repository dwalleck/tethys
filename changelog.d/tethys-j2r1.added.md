- New `tethys hierarchy <TYPE> [--direction up|down|both]` command: walk a
  type's implemented traits / base types upward and its implementors /
  derived types downward, transitively. External supertypes (`Display`,
  `Send`) appear as name-only entries.
- The index now records inheritance edges for Rust `impl Trait for Type`,
  supertrait bounds, and C# base lists — plus per-method markers on
  trait-impl methods, groundwork for dead-code suppression.
- `callers`/`impact` are unaffected: implementing a trait is not a call.
