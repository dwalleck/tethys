#!/usr/bin/env python3
"""S2 independent MSBuild oracle and explicit build-time distribution tooling.

Normal invocation never builds, restores, downloads, or uses checkout-local binaries.
--prepare is an explicit CI/developer build operation, not an evaluator fallback.
Requires Python 3.11+, dotnet, and (for --host windows) VS 17.14 MSBuild.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import tomllib
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
PROJECT = ROOT / "tools/tethys-msbuild-evaluate/Tethys.MSBuild.Evaluate.csproj"
ASSEMBLY = "Tethys.MSBuild.Evaluate"
PROPERTIES = ["AssemblyName", "TargetFramework", "TargetFrameworks",
              "TargetFrameworkIdentifier", "TargetFrameworkVersion", "TargetFrameworkProfile",
              "DefineConstants", "LangVersion", "Configuration", "VSToolsPath",
              "QualificationOverride", "MSBuildVersion", "MSBuildFileVersion", "MSBuildRuntimeType", "MSBuildBinPath"]
ITEMS = ["Compile", "ProjectReference", "Reference", "PackageReference", "PackageDownload", "PackageVersion"]
GLOBALS = {"Configuration": "Qualification", "VSToolsPath": "", "QualificationOverride": "caller value"}
# Audited legacy packages predate NuGet SPDX metadata. Never infer licenses from
# arbitrary URLs. Any dependency-version or license-content change requires review.
LEGACY_MIT = {("system.memory", "4.5.5"), ("system.buffers", "4.5.1"),
              ("system.numerics.vectors", "4.5.0"), ("system.threading.tasks.extensions", "4.5.4"),
              ("system.valuetuple", "4.5.0")}
LEGACY_MIT_SHA256 = "d7a68596ab69b06f51ca278a6545148e4269a9381c26d597c13df5d88e08cf5b"
REFERENCE_MIT = {("microsoft.netframework.referenceassemblies", "1.0.3"),
                 ("microsoft.netframework.referenceassemblies.net472", "1.0.3")}
# Reviewed MIT source: https://github.com/microsoft/dotnet/blob/7cdf34dec038c4da4d14735a34ab259704ec4a1e/LICENSE
REFERENCE_LICENSE_URL = "https://github.com/Microsoft/dotnet/blob/master/LICENSE"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def terminate_tree(process):
    """Kill the whole process tree; a lingering MSBuild node otherwise holds the pipes."""
    if os.name == "nt":
        subprocess.run(["taskkill", "/F", "/T", "/PID", str(process.pid)], capture_output=True)
    else:
        try:
            os.killpg(os.getpgid(process.pid), signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            process.kill()
    process.wait()


def feed_stdin(process, payload):
    try:
        process.stdin.write(payload)
        process.stdin.close()
    except OSError:
        pass


def run(command, cwd, payload=None, allow_failure=False, timeout=60):
    """Drain both pipes concurrently with independent caps; kill on overflow/deadline.

    Every stage is bounded: the request write runs on its own thread, the wait honours the
    deadline, the tree is killed as a group, and the drain threads are daemons joined with a
    timeout, so a child that stops reading stdin or a grandchild that keeps the pipes open
    cannot hang the caller past its own contract.
    """
    process = subprocess.Popen([str(x) for x in command], cwd=cwd, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=os.name != "nt",
                               creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0)
    buffers = [bytearray(), bytearray()]
    overflow = []

    def drain(stream, index, cap):
        while chunk := stream.read(65536):
            if len(buffers[index]) + len(chunk) > cap:
                overflow.append(index)
                terminate_tree(process)
                break
            buffers[index].extend(chunk)
        stream.close()

    threads = [threading.Thread(target=drain, args=(process.stdout, 0, 64 * 1024 * 1024), daemon=True),
               threading.Thread(target=drain, args=(process.stderr, 1, 1024 * 1024), daemon=True)]
    for thread in threads:
        thread.start()
    try:
        if payload is None:
            process.stdin.close()
        else:
            writer = threading.Thread(target=feed_stdin, args=(process, payload), daemon=True)
            writer.start()
            writer.join(timeout=timeout)
            if writer.is_alive():
                terminate_tree(process)
                raise RuntimeError(f"qualification subprocess did not consume its request: {command}")
        process.wait(timeout=timeout)
    except BaseException:
        terminate_tree(process)
        raise
    finally:
        for thread in threads:
            thread.join(timeout=30)
    require(not overflow, f"qualification subprocess output overflow: {command}")
    stdout, stderr = [bytes(value).decode("utf-8-sig") for value in buffers]
    require(allow_failure or process.returncode == 0,
            f"command failed ({process.returncode}): {command}\n{stdout}\n{stderr}")
    return process.returncode, stdout, stderr


def sdk_rank(version):
    """Total, intentional order: a release outranks its own prerelease, then prerelease text.

    `Path` ordering would otherwise decide ties by directory spelling, which silently
    prefers a preview over the stable SDK installed beside it.
    """
    core, separator, prerelease = version.partition("-")
    numbers = tuple(int(x) for x in core.split("."))
    # A release outranks its own prerelease, so the release flag is 1 when there is no
    # prerelease suffix.
    return (numbers, 0 if separator else 1, prerelease)


def selected_sdk():
    dotnet = Path(shutil.which(os.environ.get("DOTNET", "dotnet")) or "")
    require(dotnet.is_file(), "dotnet is missing; explicitly install/select a supported SDK")
    dotnet = dotnet.resolve()
    override = os.environ.get("TETHYS_SDK_MSBUILD_PATH")
    if override:
        path = Path(override).resolve(strict=True)
    else:
        _, text, _ = run([dotnet, "--list-sdks"], ROOT)
        entries = []
        for line in text.splitlines():
            version, directory = line.split(" [", 1)
            entries.append((sdk_rank(version), Path(directory.rstrip("]")) / version))
        require(entries, "No installed .NET SDK; qualification cannot be skipped")
        path = max(entries)[1].resolve(strict=True)
    require((path / "MSBuild.dll").is_file(), f"Selected SDK lacks MSBuild.dll: {path}")
    return dotnet, path


def selected_windows(strict=True):
    """Select the VS MSBuild host.

    Qualification requires the qualified 17.14 band exactly (the x86/amd64 host contract
    this slice established); packaging only needs a working MSBuild, so it selects the
    latest installation instead of blocking a release on the runner image's VS version.
    """
    require(os.name == "nt", "--host windows requires actual Windows/Visual Studio")
    vswhere = Path(os.environ["ProgramFiles(x86)"]) / "Microsoft Visual Studio/Installer/vswhere.exe"
    query = [vswhere, "-products", "*", "-requires", "Microsoft.Component.MSBuild",
             "-format", "json", "-utf8"]
    query += ["-version", "[17.14,17.15)"] if strict else ["-latest"]
    _, text, _ = run(query, ROOT)
    installations = json.loads(text)
    require(installations, "Visual Studio 17.14 MSBuild is required; no fallback to another host"
            if strict else "No Visual Studio MSBuild installation is available for packaging")
    installation = max(installations, key=lambda x: tuple(map(int, x["installationVersion"].split("."))))
    path = (Path(installation["installationPath"]) / "MSBuild/Current/Bin").resolve(strict=True)
    require((path / "MSBuild.exe").is_file(), f"VS installation lacks MSBuild.exe: {path}")
    print("VS host:", json.dumps(installation, ensure_ascii=False))
    return path


def check_licenses():
    """Inspect restored nuspecs for every locked direct/transitive managed package."""
    allow = set(tomllib.loads((ROOT / "deny.toml").read_text())["licenses"]["allow"])
    locks = json.loads(PROJECT.with_name("packages.lock.json").read_text())["dependencies"]
    assets = json.loads((PROJECT.parent / "obj/project.assets.json").read_text())
    folders = [Path(folder) for folder in assets["packageFolders"]]
    packages = {(name.lower(), info["resolved"]) for framework in locks.values()
                for name, info in framework.items() if info["type"] != "Project"}
    require(packages, "Managed dependency lock contains no packages")
    for name, version in sorted(packages):
        candidates = [folder / name / version.lower() / (name + ".nuspec") for folder in folders]
        nuspec = next((path for path in candidates if path.is_file()), None)
        require(nuspec is not None, f"Missing restored license metadata: {name}/{version}")
        nodes = list(ET.parse(nuspec).iter())
        license_nodes = [node for node in nodes if node.tag.split("}")[-1] == "license"]
        if (name, version) in LEGACY_MIT:
            digest = hashlib.sha256((nuspec.parent / "LICENSE.TXT").read_bytes()).hexdigest()
            require(digest == LEGACY_MIT_SHA256, f"Audited MIT license changed: {name}/{version}")
            expression = "MIT"
        elif (name, version) in REFERENCE_MIT:
            urls = [node.text for node in nodes if node.tag.split("}")[-1] == "licenseUrl"]
            require(urls == [REFERENCE_LICENSE_URL], f"Audited reference license changed: {name}/{version}")
            expression = "MIT"
        else:
            require(len(license_nodes) == 1, f"Missing/ambiguous SPDX license: {name}/{version}")
            node = license_nodes[0]
            require(node.attrib.get("type") == "expression", f"Unreviewed license file: {name}/{version}")
            expression = node.text
        require(expression in allow,
                f"Managed license outside deny.toml allowlist: {name}/{version}: {expression}")
        print(f"Managed license: {name}/{version}: {expression}")


def prepare(destination, host):
    dotnet, _ = selected_sdk()
    # Package hermetically: neither `dotnet publish -o` nor `-t:Build -p:OutputPath`
    # removes unknown files, so a previous prepare's stale artifacts would otherwise ship.
    shutil.rmtree(destination / "msbuild-evaluate", ignore_errors=True)
    run([dotnet, "restore", PROJECT, "--locked-mode"], ROOT, timeout=300)
    check_licenses()
    sdk = destination / "msbuild-evaluate/sdk"
    run([dotnet, "publish", PROJECT, "--no-restore", "-c", "Release", "-f", "net8.0",
         "-o", sdk], ROOT, timeout=300)
    if host == "windows":
        selected = selected_windows(strict=False)
        run([selected / "MSBuild.exe", PROJECT, "-nologo", "-t:Build", "-p:Configuration=Release",
             "-p:TargetFramework=net472", "-p:OutputPath=" + str(destination / "msbuild-evaluate/framework") + os.sep,
             "-p:AppendTargetFrameworkToOutputPath=false"], ROOT, timeout=300)
    check_distribution(destination, host)
    print("Prepared distribution:", destination)


def msbuild_runtime_closure():
    """Assembly names that must never sit app-local beside the companion.

    Derived from the locked dependency graph so a new transitive Microsoft.Build
    dependency is covered automatically, unioned with the explicit names that graph does
    not list at this version.
    """
    locks = json.loads(PROJECT.with_name("packages.lock.json").read_text())["dependencies"]
    graph = {}
    for framework in locks.values():
        for name, info in framework.items():
            graph.setdefault(name.lower(), set()).update(
                dependency.lower() for dependency in (info.get("dependencies") or {}))
    seen, pending = set(), ["microsoft.build", "microsoft.build.framework"]
    while pending:
        package = pending.pop()
        if package in seen:
            continue
        seen.add(package)
        pending.extend(graph.get(package, ()))
    # Microsoft.Build.Locator is the hosting shim and is intentionally bundled.
    seen.discard("microsoft.build.locator")
    return {package + ".dll" for package in seen} | {
        "microsoft.build.dll", "microsoft.build.framework.dll",
        "microsoft.build.utilities.core.dll", "microsoft.build.tasks.core.dll"}


def check_distribution(destination, host):
    sdk = destination / "msbuild-evaluate/sdk"
    for suffix in [".dll", ".deps.json", ".runtimeconfig.json"]:
        require((sdk / (ASSEMBLY + suffix)).is_file(), f"Missing SDK companion artifact: {suffix}")
    config = json.loads((sdk / (ASSEMBLY + ".runtimeconfig.json")).read_text())
    require(config["runtimeOptions"].get("rollForward") == "LatestMajor", "SDK worker must permit LatestMajor")
    if host == "windows":
        require((destination / "msbuild-evaluate/framework" / (ASSEMBLY + ".exe")).is_file(),
                "Missing Framework companion executable")
    forbidden = msbuild_runtime_closure()
    for path in (destination / "msbuild-evaluate").rglob("*"):
        require(path.name.lower() not in forbidden, f"Bundled MSBuild runtime is forbidden: {path}")


def normalize(path):
    return os.path.normcase(str(Path(path).resolve()))


def qualify(destination, host, installed_only):
    check_distribution(destination, host)
    dotnet, sdk_path = selected_sdk()
    selected = selected_windows() if host == "windows" else sdk_path
    direct = [selected / "MSBuild.exe"] if host == "windows" else [dotnet, selected / "MSBuild.dll"]
    # Copy only packaged output + authored inputs. Worker cwd/argv/requests contain no checkout path.
    with tempfile.TemporaryDirectory(prefix="tethys installed ü ") as temporary:
        clean = Path(temporary).resolve()
        shutil.copytree(destination / "msbuild-evaluate", clean / "msbuild-evaluate")
        workspace = clean / "workspace"
        shutil.copytree(ROOT / "tests/fixtures/msbuild/evaluator", workspace)
        command = ([clean / "msbuild-evaluate/framework" / (ASSEMBLY + ".exe")] if host == "windows"
                   else [dotnet, clean / "msbuild-evaluate/sdk" / (ASSEMBLY + ".dll")])
        require(all(str(ROOT) not in str(arg) for arg in command), "Worker launch leaked checkout path")

        def request(project, tfm=None, **overrides):
            value = dict(protocol_version=1, workspace_root=str(workspace), project_path=str(project),
                         target_framework=tfm, global_properties=GLOBALS, msbuild_path=str(selected), trust_granted=True)
            value.update(overrides)
            return value

        def worker(value, success=True):
            # A worker launch loads the whole SDK import closure; 60s is a probe budget,
            # not a cold-runner evaluation budget.
            status, text, _ = run(command, clean, json.dumps(value).encode("utf-8"),
                                  allow_failure=True, timeout=300)
            result = json.loads(text)
            require(result["protocol_version"] == 1, "C14 incompatible response protocol")
            require(secret_name not in result["properties"], "Ambient credential copied into properties")
            require(result["success"] is success, f"C5 unexpected evaluation outcome: {result}")
            require(not success or status == 0, f"C5 successful response with failure exit: {status}")
            if not success:
                require(result["diagnostics"] and not result["cache_eligible"], "Failure lost diagnostics/cache exclusion")
                require(status != 0, f"C5 failure response with success exit: {status}")
            return result

        secret_name = "TETHYS_QUALIFICATION_SECRET"
        os.environ[secret_name] = "ambient-credential-must-not-be-serialized"
        def query(project, tfm=None, target=None):
            globals_ = dict(GLOBALS)
            if tfm is not None:
                globals_["TargetFramework"] = tfm
            args = direct + [project, "-nologo", "-verbosity:quiet", "-nodeReuse:false"]
            args += [f"-p:{key}={value}" for key, value in globals_.items()]
            if target:
                run(args + ["-t:" + target], clean, timeout=300)
                return None
            _, text, _ = run(args + ["-getProperty:" + ",".join(PROPERTIES),
                                     "-getItem:" + ",".join(ITEMS)], clean, timeout=300)
            return json.loads(text)

        def identity(result, oracle):
            actual = result["host"]
            require(actual["kind"] == ("framework" if host == "windows" else "sdk"), "C14 wrong host kind")
            require(normalize(actual["path"]) == normalize(selected), "C14 selected host was substituted")
            require(normalize(oracle["Properties"]["MSBuildBinPath"]) == normalize(selected), "Oracle host drift")
            require(actual["version"] == oracle["Properties"]["MSBuildFileVersion"], "C14 actual host version mismatch")
            require(actual["runtime"] and ((".NET Framework" in actual["runtime"]) == (host == "windows")),
                    "C14 wrong loaded runtime")
            print("Loaded host:", json.dumps(actual))

        def items_agree(result, oracle):
            require(set(result["items"]) == set(ITEMS), "Wrong item wire shape")
            for kind in ITEMS:
                actual = result["items"][kind]
                reference = oracle["Items"][kind]
                require(sorted((x["include"], normalize(x["full_path"])) for x in actual) ==
                        sorted((x["Identity"], normalize(x["FullPath"])) for x in reference), f"C5 {kind} differs from MSBuild")
                by_identity = {x["Identity"]: x for x in reference}
                for item in actual:
                    for key, value in by_identity[item["include"]].items():
                        # Filesystem timestamps are not contract metadata and would make
                        # both the response and this comparison non-reproducible.
                        if key in {"Identity", "ModifiedTime", "CreatedTime", "AccessedTime"}:
                            continue
                        require(item["metadata"].get(key) == value, f"C5 {kind} metadata mismatch: {key}")

        literal = workspace / "Literal/Literal.csproj"
        result = worker(request(literal))
        oracle = query(literal)
        identity(result, oracle)
        items_agree(result, oracle)
        package = {item["include"]: item["metadata"] for item in result["items"]["PackageReference"]}
        require(package["Qualification.Dependency"]["Version"] == "2.3.4" and
                package["Qualification.Dependency"]["PrivateAssets"] == "all" and
                package["Qualification.Central"]["VersionOverride"] == "4.5.6",
                "C5 evaluated restore dependency metadata lost")
        require(result["items"]["PackageVersion"][0]["metadata"]["Version"] == "4.0.0" and
                result["items"]["PackageDownload"][0]["metadata"]["Version"] == "[1.2.3]",
                "C5 central/download dependency metadata lost")
        require([normalize(x["full_path"]) for x in result["items"]["Compile"]] == [normalize(literal.parent / "Keep.cs")],
                "Literal fixture membership mismatch")
        require(result["cache_eligible"] and not result["cache_ineligibility"], "Qualified literal recipe must be eligible")
        require(not ({"ModifiedTime", "CreatedTime", "AccessedTime"}
                     & set(result["items"]["Compile"][0]["metadata"])),
                "Filesystem timestamps are not contract metadata and break response reproducibility")
        patterns = {(x["include"], x["exclude"], x["remove"]) for x in result["glob_patterns"]
                    if x["item_type"] == "Compile" and normalize(x["project_path"]) == normalize(literal)}
        require(patterns == {("*.cs", "Excluded.cs", ""), ("", "", "Removed.cs")},
                "Literal recipe lacks complete include/exclude/remove provenance")
        worker(request(literal, protocol_version=999), success=False)
        untrusted = worker(request(workspace / "Failures/MissingImport.csproj", trust_granted=False), success=False)
        require(untrusted["host"] is None and not untrusted["imports"], "Untrusted input loaded MSBuild/project")
        worker(request(literal, msbuild_path=str(clean / "missing-host")), success=False)
        outside = worker(request(literal, project_path=str(ROOT / "Cargo.toml")), success=False)
        require(any("workspace_root" in (x["message"] or "") for x in outside["diagnostics"]),
                "A project outside workspace_root was not refused")
        failure = worker(request(workspace / "Failures/MissingImport.csproj"), success=False)
        require(any(x["code"] == "MSB4019" and x["exception_type"] == "Microsoft.Build.Exceptions.InvalidProjectFileException"
                    for x in failure["diagnostics"]), "Native missing-import code/type lost")
        unknown = worker(request(workspace / "Failures/UnknownFunction.csproj"))
        require(not unknown["cache_eligible"] and unknown["cache_ineligibility"],
                "Unknown file-reading property function was claimed pure")
        missing = clean / "missing-companion" / (ASSEMBLY + (".exe" if host == "windows" else ".dll"))
        try:
            status, _, _ = run([missing] if host == "windows" else [dotnet, missing], clean, allow_failure=True)
            require(status != 0, "Missing companion falsely succeeded")
        except FileNotFoundError:
            pass
        if installed_only:
            print("C14 PASS: clean distribution, exact host, protocol and missing-prerequisite controls")
            return

        manifest = json.loads((workspace / "expected.json").read_text(encoding="utf-8"))
        if host == "sdk":
            outer_project = workspace / "SDK Space ü/App.csproj"
            outer = worker(request(outer_project))
            outer_oracle = query(outer_project)
            require(outer["properties"]["TargetFrameworks"] == "net8.0;net9.0" ==
                    outer_oracle["Properties"]["TargetFrameworks"], "C5 outer frameworks were not enumerated exactly")
            require(outer["properties"]["TargetFramework"] == "", "C5 outer evaluation fabricated an inner framework")
        for unit in manifest["units"]:
            # Classic qualification is authoritative on VS, not an SDK-only approximation.
            if host == "sdk" and unit["tfm"] is None:
                continue
            if host == "windows" and unit["tfm"] is not None:
                continue
            project = workspace / unit["project"]
            sentinel = project.parent / "target-sentinel.txt"
            if sentinel.exists():
                sentinel.unlink()
            result = worker(request(project, unit["tfm"]))
            oracle = query(project, unit["tfm"])
            identity(result, oracle)
            properties = result["properties"]
            for name in PROPERTIES:
                require(properties.get(name, "") == oracle["Properties"][name],
                        f"C5 property differs from MSBuild: {name}: worker={properties.get(name)!r}, oracle={oracle['Properties'][name]!r}")
            expected = {"TargetFrameworkIdentifier": unit["identifier"], "TargetFrameworkVersion": unit["version"],
                        "TargetFrameworkProfile": unit["profile"], "AssemblyName": "Collision", "LangVersion": unit["lang"],
                        "QualificationOverride": "caller value", "Configuration": "Qualification", "VSToolsPath": ""}
            for name, value in expected.items():
                require(properties.get(name, "") == value, f"C5 handwritten property mismatch: {name}: {properties.get(name)}")
            require(unit["define"] in properties["DefineConstants"].split(";"), "C5 inner framework defines lost")
            require(not result["cache_eligible"] and result["cache_ineligibility"],
                    "Unknown SDK/import closure was claimed cache-eligible")
            items_agree(result, oracle)
            sources = result["items"]["Compile"]
            expected_sources = manifest["common_sources"] + unit["extra"]
            require(sorted(normalize(x["full_path"]) for x in sources) ==
                    sorted(normalize(project.parent / x) for x in expected_sources), "C5 handwritten Compile set mismatch")
            for relative, link in manifest["links"].items():
                item = next(x for x in sources if normalize(x["full_path"]) == normalize(project.parent / relative))
                require(item["metadata"]["Link"] == link, "C5 exact Link spelling was changed")
            refs = result["items"]["ProjectReference"]
            require(len(refs) == 1 and normalize(refs[0]["full_path"]) == normalize(workspace / "Twin/Twin.csproj")
                    and refs[0]["metadata"]["ReferenceOutputAssembly"] == "False", "C5 declared project reference lost")
            assemblies = [x for x in result["items"]["Reference"] if x["include"] == "Hand.Reference"]
            require(len(assemblies) == 1 and assemblies[0]["metadata"]["HintPath"] == "../shared/Hand.Reference.dll",
                    "C5 handwritten assembly-reference metadata mismatch")
            require(normalize(workspace / "shared/Imported.props") in [normalize(x) for x in result["imports"]],
                    "C5 imported input missing")
            require(all(Path(x).is_absolute() for x in result["imports"]), "Imports are not absolute")
            require(not sentinel.exists(), "C5 evaluation executed a target")
            query(project, unit["tfm"], target="QualificationSentinel")
            require(sentinel.read_text().strip() == "target ran", "Sentinel positive control did not execute")
            sentinel.unlink()
            print("C5 unit PASS:", unit["project"], unit["tfm"])
        twin = worker(request(workspace / "Twin/Twin.csproj"))
        require(twin["properties"]["AssemblyName"] == "Collision" and
                [normalize(x["full_path"]) for x in twin["items"]["Compile"]] == [normalize(workspace / "Twin/Twin.cs")],
                "C5 colliding assembly-name project was conflated")
        print("C5/C14 PASS: independent manifests + direct MSBuild, no targets, clean distribution")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", choices=["sdk", "windows"], required=True)
    parser.add_argument("--distribution", type=Path, default=Path(os.environ.get("TETHYS_EVALUATOR_DISTRIBUTION", ROOT / "target/worker-dist")))
    parser.add_argument("--prepare", action="store_true", help="explicitly restore locked dependencies, check licenses, and package")
    parser.add_argument("--installed-only", action="store_true", help="C14 installation/host/protocol checks only")
    args = parser.parse_args()
    destination = args.distribution.resolve()
    if args.prepare:
        prepare(destination, args.host)
    else:
        qualify(destination, args.host, args.installed_only)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, KeyError) as error:
        print(f"C5/C14 FAIL: {error}", file=sys.stderr)
        sys.exit(1)
