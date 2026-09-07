# MSBuild evaluation companion

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

The SDK worker targets .NET 8 with `LatestMajor` runtime roll-forward so an installed newer SDK can load its matching MSBuild. The Framework worker targets .NET Framework 4.7.2 and is qualified against Visual Studio MSBuild. A missing companion, runtime, or selected toolchain is an error; evaluation never builds or downloads its own helper.

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
| `workspace_root` | Canonical absolute workspace path |
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
- `items` has `Compile`, `ProjectReference`, and `Reference` arrays. Each item contains `include`, `full_path`, and evaluated `metadata`; imported/linked items retain their metadata, including `Link` and `HintPath` where present.
- `imports` lists imported project paths. `glob_patterns` records defining project, item type and include/exclude/remove expressions.
- `diagnostics` carries severity, native code, exception type, message, file and source position where available. Human message substrings are not failure categories.
- `cache_eligible` and `cache_ineligibility` report whether a qualified evaluation recipe can account for its input closure. They are evidence, not permission to reuse a result without validating its inputs.

A declared `ProjectReference` is not proof that MSBuild selected a particular target evaluation unit. The companion never guesses a selected framework from equal shorthand or expands declarations into all-to-all unit edges.

## Import and restore policy

Missing imports remain errors. There is no blanket ignore-missing-imports mode. If a caller explicitly supplies the approved import-tolerance profile as `VSToolsPath=""`, that value is evaluated and reported; the worker does not silently invent it or override other caller globals.

The companion has no restore or target-execution mode. Restoring project assets is a separately authorized operation of the discovery caller, using the repository's normal package policy. Evaluation metadata is not a substitute for reference assemblies, target-generated references, generated source, or Roslyn semantic facts.

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
