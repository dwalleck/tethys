#!/usr/bin/env python3
"""Pinned public-only C12 acquisition, independent capture and reviewed qualification.

Capture is deliberately not approval. Review project and enumerated-unit standing
against direct-host metadata, native restore results and prepared asset currentness.
Ordinary Restore runs only with --allow-restore; ordinary builds are never invoked.
"""
import argparse
import base64
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import signal
import shutil
import time
import sys
from qualification_environment import (controlled_environment, environment_root,
                                       freeze_environment, verify_environment, ancestor_preflight, environment_fingerprint, windows_folders)


ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "tests/fixtures/msbuild/qualification/corpus.json"
PINS = {
    "avalonia": ("AvaloniaUI/Avalonia", "11.3.7", "0834dbbbb9252406b08f2e74e8f328cc5ba502ee"),
    "xunit": ("xunit/xunit", "v3-3.2.2", "728c1dce012cd82193035dddfeaba184baaa88c6"),
    "newtonsoft-json": ("JamesNK/Newtonsoft.Json", "13.0.4", "4e13299d4b0ec96bd4df9954ef646bd2d1b5bf2a"),
    "msbuild": ("dotnet/msbuild", "v18.8.2", "ce25c01082c9c46cd02ad1ff3ff8f16fe5cc2f44"),
}
PROPERTIES = ("TargetFramework TargetFrameworks TargetFrameworkIdentifier TargetFrameworkVersion "
              "TargetFrameworkProfile TargetFrameworkMoniker TargetPlatformIdentifier TargetPlatformVersion "
              "Platform PlatformTarget Configuration AssemblyName RootNamespace DefineConstants LangVersion "
              "Nullable AllowUnsafeBlocks OutputType OutputPath TargetPath TargetFileName MSBuildProjectFullPath "
              "MSBuildProjectDirectory MSBuildProjectExtensionsPath ProjectAssetsFile RestoreProjectStyle "
              "NuGetPackageRoot RestorePackagesPath RestorePackagesConfig RestoreRepositoryPath RestoreConfigFile ManagePackageVersionsCentrally "
              "BaseIntermediateOutputPath IntermediateOutputPath RuntimeIdentifier RuntimeIdentifiers VSToolsPath "
              "UsingMicrosoftNETSdk MSBuildToolsVersion MSBuildVersion MSBuildFileVersion NETCoreSdkVersion "
              "MSBuildRuntimeType MSBuildBinPath").split()
ITEMS = ["Compile", "ProjectReference", "Reference", "PackageReference", "PackageDownload", "PackageVersion"]
QUERY = ["-nologo", "-verbosity:quiet", "-getProperty:" + ",".join(PROPERTIES),
         "-getItem:" + ",".join(ITEMS), "-p:Configuration=Debug", "-p:Platform=AnyCPU", "-p:VSToolsPath="]
RESTORE_QUERY = ["-nologo", "-verbosity:quiet", "-t:Restore", "-getItem:_RestoreGraphEntryFiltered"]


def require(condition, message):
    if not condition:
        raise RuntimeError("C12 corpus: " + message)


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def git(root, *args, binary=False):
    executable = shutil.which("git")
    require(executable is not None, "Git executable is required")
    environment = {"GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_SYSTEM": os.devnull,
                   "GIT_CONFIG_GLOBAL": os.devnull, "GIT_TERMINAL_PROMPT": "0",
                   "GIT_ASKPASS": os.devnull, "SSH_ASKPASS": os.devnull,
                   "GIT_ALLOW_PROTOCOL": "https", "GCM_INTERACTIVE": "never",
                   "LANG": "C", "LC_ALL": "C"}
    if os.name == "nt":
        folders = windows_folders()
        environment.update({name: str(folders[name]) for name in
                            ("SystemRoot", "PROGRAMFILES", "PROGRAMFILES(X86)", "PROGRAMDATA")})
        environment["PATH"] = os.pathsep.join((str(Path(executable).parent),
                                               str(folders["SystemRoot"] / "System32")))
    else:
        environment["PATH"] = os.pathsep.join((str(Path(executable).parent), "/usr/bin", "/bin"))
    command = [executable, "-c", "core.hooksPath=" + os.devnull, "-c", "credential.helper=",
               "-c", "credential.interactive=false", "-c", "core.askPass=" + os.devnull,
               "-c", "http.extraHeader=", "-c", "http.followRedirects=false",
               "-c", "protocol.allow=never", "-c", "protocol.https.allow=always",
               "-c", "protocol.ssh.allow=never", "-c", "protocol.file.allow=never",
               "-c", "submodule.recurse=false", "-C", str(root), *args]
    # Empty HOME also prevents libcurl's optional netrc lookup from reaching the
    # user's credentials. Keep scratch on the caller-selected filesystem.
    with tempfile.TemporaryDirectory(prefix="git-public-", dir=Path(root).resolve().parent) as home:
        environment.update({name: home for name in
                            ("HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "TMPDIR", "TMP", "TEMP", "GIT_TEMPLATE_DIR")})
        result = subprocess.run(command, env=environment, check=True, capture_output=True, timeout=60)
    return result.stdout if binary else result.stdout.decode("utf-8")


def load_manifest(path=None):
    manifest = json.loads(Path(path or MANIFEST).read_text(encoding="utf-8"))
    require(manifest.get("schema") == 1, "unknown manifest schema")
    require(manifest.get("restore_observation_argv") == RESTORE_QUERY,
            "restore observation must preserve the reviewed ordinary Restore operation")
    require(manifest.get("restore_currentness_schema") == 1,
            "manifest lacks the reviewed native currentness evidence schema")
    entries = manifest["repositories"]
    require(len(entries) == len(PINS) and {x["id"] for x in entries} == set(PINS), "exact four-repository roster required")
    for entry in entries:
        repo, tag, sha = PINS[entry["id"]]
        require((entry["url"], entry["tag"], entry["sha"]) ==
                (f"https://github.com/{repo}.git", tag, sha), "unapproved public pin")
        require(entry["oracle_argv"] == QUERY, "only reviewed evaluation-only host operation is allowed")
        require(entry["submodule_policy"] == "pinned-gitlinks-uninitialized-no-recursion", "unreviewed submodule policy")
        require(entry["properties"] == {"Configuration": "Debug", "Platform": "AnyCPU", "VSToolsPath": ""}, "context drift")
    return entries


def acquire(entry, destination):
    require(entry["id"] in PINS and entry["url"] == f"https://github.com/{PINS[entry['id']][0]}.git",
            "acquisition only permits fixed approved public HTTPS repositories")
    require(not destination.exists(), "fresh isolated destination required")
    destination.mkdir(parents=True)
    git(destination, "init", "--quiet")
    git(destination, "remote", "add", "origin", entry["url"])
    git(destination, "fetch", "--quiet", "--depth=1", "--no-recurse-submodules", "origin",
        f"refs/tags/{entry['tag']}:refs/tags/{entry['tag']}")
    require(git(destination, "rev-parse", f"refs/tags/{entry['tag']}^{{commit}}").strip() == entry["sha"], "tag no longer resolves to approved SHA")
    git(destination, "checkout", "--quiet", "--detach", entry["sha"])
    return verify_source(entry, destination)


def verify_source(entry, root, *, indexed=False, prepared=None):
    require(git(root, "rev-parse", "HEAD").strip() == entry["sha"], "checkout SHA drift")
    require(git(root, "remote", "get-url", "origin").strip() == entry["url"], "origin drift")
    for relative in (entry["sdk_context"]["path"], entry["license"]["path"]):
        path = (root / relative).resolve(strict=True)
        require(path.is_relative_to(root.resolve()) and path.is_file(), "source authority escapes checkout")
    declared_sdk = json.loads((root / entry["sdk_context"]["path"]).read_text(encoding="utf-8-sig"))
    require(declared_sdk == entry["sdk_context"]["global_json"], "declared SDK context drift")
    status = git(root, "status", "--porcelain=v1", "-z", "--untracked-files=all", "--ignored=no")
    # ls-files expands ignored directories to their actual leaves; status's
    # directory summaries would hide unrelated tracker/source dirt.
    ignored = git(root, "ls-files", "--others", "--ignored", "--exclude-standard", "-z")
    status += "".join("!! " + name + "\0" for name in ignored.split("\0") if name)
    generated = {".rivets/index/tethys.db" + suffix for suffix in ("", "-wal", "-shm")}
    restore_inputs = {item["path"]: item for item in (prepared or {}).get("inputs", [])}
    for name, item in restore_inputs.items():
        path = root / name
        require(path.is_file() and not path.is_symlink() and
                hashlib.sha256(path.read_bytes()).hexdigest() == item["sha256"],
                "prepared restore input changed: " + name)
    for record in status.split("\0"):
        if not record:
            continue
        name = record[3:].replace("\\", "/")
        require(record[:2] in ("??", "!!") and
                ((indexed and name in generated) or name in restore_inputs), f"source dirt: {record}")
    tree = git(root, "ls-tree", "-r", "-z", "HEAD")
    projects, gitlinks = [], []
    for record in tree.split("\0"):
        if not record:
            continue
        header, name = record.split("\t", 1)
        mode, kind, sha = header.split()
        if mode == "160000":
            path = root / name
            require(not path.exists() or (path.is_dir() and not any(path.iterdir())), "initialized submodule outside approved corpus scope")
            gitlinks.append({"path": name, "sha": sha})
        elif name.lower().endswith(".csproj"):
            require(mode != "120000", "symlink project requires separately reviewed authority")
            projects.append(name)
    require(projects, "empty project inventory")
    candidates = git(root, "ls-files", "--cached", "--others", "--exclude-standard", "-z")
    physical_sources = set()
    for name in candidates.split("\0"):
        if not name.lower().endswith(".cs"):
            continue
        physical = (root / name).resolve(strict=True)
        require(physical.is_relative_to(root.resolve()) and physical.is_file(),
                f"physical C# source escapes checkout or is not a file: {name}")
        physical_sources.add(physical.relative_to(root.resolve()).as_posix())
    physical_sources = sorted(physical_sources)
    return {"sha": entry["sha"], "projects": sorted(projects), "gitlinks": gitlinks,
            "physical_sources": physical_sources, "physical_source_count": len(physical_sources),
            "physical_source_digest": digest(physical_sources)}


def normalize(value, root, sdk, *, metadata_key=None, environment=None, host=None):
    if isinstance(value, str):
        paths = [(root, "$CORPUS"), (sdk, "$SDK")]
        if host is not None and host != sdk:
            paths.append((host, "$HOST"))
        paths.extend((Path(environment[name]), "$" + name) for name in
                     ("HOME", "DOTNET_CLI_HOME", "NUGET_PACKAGES", "NUGET_HTTP_CACHE_PATH", "TMPDIR",
                      "APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "NUGET_COMMON_APPLICATION_DATA")
                     if environment and environment.get(name))
        paths.sort(key=lambda pair: len(str(pair[0])), reverse=True)
        if metadata_key == "Directory":
            # MSBuild's built-in Directory excludes RootDir (drive or slash).
            # Only that metadata key and an exact directory prefix may use this
            # normalization; similarly spelled semantic strings are untouched.
            directory = value.replace("\\", "/")
            for base, token in paths:
                prefix = base.relative_to(base.anchor).as_posix().rstrip("/")
                if directory == prefix or directory.startswith(prefix + "/"):
                    return token + directory[len(prefix):]
        for base, token in paths:
            value = value.replace(str(base), token)
        return value.replace("\\", "/")
    if isinstance(value, list):
        return [normalize(x, root, sdk, environment=environment, host=host) for x in value]
    if isinstance(value, dict):
        return {normalize(k, root, sdk, environment=environment, host=host):
                normalize(v, root, sdk, metadata_key=k, environment=environment, host=host) for k, v in value.items()
                if k not in ("ModifiedTime", "CreatedTime", "AccessedTime")}
    return value


def host_query(command, root, environment):
    # File-backed pipes avoid unbounded in-memory capture; poll enforces both
    # output caps and the same 60-second invocation deadline as the product.
    with tempfile.TemporaryFile(dir=environment["TMPDIR"]) as stdout, tempfile.TemporaryFile(dir=environment["TMPDIR"]) as stderr:
        process = subprocess.Popen(command, cwd=root, env=environment, stdin=subprocess.DEVNULL,
                                   stdout=stdout, stderr=stderr, start_new_session=os.name != "nt")
        started = time.monotonic()
        try:
            while True:
                require(time.monotonic() - started <= 60, "direct-host evaluation deadline exceeded")
                require(os.fstat(stdout.fileno()).st_size <= 64 * 1024 * 1024 and
                        os.fstat(stderr.fileno()).st_size <= 1024 * 1024, "direct-host output overflow")
                if process.poll() is not None:
                    break
                time.sleep(0.01)
        finally:
            if os.name == "nt":
                if process.poll() is None:
                    subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                                   check=True, capture_output=True, timeout=10)
            else:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            process.wait(timeout=10)
        stdout.seek(0)
        stderr.seek(0)
        return process.returncode, stdout.read().decode("utf-8-sig"), stderr.read().decode("utf-8-sig")


def selected_host(sdk, host=None, host_kind="sdk"):
    require(host_kind in ("sdk", "framework"), "unknown host kind")
    require(host_kind != "framework" or (os.name == "nt" and host is not None),
            "framework host requires actual Windows and explicit VS MSBuild directory")
    host = Path(host or sdk).resolve(strict=True)
    require(host_kind != "sdk" or host == sdk, "SDK host must match controlled-environment SDK")
    require((host / ("MSBuild.exe" if host_kind == "framework" else "MSBuild.dll")).is_file(),
            "selected host binary is absent")
    return host


def host_identity(sdk, host, host_kind):
    binary = "MSBuild.exe" if host_kind == "framework" else "MSBuild.dll"
    return {"kind": host_kind, "platform": sys.platform, "path": str(host), "binary": binary, "sdk": str(sdk),
            "msbuild_sha256": hashlib.sha256((host / binary).read_bytes()).hexdigest(),
            "sdk_msbuild_sha256": hashlib.sha256((sdk / "MSBuild.dll").read_bytes()).hexdigest()}


def host_prefix(dotnet, host, host_kind):
    return [str(host / "MSBuild.exe")] if host_kind == "framework" else [str(dotnet), str(host / "MSBuild.dll")]


def direct_query(root, project, framework, dotnet, sdk, environment, *, host=None, host_kind="sdk"):
    require(environment is not None, "direct host requires explicit controlled environment")
    host = selected_host(sdk, host, host_kind)
    command = host_prefix(dotnet, host, host_kind) + [str(root / project), *QUERY]
    if framework is not None:
        require(re.fullmatch(r"[A-Za-z0-9_.+-]+", framework) is not None, "unsafe framework selector")
        command.append("-p:TargetFramework=" + framework)
    code, stdout, stderr = host_query(command, root, environment)
    record = {"project": project, "target_framework": framework, "exit_code": code,
              "stderr": normalize(stderr, root, sdk, environment=environment, host=host)}
    if code == 0:
        payload = json.loads(stdout)
        require(set(payload) >= {"Properties", "Items"}, "direct host did not return metadata")
        require(Path(payload["Properties"]["MSBuildBinPath"]).resolve() == host, "direct query substituted selected host")
        require(payload["Properties"]["MSBuildRuntimeType"] == ("Full" if host_kind == "framework" else "Core"),
                "direct query loaded wrong host runtime")
        record["metadata"] = normalize(payload, root, sdk, environment=environment, host=host)
    else:
        record["stdout"] = normalize(stdout, root, sdk, environment=environment, host=host)
        record["native_codes"] = sorted(set(re.findall(r"\b(?:MSB|NETSDK|NU)\d{4}\b", stdout + stderr)))
    return record


def restore_graph_projection(stdout, root, sdk, environment, host):
    """Observe post-Restore items without scheduling any additional target."""
    try:
        payload = json.loads(stdout)
    except json.JSONDecodeError:
        return {"status": "unavailable-non-json-output", "items": None}
    if not isinstance(payload, dict) or not isinstance(payload.get("Items"), dict):
        return {"status": "missing-items-object", "items": None}
    items = payload["Items"].get("_RestoreGraphEntryFiltered")
    if items is None:
        return {"status": "missing-restore-graph", "items": None}
    require(isinstance(items, list) and all(isinstance(item, dict) for item in items),
            "malformed post-Restore graph items")
    projected = []
    guid = re.compile(r"[0-9a-fA-F]{8}-(?:[0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}")
    identity_metadata = {"Identity", "FullPath", "RootDir", "Filename", "Extension",
                         "RelativeDir", "Directory", "RecursiveDir"}
    for item in items:
        # Restore graph rows are often synthetic GUID items. Only their built-in
        # identity-derived metadata is removed; actual Type, Id, VersionRange,
        # project/framework bindings and all other dependency metadata survive.
        synthetic = isinstance(item.get("Identity"), str) and guid.fullmatch(item["Identity"])
        semantic = {key: value for key, value in item.items()
                    if not (synthetic and key in identity_metadata)}
        projected.append(normalize(semantic, root, sdk, environment=environment, host=host))
    projected.sort(key=lambda item: json.dumps(item, sort_keys=True, separators=(",", ":")))
    return {"status": "observed" if projected else "empty", "items": projected}


def restore_artifact(name):
    return name in ("project.assets.json", "project.nuget.cache") or name.endswith(
        (".nuget.g.props", ".nuget.g.targets", ".nuget.dgspec.json"))


def native_input(path, root, sdk, environment, host):
    content = path.read_bytes()
    text = content.decode("utf-8-sig")
    document = json.loads(text) if path.suffix == ".json" or path.name == "project.nuget.cache" else text
    normalized = normalize(document, root, sdk, environment=environment, host=host)
    canonical = dict(normalized) if path.name == "project.nuget.cache" else normalized
    if path.name == "project.nuget.cache":
        canonical.pop("dgSpecHash", None)
    projection = normalized
    scope = "complete native JSON" if isinstance(document, dict) else "complete native XML text"
    if path.name == "project.assets.json":
        require(isinstance(normalized, dict), "assets document is not an object")
        projection = {key: value for key, value in normalized.items() if key not in ("targets", "libraries")}
        projection["libraries"] = {
            key: {**{name: value for name, value in library.items() if name != "files"},
                  "files_manifest": {"count": len(library.get("files", [])), "sha256": digest(library.get("files", []))}}
            for key, library in normalized.get("libraries", {}).items()}
        projection["targets"] = {
            framework: {identity: {key: value for key, value in item.items() if key in ("type", "dependencies")}
                        for identity, item in target.items()}
            for framework, target in normalized.get("targets", {}).items()}
        scope = ("native assets project/restore/framework/dependency/log sections; library identity/hash/path plus file-list digest; "
                 "target identity/type/dependencies; excludes compile/runtime/content/build file tables")
    require(not isinstance(projection, str) or len(content) <= 2 * 1024 * 1024,
            "generated XML exceeds bounded reviewable evidence size")
    return {"path": path.relative_to(root).as_posix(), "sha256": hashlib.sha256(content).hexdigest(),
            "bytes": len(content), "canonical_sha256": digest(canonical),
            "native_projection": projection, "projection_scope": scope}, document


def snapshot_restore_inputs(root, directories, sdk, environment, host):
    rows = []
    for directory in sorted(directories):
        path = root / directory
        if not path.exists():
            continue
        require(path.resolve().is_relative_to(root.resolve()) and not path.is_symlink(), "restore input directory escapes workspace")
        for child in sorted(path.iterdir()):
            if child.is_file() and restore_artifact(child.name):
                require(not child.is_symlink(), "restore input is symlinked")
                record, _ = native_input(child, root, sdk, environment, host)
                rows.append({key: record[key] for key in ("path", "sha256", "canonical_sha256")})
    return rows


def package_observations(documents, records, root, sdk, environment, host):
    """Observe final installed bytes, not timestamps or existence-as-currentness."""
    cache, retained, packages, expected_files = {}, {}, [], []
    allowed = (root.resolve(), sdk.resolve(), environment_root(environment))
    def observe(value, retain=True):
        path = Path(value)
        key = normalize(str(path), root, sdk, environment=environment, host=host)
        if key not in cache:
            physical = path.resolve()
            row = {"path": key, "status": "missing"}
            if not any(physical.is_relative_to(base) for base in allowed):
                row["status"] = "outside-approved-input-roots"
            elif path.is_file():
                sha256, sha512, size = hashlib.sha256(), hashlib.sha512(), 0
                with path.open("rb") as stream:
                    while block := stream.read(1024 * 1024):
                        sha256.update(block)
                        sha512.update(block)
                        size += len(block)
                row.update(status="observed-file", bytes=size, sha256=sha256.hexdigest(),
                           sha512_base64=base64.b64encode(sha512.digest()).decode())
                if size <= 16384 and path.name.endswith((".sha512", ".metadata")):
                    row["native_text"] = path.read_text(encoding="utf-8-sig")
            elif path.exists():
                row["status"] = "not-a-file"
            cache[key] = row
        if retain:
            retained[key] = cache[key]
        return cache[key]
    for source, document in documents:
        if source.endswith("/project.nuget.cache") or source == "project.nuget.cache":
            for value in document.get("expectedPackageFiles", []):
                observation = observe(value)
                expected_files.append({"input": source, "path": observation["path"]})
                if value.endswith(".nupkg.sha512"):
                    observe(value[:-len(".sha512")])
        if not source.endswith("project.assets.json"):
            continue
        for identity, library in document.get("libraries", {}).items():
            if library.get("type") != "package":
                continue
            candidates = []
            for folder in document.get("packageFolders", {}):
                package = Path(folder) / library["path"]
                archive = package / (library["path"].replace("\\", "/").replace("/", ".") + ".nupkg")
                marker = observe(str(archive) + ".sha512")
                archive_observation = observe(str(archive))
                metadata = observe(str(package / ".nupkg.metadata"))
                inventory, missing, states = hashlib.sha256(), [], {}
                files = sorted(library.get("files", []))
                for name in files:
                    row = observe(str(package / name), retain=False)
                    states[row["status"]] = states.get(row["status"], 0) + 1
                    if row["status"] != "observed-file":
                        missing.append(name)
                    inventory.update(json.dumps([name, row], sort_keys=True, separators=(",", ":")).encode() + b"\n")
                candidates.append({"root": normalize(str(package), root, sdk, environment=environment, host=host),
                                   "archive": archive_observation["path"], "hash_marker": marker["path"],
                                   "metadata": metadata["path"], "expected_file_count": len(files),
                                   "expected_files_sha256": digest(library.get("files", [])),
                                   "observed_files_sha256": inventory.hexdigest(), "file_states": states, "unavailable_files": missing})
            packages.append({"input": source, "identity": identity, "native_sha512": library.get("sha512"),
                             "native_path": library.get("path"), "candidates": candidates})
    downloads = []
    for index, record in enumerate(records):
        graph = record["restore_graph"].get("items") or []
        roots = {item.get("ProjectUniqueName"): item.get("PackagesPath") for item in graph if item.get("Type") == "ProjectSpec"}
        for item in graph:
            if item.get("Type") != "DownloadDependency":
                continue
            row = {"restore_observation": index, "project": item.get("ProjectUniqueName"),
                   "frameworks": item.get("TargetFrameworks"), "identity": item.get("Id"),
                   "version_range": item.get("VersionRange"), "packages_path": roots.get(item.get("ProjectUniqueName"))}
            version = re.fullmatch(r"\[([0-9A-Za-z][0-9A-Za-z.+-]*)\]", item.get("VersionRange", ""))
            if not version or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", item.get("Id", "")):
                row["status"] = "unavailable-nonexact-native-package-identity"
            elif row["packages_path"] not in ("$NUGET_PACKAGES", "$NUGET_PACKAGES/"):
                row["status"] = "unavailable-native-package-root-not-controlled-cache"
            else:
                identity, selected_version = item["Id"].lower(), version[1].lower()
                package = Path(environment["NUGET_PACKAGES"]) / identity / selected_version
                archive = package / f"{identity}.{selected_version}.nupkg"
                row.update(status="observed-native-download-location",
                           archive=observe(str(archive))["path"],
                           hash_marker=observe(str(archive) + ".sha512")["path"],
                           metadata=observe(str(package / ".nupkg.metadata"))["path"])
            downloads.append(row)
    return {"scope": "final cache expectedPackageFiles, assets packages and project/framework-bound native DownloadDependency locations; marker text, archive hashes and extracted expected-file content inventory",
            "files": sorted(retained.values(), key=lambda row: row["path"]),
            "packages": sorted(packages, key=lambda row: (row["input"], row["identity"])),
            "downloads": downloads,
            "expected_package_files": sorted(expected_files, key=lambda row: (row["input"], row["path"]))}


def prepare_restore(entry, root, projects, dotnet, sdk, environment, *, host, host_kind):
    """Explicitly granted ordinary Restore only; no Build/Compile target."""
    directories = {(Path(project).parent / "obj").as_posix() for project in projects}
    records = []
    for project in projects:
        project_directories = {(Path(project).parent / "obj").as_posix()}
        outer = direct_query(root, project, None, dotnet, sdk, environment, host=host, host_kind=host_kind)
        if outer["exit_code"] == 0:
            value = outer["metadata"]["Properties"]["MSBuildProjectExtensionsPath"]
            if value:
                extensions = ((root / value[len("$CORPUS/"):]) if value.startswith("$CORPUS/") else
                              (root / Path(project).parent / value)).resolve()
                require(extensions.is_relative_to(root.resolve()) and not value.startswith(("$SDK", "$NUGET")),
                        "restore extension directory escapes the prepared workspace")
                directories.add(extensions.relative_to(root.resolve()).as_posix())
                project_directories.add(extensions.relative_to(root.resolve()).as_posix())
        argv = host_prefix(dotnet, host, host_kind) + [str(root / project),
                *RESTORE_QUERY,
                *[f"-p:{key}={value}" for key, value in entry["properties"].items()]]
        code, stdout, stderr = host_query(argv, root, environment)
        graph = restore_graph_projection(stdout, root, sdk, environment, host)
        for item in graph.get("items") or []:
            value = item.get("OutputPath", "")
            if item.get("Type") == "ProjectSpec" and value.startswith("$CORPUS/"):
                directory = (root / value[len("$CORPUS/"):]).resolve()
                require(directory.is_relative_to(root.resolve()), "native graph restore output escapes workspace")
                project_directories.add(directory.relative_to(root.resolve()).as_posix())
        directories.update(project_directories)
        records.append({"project": project, "operation": "Restore", "exit_code": code,
                        "stdout": stdout, "stderr": stderr,
                        "restore_observation_argv": RESTORE_QUERY,
                        "restore_graph": graph, "input_scope": sorted(project_directories),
                        "post_restore_inputs": snapshot_restore_inputs(root, project_directories, sdk, environment, host),
                        "native_codes": sorted(set(re.findall(r"\b(?:MSB|NETSDK|NU)\d{4}\b", stdout + stderr)))})
    # Only exact NuGet-generated assets are admitted; source edits, lock-file
    # changes, arbitrary obj files and unexpected repository writes still fail.
    untracked = git(root, "ls-files", "--others", "--exclude-standard", "-z")
    ignored = git(root, "ls-files", "--others", "--ignored", "--exclude-standard", "-z")
    inputs, documents = [], []
    for name in sorted(set((untracked + ignored).split("\0")) - {""}):
        path = root / name
        require(path.parent.relative_to(root).as_posix() in directories and restore_artifact(path.name),
                "unreviewed restore output: " + name)
        require(path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(root.resolve()),
                "restore output escapes workspace: " + name)
        record, document = native_input(path, root, sdk, environment, host)
        inputs.append(record)
        if path.name == "project.assets.json":
            documents.append((record["path"], {key: document.get(key, {}) for key in ("libraries", "packageFolders")}))
        elif path.name == "project.nuget.cache":
            documents.append((record["path"], {"expectedPackageFiles": document.get("expectedPackageFiles", [])}))
    final = {item["path"]: item for item in inputs}
    for record in records:
        observed = {item["path"]: item for item in record["post_restore_inputs"]}
        scoped_final = {name for name in final if Path(name).parent.as_posix() in record["input_scope"]}
        record["final_input_relation"] = []
        for name in sorted(set(observed) | scoped_final):
            earlier, later = observed.get(name), final.get(name)
            status = ("added-after-this-restore" if earlier is None else "removed-after-this-restore" if later is None else
                      "unchanged" if earlier["sha256"] == later["sha256"] else "overwritten-after-this-restore")
            record["final_input_relation"].append({"path": name, "status": status,
                "post_restore_sha256": earlier["sha256"] if earlier else None,
                "final_sha256": later["sha256"] if later else None,
                "post_restore_canonical_sha256": earlier["canonical_sha256"] if earlier else None,
                "final_canonical_sha256": later["canonical_sha256"] if later else None})
    matching = {(item["path"], item["sha256"]): index for index, record in enumerate(records)
                for item in record["post_restore_inputs"]}
    for item in inputs:
        item["last_matching_restore_observation"] = matching.get((item["path"], item["sha256"]))
    prepared = {"authorized": True, "operation": "ordinary per-project Restore",
                "currentness": {"schema": 1, "state": "observed-final-prepared-workspace-not-a-standing-verdict",
                               "package_observations": package_observations(documents, records, root, sdk, environment, host)},
                "records": records, "inputs": inputs}
    verify_source(entry, root, prepared=prepared)
    return prepared


def stable_restore(prepared):
    return {"authorized": prepared["authorized"], "currentness": prepared.get("currentness"),
            "records": [{**{key: row[key] for key in ("project", "operation", "exit_code", "native_codes", "restore_graph", "input_scope")},
                         "post_restore_inputs": [{key: item[key] for key in ("path", "canonical_sha256")} for item in row["post_restore_inputs"]],
                         "final_input_relation": [{key: item[key] for key in ("path", "status", "post_restore_canonical_sha256", "final_canonical_sha256")}
                                                  for item in row["final_input_relation"]]}
                        for row in prepared["records"]],
            "inputs": [{key: row[key] for key in ("path", "canonical_sha256", "last_matching_restore_observation")}
                       for row in prepared["inputs"]]}


def direct_capture(entry, root, dotnet, sdk, environment, *, bootstrap=False, allow_restore=False, host=None, host_kind="sdk"):
    host = selected_host(sdk, host, host_kind)
    resolver_root = environment_root(environment)
    frozen = None if bootstrap else verify_environment(resolver_root)
    ancestor_configs = ancestor_preflight(root)
    source = verify_source(entry, root)
    prepared = (prepare_restore(entry, root, source["projects"], dotnet, sdk, environment, host=host, host_kind=host_kind) if allow_restore else
                {"authorized": False, "records": [], "inputs": []})
    records = []
    for project in source["projects"]:
        outer = direct_query(root, project, None, dotnet, sdk, environment, host=host, host_kind=host_kind)
        records.append(outer)
        if outer["exit_code"] == 0:
            props = outer["metadata"]["Properties"]
            frameworks = [x.strip() for x in props["TargetFrameworks"].split(";") if x.strip()]
            require(len(frameworks) == len(set(frameworks)), "duplicate framework selectors")
            for framework in frameworks:
                records.append(direct_query(root, project, framework, dotnet, sdk, environment, host=host, host_kind=host_kind))
    verify_source(entry, root, prepared=prepared)
    require(ancestor_preflight(root) == ancestor_configs, "ancestor NuGet config changed during capture")
    if not bootstrap:
        require(verify_environment(resolver_root) == frozen, "resolver state changed during capture")
    return {"schema": 1, "id": entry["id"], "source": source, "context": entry["properties"],
            "environment": frozen,
            "project_count": len(source["projects"]), "direct_evaluation_attempt_count": len(records),
            "restore": prepared,
            "ancestor_configs": ancestor_configs,
            "oracle_argv": QUERY, "host": host_identity(sdk, host, host_kind),
            "records": records}


def stable_direct(capture):
    # Native free-form diagnostics may contain durations/local tool paths. Preserve
    # raw output in capture, compare typed outcome and complete successful metadata.
    return [{k: row[k] for k in ("project", "target_framework", "exit_code", "metadata", "native_codes") if k in row}
            for row in capture["records"]]


def committed_json(repository, path):
    path = path.resolve(strict=True)
    relative = path.relative_to(repository.resolve()).as_posix()
    content = path.read_bytes()
    committed = git(repository, "show", "HEAD:" + relative, binary=True)
    require(content == committed, "authority must be committed, not a working-tree approval")
    return json.loads(gzip.decompress(content) if path.name.endswith(".json.gz") else content)


def expectation_name(entry, host_kind):
    name = entry["expected_standing"]["path"].format(platform=sys.platform, host_kind=host_kind)
    require(Path(name).name == name and name.endswith(".json"), "authority must be adjacent JSON")
    return name


def authority(entry, repository, manifest_path, *, host_kind="sdk"):
    name = expectation_name(entry, host_kind)
    path = manifest_path.parent / name
    require(Path(name).name == name and name.endswith(".json"), "authority must be adjacent JSON")
    require(path.is_file(), f"missing reviewed standing authority: {path}; run capture first")
    expected = committed_json(repository, path)
    require(expected.get("review", {}).get("status") == "reviewed" and
            expected["review"].get("reviewer") and expected["review"].get("rationale"), "unreviewed standing authority")
    require(expected["id"] == entry["id"] and expected["sha"] == entry["sha"], "standing pin drift")
    require(isinstance(expected.get("expected_units"), list) and expected["expected_units"], "missing per-unit reviewed standing")
    require(expected.get("expected_exit") in (0, 1), "missing reviewed indexing exit")
    capture_name = expected["capture_path"]
    require(Path(capture_name).name == capture_name, "capture must be adjacent")
    capture = committed_json(repository, manifest_path.parent / capture_name)
    require(digest(capture) == expected["capture_digest"], "capture digest drift")
    require(capture["id"] == entry["id"] and capture["source"]["sha"] == entry["sha"] and
            capture["context"] == entry["properties"] and capture["oracle_argv"] == QUERY, "capture provenance mismatch")
    prepared = capture["restore"]
    if prepared["authorized"]:
        require(prepared.get("currentness", {}).get("schema") == 1,
                "old restore capture lacks reviewable final-input currentness evidence; recapture required")
        observations = prepared["currentness"].get("package_observations", {})
        require(all(isinstance(observations.get(key), list) for key in
                    ("files", "packages", "downloads", "expected_package_files")), "incomplete installed-package evidence")
        require(all("native_projection" in item and item.get("projection_scope") and
                    "last_matching_restore_observation" in item for item in prepared["inputs"]),
                "restore inputs lack native projections/final-state binding")
        require(all(all(isinstance(row.get(key), list) for key in
                        ("input_scope", "post_restore_inputs", "final_input_relation")) for row in prepared["records"]),
                "restore observations lack later-overwrite evidence")
    project_reviews = expected.get("expected_projects")
    require(isinstance(project_reviews, list) and project_reviews, "missing reviewed project standing")
    require(len({row["project"] for row in project_reviews}) == len(project_reviews) and
            {row["project"] for row in project_reviews} == set(capture["source"]["projects"]),
            "reviewed project inventory differs from independent source inventory")
    slots = set()
    for outer in capture["records"]:
        if outer["target_framework"] is None and outer["exit_code"] == 0:
            props = outer["metadata"]["Properties"]
            selectors = [x.strip() for x in props["TargetFrameworks"].split(";") if x.strip()]
            slots.update((outer["project"], selector) for selector in
                         (selectors or [props["TargetFramework"] or None]))
    for project in project_reviews:
        require(project["standing"] in ("confirmed", "indeterminate") and project.get("rationale"),
                "project standing requires independent review rationale")
        if project["standing"] == "indeterminate":
            require(project.get("reason") and project.get("evidence"), "project failure lacks independent evidence")
        if capture["restore"]["authorized"]:
            require(project.get("restore_input_status") in ("current", "not-required", "missing", "invalid", "unavailable")
                    and project.get("restore_evidence"), "review restore outcomes and asset currentness per project")
            # Measured calls have the same explicit ordinary-Restore grant.
            # Standing/reason must be reviewed against native Restore outcomes;
            # pre-index missing assets alone no longer imply restore-required.
    keys = set()
    for unit in expected["expected_units"]:
        key = (unit["project"], unit["target_framework"])
        require(key in slots, "failed outer evaluation cannot fabricate an evaluation unit")
        require(key not in keys, "duplicate reviewed unit")
        keys.add(key)
        require(unit["standing"] in ("confirmed", "indeterminate"), "invalid standing")
        require(unit.get("rationale"), "every standing needs independent review rationale")
        if unit["standing"] == "indeterminate":
            require(unit.get("reason") and unit.get("evidence"), "Indeterminate needs reason and independent evidence")
    for project in project_reviews:
        if project["standing"] == "confirmed":
            require({key for key in keys if key[0] == project["project"]} ==
                    {key for key in slots if key[0] == project["project"]} and
                    any(key[0] == project["project"] for key in keys),
                    "confirmed project must preserve every independently enumerated unit slot")
    require(any(x["standing"] == "confirmed" for x in expected["expected_units"]),
            "blanket unsupported/failure corpus is not successful qualification")
    return expected, capture


def compare_projects(projects, expected):
    actual = {row["key"]: row["standing"] for row in projects}
    require(len(actual) == len(projects) and set(actual) ==
            {row["project"] for row in expected["expected_projects"]}, "published project roster differs")
    for review in expected["expected_projects"]:
        standing = actual[review["project"]]
        require(standing["standing"] == review["standing"], "project standing drift: " + review["project"])
        if review["standing"] == "indeterminate":
            require(standing["failure"]["reason"] == review["reason"], "project failure reason drift: " + review["project"])


def compare_units(units, expected, capture, root, sdk, environment=None):
    host = Path(capture["host"]["path"])
    for unit in units:
        if unit["host"] is not None:
            require(unit["host"]["kind"] == capture["host"]["kind"] and
                    Path(unit["host"]["path"]).resolve() == host, "product substituted the independently captured host")
    units = normalize(units, root, sdk, environment=environment, host=host)
    def key(row):
        return row["project"], row["target_framework"]
    actual = {key(row): row for row in units}
    require(len(actual) == len(units), "duplicate product unit")
    require(set(actual) == {key(row) for row in expected["expected_units"]}, "reviewed unit inventory differs")
    direct = {key(row): row for row in capture["records"]}
    for review in expected["expected_units"]:
        unit = actual[key(review)]
        standing = unit["standing"]
        require(standing["standing"] == review["standing"], f"standing drift: {key(review)}")
        if review["standing"] == "indeterminate":
            require(standing["failure"]["reason"] == review["reason"], f"failure reason drift: {key(review)}")
            continue
        oracle = direct.get(key(review))
        # Single-target SDK units have a selector although the authoritative outer
        # query already evaluated that selector; multi-targets use their inner query.
        if oracle is None:
            oracle = direct.get((unit["project"], None))
            require(oracle is not None and oracle.get("metadata", {}).get("Properties", {}).get("TargetFramework") == unit["target_framework"], "missing direct inner-framework authority")
        require(oracle["exit_code"] == 0, "confirmed unit has failed direct oracle")
        metadata = oracle["metadata"]
        props = metadata["Properties"]
        for name in PROPERTIES:
            require(name in unit["properties"] and unit["properties"][name] == props[name],
                    f"property mismatch {key(review)}: {name}")
        def relative(value):
            value = normalize(value, root, sdk)
            require(value.startswith("$CORPUS/"), "confirmed oracle item escapes corpus")
            return value[len("$CORPUS/"):]
        sources = sorted((relative(item["FullPath"]), item.get("Link") or None) for item in metadata["Items"]["Compile"])
        require(sorted((x["path"].replace("\\", "/"), x["link"]) for x in unit["sources"]) == sources, "Compile/Link membership differs from direct MSBuild")
        references = sorted((item["Identity"], relative(item["FullPath"])) for item in metadata["Items"]["ProjectReference"])
        require(sorted((x["include"], x["target"].replace("\\", "/")) for x in unit["project_references"]) == references, "declared project references differ")
        require(sorted(x["include"] for x in unit["assembly_references"]) == sorted(x["Identity"] for x in metadata["Items"]["Reference"]), "assembly references differ")
        for item_type, field in (("Compile", "sources"), ("ProjectReference", "project_references"),
                                 ("Reference", "assembly_references")):
            def item_key(item):
                return relative(item["FullPath"]) if item_type == "Compile" else item["Identity"]
            oracle_items = {item_key(item): item for item in metadata["Items"][item_type]}
            require(len(oracle_items) == len(metadata["Items"][item_type]),
                    "duplicate direct items require explicit oracle handling")
            for item in unit[field]:
                identity = item["path"].replace("\\", "/") if item_type == "Compile" else item["include"]
                actual_metadata = normalize(item["metadata"], root, sdk)
                expected_metadata = {k: v for k, v in oracle_items[identity].items() if k != "Identity"}
                require(all(actual_metadata.get(k) == v for k, v in expected_metadata.items()),
                        f"{item_type} evaluated metadata differs: {identity}")


def run_corpus(runtime, root, *, repeat=2, manifest_path=None, allow_restore=False, host=None, host_kind="sdk"):
    require(repeat >= 2, "two fresh batch/stream repetitions required")
    manifest_path = Path(manifest_path or MANIFEST).resolve()
    entries = load_manifest(manifest_path)
    # Reject missing authority before acquisition or evaluation, not after an
    # expensive run or by silently blessing the product's first observed output.
    authorities = {x["id"]: authority(x, runtime.repository, manifest_path, host_kind=host_kind) for x in entries}
    sdk = runtime.sdk.resolve(strict=True)
    host = selected_host(sdk, host, host_kind)
    resolver_root = environment_root(runtime.environment)
    expected_environment = controlled_environment(resolver_root, sdk)
    require(runtime.environment == expected_environment, "Runtime environment must exactly match controlled resolver settings")
    frozen = verify_environment(resolver_root)
    dotnet = (sdk.parent.parent / ("dotnet.exe" if os.name == "nt" else "dotnet")).resolve(strict=True)
    records = []
    for entry in entries:
        expected, baseline = authorities[entry["id"]]
        require(baseline["restore"]["authorized"] is allow_restore,
                "explicit restore grant must match reviewed prepared-input authority")
        require(baseline.get("environment") == frozen, "reviewed resolver environment differs")
        current_host = host_identity(sdk, host, host_kind)
        require(all(baseline["host"].get(key) == current_host[key] for key in
                    ("kind", "platform", "binary", "msbuild_sha256", "sdk_msbuild_sha256")),
                "reviewed authority belongs to a different platform/host; select its matching manifest")
        canonical = None
        workspace = root / ("corpus-" + entry["id"])
        require(not workspace.exists() and not workspace.is_symlink(),
                "canonical corpus path already exists; refusing to remove an unowned workspace")
        pristine = root / ("corpus-" + entry["id"] + "-prepared")
        require(not pristine.exists() and not pristine.is_symlink(),
                "prepared snapshot path already exists; refusing to remove an unowned snapshot")
        source = acquire(entry, workspace)
        capture = direct_capture(entry, workspace, dotnet, sdk, runtime.environment,
                                 allow_restore=allow_restore, host=host, host_kind=host_kind)
        require(capture["host"]["msbuild_sha256"] == baseline["host"]["msbuild_sha256"], "reviewed host binary changed")
        require(source == baseline["source"], "reviewed project/submodule roster changed")
        require(capture["ancestor_configs"] == baseline["ancestor_configs"], "reviewed ancestor configuration differs")
        require(stable_direct(capture) == stable_direct(baseline), "independent direct-host authority drift")
        require(stable_restore(capture["restore"]) == stable_restore(baseline["restore"]),
                "reviewed restore outcomes or prepared assets differ")
        shutil.copytree(workspace, pristine, copy_function=shutil.copy2, symlinks=True)
        require(not workspace.is_symlink(), "owned corpus workspace became a symlink")
        shutil.rmtree(workspace)
        for repetition in range(repeat):
            for mode in ("batch", "stream"):
                label = f"corpus-{entry['id']}-{mode}-{repetition + 1}"
                require(not pristine.is_symlink(), "owned prepared snapshot became a symlink")
                shutil.copytree(pristine, workspace, copy_function=shutil.copy2, symlinks=True)
                require(verify_environment(resolver_root) == frozen, "resolver state changed before qualification")
                require(host_identity(sdk, host, host_kind) == current_host, "selected host changed before qualification")
                require(ancestor_preflight(workspace) == capture["ancestor_configs"], "ancestor NuGet config changed before qualification")
                require(verify_source(entry, workspace, prepared=capture["restore"]) == source,
                        "fresh prepared copy differs from pinned source authority")
                result = runtime.index(workspace, label=label, mode=mode, properties=entry["properties"],
                                       bypass_cache=True, expected_exit=expected["expected_exit"], host=host,
                                       allow_restore=allow_restore)
                require(verify_environment(resolver_root) == frozen, "resolver state changed during qualification")
                require(host_identity(sdk, host, host_kind) == current_host, "selected host changed during qualification")
                require(ancestor_preflight(workspace) == capture["ancestor_configs"], "ancestor NuGet config changed during qualification")
                verify_source(entry, workspace, indexed=True, prepared=capture["restore"])
                compare_projects(result["report"]["projects"], expected)
                compare_units(result["units"], expected, capture, workspace, sdk, runtime.environment)
                if canonical is None:
                    canonical = result["canonical"]
                require(result["canonical"] == canonical, "full canonical facts differ across fresh repetitions/modes")
                records.append({"label": label, "workload": f"corpus/{entry['id']}/{host_kind}/{mode}",
                    "repetition": repetition + 1, "measurement": result["measurement"], "correctness": {
                    "source": source, "standing_authority": expectation_name(entry, host_kind), "host": current_host,
                    "workspace_lifecycle": "fresh copy2 copy of once-acquired pinned prepared checkout, preserving restore bytes/mtimes; new index/process at same canonical repository path",
                    "direct_digest": digest(stable_direct(capture)), "canonical": result["canonical"],
                    "project_count": len(result["report"]["projects"]),
                    "direct_evaluation_attempt_count": len(capture["records"]),
                    "published_unit_count": len(result["units"]), "standing_and_metadata": "PASS"}})
                # Only our successful run is removed. Failures retain both the
                # workspace and pristine snapshot for diagnosis.
                require(not workspace.is_symlink(), "owned corpus workspace became a symlink")
                shutil.rmtree(workspace)
        require(not pristine.is_symlink(), "owned prepared snapshot became a symlink")
        shutil.rmtree(pristine)
    return records


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", choices=["capture"])
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--dotnet", type=Path, required=True)
    parser.add_argument("--sdk", type=Path, required=True, help="selected SDK directory containing MSBuild.dll")
    parser.add_argument("--host-kind", choices=("sdk", "framework"), default="sdk")
    parser.add_argument("--host", type=Path, help="explicit MSBuild installation directory; required for VS framework host")
    parser.add_argument("--environment", type=Path, required=True, help="declared isolated resolver state directory")
    parser.add_argument("--allow-restore", action="store_true",
                        help="authorize fixture/corpus package preparation and measured ordinary Restore as needed; never ordinary builds")
    parser.add_argument("--bootstrap-sdk-cache", action="store_true",
                        help="evaluation-only SDK prepass, then explicitly authorized fixture/corpus package preparation, then one combined freeze")
    args = parser.parse_args()
    args.output, args.environment = args.output.resolve(), args.environment.resolve()
    dotnet, sdk = args.dotnet.resolve(strict=True), args.sdk.resolve(strict=True)
    require((sdk / "MSBuild.dll").is_file(), "selected SDK lacks MSBuild.dll")
    host = selected_host(sdk, args.host, args.host_kind)
    args.output.mkdir(parents=True, exist_ok=False)
    if args.bootstrap_sdk_cache:
        require(not args.environment.exists() or
                (args.environment.is_dir() and not any(args.environment.iterdir())),
                "explicit bootstrap requires a new empty resolver environment")
    environment = controlled_environment(args.environment, sdk)
    entries = load_manifest(args.manifest)
    if args.bootstrap_sdk_cache:
        require(not (args.environment / "frozen-environment.json").exists(),
                "cannot bootstrap an already frozen environment; choose a new directory")
        for prepare_packages in ([False, True] if args.allow_restore else [False]):
            for entry in entries:
                with tempfile.TemporaryDirectory(prefix="prepare-" if prepare_packages else "bootstrap-", dir=args.output) as temporary:
                    workspace = Path(temporary) / entry["id"]
                    acquire(entry, workspace)
                    direct_capture(entry, workspace, dotnet, sdk, environment, bootstrap=True,
                                   allow_restore=prepare_packages, host=host, host_kind=args.host_kind)
            if not prepare_packages:
                bootstrap_record = {"sdk_prepass": environment_fingerprint(args.environment),
                                    "fixture_package_warmup": None}
                if args.allow_restore:
                    from qualification_fixtures import warm_fixture_packages
                    bootstrap_record["fixture_package_warmup"] = warm_fixture_packages(
                        ROOT, args.output / "fixture-package-warmup", sdk=sdk, environment=environment)
                (args.environment / "sdk-bootstrap.json").write_text(
                    json.dumps(bootstrap_record, indent=2) + "\n", encoding="utf-8")
        freeze_environment(args.environment)
    verify_environment(args.environment)
    for entry in entries:
        with tempfile.TemporaryDirectory(prefix="capture-", dir=args.output) as temporary:
            workspace = Path(temporary) / entry["id"]
            acquire(entry, workspace)
            capture = direct_capture(entry, workspace, dotnet, sdk, environment, allow_restore=args.allow_restore,
                                     host=host, host_kind=args.host_kind)
        capture_name = f"{entry['id']}.{sys.platform}.{args.host_kind}.capture.json.gz"
        encoded = (json.dumps(capture, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")
        (args.output / capture_name).write_bytes(gzip.compress(encoded, mtime=0))
        candidate = {"schema": 1, "id": entry["id"], "sha": entry["sha"], "capture_path": capture_name,
                     "capture_digest": digest(capture), "review": {"status": "unreviewed", "reviewer": None, "rationale": None},
                     "expected_projects": None, "expected_units": None, "expected_exit": None}
        (args.output / expectation_name(entry, args.host_kind)).write_text(json.dumps(candidate, indent=2) + "\n", encoding="utf-8")
    print("Capture only. Independently review project standing, enumerated unit standing and restore-asset currentness; commit captures and reviewed expectations beside corpus.json before qualification.")


if __name__ == "__main__":
    main()
