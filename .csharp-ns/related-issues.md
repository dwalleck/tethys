# Prior art (tracker scan, 2026-06-06)

- **tethys-jwf9 [open]** — the feature itself (two-sentence body, pre-seam).
- **tethys-nmsp [open, depends on jwf9]** — fold the pre-existing
  `resolve_csharp_dependencies` namespace-map post-pass into the seam.
  Filed during PR #1 review. Scope-boundary question for THIS loop.
- **tethys-8mze [open]** — language expansion epic; jwf9 is the first
  non-declining non-Rust ModuleResolver and will harden the trait contract.
- **tethys-dsp1 [open]** — display spelling; tangential.
- **tethys-itez / tethys-z45p / tethys-778r [open]** — C# parser gaps
  (unsafe modifier, generics); tangential but may limit which using-forms
  the extractor surfaces.
- **tethys-xov3 [closed]** — nested-type extraction (done).

Established facts carried over from the separator-fix loop (probed, not assumed):

- C# qualified refs (`Foo.Bar()`) ALREADY resolve via the qualified-name
  fallback — jwf9's gain is elsewhere.
- C# using-directives are stored as glob import rows (`*|System`,
  `*|MyApp.Models`) — Pass 2's glob-import arm is where namespace
  resolution would fire, for SIMPLE-NAME refs.
- File-level C# deps ALREADY exist via the namespace-map post-pass
  (`Auth.cs→Hasher.cs` in the PR-1 baseline).
- Trait-shape tension: `resolve_import` returns `Option<PathBuf>` (one
  file); a C# namespace spans many files.
- Seam fences in force: resolver impls are DB-free and stateless
  (tests/seam_lint.rs C10); any namespace map must arrive via context.
