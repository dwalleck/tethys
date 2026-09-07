#!/usr/bin/env python3
"""S5 actual-CLI oracle: frozen Rust bytes, native unit evidence, 802-unit reads.

The large read fixture is authored persisted metadata, not a corpus/evaluation
performance claim. Native indexing supplies the separate publication control.
"""
import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile

from discovery_smoke import ROOT, ORACLES, measured_cli, require


def binary_digest(path):
    with path.open("rb") as binary:
        return hashlib.file_digest(binary, "sha256").hexdigest()


def command(binary, root, arguments, environment):
    result = subprocess.run([str(binary), "-w", str(root), *arguments],
                            env=environment, capture_output=True, timeout=60)
    return {"arguments": arguments, "exit": result.returncode,
            "stdout": result.stdout.decode(), "stderr": result.stderr.decode()}


def rust_golden(binary, environment):
    fixture = json.loads((ORACLES / "coupling_fixture.json").read_text())
    with tempfile.TemporaryDirectory(prefix="tethys-coupling-rust-") as directory:
        root = Path(directory)
        for name, contents in fixture["files"].items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(contents.encode())
        indexed = command(binary, root, ["index"], environment)
        require(indexed["exit"] == 0, f"Rust control indexing failed: {indexed}")
        for expected in fixture["queries"]:
            actual = command(binary, root, expected["arguments"], environment)
            require(actual == expected, f"C10 Rust output/exit changed: {actual}\nexpected: {expected}")
    return {"claim": "C10", "frozen_rust_queries": len(fixture["queries"]),
            "source_revision": fixture["source_revision"], "result": "PASS"}


def assert_units(report, classic=False):
    rows = [row for row in report["packages"] if "evaluation_unit" in row]
    expected = {("App/App.csproj", "net8.0"): [0, None, None],
                ("Core/Core.csproj", "net8.0"): [None, 0, None],
                ("Core/Core.csproj", "netstandard2.1"): [None, 0, None],
                ("Isolated/Isolated.csproj", "net8.0"): [0, 0, 0.0]}
    if classic:
        expected.update({("Classic48/App.csproj", None): [0, 0, 0.0],
                         ("Twin/Twin.csproj", None): [0, 0, 0.0]})
    actual = {}
    identities = set()
    for row in rows:
        unit = row["evaluation_unit"]
        identity = (unit["project"], unit["target_framework"])
        actual[identity] = [row[field] for field in ("afferent", "efferent", "instability")]
        require(unit["standing"]["standing"] == "confirmed", f"Native unit unavailable: {unit}")
        require(row["name"] == f"msbuild:{unit['project']}:{unit['key']}", "Unit detail selector lost identity")
        identities.add(row["name"])
        for field, value in zip(("afferent", "efferent", "instability"), actual[identity]):
            evidence = ({"standing": "known"} if value is not None else
                        {"standing": "indeterminate", "reason": "unselected_project_reference"})
            require(row["metric_evidence"][field] == evidence, f"Incorrect metric evidence: {row}")
    require(actual == expected and len(identities) == len(expected), f"C10 native manifest mismatch: {actual}")
    require({row["evaluation_unit"]["assembly_name"] for row in rows
             if row["evaluation_unit"]["project"] in ("Core/Core.csproj", "Isolated/Isolated.csproj")} == {"Shared"},
            "Assembly-name collision control did not reach evaluation")
    app = next(row for row in rows if row["evaluation_unit"]["project"] == "App/App.csproj")
    require([reference["target"] for reference in app["evaluation_unit"]["declared_references"]] == ["Core/Core.csproj"],
            "Declaration was lost or projected as a selected edge")
    rust = [row for row in report["packages"] if "evaluation_unit" not in row]
    require(len(rust) == 1 and [rust[0][field] for field in ("afferent", "efferent", "instability")] == [0, 0, 0.0],
            "Mixed-workspace Rust counts changed")
    return rows


def expand_read_fixture(db):
    """Author 480 projects / 802 units; no inferred edges or native timing claim."""
    sql = sqlite3.connect(db)
    try:
        existing_projects = sql.execute("SELECT COUNT(*) FROM projects").fetchone()[0]
        existing_units = sql.execute("SELECT COUNT(*) FROM evaluation_units").fetchone()[0]
        templates = {row[0]: row[1:] for row in sql.execute(
            "SELECT target_framework,framework_json,standing_json,host_json,restore_json "
            "FROM evaluation_units WHERE project_key='Core/Core.csproj'")}
        next_project = sql.execute("SELECT MAX(ordinal)+1 FROM projects").fetchone()[0]
        next_unit = sql.execute("SELECT MAX(ordinal)+1 FROM evaluation_units").fetchone()[0]
        fillers = 480 - existing_projects
        doubled = 802 - existing_units - fillers
        require(0 <= doubled <= fillers, "Invalid authored scale dimensions")
        with sql:
            for number in range(fillers):
                project = f"Scale/Project{number:04}.csproj"
                sql.execute("INSERT INTO projects VALUES (?,?,?,?)",
                            (project, next_project + number, "[]", '{"standing":"confirmed"}'))
                frameworks = ("net8.0", "netstandard2.1") if number < doubled else ("net8.0",)
                for framework in frameworks:
                    key = hashlib.sha256(f"authored:{project}:{framework}".encode()).hexdigest()
                    native_framework, standing, host, restore = templates[framework]
                    sql.execute("INSERT INTO evaluation_units VALUES (?,?,?,?,?,?,?,?,?)",
                                (key, project, next_unit, framework, native_framework, standing,
                                 '{"AssemblyName":"Shared"}', host, restore))
                    sql.execute("INSERT INTO arch_packages(name,path,source,evaluation_unit_key) VALUES (?,?,?,?)",
                                (f"msbuild:{project}:{key}", "Scale", "msbuild", key))
                    next_unit += 1
        require(sql.execute("SELECT COUNT(*) FROM projects").fetchone()[0] == 480 and
                sql.execute("SELECT COUNT(*) FROM evaluation_units").fetchone()[0] == 802,
                "Authored scale fixture did not reach 480 projects / 802 units")
    finally:
        sql.close()


def native_and_scale(binary, companion, sdk, classic_host, environment):
    with tempfile.TemporaryDirectory(prefix="tethys-coupling-native-") as directory:
        temporary = Path(directory)
        package = temporary / "package"
        package.mkdir()
        cli = package / binary.name
        shutil.copy2(binary, cli)
        shutil.copytree(companion, package / "msbuild-evaluate")
        root = temporary / "workspace"
        root.mkdir()
        (root / "global.json").write_text(json.dumps({"sdk": {"version": sdk.name, "rollForward": "disable"}}))
        (root / "NuGet.Config").write_text('<configuration><packageSources><clear /></packageSources></configuration>')
        (root / "Cargo.toml").write_text('[package]\nname="coupling-rust-control"\nversion="0.0.0"\nedition="2024"\n[lib]\npath="RustControl.rs"\n')
        (root / "RustControl.rs").write_text("pub fn stable() -> usize { 0 }\n")
        projects = {
            "App": ('<TargetFramework>net8.0</TargetFramework>',
                    '<ItemGroup><ProjectReference Include="../Core/Core.csproj" /></ItemGroup>'),
            "Core": ('<TargetFrameworks>net8.0;netstandard2.1</TargetFrameworks><AssemblyName>Shared</AssemblyName>', ''),
            "Isolated": ('<TargetFramework>net8.0</TargetFramework><AssemblyName>Shared</AssemblyName>', ''),
        }
        for name, (properties, references) in projects.items():
            project = root / name
            project.mkdir()
            (project / f"{name}.csproj").write_text(
                f'<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup>{properties}</PropertyGroup>{references}</Project>')
            (project / f"{name}.cs").write_text(f"public class {name} {{}}\n")
        dotnet = sdk.parents[1] / ("dotnet.exe" if os.name == "nt" else "dotnet")
        # Explicit fixture setup, not implicit product restore authority. Core last
        # leaves all framework assets stable before the tested invocation.
        for name in ("App", "Isolated", "Core"):
            subprocess.run([dotnet, sdk / "MSBuild.dll", root / name / f"{name}.csproj",
                            "-t:Restore", "-nologo", "-verbosity:quiet"],
                           env=environment, check=True, capture_output=True, timeout=120)
        if classic_host:
            require(os.name == "nt", "Classic qualification requires actual Windows MSBuild")
            for name in ("Classic48", "shared", "Twin"):
                shutil.copytree(ROOT / "tests/fixtures/msbuild/evaluator" / name, root / name)
            # The worker's Twin is deliberately a raw item-only project. This
            # index fixture needs a complete classic framework identity.
            (root / "Twin/Twin.csproj").write_text(
                '<Project ToolsVersion="Current" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">'
                '<PropertyGroup><TargetFrameworkVersion>v4.8</TargetFrameworkVersion>'
                '<OutputType>Library</OutputType><AssemblyName>Collision</AssemblyName></PropertyGroup>'
                '<ItemGroup><Compile Include="Twin.cs" /></ItemGroup>'
                '<Import Project="$(MSBuildToolsPath)/Microsoft.CSharp.targets" /></Project>')
        host = classic_host or sdk
        indexed = command(cli, root, ["index", "--trust-msbuild", "--msbuild-path", str(host)], environment)
        require(indexed["exit"] == 0, f"Native indexing failed: {indexed}")
        result = command(cli, root, ["coupling", "--json"], environment)
        require(result["exit"] == 0, f"Coupling query failed: {result}")
        rows = assert_units(json.loads(result["stdout"]), bool(classic_host))
        for row in rows:
            detail = command(cli, root, ["coupling", "--package", row["name"], "--json"], environment)
            require(detail["exit"] == 0, f"Exact unit selector failed: {detail}")
            value = json.loads(detail["stdout"])
            require(value["incoming"] == [] and value["outgoing"] == [], "C10 fabricated selected-unit edge")
        human = command(cli, root, ["coupling"], environment)
        require(human["exit"] == 0 and "indeterminate" in human["stdout"].lower(), "Human output hid unavailable metrics")
        db = root / ".rivets/index/tethys.db"
        sql = sqlite3.connect(db)
        try:
            require(sql.execute("SELECT COUNT(*) FROM arch_file_packages a JOIN files f ON f.id=a.file_id WHERE f.language='csharp'").fetchone()[0] == 0,
                    "C# source files were assigned to the enclosing Cargo crate")
            require(sql.execute("SELECT COUNT(*) FROM arch_package_deps").fetchone()[0] == 0, "Declarations became selected edges")
        finally:
            sql.close()
        require(not list(root.rglob("target-sentinel.txt")), "Evaluation ran a classic build target")
        expand_read_fixture(db)
        measurements = []
        for sort, field in (("instability", "instability"), ("ca", "afferent"), ("ce", "efferent"), ("name", None)):
            for repetition in range(5):
                output, measurement = measured_cli(cli, root, ["coupling", "--json", "--sort", sort], environment, 0, 10)
                report = json.loads(output)
                require(report["count"] == 803, "Scale query lost a unit or Rust control")
                if field is not None:
                    values = [row[field] for row in report["packages"]]
                    known = [value for value in values if value is not None]
                    require(values == sorted(known, reverse=True) + [None] * (len(values) - len(known)), "Unknown metrics were not sorted last")
                measurement.update(sort=sort, repetition=repetition)
                measurements.append(measurement)
        walls = sorted(item["wall_seconds"] for item in measurements)
        require(walls[18] <= 1 and walls[-1] <= 2, f"S5 query gate exceeded: p95={walls[18]}, max={walls[-1]}")
        return {"claim": "C10", "result": "PASS", "native_units": len(rows),
                "classic": bool(classic_host), "native_report": json.loads(result["stdout"]),
                "human_output": human["stdout"], "authored_read_fixture": {"projects": 480, "units": 802},
                "query_p95_seconds": walls[18], "query_max_seconds": walls[-1], "measurements": measurements}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug" / ("tethys.exe" if os.name == "nt" else "tethys"))
    parser.add_argument("--companion", type=Path, default=Path(os.environ.get("TETHYS_WORKER_DISTRIBUTION", ROOT / "target/worker-dist/msbuild-evaluate")))
    parser.add_argument("--host", type=Path, default=Path(os.environ["TETHYS_SDK_MSBUILD_PATH"]) if "TETHYS_SDK_MSBUILD_PATH" in os.environ else None)
    parser.add_argument("--classic-host", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    require(args.host is not None, "--host or TETHYS_SDK_MSBUILD_PATH must select an installed SDK")
    environment = dict(os.environ, NO_COLOR="1", CLICOLOR="0", TERM="dumb")
    binary = args.binary.resolve(strict=True)
    digest = binary_digest(binary)
    report = {"binary_sha256": digest, "platform": platform.platform(),
              "selected_sdk": str(args.host.resolve(strict=True)),
              "classic_host": str(args.classic_host) if args.classic_host else None,
              "rust": rust_golden(binary, environment),
              "units": native_and_scale(binary, args.companion.resolve(strict=True),
                                        args.host.resolve(strict=True), args.classic_host, environment)}
    require(binary_digest(binary) == digest, "The checked CLI changed during qualification")
    if args.output:
        args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"C10 PASS: {report['rust']['frozen_rust_queries']} frozen Rust outputs; native unit evidence; "
          f"802-unit query p95={report['units']['query_p95_seconds']:.3f}s max={report['units']['query_max_seconds']:.3f}s")


if __name__ == "__main__":
    main()
