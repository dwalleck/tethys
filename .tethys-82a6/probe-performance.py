#!/usr/bin/env python3
"""Throwaway empirical probe; no production implementation or qualification verdict."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / ".tethys-82a6/oracles"))
from qualification_environment import controlled_environment, verify_environment
from qualification_fixtures import generate, prepare_generated

OUT = ROOT / "target/qualification/performance-premises"
SDK = Path("/home/dwalleck/.local-dotnet/sdk/10.0.102")
DOTNET = SDK.parents[1] / "dotnet"
WORKER = ROOT / "target/worker-dist/msbuild-evaluate/sdk/Tethys.MSBuild.Evaluate.dll"
PROBE = OUT / "bin/probe-performance.dll"
NATIVE_ENV = ROOT / "target/qualification/fixture-environment"


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def run(command, *, environment, payload=None, cwd=None):
    started = time.monotonic()
    result = subprocess.run([str(x) for x in command], input=payload, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            cwd=cwd, env=environment, timeout=30, check=False)
    return {"command": [str(x) for x in command], "seconds": time.monotonic() - started,
            "exit": result.returncode, "stdout": result.stdout, "stderr": result.stderr}


def request(project, target=None, properties=None):
    return {"protocol_version": 1, "workspace_root": str(project.parent),
            "project_path": str(project), "target_framework": target,
            "global_properties": properties or {"Configuration": "Debug", "Platform": "AnyCPU", "VSToolsPath": ""},
            "msbuild_path": str(SDK), "trust_granted": True}


def normalized(value):
    if isinstance(value, dict):
        return {key: normalized(item) for key, item in value.items()
                if key not in {"ModifiedTime", "CreatedTime", "AccessedTime"}}
    if isinstance(value, list):
        return [normalized(item) for item in value]
    return value


def shipping(req, environment):
    observation = run([DOTNET, WORKER], environment=environment,
                      payload=json.dumps(req), cwd=req["workspace_root"])
    observation["response"] = json.loads(observation.pop("stdout"))
    return observation


def linked(requests, label, environment):
    source, result = OUT / f"{label}.requests.json", OUT / f"{label}.result.json"
    save(source, requests)
    observation = run([DOTNET, PROBE, source, result], environment=environment,
                      cwd=requests[0]["workspace_root"])
    observation["native"] = json.loads(result.read_text())
    for row in observation["native"]["calls"]:
        row["response"] = json.loads(row.pop("response_json"))
    return observation


def prepare(environment):
    fixture = OUT / "sdk-project"
    if fixture.exists():
        raise RuntimeError("Owned SDK fixture already exists; preparation is not an overwrite")
    before = verify_environment(NATIVE_ENV)
    source = generate(fixture, {"files": 32, "projects": 1, "units": 2}, ["net8.0", "net9.0"])
    native = prepare_generated(fixture, sdk=SDK, environment=environment, timeout=60)
    assert verify_environment(NATIVE_ENV) == before
    save(OUT / "preparation.json", {"source_sha256": source, "native_restore": native,
                                    "environment": before})
    print(json.dumps({"prepared": str(fixture), "restore_seconds": native["wall_seconds"]}))


def measure(environment):
    project = OUT / "sdk-project/Project0000/Project0000.csproj"
    if not project.is_file():
        candidates = list((OUT / "sdk-project").rglob("*.csproj"))
        assert len(candidates) == 1
        project = candidates[0]
    requests = [request(project, selector) for selector in (None, None, "net8.0", "net8.0", "net9.0", "net9.0")]
    fresh = [shipping(req, environment) for req in requests]
    fresh_probe = [linked([req], f"fresh-{index}", environment) for index, req in enumerate(requests)]
    repeated = linked(requests, "repeated", environment)
    comparisons = []
    for index, row in enumerate(fresh):
        expected = normalized(row["response"])
        first = normalized(fresh_probe[index]["native"]["calls"][0]["response"])
        second = normalized(repeated["native"]["calls"][index]["response"])
        comparisons.append({"index": index, "shipping_success": row["exit"] == 0 and row["response"]["success"],
                            "fresh_linked_equal": first == expected, "repeated_equal": second == expected})
    query = run([DOTNET, SDK / "MSBuild.dll", project, "-nologo", "-getProperty:TargetFramework,DefineConstants,AssemblyName",
                 "-getItem:Compile", "-p:TargetFramework=net8.0", "-p:Configuration=Debug", "-p:Platform=AnyCPU", "-p:VSToolsPath="],
                environment=environment, cwd=project.parent)
    direct = json.loads(query["stdout"])
    worker = fresh[2]["response"]
    oracle_equal = (query["exit"] == 0 and all(worker["properties"][key] == value for key, value in direct["Properties"].items())
                    and sorted(item["full_path"] for item in worker["items"]["Compile"]) == sorted(item["FullPath"] for item in direct["Items"]["Compile"]))
    totals = {"shipping_fresh_seconds": sum(row["seconds"] for row in fresh),
              "linked_fresh_seconds": sum(row["seconds"] for row in fresh_probe),
              "linked_repeated_seconds": repeated["seconds"],
              "repeated_evaluation_seconds": [row["evaluation_seconds"] for row in repeated["native"]["calls"]],
              "direct_native_equal": oracle_equal, "comparisons": comparisons}
    save(OUT / "timings.json", {"scope": "six identical ordered requests; diagnostic only, not full-scale acceptance", "totals": totals,
                               "shipping": fresh, "linked_fresh": fresh_probe, "linked_repeated": repeated, "native_oracle": query,
                               "source_sha256": {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in
                                                 ("tools/tethys-msbuild-evaluate/Evaluation.cs", "tools/tethys-msbuild-evaluate/Contract.cs", ".tethys-82a6/probe-performance.cs")}})
    print(json.dumps(totals, indent=2))
    assert oracle_equal and all(all(value for key, value in row.items() if key != "index") for row in comparisons)


def leak(environment):
    root = OUT / "isolation"
    root.mkdir(exist_ok=True)
    common = "<TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier><TargetFrameworkVersion>v4.7.2</TargetFrameworkVersion>"
    mutator, observer = root / "Mutator.csproj", root / "Observer.csproj"
    mutator.write_text('<Project><PropertyGroup>' + common + "<_Set>$([System.Environment]::SetEnvironmentVariable('TETHYS_PERFORMANCE_LEAK', 'leaked'))</_Set></PropertyGroup></Project>")
    observer.write_text('<Project><PropertyGroup>' + common + "<DefineConstants>$([System.Environment]::GetEnvironmentVariable('TETHYS_PERFORMANCE_LEAK'))</DefineConstants></PropertyGroup></Project>")
    context = {**environment, "MSBUILDENABLEALLPROPERTYFUNCTIONS": "1"}
    context.pop("TETHYS_PERFORMANCE_LEAK", None)
    requests = [request(mutator), request(observer)]
    fresh = [shipping(req, context) for req in requests]
    reused = linked(requests, "isolation", context)
    native = run([DOTNET, SDK / "MSBuild.dll", observer, "-nologo",
                  "-getProperty:DefineConstants,TargetFrameworkIdentifier"],
                 environment=context, cwd=root)
    oracle = json.loads(native["stdout"])
    values = {"fresh": fresh[1]["response"]["properties"]["DefineConstants"],
              "repeated_collection": reused["native"]["calls"][1]["response"]["properties"]["DefineConstants"],
              "native_fresh": oracle["Properties"]["DefineConstants"]}
    save(OUT / "isolation.json", {"values": values, "shipping": fresh, "linked": reused, "native_oracle": native})
    print(json.dumps(values))
    assert all(row["exit"] == 0 and row["response"]["success"] for row in fresh)
    assert reused["exit"] == 0
    assert values == {"fresh": "", "repeated_collection": "leaked", "native_fresh": ""}


def race(environment):
    root = OUT / "input-race"
    root.mkdir(exist_ok=True)
    imported, marker, project = root / "Observed.props", root / "consumed.marker", root / "App.csproj"
    before = '<Project><PropertyGroup><DefineConstants>BEFORE</DefineConstants><TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier><TargetFrameworkVersion>v4.7.2</TargetFrameworkVersion></PropertyGroup></Project>'
    after = before.replace("BEFORE", "AFTER!")
    imported.write_text(before)
    marker.unlink(missing_ok=True)
    project.write_text('<Project><Import Project="Observed.props"/><PropertyGroup>'
                       + f"<_Signal>$([System.IO.File]::WriteAllText('{marker}', 'consumed'))</_Signal>"
                       + "<_Wait>$([System.Threading.Thread]::Sleep(800))</_Wait></PropertyGroup></Project>")
    context = {**environment, "MSBUILDENABLEALLPROPERTYFUNCTIONS": "1"}
    child = subprocess.Popen([str(DOTNET), str(WORKER)], stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             text=True, cwd=root, env=context)
    try:
        child.stdin.write(json.dumps(request(project)))
        child.stdin.close()
        deadline = time.monotonic() + 10
        while not marker.exists():
            if child.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError("Native import-consumption marker did not arrive")
            time.sleep(0.005)
        stamp = imported.stat()
        imported.write_text(after)
        os.utime(imported, ns=(stamp.st_atime_ns, stamp.st_mtime_ns))
        child.wait(timeout=10)
        stdout, stderr = child.stdout.read(), child.stderr.read()
    finally:
        if child.poll() is None:
            child.kill()
            child.wait()
    response = json.loads(stdout)
    native = run([DOTNET, SDK / "MSBuild.dll", project, "-nologo",
                  "-getProperty:DefineConstants,TargetFrameworkIdentifier"],
                 environment=context, cwd=root)
    oracle = json.loads(native["stdout"])
    values = {"first_metadata": response["properties"]["DefineConstants"],
              "reevaluated_metadata": oracle["Properties"]["DefineConstants"],
              "size_unchanged": imported.stat().st_size == stamp.st_size,
              "mtime_unchanged": imported.stat().st_mtime_ns == stamp.st_mtime_ns,
              "returned_import_path": str(imported) in response["imports"],
              "before_sha256": hashlib.sha256(before.encode()).hexdigest(),
              "after_sha256": hashlib.sha256(after.encode()).hexdigest()}
    save(OUT / "input-race.json", {"values": values, "response": response, "stderr": stderr, "native_oracle": native})
    print(json.dumps(values, indent=2))
    assert child.returncode == 0 and response["success"] and native["exit"] == 0
    assert values["first_metadata"] == "BEFORE" and values["reevaluated_metadata"] == "AFTER!"
    assert values["size_unchanged"] and values["mtime_unchanged"] and values["returned_import_path"]


def prepare_parallel(environment):
    fixture = OUT / "parallel-projects"
    if fixture.exists():
        raise RuntimeError("Parallel fixture already exists")
    before = verify_environment(NATIVE_ENV)
    source = generate(fixture, {"files": 1024, "projects": 32, "units": 54}, ["net8.0", "net9.0"])
    native = prepare_generated(fixture, sdk=SDK, environment=environment, timeout=60)
    assert verify_environment(NATIVE_ENV) == before
    save(OUT / "parallel-preparation.json", {"source_sha256": source, "native_restore": native, "environment": before})
    print(json.dumps({"prepared": str(fixture), "restore_seconds": native["wall_seconds"]}))


def parallel(environment):
    from concurrent.futures import ThreadPoolExecutor
    import threading
    import psutil
    projects = sorted((OUT / "parallel-projects").rglob("*.csproj"))
    assert len(projects) == 32
    requests = [request(project, "net8.0") for project in projects]
    batches, reference = [], None
    for workers in (1, 4, 8, 16):
        stop = threading.Event()
        peak = [0]
        parent = psutil.Process()
        def sample():
            while not stop.is_set():
                total = 0
                for process in [parent, *parent.children(recursive=True)]:
                    try:
                        total += process.memory_info().rss
                    except psutil.NoSuchProcess:
                        pass
                peak[0] = max(peak[0], total)
                stop.wait(0.01)
        observer = threading.Thread(target=sample)
        observer.start()
        started = time.monotonic()
        try:
            with ThreadPoolExecutor(max_workers=workers) as pool:
                results = list(pool.map(lambda req: shipping(req, environment), requests))
        finally:
            elapsed = time.monotonic() - started
            stop.set()
            observer.join()
        values = [normalized(row["response"]) for row in results]
        if reference is None:
            reference = values
        summary = {"workers": workers, "wall_seconds": elapsed,
                   "sampled_runner_tree_peak_rss_bytes": peak[0],
                   "rss_scope": "10ms sampled parent plus descendants; not exact high-water",
                   "metadata_equal_to_serial": values == reference,
                   "all_success": all(row["exit"] == 0 and row["response"]["success"] for row in results)}
        batches.append({"summary": summary, "results": results})
        save(OUT / "parallel.json", {"scope": "32 distinct prepared projects, fresh shipping process per request; diagnostic not qualification", "batches": batches})
        print(json.dumps(summary), flush=True)
        assert summary["all_success"] and summary["metadata_equal_to_serial"]


def prewarm(environment):
    from collections import deque
    reference = json.loads((OUT / "parallel.json").read_text())["batches"][0]
    requests = [request(Path(row["response"]["project_path"]), "net8.0") for row in reference["results"]]
    summaries = []
    for width in (2, 4, 8):
        pending, children, results = deque(), [], []
        def spawn(index):
            source = OUT / f"park-{width}-{index}.requests.json"
            result = OUT / f"park-{width}-{index}.result.json"
            save(source, [requests[index]])
            child = subprocess.Popen([str(DOTNET), str(PROBE), str(source), str(result), "park"],
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     text=True, cwd=requests[index]["workspace_root"], env=environment)
            children.append(child)
            pending.append((index, child, result))
        started = time.monotonic()
        try:
            for index in range(min(width, len(requests))):
                spawn(index)
            next_index = min(width, len(requests))
            while pending:
                index, child, path = pending.popleft()
                assert child.stdout.readline().strip() == "READY"
                child.stdin.write("GO\n")
                child.stdin.close()
                child.wait(timeout=30)
                assert child.returncode == 0, child.stderr.read()
                result = json.loads(path.read_text())
                response = json.loads(result["calls"][0]["response_json"])
                assert normalized(response) == normalized(reference["results"][index]["response"])
                results.append(result["calls"][0]["evaluation_seconds"])
                if next_index < len(requests):
                    spawn(next_index)
                    next_index += 1
        finally:
            for child in children:
                if child.poll() is None:
                    child.kill()
                    child.wait()
        summary = {"maximum_live_workers": width, "wall_seconds": time.monotonic() - started,
                   "actual_evaluations_serial": True, "one_project_per_process": True,
                   "metadata_equal_to_shipping_serial": True, "evaluation_seconds": results}
        summaries.append(summary)
        save(OUT / "prewarm.json", {"scope": "core-only startup overlap; fresh one-shot processes, original evaluation order", "batches": summaries})
        print(json.dumps(summary), flush=True)


def ordering(environment):
    from concurrent.futures import ThreadPoolExecutor
    root = OUT / "ordering"
    root.mkdir(exist_ok=True)
    gate, shared = root / "gate.fifo", root / "shared.txt"
    gate.unlink(missing_ok=True)
    os.mkfifo(gate)
    writer, reader = root / "Writer.csproj", root / "Reader.csproj"
    identity = "<TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier><TargetFrameworkVersion>v4.7.2</TargetFrameworkVersion>"
    writer.write_text("<Project><PropertyGroup>" + identity
                      + f"<_Gate>$([System.IO.File]::ReadAllText('{gate}'))</_Gate>"
                      + f"<_Write>$([System.IO.File]::WriteAllText('{shared}', 'AFTER'))</_Write>"
                      + "</PropertyGroup></Project>")
    reader.write_text("<Project><PropertyGroup>" + identity
                      + f"<DefineConstants>$([System.IO.File]::ReadAllText('{shared}'))</DefineConstants>"
                      + "</PropertyGroup></Project>")
    context = {**environment, "MSBUILDENABLEALLPROPERTYFUNCTIONS": "1"}
    def release():
        with gate.open("w") as stream:
            stream.write("go")
    shared.write_text("BEFORE")
    with ThreadPoolExecutor(max_workers=1) as pool:
        released = pool.submit(release)
        serial_writer = shipping(request(writer), context)
        released.result()
    serial_reader = shipping(request(reader), context)
    shared.write_text("BEFORE")
    with ThreadPoolExecutor(max_workers=1) as pool:
        pending_writer = pool.submit(shipping, request(writer), context)
        concurrent_reader = shipping(request(reader), context)
        release()
        concurrent_writer = pending_writer.result()
    native = run([DOTNET, SDK / "MSBuild.dll", reader, "-nologo",
                  "-getProperty:DefineConstants,TargetFrameworkIdentifier"],
                 environment=context, cwd=root)
    values = {"serial_reader": serial_reader["response"]["properties"]["DefineConstants"],
              "concurrent_reader": concurrent_reader["response"]["properties"]["DefineConstants"],
              "native_reader_after_writer": json.loads(native["stdout"])["Properties"]["DefineConstants"]}
    save(OUT / "ordering.json", {"scope": "Linux FIFO controls actual property-function order; fresh processes throughout",
                               "values": values, "serial": [serial_writer, serial_reader],
                               "concurrent": [concurrent_writer, concurrent_reader], "native_oracle": native})
    print(json.dumps(values))
    assert all(row["exit"] == 0 and row["response"]["success"] for row in
               [serial_writer, serial_reader, concurrent_writer, concurrent_reader])
    assert values == {"serial_reader": "AFTER", "concurrent_reader": "BEFORE", "native_reader_after_writer": "AFTER"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("phase", choices=["prepare", "measure", "leak", "race", "prepare-parallel", "parallel", "prewarm", "ordering"])
    args = parser.parse_args()
    environment = controlled_environment(NATIVE_ENV, SDK)
    {"prepare": prepare, "measure": measure, "leak": leak, "race": race,
     "prepare-parallel": prepare_parallel, "parallel": parallel, "prewarm": prewarm,
     "ordering": ordering}[args.phase](environment)
