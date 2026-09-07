#!/usr/bin/env python3
"""Temporary native path differential; remove after S3 Windows diagnosis."""
import json
import os
from pathlib import Path
import runpy
import tempfile


qualification = runpy.run_path(str(Path(__file__).with_name("worker_qualification.py")))
run = qualification["run"]
require = qualification["require"]
require(os.name == "nt", "This diagnostic requires actual Windows")
dotnet, sdk = qualification["selected_sdk"]()
companion = Path(os.environ["TETHYS_WORKER_DISTRIBUTION"]).resolve(strict=True)
framework = qualification["selected_windows"]()

with tempfile.TemporaryDirectory(prefix="tethys native path ") as temporary:
    root = Path(temporary).resolve(strict=True)
    require(root.drive.endswith(":"), "Probe requires a local drive fixture")
    (root / "src").mkdir()
    (root / "src/First.cs").write_text("class First {}\n", encoding="utf-8")
    project = root / "App.csproj"
    for pattern in ("src/First.cs", "src/*.cs"):
        project.write_text(
            '<Project><PropertyGroup><TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier>'
            '<TargetFrameworkVersion>v4.8</TargetFrameworkVersion><TargetFrameworkProfile>Client</TargetFrameworkProfile>'
            '<AssemblyName>Collision</AssemblyName></PropertyGroup><ItemGroup>'
            f'<Compile Include="{pattern}"><Link>Shared.cs</Link></Compile>'
            '<ProjectReference Include="Missing.csproj"><ReferenceOutputAssembly>false</ReferenceOutputAssembly></ProjectReference>'
            '<Reference Include="Example"><HintPath>lib/Example.dll</HintPath></Reference></ItemGroup></Project>',
            encoding="utf-8",
        )
        for kind, selected in (("sdk", sdk), ("framework", framework)):
            direct = [dotnet, selected / "MSBuild.dll"] if kind == "sdk" else [selected / "MSBuild.exe"]
            worker = ([dotnet, companion / "sdk/Tethys.MSBuild.Evaluate.dll"] if kind == "sdk"
                      else [companion / "framework/Tethys.MSBuild.Evaluate.exe"])
            for form, prefix in (("ordinary", ""), ("verbatim", "\\\\?\\")):
                project_path = prefix + str(project)
                request = dict(protocol_version=1, workspace_root=prefix + str(root),
                               project_path=project_path, target_framework=None,
                               global_properties={}, msbuild_path=str(selected), trust_granted=True)
                for engine, command, payload in (
                    ("direct", direct + [project_path, "-nologo", "-verbosity:quiet", "-getItem:Compile"], None),
                    ("worker", worker, json.dumps(request).encode("utf-8")),
                ):
                    status, text, stderr = run(command, root, payload, allow_failure=True)
                    try:
                        result = json.loads(text)
                    except json.JSONDecodeError:
                        result = {}
                    rows = result.get("Items", {}).get("Compile", []) if engine == "direct" else result.get("items", {}).get("Compile", [])
                    names = [row["Identity" if engine == "direct" else "include"].replace("\\", "/") for row in rows]
                    exists = [Path(row["FullPath" if engine == "direct" else "full_path"]).is_file() for row in rows]
                    success = status == 0 and (engine == "direct" or result.get("success") is True)
                    observation = dict(host=kind, pattern=pattern, form=form, engine=engine,
                                       success=success, sources=names, physical_files=exists)
                    if not success:
                        observation["diagnostics"] = result.get("diagnostics", [text[-1000:], stderr[-1000:]])
                    print("[DEBUG-82a6-path] " + json.dumps(observation), flush=True)
                    if form == "ordinary":
                        require(success and names == ["src/First.cs"] and exists == [True],
                                "Ordinary native control must match the authored source manifest")
