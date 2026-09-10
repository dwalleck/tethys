#!/usr/bin/env python3
"""Profile an owned instrumented copy; never rewrite the shipping evaluator."""
import hashlib
import importlib.util
import json
from pathlib import Path
import statistics

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("performance_probe", ROOT / ".tethys-82a6/probe-performance.py")
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
output = probe.OUT / "profile"
output.mkdir(exist_ok=True)
source = (ROOT / "tools/tethys-msbuild-evaluate/Evaluation.cs").read_text()
original_hash = hashlib.sha256(source.encode()).hexdigest()
marks = {
    "        var logger = new EvaluationLogger(response.diagnostics);": """        var logger = new EvaluationLogger(response.diagnostics);
        var phase = System.Diagnostics.Stopwatch.StartNew();
        void Mark(string name)
        {
            Console.Error.WriteLine("PHASE:" + name + ":" + phase.Elapsed.TotalSeconds.ToString("R", System.Globalization.CultureInfo.InvariantCulture));
            phase.Restart();
        }""",
    "            var project = collection.LoadProject(request.project_path);": """            Mark("setup");
            var project = collection.LoadProject(request.project_path);
            Mark("load");""",
    "            foreach (var kind in response.items.Keys)": """            Mark("properties");
            foreach (var kind in response.items.Keys)""",
    "            var imports = new HashSet<string>(Path.DirectorySeparatorChar == '\\\\' ? StringComparer.OrdinalIgnoreCase : StringComparer.Ordinal);": """            Mark("items");
            var imports = new HashSet<string>(Path.DirectorySeparatorChar == '\\\\' ? StringComparer.OrdinalIgnoreCase : StringComparer.Ordinal);""",
    "            foreach (var element in project.GetLogicalProject())": """            Mark("imports");
            foreach (var element in project.GetLogicalProject())""",
    "            QualifyLiteralRecipe(project, response);": """            Mark("logical");
            QualifyLiteralRecipe(project, response);
            Mark("recipe");""",
    "        return response;": """        Mark("final");
        return response;""",
}
for before, after in marks.items():
    assert source.count(before) == 1, before
    source = source.replace(before, after)
copy = output / "Evaluation.cs"
copy.write_text(source)
environment = probe.controlled_environment(probe.NATIVE_ENV, probe.SDK)
build = probe.run([probe.DOTNET, "build", ROOT / ".tethys-82a6/probe-performance.csproj", "--no-restore", "-c", "Release",
                   "-o", output / "bin", f"-p:BaseIntermediateOutputPath={probe.OUT}/obj/",
                   f"-p:ProbeSdk={probe.SDK}", f"-p:ProbeWorker={probe.WORKER.parent}", f"-p:ProbeEvaluation={copy}"],
                  environment=environment, cwd=ROOT)
probe.save(output / "build.json", build)
assert build["exit"] == 0, build
project = next((probe.OUT / "sdk-project").rglob("*.csproj"))
rows = []
for index, selector in enumerate((None, "net8.0", "net9.0") * 3):
    request = probe.request(project, selector)
    expected = probe.shipping(request, environment)
    request_file, result_file = output / f"request-{index}.json", output / f"result-{index}.json"
    probe.save(request_file, [request])
    run = probe.run([probe.DOTNET, output / "bin/probe-performance.dll", request_file, result_file],
                    environment=environment, cwd=project.parent)
    native = json.loads(result_file.read_text())
    actual = json.loads(native["calls"][0]["response_json"])
    phases = {line.split(":")[1]: float(line.split(":")[2]) for line in run["stderr"].splitlines() if line.startswith("PHASE:")}
    equal = probe.normalized(actual) == probe.normalized(expected["response"])
    rows.append({"selector": selector, "phases": phases, "profile": run, "shipping": expected,
                 "metadata_equal": equal, "native_evaluation_seconds": native["calls"][0]["evaluation_seconds"]})
    assert run["exit"] == 0 and expected["exit"] == 0 and actual["success"] and equal
summary = {}
for selector in (None, "net8.0", "net9.0"):
    selected = [row for row in rows if row["selector"] == selector]
    summary[str(selector)] = {phase: statistics.median(row["phases"][phase] for row in selected) for phase in selected[0]["phases"]}
probe.save(output / "report.json", {"scope": "fresh process phase timing only; all production operations retained", "summary": summary,
                                     "source_sha256": original_hash, "instrumented_sha256": hashlib.sha256(source.encode()).hexdigest(),
                                     "rows": rows, "native_oracle": str(probe.OUT / "timings.json")})
print(json.dumps(summary, indent=2))
