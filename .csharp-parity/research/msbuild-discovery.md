# Bound MSBuild C# discovery and evaluation

Research ticket `tethys-teax` (C# parity wayfinder). Branch `research/csharp-parity-msbuild`.
Companion tickets: `research/csharp-parity-lsp` (LSP), `research/csharp-parity-workflow` (CLI/workflow).

**Scope.** Primary-source survey of the official Microsoft/MSBuild/Roslyn APIs, command
surfaces, specifications, and upstream source for enumerating C# projects from `.sln`,
`.slnx`, `.slnf`, standalone `.csproj`, and legacy project layouts, and for evaluating
each candidate project to obtain target frameworks, `Compile` items, identity, and
references. This document compares mechanisms; it deliberately does **not** select an
approach or propose a tethys design.

**Method.** Every material external claim carries a direct primary-source citation
(Microsoft Learn, devblogs.microsoft.com, or an upstream repository file at a pinned
branch). Local claims cite exact repo paths/symbols on this branch's HEAD. Claims marked
`[E]` were additionally verified empirically on 2026-08-09 with the locally installed
`.NET SDK 10.0.102 (MSBuild 18.0.7)` on Linux x64, using throwaway fixtures under
`/tmp/msbuild-exp` (no main-worktree mutation). `[I]` marks inference. Scout outputs
(`agent://SourceParity`, `agent://SurfaceParity`, `agent://TrackerParity`) were used as
leads only; the material claims below were re-verified against the cited sources.

---

## 1. Repository-shape coverage matrix

Which discovery mechanisms can enumerate projects in which repository shapes.
Legend: **Y** = complete, **P** = partial/conditional, **N** = none/unsupported.

| Repository shape | Static XML parse | MSBuild evaluation API (`Project`/`ProjectInstance`) | Design-time build (`DesignTimeBuild=true` + targets) | `dotnet msbuild` query surface (`-getItem`/`-getProperty`/`-getTargetResult`) | Roslyn `MSBuildWorkspace` |
|---|---|---|---|---|---|
| Standalone SDK-style `.csproj` (no solution) | Y (item-level only) | Y (single evaluation; per-TFM re-evaluation if multi-targeted) | Y | Y (MSB1063 only applies to solution/filter files) | Y (`OpenProjectAsync`) |
| Tree of SDK-style `.csproj` (no solution) | Y per file, P for graph (must infer P2P from `ProjectReference` XML) | Y per project; graph via `ProjectReference` items or `-graphBuild` | Y per project | Y per project; `-graphBuild` for order | Y (follows `ProjectReference`) |
| `.sln` (Format 12.00) | Y (GUID/type/name/path lines are line-parseable; config sections too) | N (MSBuild does not evaluate `.sln` as XML; use `SolutionFile` API) | P (solution builds run each project's targets; solution-level `before/after.{sln}.targets`) | P (`dotnet sln list`; `-getItem`/`-getProperty` rejected with MSB1063 `[E]`) | Y (`OpenSolutionAsync`; Roslyn ships its own `SolutionFileReader`) |
| `.slnx` (XML solution) | Y (simple `<Solution>/<Project Path>` XML) | N (not an MSBuild project; parsed by `SolutionFile` via SolutionPersistence) | P (MSBuild 17.12+ builds `.slnx`; `before/after.{slnx}.targets` 17.14+) | Y (`dotnet sln add/remove/list/migrate` on `.slnx`; `dotnet build`/`dotnet msbuild` accept it) | Y (same `OpenSolutionAsync` path via `SolutionFileReader`) |
| `.slnf` (filter over `.sln`/`.slnx`) | Y (JSON: `solution.path` + `projects[]`) | N (no evaluation; MSBuild reads it as a filter) | P (MSBuild 16.7+ builds the filtered subset and its dependencies) | P (`dotnet sln list` on `.slnf` since .NET SDK 9.0.3xx; `-getItem` rejected, MSB1063 `[E]`) | Y (`SolutionFileReader.SolutionFilterReader` exists in Roslyn) |
| Legacy non-SDK `.csproj` (ToolsVersion, `Microsoft.CSharp.targets` import) | Y (explicit `<Compile>`/`<Reference>`/`<ProjectReference>` items) | P — evaluates on Linux with the .NET SDK because the SDK ships `Microsoft.CSharp.targets` `[E]`; item globs are absent by convention so explicit items dominate | P (targets exist; design-time protocol from VS applies) | P (`-getItem` works `[E]`; full build needs reference assemblies or `Microsoft.NETFramework.ReferenceAssemblies`) | P (`BuildHostProcessKind.Mono`/`NetFramework` hosts exist for old-style projects; SDK-style assumption otherwise) |
| Mixed-language solution (C# + F#/VB/C++) | Y (type-GUID discrimination in `.sln`; C# GUIDs enumerated in `SolutionFile` source) | P (per-language SDKs; C++ projects are MSBuild-format too) | P | P | P (`AssociateFileExtensionWithLanguage`; unrecognized projects skippable) |
| File-based C# apps (`.cs` with no project, .NET 10) | N (no project file) | N (no project to evaluate) | N | P (`dotnet build MyProject.cs`, .NET SDK 10.0.100+) | Y (`OpenProjectAsync` accepts a `.cs` file) |
| Custom-SDK projects (`Sdk="MSBuild.Sdk.Extras/2.0.54"`, Aspire SDKs, etc.) | P (SDK attribute visible; imported logic invisible) | Y (requires the SDK resolvable: installed, in `global.json` `msbuild-sdks`, or from NuGet; else MSB4236) | Y (same as any project) | Y (same as any project) | P (relies on the same SDK resolution inside the build host) |

Key asymmetry: **solutions are not MSBuild projects**. MSBuild converts `.sln`/`.slnf`
into an in-memory "metaproject" only when building (docs:
[Customize solution builds](https://learn.microsoft.com/en-us/visualstudio/msbuild/customize-solution-build)
— "MSBuild builds a solution file, it first translates the file internally into a
project file, and then builds that project file"; implementation:
`src/Build/Construction/Solution/SolutionProjectGenerator.cs` in
[dotnet/msbuild](https://github.com/dotnet/msbuild/blob/main/src/Build/Construction/Solution/SolutionProjectGenerator.cs)),
and the CLI query switches are hard-rejected on solution/filter files
(`MSB1063: Cannot access properties or items when building solution files or solution
filter files` `[E]`). Solution-level enumeration therefore goes through
`Microsoft.Build.Construction.SolutionFile`, `dotnet sln`, or a solution-persistence
parser — not through project evaluation.

---

## 2. Mechanisms

### 2.1 Static XML inspection

Read `<Compile Include=.../>`, `<ProjectReference>`, `<PackageReference>`, properties
without invoking MSBuild.

- **What it can see:** explicit items and properties in the project file plus anything
  in unconditionally imported local files, if the reader also follows `<Import>`.
- **What it cannot see:** glob-expanded items (SDK default globs
  `**/*.cs` etc. — [SDK overview, "Default includes and excludes"](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/overview#default-includes-and-excludes)),
  condition evaluation (`Condition=" '$(TargetFramework)' == 'net8.0' "`), `Remove`/
  `Update` net effects, `Directory.Build.props`/`Directory.Build.targets` discovery
  ([customize-by-directory](https://learn.microsoft.com/en-us/visualstudio/msbuild/customize-by-directory)),
  SDK imports (`Sdk.props`/`Sdk.targets`), restore-generated `obj/*.nuget.g.props`
  imports, `MSBuildProjectExtensionsPath` contributions
  ([MSBuildProjectExtensionsPath documented in msbuild-props](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/msbuild-props)),
  or any target-produced items (generated `obj/` sources).
- **Determinism/security:** deterministic and **non-executing** (the only mechanism
  with no code-execution surface at all), but silently wrong on any project that relies
  on the above — which is most SDK-style projects (their `Compile` lists exist only as
  globs in the SDK's `Microsoft.NET.Sdk.DefaultItems.props`,
  [dotnet/sdk source](https://github.com/dotnet/sdk/blob/main/src/Tasks/Microsoft.NET.Build.Tasks/targets/Microsoft.NET.Sdk.DefaultItems.props)).

### 2.2 MSBuild evaluation API (`Microsoft.Build.Evaluation` / `ProjectInstance`)

Programmatic or CLI evaluation of a `.csproj` without executing targets.

- Item globs are expanded **during evaluation** ([MSBuild items docs](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-items) — wildcards `**`, `*`, `?`;
  `Exclude` scoped per `Include`); `Remove`/`Update` outside targets are evaluation-time
  (MSBuild 15+). `[E]`: `dotnet msbuild ExpLib.csproj -getItem:Compile` on a never-restored,
  never-built class library returned `Class1.cs`, `Extra.cs` (glob-defined in
  `Microsoft.NET.Sdk.DefaultItems.props`, per item's `DefiningProjectFullPath`) plus an
  explicit `../Shared/Shared.cs` with `Link=Shared/Shared.cs` — **0.28–0.54 s wall per
  invocation, no restore, no build**.
- Evaluation reads environment variables once at property-collection initialization and
  does not support registry properties under `dotnet build`
  ([MSBuild properties docs](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-properties)).
- **No restore is required to start evaluation** — `[E]`: `-getItem` on a never-restored
  fixture returned the full glob+linked item set. But evaluation state is not identical
  across restore states: a restored project imports `obj/*.nuget.g.props` and package
  `build/`/`buildTransitive`/`buildMultitargeting` assets through
  `$(MSBuildProjectExtensionsPath)` (restore outputs — [NuGet dependency resolution](https://learn.microsoft.com/en-us/nuget/concepts/dependency-resolution);
  asset auto-import — [PackageReference docs](https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files)),
  and those imports can contribute properties/items at evaluation time. "Never
  restored" is a distinct project state, not an equivalent one; restore is strictly
  required for *execution* targets like `ResolvePackageAssets` (NETSDK1004, `[E]` §5.2).
- Multi-targeted projects need **one evaluation per TFM** (see §4.2).
- Hosting APIs: `Project`/`ProjectInstance` with `ProjectLoadSettings`
  (`IgnoreMissingImports`, `FailOnUnresolvedSdk`, … —
  [ProjectLoadSettings API](https://learn.microsoft.com/en-us/dotnet/api/microsoft.build.evaluation.projectloadsettings)).
  Hosting MSBuild from an app requires an MSBuild installation context —
  [MSBuildLocator README](https://github.com/microsoft/MSBuildLocator) ("you need to
  load them in the context of one of those MSBuild installations"; probe order
  `DOTNET_ROOT` → current process path → `DOTNET_HOST_PATH` →
  `DOTNET_MSBUILD_SDK_RESOLVER_CLI_DIR` → `PATH`).
- **Code execution at evaluation time:** custom SDK resolution loads resolver
  assemblies, `<Import>` pulls in arbitrary build logic, and property functions such as
  `$([System.DateTime]::Now.ToString(...))` execute managed code during evaluation
  ([MSBuild properties docs](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-properties),
  "Property functions"). Evaluation is therefore *not* a sandbox (see §6).

### 2.3 Design-time builds

The Visual Studio project system's protocol for gathering project information without a
full build ([Visual Studio Integration (MSBuild)](https://learn.microsoft.com/en-us/visualstudio/msbuild/visual-studio-integration-msbuild)
— VS executes `Compile`, `ResolveAssemblyReferences`, `ResolveCOMReferences`,
`GetFrameworkPaths`, `CopyRunEnvironmentFiles` at load time; `$(BuildingInsideVisualStudio)`
is set inside the IDE).

- The `DesignTimeBuild=true` property is honored by the common targets: it defaults
  `BuildProjectReferences` to `false` and gates messages/side effects —
  [Microsoft.Common.CurrentVersion.targets](https://github.com/dotnet/msbuild/blob/main/src/Tasks/Microsoft.Common.CurrentVersion.targets)
  (line 375: `<BuildProjectReferences ... '$(DesignTimeBuild)' == 'true'>false</...>`; lines 845–846, 2557) and
  [Microsoft.NET.Sdk.targets](https://github.com/dotnet/sdk/blob/main/src/Tasks/Microsoft.NET.Build.Tasks/targets/Microsoft.NET.Sdk.targets)
  (line 259: `_CheckMicrosoftNetSdkCompilersToolsetPackageExists` conditioned on
  `'$(DesignTimeBuild)' != 'true'`).
- Design-time **generated** files: SDK targets materialize `obj/<TFM>/<proj>.AssemblyInfo.cs`,
  `<proj>.GlobalUsings.g.cs`, `.NETCoreApp,Version=vX.Y.AssemblyAttributes.cs` (sources:
  [Microsoft.NET.GenerateAssemblyInfo.targets](https://github.com/dotnet/sdk/blob/main/src/Tasks/Microsoft.NET.Build.Tasks/targets/Microsoft.NET.GenerateAssemblyInfo.targets),
  [Microsoft.NET.GenerateGlobalUsings.targets](https://github.com/dotnet/sdk/blob/main/src/Tasks/Microsoft.NET.Build.Tasks/targets/Microsoft.NET.GenerateGlobalUsings.targets)).
  These only appear in `Compile` **after targets run** — `[E]`: evaluation-only
  `-getItem:Compile` showed no `obj/` entries; after `-t:Build` the same query listed
  all three generated files with `DefiningProjectFullPath` pointing at the generating
  targets. (Docs example of the same shape:
  [Evaluate MSBuild items and properties](https://learn.microsoft.com/en-us/visualstudio/msbuild/evaluate-items-and-properties).)
- Roslyn source generators produce compilation inputs **in memory, not on disk**; they
  never appear as `Compile` items `[I]` — a discovery driver must treat "source files
  on disk ∪ generated `obj/` files" as an under-approximation of the compilation inputs.

### 2.4 `dotnet msbuild` / `dotnet` CLI query surfaces

- `-getProperty:{n,...}` / `-getItem:{n,...}` / `-getTargetResult:{n,...}` — MSBuild
  17.8+ ([Evaluate MSBuild items and properties](https://learn.microsoft.com/en-us/visualstudio/msbuild/evaluate-items-and-properties)):
  evaluation-only when no target is requested; JSON output for items (with well-known
  metadata incl. `FullPath`, `DefiningProjectFullPath`) and for multiple properties;
  single property as plain text. `-getTargetResult` runs the target. `dotnet build` and
  other dotnet commands pass these through ([MSBuild command-line reference](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-command-line-reference)).
  `[E]`: all three verified, including `-getTargetResult:GetTargetFrameworks` without a
  prior restore.
- `-preprocess[:file]` — single aggregated file with every import inlined, boundaries
  marked; no build ([MSBuild command-line reference](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-command-line-reference);
  `[E]`: a 3-file class library preprocessed to 17,111 lines in ~0.3 s, including the
  full SDK import chain).
- `-graphBuild` / `-graph` (MSBuild 16+), `-isolateProjects`, `-profileEvaluation`,
  `-restore`/`-restoreProperty`, binary logger `-bl`, `-targets`, `-ignoreProjectExtensions`
  — all in the [command-line reference](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-command-line-reference).
- `dotnet sln list|add|remove|migrate` — `.sln` and `.slnx`; `.slnf` listing since .NET
  SDK 9.0.3xx; remove-by-name since .NET 10; `migrate` writes a `.slnx` beside the `.sln`
  ([dotnet sln docs](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-sln)).
- `dotnet build` == `dotnet msbuild -restore` ([dotnet msbuild docs](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-msbuild));
  implicit restore on build/new/run/test/publish/pack, `--no-restore`, `--force` (deletes
  `project.assets.json`), `--disable-build-servers` ([dotnet build docs](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-build),
  [dotnet restore docs](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-restore)).
- `dotnet msbuild` is documented as having "the exact same capabilities as the existing
  MSBuild command-line client **for SDK-style projects only**"
  ([dotnet msbuild docs](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-msbuild)) —
  the cross-platform qualifier for legacy projects (but see `[E]` legacy evaluation in §1).

### 2.5 Roslyn `MSBuildWorkspace`

`Microsoft.CodeAnalysis.Workspaces.MSBuild` — a Roslyn `Workspace` that loads solutions
and projects into `Project`/`Solution` models with full compilation data
([MSBuildWorkspace.cs](https://github.com/dotnet/roslyn/blob/main/src/Workspaces/MSBuild/Core/MSBuild/MSBuildWorkspace.cs);
public API list in
[PublicAPI.Shipped.txt](https://github.com/dotnet/roslyn/blob/main/src/Workspaces/MSBuild/Core/PublicAPI.Shipped.txt)
— the class is current, not obsolete).

- Current architecture (Roslyn main): Roslyn parses the solution file itself
  ([SolutionFileReader.cs](https://github.com/dotnet/roslyn/blob/main/src/Workspaces/MSBuild/Core/MSBuild/SolutionFileReader.cs),
  including a dedicated
  [SolutionFileReader.SolutionFilterReader.cs](https://github.com/dotnet/roslyn/blob/main/src/Workspaces/MSBuild/Core/MSBuild/SolutionFileReader.SolutionFilterReader.cs)),
  sets `$(SolutionDir)`, then evaluates each project **out of process** via a
  `BuildHostProcessManager` (named-pipe IPC; `.NET Core`, `.NET Framework`, and Mono
  host kinds) ([BuildHostProcessManager.cs](https://github.com/dotnet/roslyn/blob/main/src/Workspaces/MSBuild/Core/MSBuild/BuildHostProcessManager.cs)).
- On .NET Core it starts the default `dotnet`, asks the host for the best MSBuild
  installation for the project (`FindBestMSBuildAsync` — "search through all the SDK
  install locations for a usable MSBuild instance"), and **relaunches the host** with
  that SDK's `dotnet` (`../../` layout), rolling forward `LatestMajor` with
  `DOTNET_ROLL_FORWARD_TO_PRERELEASE=1` (same source). This is the SDK-selection
  equivalent of the CLI's `global.json` logic ([global.json docs](https://learn.microsoft.com/en-us/dotnet/core/tools/global-json)).
- On .NET Framework it needs a VS/Build Tools installation discoverable through
  MSBuildLocator, else falls back to the .NET Core host with a warning; Mono fallback
  likewise (same source). `MSBuildLocator` itself documents why an installation context
  is required ([README](https://github.com/microsoft/MSBuildLocator)).
- Knobs: `LoadMetadataForReferencedProjects` (load output-assembly metadata instead of
  opening referenced projects — a large scale lever), `SkipUnrecognizedProjects`
  (default `true`), `AssociateFileExtensionWithLanguage`, per-load MSBuild properties,
  `ProjectLoadProgress` with `Evaluate`/`Build`/`Resolve` operations (same source).
- Failure reporting is non-throwing by default: diagnostics land in
  `Workspace.Diagnostics` (same source, `DiagnosticReportingMode.Log` default).
- It is a **compilation model**, not a discovery index: it runs real MSBuild evaluation
  (and can run targets) per project, and loads syntax trees for every document — heavy
  for index-scale use, and it executes build logic (same trust boundary as §6).

---

## 3. Mechanism tradeoff table

| Dimension | Static XML | MSBuild evaluation (`-getItem`/API) | Design-time build | Full build (`dotnet build`) | Roslyn `MSBuildWorkspace` |
|---|---|---|---|---|---|
| Cross-platform | yes (pure parsing) | yes — .NET SDK MSBuild runs on Windows/Linux/macOS; registry props unsupported on Core; `Path` separators render as `\` in path properties even on Linux `[E]` | yes (same engine) | yes; legacy TFMs need `Microsoft.NETFramework.ReferenceAssemblies` on non-Windows | yes — .NET Core build host; Mono host for Mono-MSBuild environments; NetFramework host needs VS/Build Tools (Windows) |
| TFM expansion | no (sees raw `TargetFramework(s)` strings) | yes — per-TFM inner evaluation via `-p:TargetFramework=`; outer evaluation yields empty `TargetFramework` and **drops glob `Compile` items** `[E]`; `GetTargetFrameworks`/`GetTargetFrameworksWithPlatform` targets enumerate TFMs+monikers+platforms `[E]` (no restore needed) | yes — same inner/outer model | yes — builds all TFMs | yes — `ProjectLoadProgress.TargetFramework`; one `Project` per TFM |
| Compile items incl. linked/generated | only explicit XML | evaluation-time: globs, `Remove`/`Update`, `Link` metadata, `DefaultItemExcludes` (bin/obj), `DefaultItemExcludesInProjectFolder` (dot-dirs); **no** target-generated `obj/` files | evaluation + designated design-time targets → generated files appear (AssemblyInfo, GlobalUsings, AssemblyAttributes) | all of the above + every target contribution | full compilation model incl. generated files and in-memory source generators |
| Project/assembly identity & references | names + explicit `Reference`/`ProjectReference`/`PackageReference` XML; GUIDs only in `.sln` | `AssemblyName`/`RootNamespace` defaults (project name), `ProjectGuid`-style properties, `ProjectReference` items; resolved *paths* require execution (`ResolveProjectReferences`/`GetTargetPath` protocol) | `GetTargetPath`-based P2P resolution (`BuildProjectReferences=false` under design time) | full resolution: `ResolveAssemblyReferences` → `ReferencePath`, package assets from `obj/project.assets.json` | full: `ProjectReferences`, `MetadataReferences`, `AnalyzerReferences` per project |
| Restore/toolchain requirements | none | none to *start* evaluation (verified on clean fixture `[E]`); restored state differs — `obj/*.nuget.g.props` and package `build/` assets import at evaluation; restore required if targets execute (NETSDK1004 `[E]`) | same as evaluation; design-time reference resolution benefits from restored assets | restore required (implicit) — needs NuGet feeds; offline = failure unless cached | restore not run by the workspace, but reference resolution quality degrades without assets; SDK must be installed (or `global.json`-pinned) |
| Custom SDK / import behavior | invisible (must replicate) | full — evaluation loads SDK props/targets and SDK resolvers; MSB4236 if unresolvable `[E]`; `IgnoreMissingImports`/`FailOnUnresolvedSdk` options | same | same | same, inside the build host |
| Code-execution / security | none (safe) | **executes code** (SDK resolvers, imports, property functions) | executes targets (design-time subset) | executes arbitrary build logic incl. `Exec`, custom tasks, package `build/` assets | executes MSBuild in child processes — same trust boundary |
| Failure modes | silent wrongness (misses globs/conditions/generated) | surface as evaluation errors: MSB4019 missing import `[E]`, MSB4236 unresolved SDK `[E]`, MSB1009 missing project `[E]`, MSB1063 `-get*` on solutions `[E]` | partial-failure: unresolved references degrade to warnings/diagnostics | hard errors: NETSDK1004, NU1xxx restore errors, NETSDK1022 duplicate items | non-throwing by default; `Workspace.Diagnostics`; project skipped or failed per `SkipUnrecognizedProjects` |
| Determinism | high (byte-parsing) | high for same inputs/state; depends on filesystem (globs), env snapshot, and property functions | medium (targets may touch filesystem, timestamps) | medium (see §7) | medium-high (host selection adds a machine-dependent step) |
| Production-scale cost signal | O(files), microseconds–ms | **~0.3–0.5 s per project invocation** cold `[E]`; one evaluation per TFM × configuration; MSBuild server (opt-in) caches context across invocations | similar to evaluation + target cost; designed for IDE responsiveness | seconds+ per project (3 s for a trivial lib incl. restore `[E]`) | heaviest: spawns build-host processes per SDK, loads all documents; `LoadMetadataForReferencedProjects` mitigates P2P cost |

---

## 4. Cross-cutting facts

### 4.1 Solution formats

- **`.sln`**: header `Microsoft Visual Studio Solution File, Format Version 12.00`;
  `Project("{<type-guid>}") = "<name>", "<path>", "{<project-guid>}"` lines; global
  sections `SolutionConfigurationPlatforms`, `ProjectConfigurationPlatforms`
  (`<guid>.<cfg>.ActiveCfg` / `.Build.0`), `NestedProjects`
  ([.sln file docs](https://learn.microsoft.com/en-us/visualstudio/extensibility/internals/solution-dot-sln-file);
  parser regexes and section handling in
  [SolutionFile.cs](https://github.com/dotnet/msbuild/blob/main/src/Build/Construction/Solution/SolutionFile.cs):
  `CrackProjectLinePattern`, `ParseProjectConfigurations`,
  `ProcessProjectConfigurationSection`). C# type GUIDs are enumerated in the same file:
  legacy C# `{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}`, SDK-style/CPS C#
  `{9A19103F-16F7-4668-BE54-9A1E7A4F7556}` (also `cpsProjectGuid`, VB, F#, C++, web,
  shared-project, solution-folder GUIDs). `[E]`: `dotnet new sln` + `dotnet sln add`
  (SDK 10) still writes the **legacy** C# GUID for SDK-style projects.
- `SolutionFile.Parse()` handles `.sln`, `.slnf`, and `.slnx`; `.slnx` (and `.sln` under
  an opt-in trait) parse via `Microsoft.VisualStudio.SolutionPersistence`
  (`SolutionSerializers.GetSerializerByMoniker`); `.slnf` routes to
  `ParseSolutionFilter`; `ProjectShouldBuild` applies the filter set with forward-slash
  normalization; on Linux the path comparer is ordinal (case-sensitive) vs
  `OrdinalIgnoreCase` on Windows ([SolutionFile.cs](https://github.com/dotnet/msbuild/blob/main/src/Build/Construction/Solution/SolutionFile.cs)).
- **`.slnx`**: XML solution format from
  [microsoft/vs-solutionpersistence](https://github.com/microsoft/vs-solutionpersistence)
  (model `SolutionModel`/`SolutionProjectModel`, serializers via `SolutionSerializers`,
  schema `Slnx.xsd`). CLI support since **.NET SDK 9.0.200**
  ([Introducing SLNX… .NET Blog, 2025-03-13](https://devblogs.microsoft.com/dotnet/introducing-slnx-support-dotnet-cli/));
  VS 17.14 GA; `dotnet new sln` **defaults to `.slnx` in .NET 10** (`--format sln` opt-out)
  ([.NET 10 compatibility doc](https://learn.microsoft.com/en-us/dotnet/core/compatibility/sdk/10.0/dotnet-new-sln-slnx-default)).
  MSBuild builds `.slnx` from **17.12** ([MSBuild command-line reference](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-command-line-reference);
  also: with both `.sln` and `.slnx` present you must name one explicitly).
  `[E]`: generated `ExpX.slnx` is 3 lines, `<Solution><Project Path="..."/></Solution>`,
  **no project GUIDs**.
- **`.slnf`**: JSON `{"solution": {"path": ..., "projects": [...]}}`; MSBuild builds
  filters directly since **16.7**; the referenced solution file is still required;
  project paths are relative to the solution file and must match it; backslashes are
  JSON-escaped; `.slnx` takes priority over `.slnf` in 17.12+
  ([Solution filters in MSBuild](https://learn.microsoft.com/en-us/visualstudio/msbuild/solution-filters),
  [Filtered solutions (VS)](https://learn.microsoft.com/en-us/visualstudio/ide/filtered-solutions)).
  `[E]`: invoking `msbuild x.slnf -t:Help` forwards into the filtered project
  (error MSB4057 naming the project file), i.e. the filter drives which projects the
  metaproject builds.
- Solution-level build hooks: `before./after.<solution>.sln.targets`
  (`$(MSBuildExtensionsPath)\$(MSBuildToolsVersion)\SolutionFile\ImportBefore|After`),
  `Directory.Solution.props/.targets`; `.slnx` solution builds 17.12+, `.slnx`
  before/after targets 17.14+
  ([Customize solution builds](https://learn.microsoft.com/en-us/visualstudio/msbuild/customize-solution-build)).
  These are discovery-relevant: a solution-level hook can add or remove projects from
  the effective build.

### 4.2 Target-framework expansion

- `TargetFrameworks` (plural) wins over `TargetFramework` (singular)
  ([msbuild-props](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/msbuild-props)).
  TFM values are **aliases** translated by the SDK into `TargetFrameworkMoniker`,
  `TargetFrameworkIdentifier`, `TargetFrameworkVersion` (+ platform moniker trio)
  ([Target frameworks in SDK-style projects](https://learn.microsoft.com/en-us/dotnet/standard/frameworks)).
  A custom alias is legal if the moniker properties are set. `[E]` confirmed: `net10.0`
  → `TargetFrameworkMoniker=.NETCoreApp,Version=v10.0` (from `GetTargetFrameworks` output).
  .NET 10.0.300 allows multiple aliases resolving to the same effective framework
  ([msbuild-props](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/msbuild-props)).
- **Outer vs inner evaluation** (multi-targeting): the outer build has an empty
  `TargetFramework`; SDK default `Compile` globs are conditioned on it and **disappear**
  in the outer evaluation `[E]` (only unconditioned explicit items remain). Inner builds
  re-invoke the project with `TargetFramework=<tfm>` — the pattern is visible in
  `GetAllRuntimeIdentifiers` in
  [Microsoft.NET.Sdk.CrossTargeting.targets](https://github.com/dotnet/sdk/blob/main/src/Tasks/Microsoft.NET.Build.Tasks/targets/Microsoft.NET.Sdk.CrossTargeting.targets)
  (`<MSBuild Projects="$(MSBuildProjectFile)" Targets="GetAllRuntimeIdentifiers"
  Properties="TargetFramework=%(...)" />`). Practical consequence: a discovery driver
  must evaluate once per TFM (plus per configuration if configuration-conditional items
  matter), and should prefer `GetTargetFrameworks`/`GetTargetFrameworksWithPlatform`
  (SDK protocol targets referenced from
  [Microsoft.NET.Sdk.targets](https://github.com/dotnet/sdk/blob/main/src/Tasks/Microsoft.NET.Build.Tasks/targets/Microsoft.NET.Sdk.targets)
  line 1325; `[E]`: executable without restore, returns TFMs, monikers, platforms,
  and per-TFM property payloads; definition lands in
  `Microsoft.Common.CrossTargeting.targets` per item `DefiningProjectFullPath`).
- OS-specific TFMs (`net10.0-windows`, `net8.0-ios17.2`, …) and default platform
  versions expand per the
  [TFM reference](https://learn.microsoft.com/en-us/dotnet/standard/frameworks).

### 4.3 Compile items: globs, links, generated files

- SDK default globs: `Compile` `**/*.cs`, excludes `**/*.user; **/*.*proj; **/*.sln(x);
  **/*.vssscc`; `bin`/`obj` excluded via `$(BaseOutputPath)`/`$(BaseIntermediateOutputPath)`
  + `DefaultItemExcludes`; dot-directories via `DefaultItemExcludesInProjectFolder`
  ([SDK overview](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/overview),
  [msbuild-props](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/msbuild-props)).
  `EnableDefaultItems`/`EnableDefaultCompileItems`/`EnableDefaultEmbeddedResourceItems`/
  `EnableDefaultNoneItems` switch them off. Duplicating globs by hand → **NETSDK1022**
  ([SDK overview](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/overview)).
- `Link` metadata = "notational path … when the file is physically located outside the
  influence of the project file"
  ([Common MSBuild project items](https://learn.microsoft.com/en-us/visualstudio/msbuild/common-msbuild-project-items),
  `Compile` section; `[E]` linked file round-trips through `-getItem` with `Link`
  preserved). Linked files can live **outside the workspace root** — a boundary decision
  for any indexer.
- Generated: `obj/` files from `GenerateAssemblyInfo`, `GenerateGlobalUsings`, and the
  framework attributes targets appear in `Compile` only after those targets run
  (`[E]` + [evaluate-items docs example](https://learn.microsoft.com/en-us/visualstudio/msbuild/evaluate-items-and-properties));
  `obj/` is excluded from tethys's own walk today
  ([is_excluded_dir, src/indexing.rs:1249](src/indexing.rs)).

### 4.4 Identity and references

- Identity: `AssemblyName` defaults to `$(MSBuildProjectName)`, `RootNamespace` to the
  project name with spaces→underscores, `Deterministic=true` by default
  ([Microsoft.NET.Sdk.props](https://github.com/dotnet/sdk/blob/main/src/Tasks/Microsoft.NET.Build.Tasks/targets/Microsoft.NET.Sdk.props);
  `[E]` both properties confirmed). The `.sln` project GUID is a solution-file concept;
  `.slnx` drops GUIDs entirely `[E]`.
- `ProjectReference`: transformed into `Reference` items by `ResolveProjectReferences`;
  metadata `SetTargetFramework`, `SkipGetTargetFrameworkProperties`,
  `ReferenceOutputAssembly`, `BuildReference`, `Targets`; P2P **transitivity differs**:
  .NET Framework projects have non-transitive P2P references, .NET Core+ are transitive
  ([Common MSBuild project items](https://learn.microsoft.com/en-us/visualstudio/msbuild/common-msbuild-project-items)).
- P2P resolution protocol: the referencing build calls `GetTargetPath`
  (`GetTargetPathWithTargetPlatformMoniker`) on referenced projects —
  [Microsoft.Common.CurrentVersion.targets](https://github.com/dotnet/msbuild/blob/main/src/Tasks/Microsoft.Common.CurrentVersion.targets)
  (lines 2119, 2227, 2780; target list `ProjectReferenceTargets` line 2789). Design-time
  builds skip building references (`BuildProjectReferences=false`, line 375).
- Package references: `PackageReference` items resolved at **restore** time into
  `obj/project.assets.json` (per-TFM dependency graphs; transitive resolution; floating
  versions) — [PackageReference docs](https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files),
  [dependency resolution](https://learn.microsoft.com/en-us/nuget/concepts/dependency-resolution).
  Package `build/`/`buildTransitive`/`buildMultitargeting`/`analyzers` assets auto-import
  build logic ([PackageReference docs](https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files)).
  Legacy `packages.config` (exact versions, per-package `targetFramework`) is the
  non-PackageReference default for .NET Framework projects
  ([packages.config reference](https://learn.microsoft.com/en-us/nuget/reference/packages-config)).
- Legacy layout: `ToolsVersion` is obsolete in VS 2019+; `Microsoft.CSharp.targets` is
  found via `$(MSBuildToolsPath)`; legacy toolsets resolve from the registry on Windows
  ([Toolset (ToolsVersion) docs](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-toolset-toolsversion)).
  `[E]`: on Linux, `$(MSBuildToolsPath)` points into the .NET SDK and the SDK **ships
  `Microsoft.CSharp.targets`**, so a ToolsVersion-4.0 project with a
  `Microsoft.CSharp.targets` import evaluated successfully with `-getItem`.
  Cross-platform *building* of net4x targets uses
  [Microsoft.NETFramework.ReferenceAssemblies](https://www.nuget.org/packages/Microsoft.NETFramework.ReferenceAssemblies)
  (README: "enable building .NET Framework projects on any machine with at least MSBuild
  or the .NET Core SDK installed"; SDK 10 auto-references it when targeting net4x:
  `AutomaticallyUseReferenceAssemblyPackages=true` in
  [Microsoft.NET.Sdk.props](https://github.com/dotnet/sdk/blob/main/src/Tasks/Microsoft.NET.Build.Tasks/targets/Microsoft.NET.Sdk.props)).

### 4.5 Restore and toolchain

- Restore state shapes evaluation: `dotnet build` runs restore implicitly (`dotnet msbuild
  -restore`); `--no-restore` skips it; building without `obj/project.assets.json` fails
  with **NETSDK1004** ([dotnet build](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-build),
  [NETSDK1004](https://learn.microsoft.com/en-us/dotnet/core/tools/sdk-errors/netsdk1004);
  `[E]` reproduced). Restore writes `obj/*.nuget.g.props/targets` `[E]` and downloads
  package assets whose `build/` props/targets are imported into evaluation — so a
  discovery driver comparing "restored" vs "pristine" checkouts may see different
  evaluated items/properties even without running targets. Restore downloads to the
  global packages folder (`~/.nuget/packages` on Linux), honors `nuget.config` hierarchy
  and `--source`/`--packages`, and supports lock files (`packages.lock.json`,
  `--locked-mode`, `RestoreLockedMode`) ([dotnet restore](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-restore)).
- SDK selection: `global.json` (version + `rollForward`, `msbuild-sdks`, .NET 10
  `paths`/`errorMessage`) pins the SDK; the muxer searches from the CWD, the **MSBuild
  project SDK resolver starts from the solution directory, else the project directory**
  ([global.json docs](https://learn.microsoft.com/en-us/dotnet/core/tools/global-json)).
- Workload manifests download asynchronously on CLI commands
  ([dotnet build](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-build)).

### 4.6 Custom SDKs and imports

- `Sdk` attribute / `<Sdk Name="..." Version="..."/>` add implicit top/bottom imports
  (`Sdk.props` first, `Sdk.targets` last); NuGet-distributed SDKs are referenced
  `Sdk="Name/version"` ([SDK overview](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/overview)).
  Unresolvable SDK → **MSB4236** with per-resolver diagnostics
  (`[E]`: workload resolver + NuGet probe messages).
- `MSBuildProjectExtensionsPath` (default `<projdir>/obj/`) is where restore-generated
  imports and other tooling drop `.props`/`.targets` `[E]`
  ([msbuild-props](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/msbuild-props)).
- Auto-discovery imports: `Directory.Build.props/.targets/.rsp` walked from the project
  directory upward ([Customize your build](https://learn.microsoft.com/en-us/visualstudio/msbuild/customize-your-build),
  [customize-by-directory](https://learn.microsoft.com/en-us/visualstudio/msbuild/customize-by-directory)),
  `MSBuildUserExtensionsPath` user-level wildcard imports
  ([customize-your-local-build](https://learn.microsoft.com/en-us/visualstudio/msbuild/customize-your-local-build)).
- Custom SDKs are code: SDK resolution loads resolver assemblies at evaluation time
  (observed via MSB4236 resolver messages `[E]`).

---

## 5. Threat / failure model

### 5.1 Threats

| # | Threat | Mechanism | Blast radius | Source |
|---|---|---|---|---|
| T1 | Arbitrary code execution on index | MSBuild evaluation/execution of an untrusted project: `Exec` tasks, `UsingTask` assemblies, inline tasks, `Import`, property functions, custom SDK resolvers, response files | Full build-account privileges (credentials, network, secrets) | [Secure MSBuild usage best practices](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-security-best-practices) ("unknown build logic should be assumed to be capable of executing arbitrary code in the build environment") |
| T2 | Supply-chain injection via packages | Restore pulls package `build/`/`buildTransitive`/`buildMultitargeting`/`analyzers` assets that auto-import into evaluation; floating versions resolve at restore time | Code execution + dependency drift | [PackageReference docs](https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files), [security page](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-security-best-practices) |
| T3 | Silent wrong graph (correctness, not RCE) | Static XML parse misses globs/conditions/imports/generated items | Incomplete `Compile` sets, wrong TFM/identity, missed P2P edges | §2.1; [SDK overview default globs](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/overview) |
| T4 | Environment/cache poisoning | `Directory.Build.rsp`, `MSBuildUserExtensionsPath`, env-var injection, files in ancestor directories up to drive root | Build-logic injection without touching the repo | [security page](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-security-best-practices) |
| T5 | Data exfiltration via logs/artifacts | Binary logs embed paths, command lines, and imported project sources (default `ProjectImports=Embed`) | Secrets in CI artifacts | [MSBuild command-line reference, `-binaryLogger`](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-command-line-reference) |
| T6 | Solution-file abuse | Malformed `.sln`/`.slnf`/`.slnx` paths (absolute, `..`, case tricks) | Path escape outside workspace; on Windows case-insensitive vs Linux case-sensitive comparer divergence | [SolutionFile.cs](https://github.com/dotnet/msbuild/blob/main/src/Build/Construction/Solution/SolutionFile.cs) (`_pathComparer`) |
| T7 | Toolchain confusion | Wrong SDK picked via `global.json`/`rollForward`, or a second SDK in `paths`; workload manifest churn | Different evaluation results per machine | [global.json docs](https://learn.microsoft.com/en-us/dotnet/core/tools/global-json) |

### 5.2 Failure modes (observed or documented)

| Error/code | Trigger | Notes / evidence |
|---|---|---|
| MSB1009 | Project file does not exist | `[E]` |
| MSB4019 | Imported file not found (e.g. pre-restore import) | `[E]`; evaluation can tolerate via `ProjectLoadSettings.IgnoreMissingImports` ([API](https://learn.microsoft.com/en-us/dotnet/api/microsoft.build.evaluation.projectloadsettings)) |
| MSB4236 | SDK could not be resolved | `[E]`; includes per-resolver probe messages (workload resolver, NuGet SDK fallback) |
| MSB1063 | `-getItem`/`-getProperty` on `.sln`/`.slnf` | `[E]`; solutions are not projects |
| MSB4057 | Target does not exist (e.g. custom `-t:` on a filtered project) | `[E]` via `.slnf` forwarding |
| MSB1001/MSB1018 | invalid solution/filter argument handling | documented switch errors ([CLI reference](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-command-line-reference)) |
| NETSDK1004 | `obj/project.assets.json` missing when targets need it | `[E]`; evaluation-only queries start without restore, but restored imports (nuget.g.props, package `build/` assets) change evaluated state — see §2.2 |
| NETSDK1022 | Duplicate default-glob items | [SDK overview](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/overview) |
| NU1xxx / restore failures | offline, missing feeds, auth, version conflicts | [dotnet restore](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-restore); floating versions need all sources at restore time ([PackageReference](https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files)) |
| MSB3821 | Mark-of-the-Web blocks downloaded build files | [security page](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-security-best-practices) |
| Silent degradation | MSBuildWorkspace logs diagnostics instead of throwing (`SkipUnrecognizedProjects=true` default) | [MSBuildProjectLoader.cs](https://github.com/dotnet/roslyn/blob/main/src/Workspaces/MSBuild/Core/MSBuild/MSBuildProjectLoader.cs) |

**Operational posture** (from the [security page](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-security-best-practices)):
run evaluation/build only on reviewed sources; dedicated low-privilege account/session;
protect the MSBuild install location and the directory hierarchy up to the drive root;
control user-extension imports and response files; use `ExcludeAssets`/`IncludeAssets`
to drop unneeded package build logic; treat restore as part of the trust boundary
(also [VS Trust Settings](https://learn.microsoft.com/en-us/visualstudio/ide/trust-settings)).
Static XML inspection is the only mechanism outside this boundary, at the cost of
correctness (T3).

---

## 6. Determinism notes

- Same-input evaluation is deterministic in the engine; the *inputs* are broader than
  the repo: filesystem state (globs `[E]`), environment snapshot taken once at
  property-collection init ([properties docs](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-properties)),
  `global.json` selection, workload state, and restore results (floating versions,
  [PackageReference](https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files)).
- Nondeterminism sources in project logic itself: property functions calling system
  time/`[System.IO.File]` (executed at evaluation,
  [properties docs](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-properties)),
  `SourceRevisionId` from git state ([msbuild-props](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/msbuild-props)),
  OS-specific paths (`[E]`: `BaseIntermediateOutputPath` renders as `obj\` with a
  backslash on Linux), case-sensitivity of solution path matching (ordinal on Linux vs
  `OrdinalIgnoreCase` on Windows — [SolutionFile.cs](https://github.com/dotnet/msbuild/blob/main/src/Build/Construction/Solution/SolutionFile.cs)),
  and `ModifiedTime`-style well-known metadata that `-getItem` emits
  ([evaluate-items docs](https://learn.microsoft.com/en-us/visualstudio/msbuild/evaluate-items-and-properties)).
- Reproducibility levers: `Deterministic=true` default ([Microsoft.NET.Sdk.props](https://github.com/dotnet/sdk/blob/main/src/Tasks/Microsoft.NET.Build.Tasks/targets/Microsoft.NET.Sdk.props)),
  `packages.lock.json` + `RestoreLockedMode`/`--locked-mode` ([dotnet restore](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-restore)),
  pinned `global.json` with `rollForward: disable` ([global.json docs](https://learn.microsoft.com/en-us/dotnet/core/tools/global-json)).

---

## 7. Production-scale cost signals

- **Per-invocation floor**: a cold `dotnet msbuild -getItem:Compile` on a trivial
  project costs **~0.3–0.5 s wall** (process spawn + SDK resolver + evaluation) `[E]`;
  `-preprocess` ~0.3 s `[E]`; a full restore+build of the same project ~3 s `[E]`.
  A discovery driver iterating hundreds of projects pays this per project **and per
  TFM** (outer evaluation is not the inner evaluation — §4.2).
- **MSBuild server** (opt-in, `DOTNET_CLI_USE_MSBUILD_SERVER=1`) caches build context in
  a long-running process to cut repeated startup; exits after 15 min idle; documented as
  not helpful for CI; disable per-invocation with `/nr:false`
  ([MSBuild Server docs](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-server)).
  Node reuse (`-nodeReuse`) and `-m` parallelism apply to builds
  ([CLI reference](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-command-line-reference)).
  `[E]`: enabling the server did not speed up `-getItem` on this trivial fixture
  (279→406 ms noise) — server benefit claims are build-shaped, not query-shaped.
- **Design-time builds** exist precisely because IDE-scale consumers cannot afford full
  builds for IntelliSense/reference data ([VS Integration](https://learn.microsoft.com/en-us/visualstudio/msbuild/visual-studio-integration-msbuild)).
- **Roslyn MSBuildWorkspace** cost structure: out-of-proc build-host process(es) per
  SDK, relaunch when the project's SDK differs from the host's, named-pipe round-trips,
  then full document loads; `LoadMetadataForReferencedProjects` trades fidelity for
  cost on P2P graphs ([BuildHostProcessManager.cs](https://github.com/dotnet/roslyn/blob/main/src/Workspaces/MSBuild/Core/MSBuild/BuildHostProcessManager.cs),
  [MSBuildProjectLoader.cs](https://github.com/dotnet/roslyn/blob/main/src/Workspaces/MSBuild/Core/MSBuild/MSBuildProjectLoader.cs)).
- Engine-side analysis hooks: `-profileEvaluation` writes an evaluation profile;
  `-graphBuild`/`-isolateProjects`/`-inputResultsCache`/`-outputResultsCache` shape
  multi-project scheduling ([CLI reference](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-command-line-reference)).
- Binary logs embed imported project sources by default — size and secrecy signal
  ([CLI reference](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-command-line-reference)).

---

## 8. Relation to tethys's `ModuleResolver` / language-neutral driver seam

Verified against this branch's HEAD (paths relative to repo root):

- The seam: `ModuleResolver` trait + `ModuleContext { current_file, crates, anchor,
  namespaces }` ([src/languages/module_resolver.rs:53](src/languages/module_resolver.rs))
  is how per-language module-path→file rules reach the language-neutral drivers in
  `resolve.rs`/`indexing.rs`; `tests/seam_lint.rs` enforces the drivers stay neutral and
  resolvers stay DB-free. C# today gets `anchor: None`, an empty `crates` slice, and a
  namespace map built by the driver (`build_namespace_map`, [src/indexing.rs:1105](src/indexing.rs)).
- Rust-side analog for project metadata: `discover_crates` parses `Cargo.toml`
  workspaces/members/globs into `CrateInfo { name, path, lib_path, bin_paths }`
  ([src/cargo.rs:25](src/cargo.rs), [src/types.rs:172](src/types.rs)); `Tethys::new`
  calls it once ([src/lib.rs:180](src/lib.rs)); files outside crates get empty
  `module_path` ([src/lib.rs:224](src/lib.rs)) and `orphan:<topdir>` pseudo-crates in
  `build_file_crate_map` ([src/indexing.rs:587](src/indexing.rs)); the no-crate
  architecture phase returns all-zero stats ([src/indexing.rs:1259](src/indexing.rs));
  the "no `.csproj` discovery" gap is documented at
  [src/db/call_edges.rs:231](src/db/call_edges.rs).
- How the surveyed mechanisms map onto that seam (observations, not a design):
  - `.sln`/`.slnx`/`.slnf` are the C# analogs of the **workspace manifest**
    (`Cargo.toml` `[workspace]`): they enumerate member projects, solution folders
    (a display grouping, not a build grouping — `SolutionFile.ProjectsInOrder`
    excludes folders for the new parser), configurations/platforms, and build
    membership (`.slnf` filters; `Build.0` toggles).
  - A `.csproj` evaluation is the analog of **one crate manifest parse**, but it is
    parametric: effective items/properties exist per (TFM × configuration) pair, and
    the outer multi-TFM evaluation is *not* representative (§4.2). A language-neutral
    driver that currently does one manifest parse per crate would need a contract that
    can express "N evaluations per project" without embedding MSBuild semantics.
  - The C#-side data an eventual `CrateInfo`-analog would carry is well-defined by the
    primary sources: assembly identity (`AssemblyName`, `RootNamespace`, `OutputType`),
    TFMs + monikers, `Compile` items with `Link`/`DefiningProjectFullPath`, P2P edges
    via `ProjectReference` (with the transitivity caveat), package identity via
    `PackageReference`/`packages.config`, and (if execution is allowed) resolved
    references via `GetTargetPath`/`project.assets.json`.
  - Trust posture differs sharply from the Rust path: `Cargo.toml` parsing in tethys
    executes no code and is bounded by `sanitize_target_path`
    ([src/cargo.rs:113](src/cargo.rs)); **every** MSBuild evaluation mechanism except
    static XML executes managed code (SDK resolvers, imports, property functions),
    and restore adds package-supplied build logic. Any seam placement must keep the
    "evaluation is untrusted code" decision at the driver boundary, not inside a
    DB-free resolver.
  - The seam's existing context-passing shape (`ModuleContext` built by the driver,
    consumed by resolvers) can carry future C# project metadata without touching the
    neutral drivers; the `tests/seam_lint.rs` fences (C4/C5/C10 per scout cross-checks
    and the module docs) and the `orphan:` pseudo-crate bucketing
    ([src/db/call_edges.rs:231](src/db/call_edges.rs)) are the load-bearing constraints
    any C# project model must respect.

---

## 9. Open factual unknowns

1. The exact metaproject XML/target surface generated by
   `SolutionProjectGenerator.cs` (class located at
   [dotnet/msbuild src/Build/Construction/Solution/SolutionProjectGenerator.cs](https://github.com/dotnet/msbuild/blob/main/src/Build/Construction/Solution/SolutionProjectGenerator.cs);
   contents not read in this pass — only the documented translation behavior and
   `MSB1063` were verified).
2. Where `GetTargetFrameworksWithPlatform` is *defined* (referenced from
   [Microsoft.NET.Sdk.targets:1325](https://github.com/dotnet/sdk/blob/main/src/Tasks/Microsoft.NET.Build.Tasks/targets/Microsoft.NET.Sdk.targets);
   `GetTargetFrameworks` observed executing via `Microsoft.Common.CrossTargeting.targets`
   `DefiningProjectFullPath` `[E]`, but the defining file was not located).
3. Roslyn build-host SDK selection fidelity: `FindBestMSBuildAsync`'s exact
   `global.json` semantics (paths, rollForward, prerelease) vs the CLI muxer are not
   documented ([BuildHostProcessManager.cs](https://github.com/dotnet/roslyn/blob/main/src/Workspaces/MSBuild/Core/MSBuild/BuildHostProcessManager.cs)).
4. Whether `MSBuildWorkspace`'s .NET Core host requires (or benefits from) an explicit
   `MSBuildLocator` registration; current source shows host discovery instead, and
   MSBuildLocator's README documents the general need for an installation context —
   the two paths' interplay for NuGet-package consumers is unverified.
5. Legacy non-SDK project evaluation breadth on Linux: verified for a minimal
   ToolsVersion 4.0 + `Microsoft.CSharp.targets` project `[E]`; full legacy-project
   fidelity (`.user` files, `packages.config` restore, WinForms/XAML targets) and
   Mono-host behavior were not exercised.
6. Scale data: no published benchmarks for batch `dotnet msbuild -getItem` evaluation
   across large solutions; only the local micro-timings `[E]` (§7).
7. `-getItem`/`-getProperty` behavior on multi-targeted projects across SDK versions:
   verified only on SDK 10.0.102/MSBuild 18.0.7 (outer-evaluation item dropout `[E]`).
8. Source-generator outputs (in-memory compilation inputs) have no MSBuild-surface
   enumeration at all `[I]`; the Roslyn compile pipeline is the only complete view.
9. `dotnet build MyProject.cs` file-based-app evaluation path (`.NET SDK 10.0.100+`)
   was not exercised; only its documented existence is cited
   ([dotnet build](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-build)).

---

## 10. Sources

### Microsoft Learn (docs)
- MSBuild command-line reference — https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-command-line-reference
- Evaluate MSBuild items and properties (17.8 `-get*`) — https://learn.microsoft.com/en-us/visualstudio/msbuild/evaluate-items-and-properties
- MSBuild Server — https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-server
- Solution filters in MSBuild — https://learn.microsoft.com/en-us/visualstudio/msbuild/solution-filters
- Filtered solutions (VS) — https://learn.microsoft.com/en-us/visualstudio/ide/filtered-solutions
- Project Solution (.sln) file — https://learn.microsoft.com/en-us/visualstudio/extensibility/internals/solution-dot-sln-file
- Customize solution builds — https://learn.microsoft.com/en-us/visualstudio/msbuild/customize-solution-build
- Customize your build / customize-by-directory / customize-your-local-build — https://learn.microsoft.com/en-us/visualstudio/msbuild/customize-your-build
- MSBuild items; MSBuild properties — https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-items , …/msbuild-properties
- Common MSBuild project items — https://learn.microsoft.com/en-us/visualstudio/msbuild/common-msbuild-project-items
- Visual Studio Integration (MSBuild) — https://learn.microsoft.com/en-us/visualstudio/msbuild/visual-studio-integration-msbuild
- Secure MSBuild usage best practices — https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-security-best-practices
- VS Trust Settings — https://learn.microsoft.com/en-us/visualstudio/ide/trust-settings
- MSBuild Toolset (ToolsVersion) — https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-toolset-toolsversion
- ProjectLoadSettings API — https://learn.microsoft.com/en-us/dotnet/api/microsoft.build.evaluation.projectloadsettings
- .NET project SDK overview; MSBuild reference for Microsoft.NET.Sdk — https://learn.microsoft.com/en-us/dotnet/core/project-sdk/overview , …/msbuild-props
- Target frameworks in SDK-style projects — https://learn.microsoft.com/en-us/dotnet/standard/frameworks
- dotnet build / dotnet msbuild / dotnet sln / dotnet restore / global.json — https://learn.microsoft.com/en-us/dotnet/core/tools/{dotnet-build,dotnet-msbuild,dotnet-sln,dotnet-restore,global-json}
- NETSDK1004 — https://learn.microsoft.com/en-us/dotnet/core/tools/sdk-errors/netsdk1004
- .NET 10 breaking change: `dotnet new sln` defaults to SLNX — https://learn.microsoft.com/en-us/dotnet/core/compatibility/sdk/10.0/dotnet-new-sln-slnx-default
- NuGet: PackageReference in project files; dependency resolution; packages.config — https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files , https://learn.microsoft.com/en-us/nuget/concepts/dependency-resolution , https://learn.microsoft.com/en-us/nuget/reference/packages-config

### Microsoft blogs
- Introducing support for SLNX in the .NET CLI (.NET Blog, 2025-03-13) — https://devblogs.microsoft.com/dotnet/introducing-slnx-support-dotnet-cli/

### Upstream source (primary)
- dotnet/msbuild `src/Build/Construction/Solution/SolutionFile.cs` — https://github.com/dotnet/msbuild/blob/main/src/Build/Construction/Solution/SolutionFile.cs
- dotnet/msbuild `src/Build/Construction/Solution/SolutionProjectGenerator.cs` — https://github.com/dotnet/msbuild/blob/main/src/Build/Construction/Solution/SolutionProjectGenerator.cs
- dotnet/msbuild `src/Tasks/Microsoft.Common.CurrentVersion.targets` — https://github.com/dotnet/msbuild/blob/main/src/Tasks/Microsoft.Common.CurrentVersion.targets (lines 375, 845–846, 2119, 2227, 2557, 2780, 2789; also verified against the installed copy at `$DOTNET_ROOT/sdk/10.0.102/`)
- dotnet/sdk `Microsoft.NET.Sdk.props`, `Microsoft.NET.Sdk.targets`, `Microsoft.NET.Sdk.CrossTargeting.targets`, `Microsoft.NET.Sdk.BeforeCommon.targets`, `Microsoft.NET.Sdk.DefaultItems.props`, `Microsoft.NET.GenerateAssemblyInfo.targets`, `Microsoft.NET.GenerateGlobalUsings.targets` — https://github.com/dotnet/sdk/tree/main/src/Tasks/Microsoft.NET.Build.Tasks/targets
- dotnet/roslyn `src/Workspaces/MSBuild/Core/MSBuild/`: `MSBuildWorkspace.cs`, `MSBuildProjectLoader.cs`, `BuildHostProcessManager.cs`, `SolutionFileReader.cs`, `SolutionFileReader.SolutionFilterReader.cs`, `PublicAPI.Shipped.txt` — https://github.com/dotnet/roslyn/tree/main/src/Workspaces/MSBuild/Core/MSBuild
- microsoft/vs-solutionpersistence (README; `Slnx.xsd`) — https://github.com/microsoft/vs-solutionpersistence
- microsoft/MSBuildLocator README — https://github.com/microsoft/MSBuildLocator
- Microsoft.NETFramework.ReferenceAssemblies (NuGet + README) — https://www.nuget.org/packages/Microsoft.NETFramework.ReferenceAssemblies , https://github.com/Microsoft/dotnet/tree/master/releases/reference-assemblies

### Empirical (2026-08-09, .NET SDK 10.0.102 / MSBuild 18.0.7, Linux x64, /tmp fixtures)
- `dotnet msbuild -getItem:Compile` evaluation-only (no restore): glob + linked items, 0.28–0.54 s
- Multi-TFM outer vs inner evaluation (item dropout); `GetTargetFrameworks` without restore
- `-preprocess` (17,111 lines for a class library); `-t:Build` post-target `Compile` incl. `obj/` generated files
- MSB1063 on `.sln`/`.slnf`; `.slnf` target forwarding; `dotnet sln`/`dotnet new sln --format`; slnx without GUIDs
- Failure modes MSB1009, MSB4019, MSB4236, MSB4057, NETSDK1004; legacy ToolsVersion-4.0 evaluation on Linux
- Path-property separator quirk (`obj\` on Linux); `dotnet build` restore+build 3.03 s

### Local (this branch HEAD)
- `src/languages/module_resolver.rs` (seam, `ModuleContext`, `NamespaceMap`), `src/languages/mod.rs` (`LanguageSupport`), `src/types.rs:172` (`CrateInfo`), `src/cargo.rs:25` (`discover_crates`), `src/lib.rs:180,224` (`Tethys::new`, `compute_module_path_for_file`), `src/indexing.rs:587,1105,1167,1249,1259` (crate map, namespace map, discovery, exclusions, arch phase), `src/db/call_edges.rs:231` (pseudo-crate comment), `tests/seam_lint.rs` (seam fences)
