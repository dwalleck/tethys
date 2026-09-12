"""THROWAWAY: inspect non-SDK .NET Framework metadata using the available host.
This proves a generated classic project shape, NOT Windows/Mono compatibility.
No restore, build targets, packages, or external repository code are executed.
"""
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

with tempfile.TemporaryDirectory(prefix="tethys-chlt-classic-") as directory:
    root = Path(directory)
    (root / "global.json").write_text(json.dumps({"sdk": {"version": "10.0.102", "rollForward": "disable"}}))
    (root / "Shared.cs").write_text("public class Shared {}\n")
    results = []
    for framework, profile in [("v4.0", "Client"), ("v4.7.2", ""), ("v4.8", "")]:
        folder = root / (framework + profile)
        folder.mkdir()
        (folder / "Own.cs").write_text("public class Own {}\n")
        (folder / "Imported.cs").write_text("public class Imported {}\n")
        (folder / "Inputs.props").write_text('<Project xmlns="http://schemas.microsoft.com/developer/msbuild/2003"><ItemGroup><Compile Include="Imported.cs" /></ItemGroup></Project>')
        project = folder / "Classic.csproj"
        project.write_text('<Project ToolsVersion="4.0" DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">'
                           '<PropertyGroup><OutputType>Library</OutputType><AssemblyName>Classic</AssemblyName>'
                           f'<TargetFrameworkVersion>{framework}</TargetFrameworkVersion><TargetFrameworkProfile>{profile}</TargetFrameworkProfile>'
                           '</PropertyGroup><Import Project="Inputs.props" />'
                           '<ItemGroup><Compile Include="Own.cs" /><Compile Include="../Shared.cs"><Link>Shared.cs</Link></Compile></ItemGroup>'
                           '<Import Project="$(MSBuildToolsPath)/Microsoft.CSharp.targets" /></Project>')
        command = ["dotnet", "msbuild", str(project), "-nologo", "-v:q", "-nr:false",
                   "-getProperty:TargetFramework,TargetFrameworks,TargetFrameworkIdentifier,TargetFrameworkVersion,TargetFrameworkProfile,TargetFrameworkMoniker,AssemblyName",
                   "-getItem:Compile"]
        timings = []
        for _ in range(5):
            start = time.perf_counter()
            process = subprocess.run(command, cwd=folder, capture_output=True, text=True, timeout=60, check=True,
                                     env={**os.environ, "DOTNET_NOLOGO": "1", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"})
            timings.append(time.perf_counter() - start)
            data = json.loads(process.stdout)
            if len(data["Items"]["Compile"]) != 3:
                raise RuntimeError("Expected explicit, imported, and linked Compile inputs")
            if data["Properties"]["TargetFrameworkIdentifier"] != ".NETFramework":
                raise RuntimeError("Expected evaluated .NET Framework identity")
            if data["Properties"]["TargetFrameworkVersion"] != framework or data["Properties"]["TargetFrameworkProfile"] != profile:
                raise RuntimeError("Framework version/profile did not round trip")
        results.append({"properties": data["Properties"], "compile_items": [{"identity": i["Identity"], "link": i.get("Link")} for i in data["Items"]["Compile"]],
                        "median_seconds": statistics.median(timings), "samples_seconds": timings})
    print(json.dumps({"host": "dotnet SDK 10.0.102 / MSBuild 18.0.7 on Linux", "results": results,
                      "limits": "Generated non-SDK projects only. No Windows/Mono host, packages.config restore, proprietary imports, framework reference-assembly resolution, build, or tethys indexing. Success is metadata evaluation, not day-one legacy qualification."}, indent=2))
