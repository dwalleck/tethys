#!/usr/bin/env python3
"""Direct MSBuild CLI oracle for the fresh-process concurrency observations."""
import importlib.util
import json
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("performance_probe", ROOT / ".tethys-82a6/probe-performance.py")
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
environment = probe.controlled_environment(probe.NATIVE_ENV, probe.SDK)
projects = sorted((probe.OUT / "parallel-projects").rglob("*.csproj"))
assert len(projects) == 32

def native(project):
    result = probe.run([probe.DOTNET, probe.SDK / "MSBuild.dll", project, "-nologo",
                        "-getProperty:TargetFramework,DefineConstants,AssemblyName", "-getItem:Compile",
                        "-p:TargetFramework=net8.0", "-p:Configuration=Debug", "-p:Platform=AnyCPU", "-p:VSToolsPath="],
                       environment=environment, cwd=project.parent)
    assert result["exit"] == 0, result
    result["response"] = json.loads(result.pop("stdout"))
    return str(project), result

with ThreadPoolExecutor(max_workers=4) as pool:
    expected = dict(pool.map(native, projects))
observed = json.loads((probe.OUT / "parallel.json").read_text())
comparisons = []
for batch in observed["batches"]:
    for row in batch["results"]:
        response = row["response"]
        native_response = expected[response["project_path"]]["response"]
        assert all(response["properties"][key] == value for key, value in native_response["Properties"].items())
        assert sorted(item["full_path"] for item in response["items"]["Compile"]) == sorted(item["FullPath"] for item in native_response["Items"]["Compile"])
    comparisons.append({"summary": batch["summary"], "all_32_native_property_and_compile_sets_equal": True})
probe.save(probe.OUT / "parallel-oracle.json", {"scope": "independent MSBuild CLI, not companion or linked evaluator", "native": expected, "comparisons": comparisons})
print(json.dumps({"native_projects": len(expected), "concurrency_batches": len(comparisons), "all_equal": True}))
