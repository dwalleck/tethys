"""THROWAWAY: measure evaluation-only MSBuild process overhead, not tethys indexing.
Uses only generated projects under a temporary directory. No restore or targets.
Five repetitions use fresh MSBuild processes, with potentially warm OS/SDK caches.
"""
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

SDK = "10.0.102"
REPEATS = 5

def evaluate(project, tfm=None):
    command = ["dotnet", "msbuild", str(project), "-nologo", "-v:q", "-nr:false",
               "-getProperty:TargetFramework,TargetFrameworks,AssemblyName,DefineConstants",
               "-getItem:Compile,ProjectReference"]
    if tfm:
        command.append(f"-p:TargetFramework={tfm}")
    started = time.perf_counter()
    result = subprocess.run(command, cwd=project.parent, text=True, capture_output=True,
                            timeout=60, check=True, env={**os.environ, "DOTNET_NOLOGO": "1",
                            "DOTNET_CLI_TELEMETRY_OPTOUT": "1"})
    elapsed = time.perf_counter() - started
    data = json.loads(result.stdout)
    if tfm and data["Properties"]["TargetFramework"] != tfm:
        raise RuntimeError("Evaluation did not honor the requested target framework")
    if tfm and len(data["Items"]["Compile"]) != 17:
        raise RuntimeError("Expected 16 local files and one linked file")
    return elapsed, data

def main():
    with tempfile.TemporaryDirectory(prefix="tethys-chlt-evaluation-") as directory:
        root = Path(directory)
        (root / "global.json").write_text(json.dumps({"sdk": {"version": SDK, "rollForward": "disable"}}))
        shared = root / "Shared.cs"
        shared.write_text("public class Shared {}\n")
        projects = []
        for name in ["Core", "Tool"]:
            folder = root / name
            folder.mkdir()
            for index in range(16):
                (folder / f"Type{index}.cs").write_text(f"namespace {name}; public class Type{index} {{}}\n")
            reference = '<ProjectReference Include="../Core/Core.csproj" />' if name == "Tool" else ""
            project = folder / f"{name}.csproj"
            project.write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup>'
                               '<TargetFrameworks>net8.0;net9.0</TargetFrameworks>'
                               '<AssemblyName>Company.Core</AssemblyName>'
                               '</PropertyGroup><ItemGroup><Compile Include="../Shared.cs" Link="Shared.cs" />'
                               + reference + '</ItemGroup></Project>')
            projects.append(project)
        initial_seconds, initial = evaluate(projects[0], "net8.0")
        rows = []
        for project_count, tfms in [(1, ["net8.0"]), (1, ["net8.0", "net9.0"]), (2, ["net8.0", "net9.0"])]:
            rounds = []
            for repeat in range(REPEATS):
                outer = []
                inner = []
                evidence = []
                for project in projects[:project_count]:
                    seconds, _ = evaluate(project)
                    outer.append(seconds)
                    for tfm in tfms:
                        seconds, data = evaluate(project, tfm)
                        inner.append(seconds)
                        evidence.append({"project": project.name, "tfm": tfm,
                                         "compile_items": len(data["Items"]["Compile"]),
                                         "project_references": len(data["Items"]["ProjectReference"]),
                                         "assembly_name": data["Properties"]["AssemblyName"]})
                rounds.append({"outer_seconds": outer, "inner_seconds": inner,
                               "total_seconds": sum(outer) + sum(inner)})
            rows.append({"projects": project_count, "selected_tfms": tfms,
                         "units": project_count * len(tfms),
                         "processes_per_round": project_count * (1 + len(tfms)),
                         "median_total_seconds": statistics.median(r["total_seconds"] for r in rounds),
                         "min_total_seconds": min(r["total_seconds"] for r in rounds),
                         "max_total_seconds": max(r["total_seconds"] for r in rounds),
                         "median_inner_seconds": statistics.median(t for r in rounds for t in r["inner_seconds"]),
                         "rounds": rounds, "evidence": evidence})
        print(json.dumps({"sdk": SDK, "repeats": REPEATS,
                          "first_observed_inner_seconds": initial_seconds,
                          "fixture": "2 projects, 33 physical C# files, 17 Compile items per project, shared AssemblyName; Tool references Core",
                          "scope": "Evaluation only; no restore, compilation, reference target selection, tethys indexing, RSS measurement, or cold-cache guarantee. Single-TFM row selects one TFM from the same multi-target fixture. Serial calls include one outer discovery evaluation per project.",
                          "measurements": rows}, indent=2))

if __name__ == "__main__":
    main()
