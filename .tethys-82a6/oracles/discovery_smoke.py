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
import sqlite3
import subprocess
import tempfile
import time

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


def measured_cli(binary, root, arguments, environment, expected, timeout):
    """Sample DB/sidecars and optional tree RSS; keep wait4 accounting separate."""
    started = time.monotonic()
    command = [str(binary), "-w", str(root), *map(str, arguments)]
    db = root / ".rivets/index/tethys.db"
    database_paths = (db, db.with_name("tethys.db-wal"), db.with_name("tethys.db-shm"))

    def database_bytes():
        total = 0
        for path in database_paths:
            try:
                total += path.stat().st_size
            except FileNotFoundError:
                pass  # A sidecar can disappear during checkpoint/close.
        return total

    usage = None
    try:
        import psutil
    except ImportError:
        psutil = None
    tree_peak = None
    tree_status = "unavailable: optional psutil is not installed"
    database_peak = database_bytes()
    known_descendants = {}
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        child = subprocess.Popen(command, cwd=root, env=environment, stdout=stdout, stderr=stderr)
        tree = None
        try:
            if psutil:
                try:
                    tree = psutil.Process(child.pid)
                except psutil.NoSuchProcess:
                    tree_status = "unavailable: CLI exited before the first process-tree sample"
            while True:
                database_peak = max(database_peak, database_bytes())
                if tree:
                    try:
                        descendants = tree.children(recursive=True)
                        known_descendants.update((process.pid, process) for process in descendants)
                        rss = sum(process.memory_info().rss for process in [tree, *descendants])
                        tree_peak = max(tree_peak or 0, rss)
                        tree_status = "measured: sampled process-tree sum, 10ms interval (not an exact high-water mark)"
                    except psutil.NoSuchProcess:
                        pass  # Normal sampling race with an exiting native child.
                    except psutil.AccessDenied:
                        tree_status = "unavailable: process-tree access denied"
                        tree = None
                if hasattr(os, "wait4"):
                    pid, status, usage = os.wait4(child.pid, os.WNOHANG)
                    if pid:
                        child.returncode = os.waitstatus_to_exitcode(status)
                        break
                elif child.poll() is not None:
                    break
                require(time.monotonic() - started < timeout, f"CLI timeout: {command}")
                time.sleep(0.01)
        finally:
            if child.returncode is None:
                try:
                    if psutil:
                        try:
                            parent = psutil.Process(child.pid)
                            parent.suspend()
                            known_descendants.update((process.pid, process)
                                                     for process in parent.children(recursive=True))
                        except psutil.NoSuchProcess:
                            pass
                        # Also retain previously sampled descendants that became
                        # reparented before timeout; psutil guards PID reuse.
                        for process in reversed(list(known_descendants.values())):
                            try:
                                process.kill()
                            except psutil.NoSuchProcess:
                                pass
                        child.kill()
                        child.wait()
                        _, alive = psutil.wait_procs(list(known_descendants.values()), timeout=10)
                        require(not alive, f"CLI timeout cleanup: descendants still alive: {[p.pid for p in alive]}")
                finally:
                    if child.returncode is None:
                        child.kill()
                        child.wait()
        stdout.seek(0)
        stderr.seek(0)
        output, errors = stdout.read().decode(), stderr.read().decode()
    final_database_bytes = database_bytes()
    database_peak = max(database_peak, final_database_bytes)
    if tree_peak == 0:
        tree_peak = None
        tree_status = "unavailable: process exited before a positive RSS sample"
    require(child.returncode == expected,
            f"expected exit {expected}, got {child.returncode}: {command}\n{output}\n{errors}")
    measurement = {
        "command": command, "exit": child.returncode, "wall_seconds": time.monotonic() - started,
        "wait4_maxrss_native_units": usage.ru_maxrss if usage else None,
        "wait4_status": "measured; KiB on Linux, bytes on macOS" if usage else "unavailable on this platform",
        "process_tree_sampled_peak_rss_bytes": tree_peak,
        "process_tree_rss_status": tree_status,
        "index_and_sidecar_sampled_peak_bytes": database_peak,
        "index_and_sidecar_bytes": final_database_bytes,
        "index_and_sidecar_status": "measured: DB/WAL/SHM sum sampled every 10ms plus after exit; not an exact high-water mark",
    }
    return output, measurement


def authored_project(root, name, sources, define="FIRST"):
    items = "".join(f'<Compile Include="{source}" />' for source in sources)
    (root / name).write_text(
        '<Project><PropertyGroup><TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier>'
        '<TargetFrameworkVersion>v4.8</TargetFrameworkVersion><AssemblyName>Collision</AssemblyName>'
        f'<DefineConstants>{define}</DefineConstants></PropertyGroup><ItemGroup>{items}</ItemGroup></Project>'
    )


def sql_manifest(db):
    sql = sqlite3.connect(db.as_uri() + "?mode=ro", uri=True)
    try:
        def rows(query):
            return sql.execute(query).fetchall()
        return {
            "revision": rows("SELECT revision FROM index_revision")[0][0],
            "files": rows("SELECT path FROM files ORDER BY path"),
            "symbols": rows("SELECT f.path,s.name FROM symbols s JOIN files f ON f.id=s.file_id ORDER BY f.path,s.name"),
            "projects": [(key, json.loads(status)) for key, status in
                         rows("SELECT project_key,standing_json FROM projects ORDER BY project_key")],
            "units": [(key, project, json.loads(status), json.loads(properties)) for key, project, status, properties in
                      rows("SELECT unit_key,project_key,standing_json,properties_json FROM evaluation_units ORDER BY project_key,unit_key")],
            "membership": rows("SELECT u.project_key,m.path,f.path FROM file_participation m "
                               "JOIN evaluation_units u ON u.unit_key=m.unit_key "
                               "LEFT JOIN files f ON f.id=m.file_id ORDER BY u.project_key,m.path"),
            "issues": rows("SELECT path,failure_json FROM discovery_issues ORDER BY ordinal"),
            "diagnostics": rows("SELECT path,error_json,directory_reason FROM source_diagnostics ORDER BY ordinal"),
        }
    finally:
        sql.close()

def assert_authored(actual, sources, projects, membership, trusted=True):
    require(actual["files"] == [(path,) for path in sorted(sources)], f"C8 physical source manifest: {actual}")
    require(actual["symbols"] == sorted((path, symbol) for path, symbol in sources.items()),
            f"C8 exact source symbols (distinct second-source control): {actual}")
    require([key for key, _ in actual["projects"]] == sorted(projects), f"C8 project identity: {actual}")
    require(not actual["issues"] and not actual["diagnostics"], f"C8 unexpected diagnostics: {actual}")
    if not trusted:
        require(not actual["units"] and not actual["membership"], f"C8 untrusted membership leaked: {actual}")
        require(all(status["standing"] == "indeterminate" and status["failure"]["reason"] == "trust-required"
                    for _, status in actual["projects"]), f"C8 TrustRequired not persisted: {actual}")
        return
    require(all(status == {"standing": "confirmed"} for _, status in actual["projects"]),
            f"C8 project standing: {actual}")
    require([project for _, project, _, _ in actual["units"]] == sorted(projects),
            f"C8 exact unit/project manifest: {actual}")
    require(len({key for key, _, _, _ in actual["units"]}) == len(projects), "C8 unit identities collided")
    for _, project, status, properties in actual["units"]:
        require(status == {"standing": "confirmed"} and properties["DefineConstants"] == projects[project]
                and properties["AssemblyName"] == "Collision", f"C8 stale metadata: {actual}")
    require(actual["membership"] == sorted((project, path, path) for project, path in membership),
            f"C8 exact many-to-many membership: {actual}")


def index_smoke(binary, companion, host, stress, unit_count, file_count, timeout):
    reports = []
    with tempfile.TemporaryDirectory(prefix="tethys-index-discovery-") as directory:
        temporary = Path(directory)
        package = temporary / "package"
        package.mkdir()
        cli = package / binary.name
        shutil.copy2(binary, cli)
        shutil.copytree(companion, package / "msbuild-evaluate")
        root = temporary / "workspace"
        root.mkdir()
        (root / "global.json").write_text(json.dumps({"sdk": {"version": host.name, "rollForward": "disable"}}))
        trace = temporary / "host.trace"
        environment = dict(os.environ, COREHOST_TRACE="1", COREHOST_TRACE_VERBOSITY="4",
                           COREHOST_TRACEFILE=str(trace), DOTNET_HOST_TRACE="1",
                           DOTNET_HOST_TRACE_VERBOSITY="4", DOTNET_HOST_TRACEFILE=str(trace))
        db = root / ".rivets/index/tethys.db"

        def run(label, arguments, expected=0):
            trace.unlink(missing_ok=True)
            output, resource = measured_cli(cli, root, arguments, environment, expected, timeout)
            resource.update(phase=label, native_worker_launches=worker_launches(trace.read_text() if trace.exists() else ""))
            reports.append(resource)
            return output, resource

        if stress:
            require(unit_count >= 2 and file_count >= 2, "stress requires at least two units and physical sources")
            sources = {f"Source{i:05}.cs": f"Source{i:05}" for i in range(file_count)}
            for path, symbol in sources.items():
                (root / path).write_text(f"public class {symbol} {{}}\n")
            names = [f"Project{i:04}.csproj" for i in range(unit_count)]
            projects = {name: f"UNIT_{i}" for i, name in enumerate(names)}
            # Deterministic authored partition plus one shared physical source in
            # every unit. Exactly 802 units / 15,124 physical files by default.
            membership = {(names[i % unit_count], path) for i, path in enumerate(sources)}
            membership.update((name, "Source00000.cs") for name in names)
            for name in names:
                authored_project(root, name, sorted(path for project, path in membership if project == name), projects[name])
            run("stress-index", ["index", "--trust-msbuild", "--msbuild-path", host])
            assert_authored(sql_manifest(db), sources, projects, membership)
            return {"claim": "C8", "mode": "stress", "result": "PASS",
                    "units": unit_count, "physical_files": file_count,
                    "membership_rows": len(membership), "budget_verdict": "not evaluated; measurements only",
                    "runs": reports}

        sources = {"Second.cs": "Second", "Shared.cs": "Source"}
        for path, symbol in sources.items():
            (root / path).write_text(f"public class {symbol} {{}}\n")
        projects = {"A.csproj": "FIRST", "B.csproj": "SECOND"}
        authored_project(root, "A.csproj", ["Shared.cs", "Second.cs"])
        authored_project(root, "B.csproj", ["Shared.cs"], "SECOND")
        _, resource = run("no-trust", ["index", "--msbuild-path", host], 1)
        require(resource["native_worker_launches"] == 0 and not trace.exists(), "C6 unauthorized native process")
        assert_authored(sql_manifest(db), sources, projects, [], trusted=False)
        _, resource = run("trusted", ["index", "--trust-msbuild", "--msbuild-path", host])
        require(resource["native_worker_launches"] > 0, "C8 real packaged host positive control did not launch")
        initial = sql_manifest(db)
        assert_authored(initial, sources, projects,
                        [("A.csproj", "Shared.cs"), ("A.csproj", "Second.cs"), ("B.csproj", "Shared.cs")])
        stamps = {path: ((root / path).read_bytes(), (root / path).stat().st_mtime_ns) for path in sources}
        authored_project(root, "A.csproj", ["Second.cs"], "CHANGED")
        (root / "B.csproj").rename(root / "Moved.csproj")
        run("metadata-only-replacement", ["index", "--trust-msbuild", "--msbuild-path", host])
        replacement = sql_manifest(db)
        assert_authored(replacement, sources, {"A.csproj": "CHANGED", "Moved.csproj": "SECOND"},
                        [("A.csproj", "Second.cs"), ("Moved.csproj", "Shared.cs")])
        require(replacement["revision"] > initial["revision"], "C8 metadata did not publish a revision")
        require(stamps == {path: ((root / path).read_bytes(), (root / path).stat().st_mtime_ns) for path in sources},
                "C8 metadata-only control changed source bytes/timestamps")
        # Disable only the disposable package, never the user's installed SDK.
        (package / "msbuild-evaluate").rename(temporary / "unavailable-worker")
        environment.update(PATH=str(temporary / "empty-path"), DOTNET_ROOT=str(temporary / "absent-dotnet"),
                           DOTNET_HOST_PATH=str(temporary / "absent-dotnet/dotnet"))
        for symbol in ("Source", "Second"):
            output, resource = run(f"persisted-query-{symbol}", ["search", symbol])
            require(symbol in output, f"C8 persisted query lost {symbol}: {output}")
            require(resource["native_worker_launches"] == 0 and not trace.exists(),
                    "C8 persisted query invoked native discovery")
        require(sql_manifest(db) == replacement, "C8 queries modified the persisted publication")
        return {"claim": "C8", "mode": "index", "result": "PASS", "runs": reports}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug" / ("tethys.exe" if os.name == "nt" else "tethys"))
    parser.add_argument("--companion", type=Path,
                        default=Path(os.environ.get("TETHYS_WORKER_DISTRIBUTION", ROOT / "target/worker-dist/msbuild-evaluate")))
    parser.add_argument("--host", type=Path, default=os.environ.get("TETHYS_SDK_MSBUILD_PATH"))
    parser.add_argument("--cargo-only", action="store_true")
    parser.add_argument("--index", action="store_true", help="exercise actual packaged CLI and independent SQLite oracle")
    parser.add_argument("--stress", action="store_true", help="explicit index stress workload; no automatic budget verdict")
    parser.add_argument("--stress-units", type=int, default=802)
    parser.add_argument("--stress-files", type=int, default=15124)
    parser.add_argument("--timeout", type=int, default=1800, help="seconds per index/query invocation")
    parser.add_argument("--output", type=Path, help="retain the exact index/stress JSON report")
    args = parser.parse_args()
    require(not (args.cargo_only and (args.index or args.stress)), "--cargo-only conflicts with --index/--stress")
    require(args.timeout > 0, "--timeout must be positive")
    require(args.output is None or args.index or args.stress, "--output requires --index or --stress")
    if args.index or args.stress:
        require(args.host is not None, "Set TETHYS_SDK_MSBUILD_PATH to the selected installed SDK directory")
        report = index_smoke(args.binary.resolve(strict=True), args.companion.resolve(strict=True),
                             args.host.resolve(strict=True), args.stress, args.stress_units, args.stress_files, args.timeout)
        rendered = json.dumps(report, indent=2) + "\n"
        if args.output is not None:
            args.output.write_text(rendered)
        print(rendered, end="")
        return
    cargo_baseline(args.binary.resolve(strict=True))
    if not args.cargo_only:
        require(args.host is not None, "Set TETHYS_SDK_MSBUILD_PATH to the selected installed SDK directory")
        discovery(args.companion.resolve(strict=True), args.host.resolve(strict=True))


if __name__ == "__main__":
    main()
