#!/usr/bin/env python3
"""Independent native MSBuild evaluation profiler; no companion instrumentation."""
import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("performance_probe", ROOT / ".tethys-82a6/probe-performance.py")
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
environment = probe.controlled_environment(probe.NATIVE_ENV, probe.SDK)
project = next((probe.OUT / "sdk-project").rglob("*.csproj"))
rows = []
for label, selector in (("outer", None), ("net8", "net8.0"), ("net9", "net9.0")):
    profile = probe.OUT / "profile" / f"native-{label}.tsv"
    command = [probe.DOTNET, probe.SDK / "MSBuild.dll", project, "-nologo",
               "-getProperty:TargetFramework,DefineConstants,AssemblyName", "-getItem:Compile",
               f"-profileEvaluation:{profile}", "-p:Configuration=Debug", "-p:Platform=AnyCPU", "-p:VSToolsPath="]
    if selector:
        command.append(f"-p:TargetFramework={selector}")
    result = probe.run(command, environment=environment, cwd=project.parent)
    rows.append({"label": label, "profile": str(profile), "native": result})
    probe.save(probe.OUT / "profile/native-oracle.json", rows)
    assert result["exit"] == 0 and profile.is_file(), result
    print(f"{label}: {result['seconds']:.6f}s; {profile}")
