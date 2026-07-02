---
status: accepted
---

# Language-specific module semantics live behind the ModuleResolver seam

The indexing and reference-resolution drivers (`src/resolve.rs`, `src/indexing.rs`,
`src/batch_writer.rs`) are kept strictly language-neutral; every language-specific
module semantic — Rust `crate::`/`self::`/`super::` path resolution, C#
namespace/using corroboration, import-string joining — lives behind the
`ModuleResolver` trait (`src/languages/module_resolver.rs`), and `ModuleResolver`
implementations may not touch the database. This keeps "add a language" a fixed,
driver-free procedure and stops the drivers from accreting per-language special
cases — the failure mode the pre-seam `resolve.rs` had already fallen into (9
Rust-specific matches at git `d5cb3d3`).

## Enforcement

The seam is not a convention — `tests/seam_lint.rs` greps the driver sources at
build time and fails CI if:

- Rust module keywords (`"crate"`, `"self"`, `"super"`), `resolve_module_path`
  calls, or `CrateInfo` handling appear in `resolve.rs`;
- `indexing.rs` calls `resolve_module_path` directly or reintroduces the deleted
  C# namespace post-pass (`resolve_csharp_dependencies`);
- driver code joins import segments with a raw `.join(".")` instead of going
  through `ModuleResolver::join_import`;
- a resolver implementation reaches for the database (`use crate::db`, `&Index`).

## Consequences

New resolution logic must route through the trait, not the drivers; a "quick fix"
placed in a driver fails the build by design. The considered alternative —
letting the drivers branch on `Language` inline — is simpler in the small but is
exactly the entropy the seam exists to prevent, and it had already begun before
the seam was introduced.
