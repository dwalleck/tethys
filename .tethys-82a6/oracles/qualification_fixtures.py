"""C12 authored, native-evaluated workload and eligible-cache qualification.

No expected standing is learned from tethys. Generated shape arithmetic and the
shipped evaluator expected.json are the independent authorities. Density captures
are observations awaiting review, never self-authorizing ratio baselines.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time
import uuid


MANIFEST = Path("tests/fixtures/msbuild/qualification/shapes.json")


def require(condition, message):
    if not condition:
        raise RuntimeError("C12 fixture qualification: " + message)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def database(root):
    path = root / ".rivets/index/tethys.db"
    require(path.is_file(), f"index did not publish {path}")
    return sqlite3.connect(path.resolve().as_uri() + "?mode=ro", uri=True)


def put(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def project_name(index):
    return f"Project{index:04d}.csproj"


def source_name(index):
    return f"Source{index:05d}.cs"


def frameworks(shape, index, defaults):
    if "frameworks" in shape:
        return shape["frameworks"]
    return defaults if index < shape["units"] - shape["projects"] else defaults[:1]


def generate(root, shape, defaults):
    """Generate real SDK projects, not unit-row or one-project-per-unit substitutes."""
    require(not root.exists(), f"fresh fixture root already exists: {root}")
    root.mkdir(parents=True)
    files, projects = shape["files"], shape["projects"]
    require(files >= projects > 0, "invalid file/project shape")
    require(sum(len(frameworks(shape, i, defaults)) for i in range(projects)) == shape["units"],
            "framework partition does not yield authored unit count")
    source_hash = hashlib.sha256()
    for i in range(files):
        # Repeated simple names exercise collision handling; qualified cross-file
        # references and same-file calls exercise two distinct resolution paths.
        text = "".join(
            f"namespace Q{i:05d}D{copy} {{\npublic class Node {{\n"
            f"public static int Value() {{ return {i}; }}\n"
            f"public static int Step() {{ return Q00000D{copy}.Node.Value() + Value(); }}\n"
            "}\n}\n" for copy in range(shape.get("density_multiplier", 1)))
        put(root / source_name(i), text)
        source_hash.update(source_name(i).encode() + b"\0" + text.encode())
    for i in range(projects):
        selected = sorted(set(range(i, files, projects)) | {0})
        items = "".join(f'<Compile Include="{source_name(j)}" />' for j in selected)
        tfms = ";".join(frameworks(shape, i, defaults))
        # Preserve assembly collisions without NuGet package-identity cycles.
        text = ('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup>'
                f'<TargetFrameworks>{tfms}</TargetFrameworks>'
                '<EnableDefaultCompileItems>false</EnableDefaultCompileItems>'
                '<GenerateAssemblyInfo>false</GenerateAssemblyInfo>'
                f'<AssemblyName>Collision{i // 8:04d}</AssemblyName><PackageId>Project{i:04d}</PackageId>'
                '</PropertyGroup><ItemGroup>' + items +
                (f'<ProjectReference Include="{project_name(projects - 1)}">'
                 '<ReferenceOutputAssembly>False</ReferenceOutputAssembly></ProjectReference>'
                 if i != projects - 1 else '') + '</ItemGroup></Project>')
        put(root / project_name(i), text)
    # Projects share a source directory, never an assets/output directory.
    # Directory.Build.props is imported early enough for NuGet/MSBuild paths.
    put(root / "Directory.Build.props", '<Project><PropertyGroup>'
        '<BaseIntermediateOutputPath>obj/$(MSBuildProjectName)/</BaseIntermediateOutputPath>'
        '<MSBuildProjectExtensionsPath>$(BaseIntermediateOutputPath)</MSBuildProjectExtensionsPath>'
        '<BaseOutputPath>bin/$(MSBuildProjectName)/</BaseOutputPath></PropertyGroup></Project>')
    solution = ["Microsoft Visual Studio Solution File, Format Version 12.00", "# Visual Studio Version 17"]
    identifiers = [str(uuid.UUID(int=i + 1)).upper() for i in range(projects)]
    for i, identifier in enumerate(identifiers):
        solution += [f'Project("{{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}}") = "Project{i:04d}", '
                     f'"{project_name(i)}", "{{{identifier}}}"', "EndProject"]
    solution += ["Global", "\tGlobalSection(SolutionConfigurationPlatforms) = preSolution",
                 "\t\tDebug|Any CPU = Debug|Any CPU", "\tEndGlobalSection",
                 "\tGlobalSection(ProjectConfigurationPlatforms) = postSolution"]
    for identifier in identifiers:
        solution += [f"\t\t{{{identifier}}}.Debug|Any CPU.ActiveCfg = Debug|Any CPU",
                     f"\t\t{{{identifier}}}.Debug|Any CPU.Build.0 = Debug|Any CPU"]
    solution += ["\tEndGlobalSection", "EndGlobal", ""]
    put(root / "Qualification.sln", "\n".join(solution))
    return source_hash.hexdigest()


def prepare_generated(root, *, sdk, environment, timeout):
    """Restore the solution before product measurement."""
    root, sdk = root.resolve(strict=True), sdk.resolve(strict=True)
    dotnet = sdk.parents[1] / ("dotnet.exe" if os.name == "nt" else "dotnet")
    require(dotnet.is_file() and (sdk / "MSBuild.dll").is_file(),
            f"fixture preparation needs the selected SDK host at {sdk}")
    put(root / "global.json", json.dumps({"sdk": {"version": sdk.name, "rollForward": "disable"}}))
    command = [str(dotnet), str(sdk / "MSBuild.dll"), str(root / "Qualification.sln"),
               "-t:Restore", "-nologo", "-verbosity:quiet", "-p:RestoreUseStaticGraphEvaluation=true"]
    log_path = root.parent / f"{root.name}-restore.log"
    started = time.monotonic()
    with log_path.open("wb") as log:
        try:
            process = subprocess.run(command, cwd=root, env=environment, stdout=log,
                                     stderr=subprocess.STDOUT, timeout=timeout, check=False)
        except subprocess.TimeoutExpired as error:
            raise RuntimeError(f"C12 fixture native Restore exceeded {timeout}s; see {log_path}") from error
    duration = time.monotonic() - started
    if process.returncode:
        with log_path.open("rb") as log:
            log.seek(max(0, log_path.stat().st_size - 8192))
            detail = log.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"C12 fixture native Restore failed ({process.returncode}); required native "
                           f"reference packages/assets must be available, no TFM is omitted. {detail}; log: {log_path}")
    return {"operation": "explicit native fixture Restore, not product restore permission",
            "command": command, "exit_code": process.returncode, "wall_seconds": duration,
            "log": str(log_path), "log_sha256": digest(log_path)}


def warm_fixture_packages(repository, root, *, sdk, environment, timeout=600):
    """Restore all authored TFMs natively before package-cache identity freezes."""
    manifest_path = repository / MANIFEST
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    tfms = list(dict.fromkeys(manifest["frameworks"] + [
        tfm for shape in manifest["shapes"].values() for tfm in shape.get("frameworks", [])]))
    shape = {"files": 1, "projects": 1, "units": len(tfms), "frameworks": tfms}
    source_sha = generate(root, shape, tfms)
    preparation = prepare_generated(root, sdk=sdk, environment=environment, timeout=timeout)
    evidence = {"frameworks": tfms, "source_sha256": source_sha,
                "manifest_sha256": digest(manifest_path), "preparation": preparation}
    shutil.rmtree(root)
    return evidence


def check_generated(root, shape, defaults):
    """Compare physical files, units and every membership/declaration to formula."""
    sql = database(root)
    try:
        files, projects = shape["files"], shape["projects"]
        expected_files = [source_name(i) for i in range(files)]
        require([r[0] for r in sql.execute("SELECT path FROM files ORDER BY path")] == expected_files,
                "physical source set differs from formula")
        project_rows = sql.execute("SELECT project_key,standing_json FROM projects ORDER BY project_key").fetchall()
        require([r[0] for r in project_rows] == [project_name(i) for i in range(projects)], "project set differs")
        require(all(json.loads(r[1]) == {"standing": "confirmed"} for r in project_rows),
                "native project evaluation unavailable; install the selected SDK and required native imports")
        unit_rows = sql.execute("SELECT unit_key,project_key,target_framework,standing_json,properties_json "
                                "FROM evaluation_units ORDER BY project_key,target_framework").fetchall()
        expected_units = sorted((project_name(i), tfm) for i in range(projects)
                                for tfm in frameworks(shape, i, defaults))
        require([(r[1], r[2]) for r in unit_rows] == expected_units, "selected native TFM set differs")
        memberships = 0
        for key, project, tfm, standing, properties in unit_rows:
            require(json.loads(standing) == {"standing": "confirmed"}, f"native evaluation failed: {project}/{tfm}: {standing}")
            i = int(project[7:11])
            require(json.loads(properties)["AssemblyName"] == f"Collision{i // 8:04d}", "assembly collision metadata differs")
            expected = [source_name(j) for j in sorted(set(range(i, files, projects)) | {0})]
            actual = sql.execute("SELECT m.path,f.path FROM file_participation m LEFT JOIN files f ON f.id=m.file_id "
                                 "WHERE m.unit_key=? ORDER BY m.path", (key,)).fetchall()
            require(actual == [(p, p) for p in expected], f"membership differs: {project}/{tfm}")
            memberships += len(expected)
            refs = sql.execute("SELECT target_project_key,metadata_json FROM declared_project_references WHERE unit_key=?", (key,)).fetchall()
            expected_count = 0 if i == projects - 1 else 1
            require(len(refs) == expected_count and (not refs or (
                    refs[0][0] == project_name(projects - 1) and
                    json.loads(refs[0][1]).get("ReferenceOutputAssembly", "").lower() == "false")),
                    f"acyclic star declaration differs: {project}/{tfm}")
        for name in ("Node", "Value", "Step"):
            rows = sql.execute("SELECT f.path,count(*) FROM symbols s JOIN files f ON f.id=s.file_id "
                               "WHERE s.name=? GROUP BY f.path ORDER BY f.path", (name,)).fetchall()
            require(rows == [(p, shape.get("density_multiplier", 1)) for p in expected_files],
                    f"formula declaration {name} lost or duplicated")
        require(sql.execute("SELECT count(*) FROM discovery_issues").fetchone()[0] == 0, "unexpected discovery issue")
        require(sql.execute("SELECT count(*) FROM source_diagnostics").fetchone()[0] == 0, "generated syntax produced diagnostics")
        counts = {table: sql.execute(f"SELECT count(*) FROM {table}").fetchone()[0]
                  for table in ("symbols", "refs", "call_edges", "declared_project_references", "file_participation")}
        multiplier = shape.get("density_multiplier", 1)
        require(counts["refs"] >= files * 2 * multiplier, "meaningful reference workload disappeared")
        require(counts["call_edges"] >= files * multiplier, "meaningful call workload disappeared")
        return {"authority": "shapes.json plus deterministic source/project formula v3", "physical_files": files,
                "projects": projects, "selected_units": len(unit_rows), "membership_rows": memberships,
                "formula_named_declarations": files * 3 * multiplier, "formula_call_expressions": files * 2 * multiplier,
                "counts": counts, "density": {name + "_per_file": count / files for name, count in counts.items()},
                "collision_occurrences": {name: files * multiplier for name in ("Node", "Value", "Step")},
                "graph_authority": "observed retained syntax graph, not compiler-selected semantic bindings",
                "standing": "confirmed", "result": "PASS"}
    finally:
        sql.close()


def record(label, result, correctness):
    return {"label": label, "measurement": result["measurement"], "correctness": correctness,
            "canonical": result.get("canonical"), "exit_code": result["exit_code"]}


def density_authority(runtime, manifest, source_sha):
    return {"formula_version": 3, "shape": manifest["shapes"]["baseline"],
            "frameworks": manifest["frameworks"], "source_sha256": source_sha,
            "cli_sha256": digest(runtime.cli), "sdk": str(runtime.sdk.resolve())}


def review_density_capture(capture, *, reviewer, review_reference):
    """Return manifest density_baseline authority after explicit human review.

    Main persists this returned object into shapes.json. The capture is taken
    from capture_density's baseline correctness.density_baseline; no execution
    automatically turns its own observations into reviewed authority.
    """
    require(bool(reviewer.strip()) and bool(review_reference.strip()), "density review needs reviewer and evidence reference")
    require(capture["status"] == "oracle-valid-capture-awaiting-review", "not an oracle-valid density capture")
    require(all(capture["values"][key] > 0 for key in ("symbols", "refs", "call_edges")), "empty retained graph baseline")
    return {**capture, "status": "reviewed", "reviewer": reviewer, "review_reference": review_reference}


def capture_density(runtime, root, *, repeat=2, allow_restore=False):
    """Explicit first-capture path: exercises both native 1x and 2x workloads.

    Returns unreviewed evidence, not qualification acceptance. Review the
    baseline capture with review_density_capture and persist that authority,
    then run run_fixtures normally to enforce the reviewed baseline.
    """
    return _run_fixtures(runtime, root, repeat, ["baseline", "density-2x"], capture=True, allow_restore=allow_restore)


def run_fixtures(runtime, root, *, repeat=2, shapes=None, allow_restore=False):
    return _run_fixtures(runtime, root, repeat, shapes, capture=False, allow_restore=allow_restore)


def _run_fixtures(runtime, root, repeat, shapes, *, capture, allow_restore):
    require(repeat >= 2, "determinism requires at least two fresh repetitions")
    require(allow_restore is True, "fixture qualification requires explicit allow_restore=True before native preparation")
    path = runtime.repository / MANIFEST
    manifest = json.loads(path.read_text(encoding="utf-8"))
    selected = list(manifest["shapes"]) if shapes is None else list(shapes)
    require(selected and len(selected) == len(set(selected)), "shape selection must be nonempty and unique")
    require(set(selected) <= set(manifest["shapes"]), f"unknown requested shapes: {selected}")
    if "density-2x" in selected:
        selected = ["baseline"] + [name for name in selected if name != "baseline"]
        require(capture or manifest["density_baseline"].get("status") == "reviewed",
                "reviewed retained-graph density baseline missing; execute capture_density, explicitly review "
                "baseline correctness.density_baseline via review_density_capture, persist it in shapes.json, then rerun")
    from qualification_environment import environment_root, freeze_environment, verify_environment
    native_root = environment_root(runtime.environment)
    if (native_root / "frozen-environment.json").exists():
        frozen = verify_environment(native_root)
        warmup = {"status": "using existing verified frozen native inputs; no rebaseline"}
    else:
        warmup = warm_fixture_packages(runtime.repository, root / "fixture-package-warmup",
                                       sdk=runtime.sdk, environment=runtime.environment, timeout=runtime.timeout)
        frozen = freeze_environment(native_root)
    records, baseline = [], None
    for name in selected:
        shape = manifest["shapes"][name]
        canonical = None
        workspace = root / f"fixture-{name}-workspace"
        pristine = root / f"fixture-{name}-prepared"
        require(not pristine.exists(), f"prepared fixture already exists: {pristine}")
        source_sha = generate(workspace, shape, manifest["frameworks"])
        restore = prepare_generated(workspace, sdk=runtime.sdk, environment=runtime.environment, timeout=runtime.timeout)
        require(verify_environment(native_root) == frozen, "shape preparation changed frozen native inputs")
        shutil.copytree(workspace, pristine, copy_function=shutil.copy2)
        shutil.rmtree(workspace)
        first = True
        for mode in ("batch", "stream"):
            for iteration in range(repeat):
                label = f"fixture-{name}-{mode}-{iteration + 1}"
                copied_at = time.monotonic()
                shutil.copytree(pristine, workspace, copy_function=shutil.copy2)
                preparation = {**restore, "restore_executed_for_this_run": first,
                               "restored_inputs_reused": not first,
                               "wall_seconds": time.monotonic() - copied_at + (restore["wall_seconds"] if first else 0)}
                first = False
                result = runtime.index(workspace, label=label, mode=mode, allow_restore=True)
                require(verify_environment(native_root) == frozen, "authorized index changed frozen native inputs")
                correctness = check_generated(workspace, shape, manifest["frameworks"])
                if canonical is None:
                    canonical = result["canonical"]
                require(result["canonical"] == canonical, f"full canonical drift in {label}")
                correctness.update(manifest_sha256=digest(path), source_formula_sha256=source_sha,
                                   fresh_index=True, canonical_workspace_reused=True,
                                   full_canonical_equal=True, preparation=preparation,
                                   native_environment=frozen, fixture_package_warmup=warmup)
                if name == "baseline":
                    baseline = correctness
                    authority = density_authority(runtime, manifest, source_sha)
                    captured = {"status": "oracle-valid-capture-awaiting-review", "provenance": authority,
                                "capture_label": label, "canonical": result["canonical"],
                                "values": correctness["counts"], "collisions": correctness["collision_occurrences"]}
                    correctness["density_baseline"] = captured
                    reviewed = manifest["density_baseline"]
                    if not capture and reviewed.get("status") == "reviewed":
                        require(reviewed.get("reviewer") and reviewed.get("review_reference"), "density review provenance missing")
                        require(reviewed["provenance"] == authority, "reviewed density baseline source/shape/CLI/SDK provenance drift")
                        require(reviewed["values"] == correctness["counts"] and
                                reviewed["collisions"] == correctness["collision_occurrences"],
                                "observed retained baseline differs from reviewed authority")
                if name == "density-2x":
                    require(baseline is not None and shape["files"] == baseline["physical_files"] and
                            shape["projects"] == baseline["projects"] and shape["units"] == baseline["selected_units"],
                            "2x comparison requires identical source/project/unit shape")
                    ratios = {}
                    for metric in ("symbols", "refs", "call_edges"):
                        actual, previous = correctness["counts"][metric], baseline["counts"][metric]
                        require(previous > 0 and actual >= previous * 2, f"retained {metric} density below required 2x")
                        ratios[metric] = actual / previous
                    for name_ in ("Node", "Value", "Step"):
                        require(correctness["collision_occurrences"][name_] == baseline["collision_occurrences"][name_] * 2,
                                f"authored {name_} collision multiplicity is not exactly doubled")
                    correctness["density_comparison"] = {"required_ratio": 2, "retained_graph_ratios": ratios,
                        "ratio_qualified": not capture, "review_status": "capture-only" if capture else "reviewed",
                        "baseline_label": baseline["density_baseline"]["capture_label"]}
                records.append({**record(label, result, correctness),
                                "workload": f"fixture/{name}/{mode}", "repetition": iteration + 1})
                shutil.rmtree(workspace)
        shutil.rmtree(pristine)
    return records


def cache_expected(root, define):
    sql = database(root)
    try:
        units = sql.execute("SELECT standing_json,properties_json FROM evaluation_units").fetchall()
        require(len(units) == 1 and json.loads(units[0][0]) == {"standing": "confirmed"}, "cache control never reached eligible confirmed evaluation")
        require(json.loads(units[0][1])["DefineConstants"] == define, "imported metadata was stale")
        require(sql.execute("SELECT path FROM files").fetchall() == [("Keep.cs",)], "cache fixture source set changed")
        require(sql.execute("SELECT path FROM file_participation").fetchall() == [("Keep.cs",)], "cache fixture membership changed")
        return {"authority": "authored literal import recipe", "define": define, "selected_units": 1, "result": "PASS"}
    finally:
        sql.close()


def run_cache(runtime, root):
    workspace = root / "eligible-cache"
    require(not workspace.exists(), f"cache root already exists: {workspace}")
    workspace.mkdir(parents=True)
    put(workspace / "Keep.cs", "public class Keep {}\n")
    # Match discovery_smoke's qualified literal recipe: no imports, SDK,
    # conditions or expressions. Imported metadata is a separate ineligible
    # phase below; even a literal Import is never eligible in the native worker.
    literal = ('<Project><PropertyGroup><TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier>'
               '<TargetFrameworkVersion>v4.8</TargetFrameworkVersion><AssemblyName>Literal</AssemblyName>'
               '<DefineConstants>FIRST</DefineConstants></PropertyGroup>'
               '<ItemGroup><Compile Include="Keep.cs"/></ItemGroup></Project>')
    put(workspace / "App.csproj", literal)
    records = []
    cold = runtime.index(workspace, label="cache-cold")
    require(cold["measurement"]["evaluation_invocations"] > 0, "cold evaluator positive control did not launch")
    records.append(record("cache-cold", cold, cache_expected(workspace, "FIRST")))
    hit = runtime.index(workspace, label="cache-unchanged")
    require(hit["measurement"]["evaluation_invocations"] == 0, "unchanged eligible request evaluated")
    require(hit["canonical"] == cold["canonical"], "eligible cache hit changed canonical facts")
    records.append(record("cache-unchanged", hit, cache_expected(workspace, "FIRST")))
    # Establish the imported FIRST baseline before changing only the import.
    # This preserves the zero-launch eligible control above without pretending
    # the closed native recipe classifier qualifies any import closure.
    put(workspace / "Imported.props", "<Project><PropertyGroup><DefineConstants>FIRST</DefineConstants></PropertyGroup></Project>")
    put(workspace / "App.csproj", literal.replace(
        "<DefineConstants>FIRST</DefineConstants>", "").replace(
        "</PropertyGroup>", '</PropertyGroup><Import Project="Imported.props"/>'))
    imported = runtime.index(workspace, label="cache-import-baseline")
    require(imported["measurement"]["evaluation_invocations"] > 0, "import recipe transition did not evaluate")
    imported_evidence = cache_expected(workspace, "FIRST")
    imported_evidence["eligibility"] = "ineligible: native classifier rejects import closures"
    records.append(record("cache-import-baseline", imported, imported_evidence))
    put(workspace / "Imported.props", "<Project><PropertyGroup><DefineConstants>CHANGED</DefineConstants></PropertyGroup></Project>")
    changed = runtime.index(workspace, label="cache-import-trusted-reindex")
    require(changed["measurement"]["evaluation_invocations"] > 0, "changed import trusted reindex did not evaluate")
    require(changed["canonical"] != imported["canonical"], "import-only metadata change was not published")
    changed_evidence = cache_expected(workspace, "CHANGED")
    changed_evidence["operation"] = "trusted reindex via index_with_options; fresh grant"
    changed_evidence["eligibility"] = "ineligible imported recipe; evaluator must observe changed import"
    records.append(record("cache-import-trusted-reindex", changed, changed_evidence))
    forced = runtime.index(workspace, label="cache-forced", bypass_cache=True)
    require(forced["measurement"]["evaluation_invocations"] > 0, "bypass did not evaluate")
    require(forced["canonical"] == changed["canonical"], "forced evaluation differs from changed-import trusted reindex")
    records.append(record("cache-forced", forced, cache_expected(workspace, "CHANGED")))
    query = runtime.query(workspace, ["search", "Keep"], label="cache-query")
    require(query["measurement"]["evaluation_invocations"] == 0 and "Keep" in query["output"], "query evaluated or lost persisted symbol")
    from qualification_runtime import canonical_snapshot
    require(canonical_snapshot(workspace) == forced["canonical"], "query modified persisted canonical facts")
    records.append(record("cache-query", query, {"result": "PASS", "evaluator_free": True, "persisted_symbol": "Keep"}))
    return records


def check_legacy(workspace, manifest, host):
    sql = database(workspace)
    try:
        rows = sql.execute("SELECT unit_key,project_key,target_framework,framework_json,standing_json,properties_json,host_json "
                           "FROM evaluation_units ORDER BY project_key").fetchall()
        authored = {unit["project"]: unit for unit in manifest["units"] if unit["tfm"] is None}
        require({row[1] for row in rows} == set(authored) | {"Twin/Twin.csproj"} and len(rows) == 4, "legacy selected project set differs")
        membership_count = 0
        for key, project, tfm, framework, standing, properties, host_json in rows:
            require(tfm is None and json.loads(standing) == {"standing": "confirmed"}, f"legacy native prerequisite/evaluation failed: {project}: {standing}")
            props = json.loads(properties)
            loaded = json.loads(host_json)
            require(loaded["kind"] == "framework" and Path(loaded["path"]).resolve() == host.resolve(), "legacy selected VS host substituted")
            require(props["AssemblyName"] == "Collision", "legacy assembly-name collision lost")
            if project in authored:
                unit = authored[project]
                for field, expected in (("TargetFrameworkIdentifier", unit["identifier"]), ("TargetFrameworkVersion", unit["version"]),
                                        ("TargetFrameworkProfile", unit["profile"]), ("LangVersion", unit["lang"])):
                    require(props.get(field, "") == expected, f"legacy {field} differs for {project}")
                require(unit["define"] in props["DefineConstants"].split(";"), "legacy define lost")
                paths = manifest["common_sources"] + unit["extra"]
            else:
                paths = ["Twin.cs"]
                require(props["TargetFrameworkIdentifier"] == ".NETFramework" and props["TargetFrameworkVersion"] == "v4.8", "native Twin target imports lost framework identity")
            expected_paths = sorted((workspace / project).parent.joinpath(p).resolve().relative_to(workspace.resolve()).as_posix() for p in paths)
            actual = sql.execute("SELECT path FROM file_participation WHERE unit_key=? ORDER BY path", (key,)).fetchall()
            require([r[0] for r in actual] == expected_paths, f"legacy authored Compile membership differs: {project}")
            membership_count += len(actual)
            if project in authored:
                links = dict(sql.execute("SELECT path,link FROM file_participation WHERE unit_key=?", (key,)))
                for relative, expected_link in manifest["links"].items():
                    path = (workspace / project).parent.joinpath(relative).resolve().relative_to(workspace.resolve()).as_posix()
                    require(links[path] == expected_link, f"legacy authored Link differs: {project}/{path}")
                refs = sql.execute("SELECT target_project_key,metadata_json FROM declared_project_references WHERE unit_key=?", (key,)).fetchall()
                require(len(refs) == 1 and refs[0][0] == "Twin/Twin.csproj" and json.loads(refs[0][1])["ReferenceOutputAssembly"].lower() == "false", "legacy authored declaration differs")
        require(sql.execute("SELECT count(*) FROM discovery_issues").fetchone()[0] == 0, "legacy discovery issues")
        return {"authority": "tests/fixtures/msbuild/evaluator/expected.json and shipped Twin", "selected_units": 4,
                "membership_rows": membership_count, "platform": "Windows", "host": str(host), "result": "PASS"}
    finally:
        sql.close()


def run_legacy(runtime, root, *, repeat=2):
    require(os.name == "nt", "legacy acceptance requires actual Windows with selected Visual Studio MSBuild; SDK-on-Linux is not Windows acceptance")
    host = runtime.classic_host
    require(host is not None and (host / "MSBuild.exe").is_file(), "legacy lane requires a selected installed Visual Studio MSBuild directory")
    require(repeat >= 2, "legacy determinism requires at least two fresh repetitions")
    source = runtime.repository / "tests/fixtures/msbuild/evaluator"
    authority = source / "expected.json"
    manifest = json.loads(authority.read_text(encoding="utf-8"))
    records, canonical = [], None
    for mode in ("batch", "stream"):
        for iteration in range(repeat):
            label = f"legacy-{mode}-{iteration + 1}"
            workspace = root / label
            require(not workspace.exists(), f"legacy fresh root already exists: {workspace}")
            workspace.mkdir(parents=True)
            for name in ("Client40", "Classic472", "Classic48", "Twin", "shared"):
                shutil.copytree(source / name, workspace / name)
            result = runtime.index(workspace, label=label, mode=mode, host=host,
                                   properties={"Configuration": "Qualification", "VSToolsPath": "", "QualificationOverride": "caller value"})
            correctness = check_legacy(workspace, manifest, host)
            require(not list(workspace.rglob("target-sentinel.txt")), "native evaluation executed a target")
            if canonical is None:
                canonical = result["canonical"]
            require(result["canonical"] == canonical, f"legacy full canonical drift: {label}")
            correctness.update(manifest_sha256=digest(authority), full_canonical_equal=True, fresh_root=True)
            records.append({**record(label, result, correctness),
                            "workload": f"legacy/{mode}", "repetition": iteration + 1})
    return records