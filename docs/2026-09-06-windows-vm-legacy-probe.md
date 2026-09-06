# Windows VM legacy probe: Roslyn `MSBuildWorkspace` over the legacy reference solution

Date: 2026-09-06. Companion to `2026-09-06-analysis-csharp-binding-model-review.md`
(§8 named this as the decisive day-one legacy probe). Runbook and sidecar source
in `2026-09-06-legacy-probe-sidecar/`.

Identifiers in this file are neutral phrases; the repository identity and
the real names behind each phrase are in the gitignored
`docs/private/legacy-reference-repo.md`.

## Setup (all done from Linux over WinRM, 127.0.0.1:55985)

| Step | Result |
|---|---|
| VM before | Windows 11 Pro, .NET Framework 4.8.1 runtime only; no VS, MSBuild, SDK, nuget.exe, git; internet OK |
| VS Build Tools 2022 17.14 silent install (MSBuildTools + WebBuildTools workloads, 4.8 + 4.7.2 targeting packs, NuGet build tools, Roslyn compiler) | completed in about 3 minutes; also installed .NET SDK 9.0.317 |
| Sources | `src/` + `tests/` of the reference repository (3,326 files, 29.8 MB zip) over HTTP from the host gateway address |
| `nuget.exe restore <solution>` | 41 s; 280 package folders; exit 1 with 16 packages missing from the private feed (ids in the private file); `.sqlproj` skipped |
| Sidecar | 100-line C# console app on `Microsoft.CodeAnalysis.Workspaces.MSBuild` 5.3.0, published self-contained win-x64 from Linux (114 MB) |

The 16 missing private packages (all `Unable to find version`) are listed in
the private file.

## Load results

| Measure | Value |
|---|---|
| Projects in solution | 84 entries: 50 csproj, 15 vbproj, 1 sqlproj, 18 solution folders |
| Projects loaded | 65 of 65 language projects (50 C#, 15 VB) |
| Workspace diagnostics | 30: 28 warnings "project reference without a matching metadata reference" (benign, the reference is still added), 2 failures |
| Failure 1 | `.sqlproj` "not associated with a language" (expected) |
| Failure 2 | a test project's ruleset path `..\<library>\<library>.ruleset` does not exist relative to `tests/unit` (real repo defect); project still loaded |
| Host | net472 BuildHost with VS Build Tools MSBuild (proof: the 11 web projects importing `Microsoft.WebApplication.targets` loaded, and that file exists only under `C:\BuildTools`, not under the SDK) |
| Solution load | 16.3 s cold, 8.0 s warm |
| Load plus 65 compilations plus a full symbol walk | 41.7 s cold, 34.4 s warm; peak working set 700 MB |
| `MSBuildLocator` from a .NET (Core) process | sees only the SDK instance; VS instances are invisible to .NET-Core-hosted locator, which is fine because BuildHost selects its own host |
| Analyzer references carried by projects | 67 across 27 projects (SonarAnalyzer, Microsoft.CodeAnalysis.Analyzers); a ruleset escalates CS8019 "unnecessary using" to Error in one project |

## Compilation results

| Measure | Value |
|---|---|
| Projects with zero errors | 31 of 65 |
| Total errors | 4,492 |
| Attributed to the 16 missing private packages (type or namespace not declared anywhere in the repo) | 2,330 (52 %) |
| Cross-language project reference dropped because the referenced project has errors | 1,423 VB→C# direction plus most of 280 C#→VB direction (about 38 %) |
| Dependent cascades (CS1061, CS1503, ...) | 384 (9 %) |
| CS8019 escalated by ruleset | 75 |

The cross-language number is the important one. Roslyn represents a
`ProjectReference` across languages as a metadata-only "skeleton" emit of the
referenced compilation. Emit fails when the referenced compilation has any
error, so the reference is dropped wholesale and every type from that project
becomes CS0246 in the consumer. Chain observed here: private packages missing →
a leaf C# project has 243 errors → the VB class library that references it
cannot bind it and gets 99 errors → the 17 C# projects that reference that VB
library lose its whole namespace. 27 of 65 projects lost at least one project
reference this way. Same-language references (`CompilationReference`) survive
errors, so C#→C# cascades stay local.

## Binding coverage (C# projects, even with the errors above)

| Node kind | Total | Bound | Notes |
|---|---|---|---|
| Invocations | 25,855 | 22,700 (88 %) | 14,091 bound targets live in metadata (BCL, packages, VB skeletons); unbound: 1,101 overload-resolution failures, 2,054 no candidate |
| Object creations | 6,229 | 5,738 (92 %) | |
| Member accesses (non-call) | 40,840 | 37,836 (93 %) | |

For comparison the tree-sitter index on the same sources resolved 26 % of
calls and 44 % of constructs, and by construction can never bind the 14,091
metadata targets.

## Run 2: private packages restored

The 16 missing packages were downloaded on the Linux host from the Azure
Artifacts feed named in the repository's `NuGet.config` (using `ADO_TOKEN`, sent
only to `pkgs.dev.azure.com`), laid out as `packages/<Id>.<Version>/` and copied
to the VM. Two gotchas worth keeping: the NuGet v3 flat container redirects to
blob storage and rejects a forwarded `Authorization` header (403 until the
client strips it on redirect, which real NuGet clients do), and four versions
existed only in the un-viewed feed, not in the `@Production` view the config
names. `nuget.exe restore` then exited 0 with nothing missing.

| Measure | Run 1 (16 packages missing) | Run 2 (all packages) |
|---|---|---|
| Projects with zero errors | 31 of 65 | 50 of 65 |
| Total errors | 4,492 | 196 |
| Projects that lost a cross-language project reference | 27 | 7 (all VB consumers of one C# class library, which has 6 errors of its own) |
| Invocations bound | 22,700 of 25,855 (88 %) | 25,321 of 25,855 (97.9 %) |
| Object creations bound | 5,738 of 6,229 (92 %) | 6,183 of 6,229 (99.3 %) |
| Member accesses bound | 37,836 of 40,840 (93 %) | 40,784 of 40,840 (99.9 %) |
| C# calls bound to VB metadata | 13 | 680 across 10 C# projects |
| Bound call targets in metadata | 14,091 | 16,263 |
| Unbound reasons | 2,054 none, 1,101 overload failure | 528 none, 6 overload failure |
| Wall clock, peak working set | 41.7 s, 700 MB | 36.5 s, 678 MB |

Residual 196 errors, attributed:

- 104 in the seven VB scheduled-task and web-service projects that reference
  one C# class library: the cross-language skeleton drop again, now triggered
  by only six errors in that library (types from a private utilities package
  used without a usable direct reference, CS0012). Cross-language references remain
  all-or-nothing.
- 47 in two test projects: they
  reference `Microsoft.VisualStudio.QualityTools.UnitTestFramework`, an assembly
  that ships with full Visual Studio, not with Build Tools. A legacy shape the
  fixture corpus needs: "VS-only assembly reference".
- 12 in one class library: CS8019 "unnecessary using" escalated to Error
  by the project's ruleset inside a generated WCF `Reference.cs`. A semantic
  engine must classify diagnostics by their effect on binding, not by severity.
- 33 real missing-reference defects in the repository: a mapping type not
  referenced by one web project, a logging library's `ILog` and `LogManager`
  not resolvable in three web projects, plus the six above.

## What this establishes for the roadmap

1. Day-one classic .NET Framework support is real on Windows with Build Tools
   and needs no tethys-owned host adapter: Roslyn's out-of-process BuildHost
   selected VS MSBuild, evaluated VS-only imports, and produced compilations for
   all 65 projects in under a minute at 700 MB.
2. Under the approved contract, `packages.config` restore succeeded through
   `nuget.exe` on Windows; the residual failures are private-feed access, which
   is exactly the `restore-failed` outcome rvr5 defines. With feed credentials,
   expect the private-package and cross-language buckets (about 90 % of errors)
   to disappear; this run cannot prove that.
3. Cross-language references are all-or-nothing under skeleton emit. The
   binding model's "preserve established bindings through unrelated compiler
   errors" (Q17) does not hold across the VB boundary without either a built
   assembly on disk or a tolerant skeleton strategy; the standing contract must
   scope that loss to the consumer projects, and complog ingestion of a real
   build avoids it entirely.
4. Two real-world "malformed input" shapes were met and tolerated: an unknown
   project type in the solution and a wrong ruleset path. Both should be
   fixtures.
5. With the private feed available, Roslyn bound 97.9 % of invocations and
   99.9 % of member accesses on the real repository, including 680 C# calls
   into VB projects; the tree-sitter path resolved 26 % of calls on the same
   sources and cannot reach the 16,263 metadata targets at all. The semantic
   route is the right one, and it needs no tethys-owned host adapter.
6. The remaining defects are the repository's own (VS-only test assembly,
   missing references, ruleset noise) and each is a fixture the failure
   contract should name rather than a reason to treat the run as Indeterminate
   wholesale.

## VM state left behind

`C:\BuildTools` (VS Build Tools 17.14), `C:\Program Files\dotnet` (SDK
9.0.317), and `C:\probe\` (staged sources, sidecar, nuget.exe, reports, logs). Remove with
the Build Tools uninstaller and `Remove-Item -Recurse C:\probe` if the VM must
return to its previous state.
