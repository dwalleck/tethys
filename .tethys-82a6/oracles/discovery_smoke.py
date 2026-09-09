#!/usr/bin/env python3
"""Exercise the public discovery seam with real MSBuild and native host tracing.

The temporary Rust caller consumes the repository's locked dependency graph. No
mock evaluator successes or forwarding launchers are used. The selected SDK's
native muxer trace independently proves that eligible hits launch no worker.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
ORACLES = Path(__file__).resolve().parent

CALLER = r'''
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tethys::discovery::{
    discover_workspace, DiscoveryCacheObservation, DiscoveryCachePolicy,
    DiscoveryIssue, DiscoveryOptions, DiscoveryRequest, EvaluationContext,
    EvaluationEnvironment, EvaluationUnit, ProjectDiscovery,
};
#[derive(Deserialize)]
struct Input {
    root: PathBuf,
    companion: PathBuf,
    host: Option<PathBuf>,
    trust: bool,
    force: bool,
}
#[derive(Serialize)]
struct Report<'a> {
    context: &'a EvaluationContext,
    projects: &'a [ProjectDiscovery],
    units: &'a [EvaluationUnit],
    issues: &'a [DiscoveryIssue],
    cache_observations: &'a [DiscoveryCacheObservation],
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cache = Vec::new();
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let options = DiscoveryOptions {
            // Diagnostic tracing settings are inherited and reach the evaluator;
            // harness loader settings are dropped by name, not silently exempted.
            environment: EvaluationEnvironment::inherited()
                .without(EvaluationEnvironment::RUNTIME_CODE_EXTENSIONS),
            trust_msbuild: input.trust,
            companion_directory: Some(input.companion),
            msbuild_path: input.host,
            cache_policy: if input.force { DiscoveryCachePolicy::Disabled }
                          else { DiscoveryCachePolicy::Enabled },
            ..DiscoveryOptions::default()
        };
        let request = DiscoveryRequest::new(&input.root, options)?
            .with_cache(std::mem::take(&mut cache));
        let snapshot = discover_workspace(&request)?;
        serde_json::to_writer(&mut output, &Report {
            context: &snapshot.context, projects: &snapshot.projects,
            units: &snapshot.units, issues: &snapshot.issues,
            cache_observations: &snapshot.cache_observations,
        })?;
        writeln!(output)?;
        output.flush()?;
        cache = snapshot.cache;
    }
    Ok(())
}
'''


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def cargo_baseline(binary):
    fixture = json.loads((ORACLES / "cargo_fixture.json").read_text())
    with tempfile.TemporaryDirectory(prefix="tethys-cargo-smoke-") as directory:
        root = Path(directory)
        for name, contents in fixture["files"].items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(contents.encode("utf-8"))
        subprocess.run([binary, "index", "-w", root], check=True, timeout=60)
        actual = subprocess.run(
            ["python3", ROOT / ".idxperf/probe-dump.py", root / ".rivets/index/tethys.db"],
            check=True, capture_output=True, text=True, timeout=30,
        ).stdout
        expected = (ORACLES / "cargo_baseline.txt").read_text()
        require(actual == expected, "C3 frozen Cargo symbols/references/edges/attribution changed")
    print("C3 PASS: pre-adapter canonical Rust output unchanged")


def build_caller(directory):
    package = directory / "caller"
    (package / "src").mkdir(parents=True)
    (package / "Cargo.toml").write_text(
        '[package]\nname = "tethys-discovery-smoke"\nversion = "0.0.0"\nedition = "2024"\n'
        '[workspace]\n[dependencies]\ntethys = { path = ' + json.dumps(str(ROOT)) + ' }\n'
        'serde = { version = "1", features = ["derive"] }\nserde_json = "1"\n'
    )
    shutil.copyfile(ROOT / "Cargo.lock", package / "Cargo.lock")
    (package / "src/main.rs").write_text(CALLER)
    subprocess.run(
        ["cargo", "build", "--offline", "--manifest-path", package / "Cargo.toml",
         "--target-dir", ROOT / "target"], cwd=ROOT, check=True, timeout=600,
    )
    return ROOT / "target/debug" / ("tethys-discovery-smoke.exe" if os.name == "nt" else "tethys-discovery-smoke")


def semantic(report):
    return {name: value for name, value in report.items() if name != "cache_observations"}


def worker_launches(trace):
    # hostpolicy.cpp run_app_for_context logs this immediately before
    # execute_assembly. DLL mentions in arguments/dependencies are not launches;
    # dotnet --version may launch the SDK's dotnet.dll, not our evaluator.
    return sum(line.startswith("Launch host: ") and
               "Tethys.MSBuild.Evaluate.dll, argc: " in line
               for line in trace.splitlines())


def confirmed(report, sources, define):
    require(not report["issues"], f"C7 unexpected candidate issue: {report}")
    require(len(report["projects"]) == 1 and report["projects"][0]["standing"] == {"standing": "confirmed"},
            f"C7 expected one confirmed project: {report}")
    require(len(report["units"]) == 1, f"C7 expected exactly one unit: {report}")
    unit = report["units"][0]
    require(unit["standing"] == {"standing": "confirmed"}, f"C7 expected confirmed unit: {unit}")
    require(unit["framework"]["identifier"] == ".NETCoreApp" and
            unit["framework"]["version"] == "v8.0", "C7 native framework identity changed")
    require(sorted(item["path"].replace("\\", "/") for item in unit["sources"]) == sources,
            f"C9 native Compile membership differs: {unit['sources']}")
    require(unit["properties"]["DefineConstants"] == define, "C9 effective define was stale")


def discovery(companion, host):
    with tempfile.TemporaryDirectory(prefix="tethys-discovery-smoke-") as directory:
        temporary = Path(directory)
        binary = build_caller(temporary)
        root = temporary / "workspace"
        root.mkdir()
        (root / "global.json").write_text(json.dumps({"sdk": {"version": host.name, "rollForward": "disable"}}))
        project = root / "Literal.csproj"
        xml = ('<Project><PropertyGroup><TargetFramework>net8.0</TargetFramework>'
               '<TargetFrameworkIdentifier>.NETCoreApp</TargetFrameworkIdentifier>'
               '<TargetFrameworkVersion>v8.0</TargetFrameworkVersion>'
               '<AssemblyName>Literal</AssemblyName><DefineConstants>BEFORE</DefineConstants>'
               '</PropertyGroup><ItemGroup><Compile Include="*.cs" /></ItemGroup></Project>')
        project.write_text(xml)
        (root / "One.cs").write_text("public class One {}\n")
        # Outside the workspace: diagnostic writes must not mutate recipe inputs.
        # Set both names for pre-.NET 10 and .NET 10+ native hosts, including a
        # newer muxer loading an older runtime's hostpolicy.
        # The caller states the environment it evaluates under, so this harness
        # passes its own through unfiltered.
        log = temporary / "host.trace"
        environment = dict(os.environ)
        environment.update(
            COREHOST_TRACE="1", COREHOST_TRACE_VERBOSITY="4",
            COREHOST_TRACEFILE=str(log), DOTNET_HOST_TRACE="1",
            DOTNET_HOST_TRACE_VERBOSITY="4", DOTNET_HOST_TRACEFILE=str(log))
        with tempfile.TemporaryFile(mode="w+") as errors, ThreadPoolExecutor(max_workers=1) as reader:
            process = subprocess.Popen([binary], cwd=temporary, env=environment, text=True,
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors)
            try:
                def invoke(trust=True, force=False):
                    # Native trace.cpp appends. Delete between completed requests
                    # so neither append nor overwrite implies a cumulative delta.
                    # Keep the path/environment constant to preserve cache inputs.
                    log.unlink(missing_ok=True)
                    process.stdin.write(json.dumps(dict(root=str(root), companion=str(companion),
                                                        host=str(host), trust=trust, force=force)) + "\n")
                    process.stdin.flush()
                    text = reader.submit(process.stdout.readline).result(timeout=180)
                    if not text:
                        errors.seek(0)
                        raise RuntimeError("Public discovery caller failed: " + errors.read())
                    trace = log.read_text() if log.exists() else ""
                    count = worker_launches(trace)
                    if count == 0:
                        require("Tethys.MSBuild.Evaluate.dll" not in trace,
                                "Native trace shows an attempted worker startup without a launch marker")
                    return json.loads(text), count

                untrusted, count = invoke(trust=False)
                require(count == 0 and not log.exists(), "C6 discovery launched a process without trust")
                require(not untrusted["units"] and len(untrusted["projects"]) == 1 and
                        untrusted["projects"][0]["standing"]["failure"]["reason"] == "trust-required",
                        f"C6 trust failure was hidden: {untrusted}")
                fresh, count = invoke()
                confirmed(fresh, ["One.cs"], "BEFORE")
                require(count > 0, "C9 fresh positive control never launched the real evaluator")
                cached, count = invoke()
                require(count == 0 and semantic(cached) == semantic(fresh),
                        "C9 eligible hit launched an evaluator or changed semantic records")
                (root / "Two.cs").write_text("public class Two {}\n")
                changed, count = invoke()
                confirmed(changed, ["One.cs", "Two.cs"], "BEFORE")
                require(count > 0, "C9 added glob input incorrectly reused old evaluation")
                project.write_text(xml.replace("BEFORE", "AFTER"))
                changed, count = invoke()
                confirmed(changed, ["One.cs", "Two.cs"], "AFTER")
                require(count > 0, "C9 changed project incorrectly reused old evaluation")
                cached, count = invoke()
                require(count == 0, "C9 unchanged qualified request missed its cache")
                forced, count = invoke(force=True)
                require(count > 0 and semantic(forced) == semantic(cached),
                        "C9 forced native evaluation differs from cached semantic records")
                process.stdin.close()
                require(process.wait(timeout=10) == 0, "Discovery caller failed during shutdown")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=10)
                process.stdout.close()
        print("C6/C7/C9 PASS: real public seam, no unauthorized process, native metadata, "
              "zero-evaluation hit, glob/project mutations, cached/forced equivalence")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/tethys")
    parser.add_argument("--companion", type=Path,
                        default=Path(os.environ.get("TETHYS_WORKER_DISTRIBUTION", ROOT / "target/worker-dist/msbuild-evaluate")))
    parser.add_argument("--host", type=Path, default=os.environ.get("TETHYS_SDK_MSBUILD_PATH"))
    parser.add_argument("--cargo-only", action="store_true")
    args = parser.parse_args()
    cargo_baseline(args.binary.resolve(strict=True))
    if not args.cargo_only:
        require(args.host is not None, "Set TETHYS_SDK_MSBUILD_PATH to the selected installed SDK directory")
        discovery(args.companion.resolve(strict=True), args.host.resolve(strict=True))


if __name__ == "__main__":
    main()
