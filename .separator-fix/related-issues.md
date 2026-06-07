# Prior art (tracker scan, .rivets/issues.jsonl, 2026-06-06)

Directly relevant:

- **tethys-jwf9 [open]** — "C# namespace resolution for using statements."
  `resolve_import()` in `csharp.rs:109-111` returns empty vec; C# `using` statements
  don't resolve to files at all. Was "Task 6" in the original TODO.
  **Implication: even with the separator fixed, the explicit-import and
  module-path arms of qualified resolution dead-end for C# — there is no
  namespace→file mapping. The separator fix and jwf9 are separable but the
  measurable C# win may require both.**

- **tethys-8mze [open]** — "Expand language support: Python, TypeScript, Go" (epic).
  The separator seam is a prerequisite-shaped refactor for this epic.

No existing issue covers the `::` hardcoding in `resolve.rs` itself. This fix is new.

Relevant code facts established during scan:

- C# extraction layer is already separator-free: `ExtractedReference { name, path: Option<Vec<String>> }`
  (`csharp.rs:254`, `csharp.rs:295`). The separator is baked in at write time
  (`batch_writer.rs:378-380`: Rust → `"::"`, C# → `"."`).
- `csharp.rs:736` comment says nested types build qualified names like `Outer::Inner` —
  C# symbol storage may contain MIXED separators. Must be probed before design.
