# OmniSharp for legacy C# project support

Research ticket `tethys-xfhp` (C# parity wayfinder). Branch `research/csharp-parity-omnisharp`.
Companion tickets: `research/csharp-parity-msbuild` (MSBuild discovery, artifact
`.csharp-parity/research/msbuild-discovery.md` at commit `7477c36`), `research/csharp-parity-lsp`
(csharp-ls/LSP survey, artifact `.csharp-parity/research/csharp-ls-cli.md` on branch
`research/csharp-parity-lsp`).

**Scope.** Primary-source survey of OmniSharp (OmniSharp/omnisharp-roslyn and the VS Code
C# extension) as a candidate language-server and legacy-project vehicle for tethys: maintained
distributions and wire protocols, MSBuild/Roslyn host selection, SDK-style vs legacy
(non-SDK) project loading, runtime/install/licensing/maintenance posture, and what a tethys
`LspProvider` adapter would require. This document compares options; it deliberately does
**not** select an approach.

**Method.** Every material external claim carries a direct primary-source citation (an
upstream repository file at a pinned commit, Microsoft Learn, or a Microsoft blog). Local
claims cite exact repo paths/symbols on this branch's HEAD (`7ca48f8`). Tags: `[S]` =
verified in source at the pinned commit; `[E]` = empirical (probe by `LspResearch` for
csharp-ls, 2026-08-09); `[I]` = inference from cited facts. Scout/peer outputs were used as
leads only; material claims were re-verified against the cited sources.

Pinned sources:

- omnisharp-roslyn at `83fd615e` (2025-11-13, release 1.39.15) — cloned to `/tmp/omnisharp-roslyn`
- omnisharp-vscode (now dotnet/vscode-csharp) at `d8377e37` (2026-08-07) — `/tmp/omnisharp-vscode`
- csharp-ls (razzmatazz/csharp-language-server) at `e2efc47` (2026-08-04) — `/tmp/csharp-language-server`
- microsoft/MSBuildLocator at `4703577f` (2026-06-15) — `/tmp/MSBuildLocator` (host-discovery mechanics used by OmniSharp's SDK instance provider)

---

## 1. Maintained modes and wire protocols

OmniSharp's server binary ships one codebase with three transports, selected by CLI flags;
the HTTP transport lives in a separate distribution package. The LSP mode registers every
custom-protocol endpoint as an `o#/…` interop method on the same connection, so the custom
protocol is reachable even over LSP.

| Mode | Package / flag | Transport & framing | Indices | Status (2026-08) |
|---|---|---|---|---|
| Custom HTTP protocol | `omnisharp.http` package (`OmniSharp.Http.Driver`, HTTP interface) | JSON over HTTP, pull-based; `-p <port>` (`doc/Using-Omnisharp.md`; driver `src/OmniSharp.Http.Driver/Program.cs`) | 1-based unless `-z` (`CommandLineApplication.cs:106` `ZeroBasedIndices`) | Legacy; README still documents "HTTP server" flavor ([README.md](https://github.com/OmniSharp/omnisharp-roslyn/blob/83fd615e/README.md)) |
| Custom stdio protocol | `omnisharp` package (default, no flag) | Line-based JSON over stdin/stdout (server `OmniSharp.Stdio` `Host`; client `StdioEngine` uses `readline` per line — `src/omnisharp/engines/stdioEngine.ts`); two-way push (diagnostics, project events); `-e <encoding>` | 1-based by default; `-z`/`--zero-based-indices` (`CommandLineApplication.cs:106`; conversion in `src/OmniSharp.Abstractions/Models/ZeroBasedIndexConverter.cs`) | Still the default engine for OmniSharp mode in the VS Code extension (`server.ts:329` `StdioEngine`); `--stdio` flag itself **deprecated** ([CHANGELOG 1.39.9](https://github.com/OmniSharp/omnisharp-roslyn/blob/83fd615e/CHANGELOG.md), PR [#2554] resolving [#2439]) |
| LSP | `omnisharp` package, `-lsp \| --languageserver` (`src/OmniSharp.Stdio/StdioCommandLineApplication.cs`) | Standard LSP Content-Length framing over stdio via `OmniSharp.Extensions.LanguageServer` **0.19.9** (`Directory.Packages.props:7,83`); logs routed to stderr (`LanguageServerHost.cs` `LogToStandardErrorThreshold`) | 0-based forced (`Stdio.Driver/Program.cs:36` `Configuration.ZeroBasedIndices = true`; `ZeroBasedIndexConverter` pass-through) | Optional (`omnisharp.enableLspDriver`, default false — `src/shared/options.ts:379-381`); used by the extension's `LspEngine` with `['-lsp', '--encoding', 'ascii']` (`src/omnisharp/engines/lspEngine.ts:57`) |

LSP-mode interop surface (`src/OmniSharp.LanguageServerProtocol/LanguageServerHost.cs`,
`ConfigureCompositionHost`): every custom endpoint is re-published as `o#/<name>` — including
`o#/checkalivestatus`, `o#/checkreadystatus` (returns `OmniSharpWorkspace.Initialized`),
`o#/stopserver`, and `o#/projects`/`o#/project` (workspace/project models). Handlers run with
`RequestProcessType.Parallel`.

Workspace/root selection: `-s <solution|dir>` (or CWD) wins; the LSP `rootUri` is effectively
**ignored** — `CreateCompositionHost` uses `application.ApplicationRoot` which defaults to
`Directory.GetCurrentDirectory()` and is therefore never empty (`LanguageServerHost.cs`
`CreateCompositionHost`; `CommandLineApplication.cs` `ApplicationRoot`). The custom-protocol
doc states "There's no way of loading new projects after the server has started"
([doc/Using-Omnisharp.md](https://github.com/OmniSharp/omnisharp-roslyn/blob/83fd615e/doc/Using-Omnisharp.md)).

---

## 2. MSBuild/Roslyn host selection and project loading (from source)

### 2.1 Host selection

`MSBuildLocator.CreateDefault` (`src/OmniSharp.Host/MSBuild/Discovery/MSBuildLocator.cs`)
registers different instance providers **per target framework**:

| Build | Providers | What they find |
|---|---|---|
| `net6.0` (the "modern .NET" build; vscode `modernNetVersion = '6.0'`, launched as `dotnet OmniSharp.dll` — `src/omnisharp/omnisharpPackageCreator.ts:8`, `omnisharpManager.ts`) | `SdkInstanceProvider`, `SdkOverrideInstanceProvider` | .NET SDKs ≥ 6.0 discovered via `Microsoft.Build.Locator.QueryVisualStudioInstances()`, which on .NET Core enumerates dotnet SDKs (`DotNetSdkLocationHelper`: `sdk/*/.version` + `Microsoft.Build.dll`, `DiscoveryType.DotNetSdk`, `MSBuildPath` = SDK dir) filtered by OmniSharp on `.version` files (`SdkInstanceProvider.cs`); MSBuildLocator additionally excludes SDKs newer than the host runtime unless `AllowAllRuntimeVersions` (default query) — largely moot given `RollForward=LatestMajor` `[I]`; non-prerelease unless `SDK:IncludePrereleases`; overridable via `sdk` config section (`Path`, `Version`, `IncludePrereleases`, `PropertyOverrides`) |
| `net472` (framework-dependent; Mono on macOS/Linux, .NET Framework on Windows) | `MicrosoftBuildLocatorInstanceProvider`, `MonoInstanceProvider`, `UserOverrideInstanceProvider` | Windows: VS instances via `Microsoft.Build.Locator` (`MicrosoftBuildLocatorInstanceProvider.cs`); macOS/Linux: Mono's MSBuild, **only if OmniSharp is running on the installed Mono runtime**, Mono ≥ 6.4.0, with Mono's `Microsoft.Build.dll` present (`MonoInstanceProvider.cs`); user override via `msbuildoverride` config |

Selection: `GetBestInstance` scores instances — user override wins; VS instances are rejected
without `Microsoft.DotNet.MSBuildSdkResolver` in `SdkResolvers`; version breaks ties
(`src/OmniSharp.Host/MSBuild/Discovery/Extensions.cs`). `RegisterInstance` sets
`MSBUILD_EXE_PATH` and calls `Microsoft.Build.Locator.RegisterMSBuildPath(instance.MSBuildPath)`
(`MSBuildLocator.cs` `RegisterInstance`) — MSBuild assemblies are loaded from the discovered
installation, not shipped (`OmniSharp.MSBuild.csproj`: `Microsoft.Build*` with
`ExcludeAssets="Runtime"`). **If no instance is found, OmniSharp throws
`MSBuildNotFoundException` at startup** ("Could not locate MSBuild instance to register with
OmniSharp." — `Extensions.cs` `RegisterDefaultInstance`), i.e. the server cannot start without
a .NET SDK (net6.0 build) or VS MSBuild/Mono (net472 build).

Driver csprojs set `<RuntimeFrameworkVersion>6.0.0-preview.7.21317.1</RuntimeFrameworkVersion>`
and `<RollForward>LatestMajor</RollForward>` (`OmniSharp.Stdio.Driver.csproj:12-13`,
`OmniSharp.Http.Driver.csproj:12-13`, `OmniSharp.LanguageServerProtocol.csproj`) — the net6.0
build runs on any newer installed runtime.

### 2.2 Project discovery

`ProjectSystem.Initalize` (`src/OmniSharp.MSBuild/ProjectSystem.cs`):

1. `-s` solution given → parse with OmniSharp's **own** `.sln` parser
   (`SolutionParsing/SolutionFile.cs` `Scanner`/`ProjectBlock`/`GlobalSectionBlock`) or
   `.slnf` filter (`SolutionFilterReader`); only `.csproj` entries load; per-project
   `Configuration|Platform` mapped from the solution's `ProjectConfigurationPlatforms`
   section; **no `.slnx` support anywhere in the source** (zero hits for `slnx`).
2. No `-s` → first `*.sln`/`*.slnf` in the target directory (`FindSolutionFilePath` +
   `SolutionSelector.Pick`).
3. No solution → glob `**/*.csproj` under the target directory.

`LoadProjectsOnDemand` (config) defers loading until documents open. Project GUIDs from the
`.sln` become Roslyn `ProjectId`s (`ProjectSystem.cs` `GetProjectPathsAndIdsFromSolutionOrFilter`).

### 2.3 Project loading (design-time build)

`ProjectLoader` (`src/OmniSharp.MSBuild/ProjectLoader.cs`) sets global properties:
`DesignTimeBuild=true`, `BuildingInsideVisualStudio=true`, `BuildProjectReferences=false`,
`_ResolveReferenceDependencies=true`, `SolutionDir=<dir>/`,
`AlwaysCompileMarkupFilesInSeparateDomain=false`, `ProvideCommandLineArgs=true`,
`SkipCompilerExecution=true`, `UseAppHost=false` — the Visual Studio design-time protocol
(cf. [VS Integration (MSBuild)](https://learn.microsoft.com/en-us/visualstudio/msbuild/visual-studio-integration-msbuild)).
Each project is evaluated (`ProjectCollection` with the selected ToolsVersion via
`GetLegalToolsetVersion` — highest legal toolset when the requested one is missing) and then
**executed**: `projectInstance.Build([Compile, CoreCompile])` with `SkipCompilerExecution`,
i.e. a design-time build per project (queue-processed asynchronously by `ProjectManager`,
100 ms loop; reload on `.cs`/project file changes).

**Multi-targeting:** `SetTargetFrameworkIfNeeded` — if `TargetFramework` is empty and
`TargetFrameworks` non-empty, **the first TFM is picked and set as a global property**; the
in-source comment concedes "For now, we'll just pick the first target framework"
(`ProjectLoader.cs`). OmniSharp loads exactly **one TFM per project**.

**Extracted data** (`ProjectFileInfo.ProjectData.Create` from the built
`ProjectInstance`): `Compile` items → `SourceFiles` (includes design-time generated
`obj/` files, minus `TemporaryGeneratedFile_*`); `ReferencePath` items → `References`
(resolved assembly paths via `FullPath`), with project references identified by
`ReferenceSourceTarget == ProjectReference` and counted separately
(`ProjectReferences`) — resolved P2P edges, not raw `ProjectReference` XML;
`PackageReference` items → `PackageReferences`; `Analyzer`/`AdditionalFiles`/
`EditorConfigFiles`; `GetAllGlobs()` → `FileInclusionGlobs` for new-file inclusion checks;
plus `AssemblyName`, `TargetPath`, `TargetFrameworkMoniker`, `OutputKind`, `LangVersion`,
`RootNamespace`, `ProjectGuid`. `ProjectFileInfo.cs` exposes these and
`IsUnityProject()` (UnityEngine/UnityEditor reference detection).

**packages.config:** NOT surfaced as `PackageReference` (only `PackageReference` items are
read); legacy references appear only if the design-time build resolved them into
`ReferencePath`. `PackageDependencyChecker` checks `PackageReference` items against
`project.assets.json` and, when `EnablePackageAutoRestore` + per-load `AllowAutoRestore`,
runs `dotnet restore`; otherwise emits an "UnresolvedDependencies" event
(`PackageDependencyChecker.cs`). The VS Code extension disables restore with
`DotNet:enablePackageRestore=false` (`server.ts` args).

**Legacy (non-SDK) project handling:** no special casing in the loader — legacy projects are
evaluated with the same design-time protocol against whatever MSBuild was discovered
(SDK MSBuild on the net6.0 build — the .NET SDK ships `Microsoft.CSharp.targets`, so
ToolsVersion projects evaluate on Linux, cf. msbuild-discovery.md §1/§4.4 `[E]` — or Mono
MSBuild on the net472 build). Toolset fallback: `GetLegalToolsetVersion`. Config knobs for
legacy fidelity: `ToolsVersion`, `VisualStudioVersion`, `MSBuildExtensionsPath`,
`TargetFrameworkRootPath`, `RoslynTargetsPath`, `CscToolPath`, `CscToolExe`,
`UseLegacySdkResolver` (sets `MSBuildSDKsPath` from `dotnet --info` — `SdksPathResolver.cs`),
`GenerateBinaryLogs` (`Options/MSBuildOptions.cs`).

**Failure behavior:** a project that fails to load logs diagnostics (`MSBuildProjectDiagnostics`
event) and joins `_failedToLoadProjectFiles`; later P2P references to it are skipped with a
warning (`ProjectManager.cs`); it is retried when re-queued (file change). The server keeps
running.

### 2.4 Files outside any project

`BufferManager` adds opened `.cs` files not in the workspace as **miscellaneous documents**
(`src/OmniSharp.Roslyn/BufferManager.cs:179` `TryAddMiscellaneousDocument`), i.e. OmniSharp
does not crash or fail on project-less workspaces; definition/references for such files are
limited to the synthetic misc compilation (no cross-project references) `[I]`.

---

## 3. LSP semantics relevant to an adapter

- **initialize:** `Initialize` handler builds the composition host (which registers the
  MSBuild instance — `MSBuildNotFoundException` here if none) and registers handlers;
  `Initialized` (lifecycle) runs `WorkspaceInitializer.Initialize` and then awaits
  `IProjectSystem.WaitForIdleAsync()` server-side (`LanguageServerHost.cs`). Project loads are
  queued asynchronously; nothing signals completion to the client.
- **Readiness:** there is **no `$/progress` "Loading workspace"** anywhere in the LSP project
  (zero hits) — the csharp-ls progress pattern (WorkspaceFolder.fs:731) does not exist here.
  `o#/checkreadystatus` returns `OmniSharpWorkspace.Initialized`, which is set at the END of
  `WorkspaceInitializer.Initialize` — after project loads were *queued*, not completed
  (`WorkspaceInitializer.cs` `workspace.Initialized = true`). It is therefore not a
  load-complete gate. The only client-visible load-complete signal is polling `o#/projects`
  (`MSBuildWorkspaceInfo.Projects`, `MSBuildProjectInfo` with `Path`, `AssemblyName`,
  `TargetFramework(s)`, `SourceFiles`, `IsUnityProject`, …) until the expected `.csproj`
  entries appear `[I]` (needs probing; see §8).
- **Position encoding:** no `PositionEncodingKind`/`positionEncoding` anywhere in the source
  (zero hits) — the server never negotiates, so the LSP default **UTF-16** applies. In LSP
  mode positions are 0-based end to end (`ZeroBasedIndices=true`; the LSP handlers pass
  `Position.Line/Character` straight into request models — `OmniSharpDefinitionHandler.cs`,
  `OmniSharpReferencesHandler.cs`; the `ZeroBasedIndexConverter` then passes them through).
  Columns are consumed as Roslyn `LinePosition.Character` indices, i.e. UTF-16 code units
  `[I]` (consistent with the LSP default).
- **definition:** `textDocument/definition` → `GotoDefinitionRequest` → `Location`(s)
  (`OmniSharpDefinitionHandler.cs`).
- **references:** `textDocument/references` → `FindUsagesRequest` with `OnlyThisFile=false`
  and `ExcludeDefinition = !IncludeDeclaration` (`OmniSharpReferencesHandler.cs`) →
  `Location[]`.
- **text sync:** `textDocument/didOpen|didChange|didSave|didClose` (incremental sync;
  `TextDocumentSyncKind.Incremental`), `DocumentVersions` tracking, didSave version reset
  (1.39.15 fix).
- **Timeouts:** the server has no documented per-request timeout `[I]`; requests issued while
  a project is still loading return empty results (the file is not yet in a project; only the
  misc compilation answers) — the same silent-empty hazard the tethys readiness gates exist to
  prevent (see §6).
- **Failure:** startup failure (no MSBuild instance) throws inside the `initialize` handler
  — the client observes an initialize error or a dead process `[I]` (exact LSP library error
  path unverified, §8); per-project failures are non-fatal (diagnostics + skip).

---

## 4. Distribution, runtime, installation, licensing, maintenance, security

| Dimension | Facts |
|---|---|
| Distributions | GitHub releases + Azure blob feed `roslynomnisharp.blob.core.windows.net/releases/{version}/omnisharp(-http)-{os/arch}.{zip,tar.gz}` with `win-x64/-x86/-arm64`, `linux-x64/-musl-x64/-arm64/-musl-arm64/-bionic-arm64`, `osx`, `mono` ("Requires global mono installed") ([README.md](https://github.com/OmniSharp/omnisharp-roslyn/blob/83fd615e/README.md)); extension downloads to `.omnisharp/<version>(-net6.0)/` (`omnisharpManager.ts`) |
| Runtime | net6.0 build: **.NET SDK ≥ 6.0 required** (SdkInstanceProvider error text: "OmniSharp requires the .NET 6 SDK or higher"; vscode requirement check `semver < 6.0.0` — `requirementCheck.ts:55-60`), runs on newer runtimes via `RollForward=LatestMajor`; net472 build: Windows needs .NET Framework + VS/Build-Tools MSBuild; macOS/Linux needs **Mono ≥ 6.4.0 including Mono's MSBuild** (`MonoInstanceProvider.cs`; vscode requirement check `requirementCheck.ts:104-120`; README "Note: If working on a solution that requires versions prior to .NET 6 … install a .NET Framework runtime and MSBuild tooling … macOS/Linux: Mono with MSBuild") |
| Installation | No `dotnet tool`; server binaries downloaded by editors (vscode pins `"omniSharp": "1.39.14"` in `package.json:45`), or built from source (needs .NET SDK per `global.json`: `10.0.100-preview.6.25358.103`); config via `omnisharp.json` (`MSBuild`, `SDK`, `RoslynExtensionsOptions`, `FormattingOptions`, … — repo sample `omnisharp.json`) |
| Licensing | **MIT**, © .NET Foundation and Contributors (`license.md`) |
| Release cadence | 1.39.11 (2023-12-19) → 1.39.12 (2024-07-26) → 1.39.13 (2024-12-31) → 1.39.14 (2025-09-01) → 1.39.15 (2025-11-13) ([CHANGELOG.md](https://github.com/OmniSharp/omnisharp-roslyn/blob/83fd615e/CHANGELOG.md)); ~1-3 releases/year, Roslyn version bumps dominate |
| Maintenance posture | The C# extension (omnisharp-vscode → dotnet/vscode-csharp) is "powered by a Language Server Protocol (LSP) server" with **Roslyn LSP as the default**; OmniSharp is opt-in via `dotnet.server.useOmnisharp: true` and C# Dev Kit must be disabled ([README.md](https://github.com/dotnet/vscode-csharp/blob/d8377e37/README.md), `src/shared/options.ts:103-105`); the June 2023 announcement describes the extension as "powered by a new fully open-source Language Server Protocol (LSP) host … built on the incredible foundation started with OmniSharp" ([Announcing C# Dev Kit](https://devblogs.microsoft.com/visualstudio/announcing-csharp-dev-kit-for-visual-studio-code/)). No formal "deprecated" label on the server itself; effectively the legacy/fallback backend of the VS Code toolchain |
| Cross-platform | net6.0 build: Windows/macOS/Linux for SDK-style projects; legacy .NET Framework fidelity: Windows via VS MSBuild, macOS/Linux via Mono MSBuild (net472 build); no `.slnx`; `.sln`/`.slnf` only; Unity special-cased |
| Security | Same trust boundary as any MSBuild evaluation/design-time build: imports, custom tasks, SDK resolvers, property functions execute managed code in the server process (cf. msbuild-discovery.md §5 T1/T4 and [Secure MSBuild usage best practices](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-security-best-practices)); auto-restore adds `dotnet restore` (network + package `build/` assets, T2); `GenerateBinaryLogs` embeds project sources (T5); HTTP mode is unauthenticated localhost `[I]`; legacy Mono/VS path couples behavior to system-wide toolchain state (T7) |

---

## 5. Legacy-project compatibility matrix

Legend: **Y** = full, **P** = partial/conditional, **N** = none, **?** = unverified.

| Capability | OmniSharp net6.0 (+.NET SDK) | OmniSharp net472 (Mono / VS MSBuild) | csharp-ls (Roslyn MSBuildWorkspace, .NET 10 SDK) | MSBuild CLI eval (`dotnet msbuild -get*`, msbuild-discovery) |
|---|---|---|---|---|
| SDK-style `.csproj` | Y | Y (Mono MSBuild 16.x; SDK resolvers from Mono) | Y | Y |
| Legacy non-SDK `.csproj` (ToolsVersion, `Microsoft.CSharp.targets`) | P `[I]` — same engine as the CLI path, which evaluates ToolsVersion projects on Linux via SDK-shipped targets (msbuild-discovery §1 `[E]`); ref-assembly fidelity needs `Microsoft.NETFramework.ReferenceAssemblies` | Y (Mono MSBuild or VS MSBuild, full toolset) | ? — README claims ".NET Framework 4.8 and potentially earlier" support ([README](https://github.com/razzmatazz/csharp-language-server/blob/e2efc47/README.md)); Linux path unverified `[E]` | P `[E]` (evaluation; full build needs reference assemblies) |
| `.sln` (Format 12) | Y (own parser) | Y | Y (Roslyn `SolutionFileReader`) | Y (via `SolutionFile` API; `-get*` rejected MSB1063) |
| `.slnf` | Y (own filter reader) | Y | N (not searched; only `*.sln`/`*.slnx` — `Solution.fs:339-343`) | Y |
| `.slnx` | **N** (no source support) | N | Y (searched) | Y |
| No solution → directory glob | Y (`**/*.csproj`) | Y | Y (`*.csproj;*.fsproj`) | Y per project |
| Multi-targeting | P — **first TFM only** | P — first TFM only | P — one workspace-wide "best compatible" TFM (`workspaceTargetFramework`/`bestTfm` in `Solution.fs`) | Y — per-TFM inner evaluation |
| `packages.config` | P — refs surface via `ReferencePath` if restored; not as package identity; auto-restore keys on `PackageReference` only | P — same | ? (not surfaced in MSBuildWorkspace model either; PackageReference items only `[I]`) | P — explicit XML sees `packages.config` file, not resolved graph |
| Generated sources | Y — design-time `Compile` includes `obj/` generated files; source generators in-memory | Y | Y — MSBuildWorkspace design-time load includes generated + in-memory generators | P — only after targets run |
| Custom SDKs | Y (SDK resolvers of the registered MSBuild) | Y | Y (inside build host) | Y |
| Project-less workspace | Y — misc documents; server survives | Y | **N — unhandled exception, process crash** (`Solution.fs:375-377`; probe SIGABRT `[E]`) | N/A (not a server) |
| Unity | Y — `IsUnityProject` | Y | ? | ? |
| C# type GUID discrimination | Y — `.sln` parser + `.csproj` filter | Y | N — loads any `.sln` project (mixed-language solutions open non-C# too `[I]`) | Y (SolutionFile GUIDs) |

---

## 6. tethys provider-fit analysis (exact local symbols, HEAD `7ca48f8`)

The seam: `src/lsp/provider.rs` `LspProvider` trait (`command`, `args`, `initialize_options`,
`install_hint`, `readiness_wait` — lines 42-70), implemented by `RustAnalyzerProvider`
(`"rust-analyzer"`, `ReadinessWait::Quiescence`) and `CSharpLsProvider` (`"csharp-ls"`,
`ReadinessWait::SolutionLoad`, hint `dotnet tool install --global csharp-ls`); `AnyProvider::for_language`
selects per `Language` (`provider.rs:186-191`). `LspClient::start(provider, workspace_path)`
spawns with piped stdio and runs the initialize handshake (`transport.rs:69`); Pass 3
(`resolve_via_lsp`, `resolve.rs:688`) pre-opens files, waits readiness
(`wait_until_ready`, `transport.rs:607`), then `goto_definition` per unresolved reference,
converting DB 1-indexed byte columns to 0-indexed and to the negotiated encoding
(`transport.rs:186` `encode_outgoing_col`; `src/lsp/encoding.rs` `PositionEncoding`,
`from_initialize_result`). `DEFAULT_RESPONSE_TIMEOUT` = 60 s per request
(`transport.rs:32`); solution-load timeout = `DEFAULT_LSP_TIMEOUT_SECS` 60 s / `TETHYS_LSP_TIMEOUT`
(`types.rs:905`, `IndexOptions::with_lsp` `types.rs:952`). Failures surface as
`LspOutcome::ServerUnavailable`/`Completed{error_count}` (`types.rs:1158-1192`); the CLI gate
`check_lsp_availability` checks **only rust-analyzer** (`cli/mod.rs:76-77`).

An `OmniSharpProvider` would require, per seam element:

| Seam element | Requirement | Gap / fit |
|---|---|---|
| `command()`/`args()` | `dotnet <path>/OmniSharp.dll --lsp -s <workspace>` (server is `OmniSharp.dll` run by the system dotnet — `omnisharpManager.ts`; net6.0 distribution) | **Workspace path is not available to the provider**: `LspClient::start` passes `workspace_path` only in `initializeParams.root_uri` (`transport.rs:155-175`), and OmniSharp **ignores `rootUri`** (§1). The process inherits tethys's CWD, which becomes the workspace. → `-s` must be injected, requiring either a provider method that receives the workspace path, a `cwd` on spawn, or an extension of `LspProvider`. **This is the largest adapter change.** |
| `initialize` | Standard LSP handshake; OmniSharp accepts `rootUri`-less init if `-s` given | No server-side obstacle; but no-MSBuild-instance → `MSBuildNotFoundException` inside the initialize handler → `LspError::InitializeFailed`/`ServerExited` path (`error.rs` variants) — handled by `resolve_via_lsp` as `ServerUnavailable` |
| `readiness_wait()` | Must return a `ReadinessWait` | **Neither existing variant fits**: OmniSharp emits no `$/progress` "Loading workspace" and no `experimental/serverStatus`. Declaring `SolutionLoad` would wait the full 60 s then proceed (warned timeout path, `transport.rs:494`); declaring `Quiescence` never fires. A new variant (e.g. poll `o#/projects` until the workspace's `.csproj` files appear) requires new transport machinery — `wait_until_ready` only reads notifications (`transport.rs:607-611`). Without a real gate, Pass 3 queries during load return empty definitions (misc-compilation-only view) — the exact silent degradation the readiness wait exists to prevent (`resolve.rs` comment above the wait, and `provider.rs` `ReadinessWait` docs) |
| Position encoding | UTF-16 (server never advertises `positionEncoding`) | Fits existing machinery: `from_initialize_result` → `Utf16` (`encoding.rs:31-36`), `encode_outgoing_col` re-measures byte→UTF-16 per line (`transport.rs:186-212`). No change needed. |
| `goto_definition` / `find_references` | Standard `textDocument/definition` / `textDocument/references` | Both implemented (`OmniSharpDefinitionHandler`/`OmniSharpReferencesHandler`); references honor `includeDeclaration` (tethys sends `include_declaration: false` → `ExcludeDefinition=true`). Note: OmniSharp references can include generated/misc documents `[I]`; `uri_to_path` + `find_symbol_at_line` matching (`resolve.rs` post-processing) is unchanged |
| `did_open` | `didOpen` with languageId `"csharp"` | Fits; OmniSharp buffers via `FileOpenRequest`/`UpdateBuffer` and creates misc documents for non-project files (`BufferManager.cs:179`) |
| Timeouts/failure | 60 s per request; 60 s readiness | OmniSharp loads projects in an async queue; large solutions can exceed the readiness budget with no signal — timeout tuning would be per-workspace `[I]`. Server survives project failures (unlike csharp-ls's crash on empty workspaces `[E]`), so `shutdown()` (`transport.rs:751`) and session reporting (`LspCompletedSession`) behave |
| Install/availability | Not a dotnet tool; needs downloaded binaries + .NET SDK ≥ 6.0 (+ optionally Mono/VS for legacy) | `install_hint` string fits the trait, but the CLI availability gate only checks rust-analyzer (`cli/mod.rs:76-77`) — a C#-only machine already fails `--lsp` today; an OmniSharp provider would not change that gate |

**Separating refinement from discovery (the seam constraint).** OmniSharp's LSP mode exposes a
full project model (`o#/projects` → `MSBuildWorkspaceInfo`/`MSBuildProjectInfo`: per-project
`SourceFiles`, `References`, `TargetFramework(s)`, `AssemblyName`, `IsUnityProject`), and its
misc-document fallback silently synthesizes a compilation for non-project files. Using the
server as the persisted project-discovery authority would inherit its model's distortions:
single-TFM-per-project (first TFM only — §2.3), `.sln`/`.slnf`-only (no `.slnx`), no
`packages.config` package identity, `obj/`-generated files inside `SourceFiles` (tethys
excludes `obj` today — `indexing.rs:1249` `is_excluded_dir`), and an in-process design-time
build with the full MSBuild code-execution trust boundary (§4; msbuild-discovery §5 T1/T4).
The existing seam already encodes this separation: LSP is Pass-3 refinement gated by
`IndexOptions::with_lsp` (`types.rs:952`), while discovery remains the tree-sitter walk plus
(optionally) the MSBuild mechanisms surveyed in msbuild-discovery.md §2. Any OmniSharp
integration should stay on the refinement side — its project data may be used to *compare
against* discovery results, never to replace the discovery contract (the `ModuleResolver`
seam, `src/languages/module_resolver.rs`, and `tests/seam_lint.rs` fences).

**Where OmniSharp would genuinely add legacy coverage** (vs csharp-ls):

- Project-less / no-solution / non-SDK workspaces: OmniSharp survives and answers from misc
  documents; csharp-ls crashes when no `.sln`/`.slnx`/`.csproj` exists under the root
  (`Solution.fs:375-377`; SIGABRT probe `[E]`) and has no legacy-MSBuild path on Linux
  (`?`).
- Legacy .NET Framework on macOS/Linux: OmniSharp has a first-class Mono-MSBuild design-time
  path (net472 build) and a VS-MSBuild path on Windows — the only surveyed LSP server with
  one. csharp-ls relies on MSBuildWorkspace's build-host machinery, whose legacy fidelity on
  Linux is unverified (§8).
- `.slnf` filtering (csharp-ls ignores `.slnf`).
- Resolved `ReferencePath` data (`ProjectData` reads resolved references; csharp-ls's
  MSBuildWorkspace also resolves them, but with `LoadMetadataForReferencedProjects=true`
  which trades fidelity for cost — `Solution.fs:270`).

**Where it would merely duplicate csharp-ls** (for SDK-style workspaces, tethys's current
target): both are Roslyn-workspace servers answering the same two LSP requests; OmniSharp
adds a second runtime dependency (.NET SDK ≥ 6 + download/install path) and a slower
load model (sequential queue) for no refinement gain.

**Where it would introduce regressions/risks:**

- Toolchain coupling: server startup fails outright without a discoverable .NET SDK
  (net6.0) or Mono/VS (net472); behavior changes with system toolchain state (T7).
- Readiness regression risk: no load-complete signal means Pass 3 can silently resolve
  nothing on cold workspaces unless new poll-based machinery is built — worse than the
  status quo (csharp-ls `SolutionLoad` works, `[E]`).
- Security surface: in-process MSBuild evaluation + optional `dotnet restore` — the same
  boundary as MSBuild CLI evaluation but running as a long-lived server with the editor's
  privileges.
- Maintenance: slow release cadence and ecosystem migration to Roslyn LSP (Dev Kit); a
  tethys dependency on OmniSharp inherits that trajectory.
- `-s` plumbing requires a seam extension (see table) — not a drop-in provider.

---

## 7. Recommendation candidates (alternatives, not a decision)

- **A — Keep csharp-ls as the sole C# provider; do not adopt OmniSharp.** Zero seam change;
  legacy coverage stays as-is (crash on project-less workspaces, `.slnf`/legacy-fidelity
  gaps). Cheapest; leaves the legacy parity question unanswered.
- **B — Add an `OmniSharpProvider` as an opt-in alternative C# provider, gated to legacy
  workspaces** (no `.slnx`, non-SDK, or project-less roots) **and bundled with a new
  readiness mechanism** (e.g. `o#/projects` polling until expected `.csproj` set appears)
  and `-s` plumbing (provider receives workspace path or spawn `cwd`). Requires the largest
  seam change of the options; delivers the only surveyed LSP path to Mono/VS-backed legacy
  project loading; inherits OmniSharp's toolchain coupling and maintenance trajectory.
- **C — Use OmniSharp as an evidence/calibration source only** (or, equivalently, adopt the
  MSBuild design-time-build mechanism it implements — already surveyed in msbuild-discovery.md
  §2.3) **for authoritative project discovery, and keep csharp-ls for refinement.** The
  discovery contract stays on tethys's side (per-TFM, `.sln`/`.slnx`/`.slnf`, explicit
  packages.config handling); the LSP server never becomes the discovery authority. No `LspProvider`
  change needed if discovery runs via the MSBuild CLI surface instead; OmniSharp's `o#/projects`
  model can serve as a cross-check oracle.
- **D — Defer legacy parity; document the gap** (existing note at `src/db/call_edges.rs:231`
  and `orphan:` pseudo-crate bucketing) until a concrete workspace demands it.

Decision-relevant constraints, not a choice: (1) OmniSharp is the only surveyed LSP server
with an explicit legacy-project design-time path (Mono/VS MSBuild), but it is not the
default backend of its own primary client anymore; (2) every OmniSharp mode except the
custom stdio protocol is a second-class surface (`--lsp` opt-in flag, `enableLspDriver`
default false), and the custom protocol is 1-based with a deprecated `--stdio` flag — the
LSP surface is the correct one for tethys; (3) readiness is the make-or-break adapter
problem, not the protocol.

---

## 8. Unresolved empirical cells

1. OmniSharp LSP `initialize` failure shape when no MSBuild instance exists
   (`MSBuildNotFoundException` inside the handler): error response vs process exit — not
   probed (would require installing/launching the server, out of scope here).
2. `o#/projects` polling latency and correctness as a readiness gate (how quickly
   `MSBuildWorkspaceInfo.Projects` reflects queued loads; behavior for `LoadProjectsOnDemand`).
3. Legacy non-SDK `.NET Framework` project fidelity on Linux through the **net6.0** build
   (SDK-shipped `Microsoft.CSharp.targets` evaluation) vs the **net472/Mono** build
   (reference assemblies, WinForms/XAML targets, `packages.config` restore interplay) —
   neither probed.
4. Whether OmniSharp's auto-restore ever fires for `packages.config` projects
   (`PackageDependencyChecker` keys on `PackageReference` items only — legacy projects have
   none, so restore is likely never triggered `[I]`).
5. UTF-16 position semantics on non-ASCII lines through OmniSharp's LSP handlers (assumed
   from absence of negotiation; no probe).
6. csharp-ls legacy-project behavior on Linux (its README's ".NET Framework 4.8" claim) —
   `LspResearch`'s probes used SDK-style projects only `[E]`; the `.slnf` gap is source-
   verified but not probed.
7. Multi-TFM selection divergence: OmniSharp picks the **first** TFM, csharp-ls picks a
   workspace-wide **best-compatible** TFM — which lands for mixed `net48`+`net8.0` projects
   is unverified and likely differs between the two servers.
8. OmniSharp `.sln` parser behavior on solution files that MSBuild's own parser accepts
   (e.g. trailing content, unusual section ordering) — parser is independent
   (`SolutionParsing/Scanner.cs`), equivalence not tested.

---

## 9. Sources

### omnisharp-roslyn @ `83fd615e` (2025-11-13) — [primary]
- README.md, CHANGELOG.md (1.39.9-1.39.15; `--stdio` deprecation PR #2554 / #2439), license.md (MIT), global.json, omnisharp.json (sample config), doc/Using-Omnisharp.md
- src/OmniSharp.Host/CommandLineApplication.cs (`-s`, `-z`, `--hostPID`, `ApplicationRoot` default CWD)
- src/OmniSharp.Host/MSBuild/Discovery/MSBuildLocator.cs; Providers/{SdkInstanceProvider,SdkOverrideInstanceProvider,MonoInstanceProvider,MicrosoftBuildLocatorInstanceProvider,UserOverrideInstanceProvider}.cs; Extensions.cs (`RegisterDefaultInstance`, `GetBestInstance`, `MSBuildNotFoundException`)
- src/OmniSharp.Stdio/StdioCommandLineApplication.cs (`-lsp|--languageserver`, `-e`); src/OmniSharp.Stdio.Driver/Program.cs (LSP mode, `ZeroBasedIndices=true`)
- src/OmniSharp.Http.Driver/Program.cs
- src/OmniSharp.MSBuild/ProjectSystem.cs (solution/slnf/csproj discovery); ProjectManager.cs (queue, `_failedToLoadProjectFiles`); ProjectLoader.cs (design-time global properties, `Compile`+`CoreCompile` build, `SetTargetFrameworkIfNeeded`, toolset fallback); SdksPathResolver.cs; PackageDependencyChecker.cs; ProjectFile/ProjectFileInfo.cs + ProjectFileInfo.ProjectData.cs (items extraction); ProjectFile/PropertyNames.cs, ItemNames.cs; Options/MSBuildOptions.cs; Models/MSBuildWorkspaceInfo.cs + MSBuildProjectInfo.cs; SolutionParsing/{SolutionFile,Scanner,SolutionFilterReader}.cs (no `.slnx`)
- src/OmniSharp.LanguageServerProtocol/LanguageServerHost.cs (`o#/` interop, `checkreadystatus`, `Initialized`/`WaitForIdleAsync`, `CreateCompositionHost` root selection); Handlers/OmniSharpDefinitionHandler.cs, OmniSharpReferencesHandler.cs, OmniSharpTextDocumentSyncHandler.cs; OmniSharp.LanguageServerProtocol.csproj (`RollForward=LatestMajor`); Directory.Packages.props (OmniSharp.Extensions.LanguageServer 0.19.9)
- src/OmniSharp.Roslyn/OmniSharpWorkspace.cs (`TryAddMiscellaneousDocument`, `Initialized`), BufferManager.cs:179; src/OmniSharp.Roslyn.CSharp/Services/Files/FileOpenService.cs; src/OmniSharp.Host/WorkspaceInitializer.cs (`workspace.Initialized = true`); src/OmniSharp.Abstractions/Models/ZeroBasedIndexConverter.cs; src/OmniSharp.MSBuild/OmniSharp.MSBuild.csproj (MSBuild packages `ExcludeAssets=Runtime`); OmniSharp.Stdio.Driver/OmniSharp.Stdio.Driver.csproj (`RollForward`)
- Zero hits (absence evidence): `slnx`, `PositionEncoding`/`positionEncoding`, `Loading workspace`

### omnisharp-vscode / dotnet/vscode-csharp @ `d8377e37` (2026-08-07) — [primary]
- README.md (Roslyn LSP default; `dotnet.server.useOmnisharp`; legacy prerequisites .NET Framework + MSBuild Tools / Mono with MSBuild); package.json (`"omniSharp": "1.39.14"`); CHANGELOG.md
- src/shared/options.ts (`useOmnisharp` default false :103-105; `enableLspDriver` default false :379-381; `useModernNet` default true :173-174)
- src/omnisharp/server.ts (args `-z -s <solution> --hostPID … DotNet:enablePackageRestore=false`, engine selection :316/:329); src/omnisharp/engines/stdioEngine.ts (line-based client), lspEngine.ts (`-lsp --encoding ascii`); src/omnisharp/launcher.ts (`.slnx` among launch targets, project-file fallback); src/omnisharp/omnisharpManager.ts + omnisharpPackageCreator.ts (`modernNetVersion='6.0'`, `dotnet OmniSharp.dll`); src/omnisharp/requirementCheck.ts (SDK ≥ 6.0 / Mono + MSBuild requirements)
- docs/Troubleshooting-'The-.NET-Core-SDK-cannot-be-located.'-errors.md (`UseLegacySdkResolver` workaround)

### csharp-ls (razzmatazz/csharp-language-server) @ `e2efc47` (2026-08-04) — [primary]
- README.md (MIT, ".NET Framework 4.8 and potentially earlier", .NET 10 SDK requirement)
- src/CSharpLanguageServer/Roslyn/Solution.fs (`MSBuildWorkspace.Create`, `OpenSolutionAsync`/`OpenProjectAsync`, `LoadMetadataForReferencedProjects=true`, `*.sln;*.slnx` then `*.csproj;*.fsproj` search :339-377, crash on none :375-377, `workspaceTargetFramework`/`bestTfm` single-TFM props)
- src/CSharpLanguageServer/Lsp/Server.fs (`getServerCapabilities` — no `positionEncoding`); src/CSharpLanguageServer/Lsp/WorkspaceFolder.fs:731 (progress title); LspResearch probe facts (csharp-ls 0.26.0, nuspec pin `b714e27c`, SIGABRT on empty workspace, load-on-demand, UTF-16) `[E]`

### Microsoft (docs/blog/source) — [primary]
- microsoft/MSBuildLocator @ `4703577f` (2026-06-15) — `src/MSBuildLocator/MSBuildLocator.cs` (`QueryVisualStudioInstances` → `DotNetSdkLocationHelper.GetInstances` on NETCOREAPP), `DotNetSdkLocationHelper.cs` (`.version` + `Microsoft.Build.dll` checks, runtime-version gating, `DiscoveryType.DotNetSdk`, `MSBuildPath == SDK dir`), `VisualStudioInstance.cs`
- Announcing C# Dev Kit (2023-06-06) — https://devblogs.microsoft.com/visualstudio/announcing-csharp-dev-kit-for-visual-studio-code/
- VS Code C# extension repo README — https://github.com/dotnet/vscode-csharp (pinned d8377e37 above)
- Visual Studio Integration (MSBuild) design-time protocol — https://learn.microsoft.com/en-us/visualstudio/msbuild/visual-studio-integration-msbuild
- Secure MSBuild usage best practices — https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-security-best-practices

### Local (this branch HEAD `7ca48f8`)
- src/lsp/provider.rs:14-23 (`ReadinessWait`), 42-70 (`LspProvider`), 87-103 (`RustAnalyzerProvider`), 118-131 (`CSharpLsProvider`), 186-191 (`AnyProvider::for_language`)
- src/lsp/transport.rs:32 (`DEFAULT_RESPONSE_TIMEOUT` 60 s), 69 (`LspClient::start`), 155 (initialize/rootUri), 186 (`encode_outgoing_col`), 299 (`read_response`), 414 (`goto_definition`), 453 (`did_open`), 494 (`wait_for_solution_load`), 607 (`wait_until_ready`), 721 (`find_references`), 751 (`shutdown`), 841 (`client_capabilities`)
- src/lsp/encoding.rs:31 (`from_initialize_result`), src/lsp/status.rs (`SERVER_STATUS_METHOD`), src/lsp/error.rs (`LspError` variants)
- src/types.rs:115-152 (`Language`, `from_extension`, `as_str`), 836 (`UnresolvedRefForLsp`), 905 (`DEFAULT_LSP_TIMEOUT_SECS`), 952 (`IndexOptions::with_lsp`), 1118-1192 (`LspSessionResult`/`LspOutcome`/`LspCompletedSession`)
- src/resolve.rs:688 (`resolve_via_lsp`), 906 (`resolve_single_ref_via_lsp`)
- src/cli/mod.rs:76-77 (`check_lsp_availability` — rust-analyzer only), 114 (`ensure_lsp_if_requested`)
- src/indexing.rs:593 (`build_file_crate_map`), 1105 (`build_namespace_map`), 1168 (`discover_files`), 1249 (`is_excluded_dir` — `obj` excluded), 1260 (`run_architecture_phase`)
- src/db/call_edges.rs:21 (`ORPHAN_PSEUDO_CRATE_PREFIX`), :231-area (C# pseudo-crate / no `.csproj` discovery note); src/languages/module_resolver.rs (seam), tests/seam_lint.rs (fences)
- msbuild-discovery.md at `7477c36` (sibling branch `research/csharp-parity-msbuild`) — MSBuild/Roslyn mechanisms baseline; csharp-ls-cli.md on `research/csharp-parity-lsp` (peer artifact, leads only)
