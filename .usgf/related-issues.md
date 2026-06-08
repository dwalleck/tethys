# Prior art (tracker scan, 2026-06-07)

- **tethys-usgf [open]** — the feature; bundles three using-forms.
- **tethys-jwf9 [closed]** — plain `using Namespace;` resolution (the
  mechanism this loop extends). Probe artifacts in `.csharp-ns/`.
- **tethys-nnst [open]** — nested block namespaces don't enter the map;
  adjacent gap, NOT this loop.
- **tethys-itez / tethys-778r [open]** — C# parser: unsafe modifier +
  generic param extraction. The usgf body flags these as possibly adjacent
  ("may surface parser gaps"). Distinct concern.
- **tethys-8mze [open]** — language expansion; consumes the seam.
- **tethys-dsp1 / tethys-mpth [open]** — display spelling / type hardening;
  tangential.

Established facts (probed in the jwf9 loop, .csharp-ns/probe-findings.md):
- `using static My.Models.Helper;` → stored glob row `*|My.Models.Helper`;
  **is_static is DROPPED** at csharp.rs to_import_statement.
- `using W = My.Models.Widget;` → stored `*|My.Models.Widget|W` (alias kept).
- `global using My.Globals;` → plain glob row in ITS OWN file only.
- Workspace-UNIQUE names already resolve via the fallback regardless of
  using-form (the jwf9 lesson — the gain is always disambiguation, never
  first-time resolution).
- The C# glob arm is types-only [Class,Struct,Interface,Enum].

Decomposition note: three forms, three mechanisms —
1. static: member-kind resolution + is_static propagation to storage.
2. alias: single-target rename (explicit-import arm, not glob).
3. global: cross-file using propagation (breaks the per-file model).
The interrogation must pick. Each remaining form stays declining and gets
its own follow-up issue.
