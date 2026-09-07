# MSBuild discovery and evaluation

The companion evaluates C# project metadata through the selected MSBuild installation. It does not compile source, execute targets, restore project dependencies, or acquire compiler bindings. Evaluation can execute MSBuild property functions and SDK-resolver code: the evaluation grant is trust, not a sandbox.

## Distribution and development

Release archives place the SDK companion in `msbuild-evaluate/sdk/`. Windows archives also contain `msbuild-evaluate/framework/` for Visual Studio MSBuild. Keep each directory intact: its runtime configuration and supporting assemblies are part of the distribution. Microsoft.Build itself comes from the explicitly selected installed toolchain, not a copied runtime assembly in the companion directory.

The SDK companion runs as:

```text
dotnet msbuild-evaluate/sdk/Tethys.MSBuild.Evaluate.dll
```

The Windows Framework companion runs as:

```text
msbuild-evaluate/framework/Tethys.MSBuild.Evaluate.exe
```

The SDK worker targets .NET 8 with `LatestMajor` runtime roll-forward so an installed newer SDK can load its matching MSBuild. The Framework worker targets .NET Framework 4.7.2 and runs x86 to match the qualified Visual Studio 17.14 `MSBuild/Current/Bin/MSBuild.exe` host; select that installation directory, not its `amd64` sibling. Assembly loading alone does not establish the effective toolset: an AnyCPU worker can load from `Bin` yet evaluate using `Bin/amd64`. A missing companion, runtime, or selected toolchain is an error; evaluation never builds or downloads its own helper.

For development, build the companion explicitly before invoking it:

```sh
dotnet restore tools/tethys-msbuild-evaluate/Tethys.MSBuild.Evaluate.csproj --locked-mode
dotnet build tools/tethys-msbuild-evaluate/Tethys.MSBuild.Evaluate.csproj --configuration Release --no-restore
```

The managed lockfile pins development dependencies. CI checks their licenses against the repository's allowed-license policy and packages the matching runtime flavor.

## Protocol 1

One process handles one UTF-8 JSON request from stdin and writes one JSON response to stdout. Stderr is diagnostic output, not another protocol channel. The input bound is 1 MiB; response bound is 64 MiB; diagnostic output bound is 1 MiB. A bound violation is a failure, not a truncated successful result.

Request fields:

| Field | Meaning |
|---|---|
| `protocol_version` | `1` |
| `workspace_root` | Absolute workspace path in native-compatible form |
| `project_path` | Absolute project-file path |
| `target_framework` | Inner SDK framework selector, or `null` for outer/classic evaluation |
| `global_properties` | Explicit caller property map |
| `msbuild_path` | Selected installed MSBuild directory |
| `trust_granted` | Explicit permission to evaluate project code |

The selected host is registered before Microsoft.Build types load. An absent trust grant cannot evaluate a project. An inner request evaluates with its selected `TargetFramework`; a conflicting explicit `TargetFramework` global is rejected, not overwritten. Outer multi-target values are not a substitute for inner metadata. Classic projects may have empty `TargetFramework` and `TargetFrameworks`: their identifier, version, profile and platform remain meaningful framework identity.

Response fields:

- `protocol_version`, `success`, and `project_path` identify the result.
- `host` records actual runtime kind, loaded toolchain path, full MSBuild file version and runtime; failures before loading may have no host.
- `properties` contains evaluated framework, assembly, compiler-option, output, restore-location and toolchain metadata, plus explicitly requested global-property keys. It is not a dump of ambient environment variables.
- `items` has `Compile`, `ProjectReference`, `Reference`, `PackageReference`, `PackageVersion`, and `PackageDownload` arrays. Each item contains `include`, `full_path`, and evaluated `metadata`. Authored metadata spelling is retained; known names such as `Link`, `HintPath`, and package versions are interpreted case-insensitively, as MSBuild does.
- `imports` lists imported project paths. `glob_patterns` records defining project, item type and include/exclude/remove expressions.
- `diagnostics` carries severity, native code, exception type, message, file and source position where available. Human message substrings are not failure categories.
- `cache_eligible` and `cache_ineligibility` report whether a qualified evaluation recipe can account for its input closure. They are evidence, not permission to reuse a result without validating its inputs.

A declared `ProjectReference` is not proof that MSBuild selected a particular target evaluation unit. The companion never guesses a selected framework from equal shorthand or expands declarations into all-to-all unit edges.

## Import and restore policy

Missing imports remain errors. There is no blanket ignore-missing-imports mode. If a caller explicitly supplies the approved import-tolerance profile as `VSToolsPath=""`, that value is evaluated and reported; the worker does not silently invent it or override other caller globals.

The companion has no restore or target-execution mode. Restoring project assets is a separately authorized operation of the discovery caller, using the repository's normal package policy. Evaluation metadata is not a substitute for reference assemblies, target-generated references, generated source, or Roslyn semantic facts.

## Standalone discovery

`tethys::discovery::discover_workspace` accepts a `DiscoveryRequest` and returns Cargo attribution plus C# projects, evaluation units, source memberships, diagnostics and opaque cache entries. `CargoDiscovery` reuses the existing Cargo discovery algorithms. This API is independently usable; CLI indexing does not yet invoke it.

Candidate discovery reads `.sln`, `.slnx`, `.slnf`, and standalone `.csproj` files without executing project code. Filters resolve relative to their referenced solution and do not suppress unrelated standalone projects. Canonical path containment rejects outside-workspace declarations and source links. Automatic candidates exclude generated and internal directory identities; explicit solution declarations may name projects in generated directories. An empty workspace differs from malformed containers or failed enumeration.

The Rust adapter keeps canonical workspace, project and host identities, but presents filesystem-equivalent ordinary Windows paths to MSBuild/NuGet when possible. SDK MSBuild and VS17.14 can silently omit globbed items for verbatim project paths (`tethys-82a6`). The adapter validates the exact presented response identity before restoring canonical project/host identity for domain records and cache receipts; authored global values and native item metadata are not rewritten.

`DiscoveryOptions::trust_msbuild` is required before host probes or evaluation. `allow_restore` is a separate grant, not implied by trust. An explicit installed `msbuild_path` selects its own SDK muxer and cannot be overridden by ambient `DOTNET`; implicit SDK selection honors applicable `global.json` policy. Missing helpers and unsupported toolchains are reported, never installed on demand.

Confirmed units retain their full framework identity, compiler/output properties, source memberships and declared references. A failed outer evaluation remains an indeterminate project; failed inner evaluations retain their selectors beside successful sibling units. Stable reasons are `trust-required`, `restore-required`, `restore-failed`, `restore-unsupported-on-host`, `toolchain-unavailable`, `sdk-unresolved`, `malformed-input`, `evaluation-failed`, `timeout`, `outside-workspace-input`, and `partial-target-frameworks`.

Restore uses normal selected-host MSBuild/NuGet policy without replacing feeds or disabling lock enforcement. `packages.config` restore requires Windows and installed NuGet. Native restore success is followed by input validation and authoritative reevaluation; it does not excuse unrelated source or glob changes. Existing valid restore inputs do not require a new restore grant.

Legacy restore retains the actual solution behind a solution filter and supplies its directory to NuGet. A repository `repositoryPath` takes precedence over the solution's default `packages` directory; multiple solution directories are not resolved by choosing the first or searching arbitrary ancestor folders. Applicable user/machine configuration files participate in currentness checks. When their destination cannot be established, or a repository path requires environment expansion that is not qualified, discovery declines rather than accepting a guessed directory. A nearer repository configuration assignment or explicit `<config><clear /></config>` can establish policy; tethys never writes that override itself.

## Freshness and reuse

Every discovery invocation validates current inputs. Pass previous opaque entries through `DiscoveryRequest::with_cache`; `DiscoveryCachePolicy::Disabled` forces evaluation while still allowing existing restore evidence to be checked. Cache entries are acceleration data, not an authority for project standing.

Reusable evaluation requires a qualified recipe and matching project, inventory/glob, environment, selected host/runtime, companion and restore evidence. Corrupt or unknown receipts cause misses. Imported or otherwise unqualified input closures, runtime hooks/profilers, and forwarding muxers force reevaluation. Framework evaluation also bypasses evaluation reuse: CLR4's stable runtime version does not establish an exact CLR/BCL closure. Metadata evaluation remains available on that host.

The supervisor enforces the per-process deadline and protocol byte limits, terminates the process group, and bounds reaping. These controls do not make arbitrary trusted project code a security sandbox.

## Qualification

The qualification runner compares the companion with direct selected-host MSBuild evaluation and independently authored fixture manifests. Prepare the distribution explicitly first; normal qualification never builds or restores. On Linux/macOS:

```sh
python3 .tethys-82a6/oracles/worker_qualification.py --host sdk --prepare
python3 .tethys-82a6/oracles/worker_qualification.py --host sdk
```

On Windows with Visual Studio 17.14 installed:

```sh
python .tethys-82a6/oracles/worker_qualification.py --host windows --prepare
python .tethys-82a6/oracles/worker_qualification.py --host sdk
python .tethys-82a6/oracles/worker_qualification.py --host windows
```

`DOTNET` selects the dotnet executable; `TETHYS_SDK_MSBUILD_PATH` selects an installed SDK directory. `--distribution` overrides the default `target/worker-dist` directory. Windows qualification uses `vswhere` to select and report an actual VS 17.14 installation, with no fallback to SDK MSBuild for classic qualification.

The fixtures cover SDK multi-targeting, classic v4.0 Client/v4.7.2/v4.8, imported and linked source, conditioned/globbed/removed items, assembly-name collisions, spaces and Unicode. A target sentinel must remain absent during evaluation; a separately authorized target invocation is the positive control. Clean-install qualification copies the distribution away from build outputs and evaluates using the explicitly selected host.

The native Rust discovery gate additionally runs `discovery_candidates`, `discovery_failures`, `discovery_cache`, and `discovery_runtime` with `--run-ignored all`. Set `TETHYS_WORKER_DISTRIBUTION` to the packaged `msbuild-evaluate` directory and `TETHYS_SDK_MSBUILD_PATH` to the selected SDK. Windows CI separately supplies VS17.14 and NuGet6.14 for an offline `packages.config` restore transition.

Cache-launch proof uses native `COREHOST_TRACE`/`DOTNET_HOST_TRACE`, with a fresh trace file for each completed invocation. Fresh and forced controls must launch the actual evaluator; an eligible hit must neither launch nor attempt to start it. Runtime-hook and forwarding-muxer fixtures change an external value at the same path and require changed native metadata, rather than trusting the cache's own reuse flag.
