"""Real public-call qualification with disposable, provenance-recorded instrumentation.

Canonical policy: replace integer row IDs with natural identities; omit insertion
ordinals, revision counters, file mtimes/index timestamps and input-stamp modified
times. Normalize the exact workspace-root prefix in paths and diagnostic strings.
Cache payloads and cache observations are operational evidence, not semantic facts;
they remain available in the caller report, but are excluded from fact equivalence.
All remaining columns, including standing, diagnostics and dependencies, are hashed.
"""
import difflib
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
import tomllib

from discovery_smoke import measured_cli, require

FRAME = "TETHYS_QUALIFICATION_V1 "


def _digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def _json(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def canonical_snapshot(root):
    """Stream every fact table through a disk-backed canonical row sort and SHA256."""
    root = Path(root).resolve(strict=True)
    # MSBuild's well-known item Directory metadata omits RootDir, unlike FullPath.
    # Keep both native and slash presentations, with an explicit path boundary.
    directory_root = str(root)[len(root.anchor):].rstrip("/\\")
    directory_prefixes = {directory_root, directory_root.replace("\\", "/")} - {""}
    database = root / ".rivets/index/tethys.db"
    require(database.is_file(), f"missing published database: {database}")
    sql = sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)
    sql.execute("BEGIN")
    # SQL joins resolve IDs, avoiding per-row queries and Python-sized ID maps.
    symbol = "json_array(f.path,s.qualified_name,s.kind,s.line,s.column,s.end_line,s.end_column)"
    keys = {
        "files": "SELECT id,path AS natural FROM files",
        "symbols": f"SELECT s.id,{symbol} AS natural FROM symbols s JOIN files f ON f.id=s.file_id",
        "arch_packages": "SELECT id,json_array(name,path,source,evaluation_unit_key) AS natural FROM arch_packages",
    }
    unit_keys = {key: [project, target, json.loads(framework)] for key, project, target, framework
                 in sql.execute("SELECT unit_key,project_key,target_framework,framework_json FROM evaluation_units")}
    require(len({_json(value) for value in unit_keys.values()}) == len(unit_keys),
            "evaluation units lack unique project/framework natural keys")
    omitted = {
        "index_revision": {"singleton", "revision"},
        "files": {"id", "mtime_ns", "indexed_at"},
        "symbols": {"id"}, "refs": {"id"}, "attributes": {"id"},
        "arch_packages": {"id"}, "evaluation_context": {"singleton", "cache_observations_json"},
    }
    tables = {}
    try:
        names = [row[0] for row in sql.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
        for table in names:
            if table == "evaluation_cache":
                continue
            columns = [row[1] for row in sql.execute(f'PRAGMA table_info("{table}")')]
            foreign = {row[3]: (row[2], row[4]) for row in sql.execute(f'PRAGMA foreign_key_list("{table}")')}
            projections, joins, retained = [], [], []
            for column in columns:
                if column == "ordinal" or column in omitted.get(table, set()):
                    continue
                target = foreign.get(column)
                if target and target[0] in keys and target[1] == "id":
                    alias = f"key_{len(joins)}"
                    joins.append(f'LEFT JOIN ({keys[target[0]]}) {alias} ON {alias}.id=t."{column}"')
                    projections.append(f"{alias}.natural")
                else:
                    projections.append(f't."{column}"')
                retained.append(column)
            query = f'SELECT {",".join(projections)} FROM "{table}" t ' + " ".join(joins)

            def normalize(value, key=None):
                if isinstance(value, dict):
                    return {k: normalize(v, k) for k, v in value.items()
                            if not (table == "evaluation_inputs" and k == "modified")}
                if isinstance(value, list):
                    return [normalize(v, key) for v in value]
                if isinstance(value, str):
                    if key == "Directory" and table in {
                        "file_participation", "declared_project_references", "declared_assembly_references",
                    }:
                        for prefix in directory_prefixes:
                            if value == prefix or value.startswith((prefix + "/", prefix + "\\")):
                                return "$WORKSPACE" + value[len(prefix):]
                    if value in unit_keys:
                        return normalize(unit_keys[value])
                    if value.startswith("msbuild:") and value.rsplit(":", 1)[-1] in unit_keys:
                        prefix, unit = value.rsplit(":", 1)
                        return [prefix, normalize(unit_keys[unit])]
                    return value.replace(str(root), "$WORKSPACE").replace(root.as_posix(), "$WORKSPACE")
                if isinstance(value, bytes):
                    return {"bytes_hex": value.hex()}
                return value

            # A separate on-disk SQLite sorter bounds Python memory even for millions of refs.
            with tempfile.TemporaryDirectory(prefix="canonical-", dir=root.parent) as scratch:
                sorter = sqlite3.connect(str(Path(scratch) / "rows.db"))
                try:
                    sorter.execute("PRAGMA temp_store=FILE")
                    sorter.execute("PRAGMA cache_size=-2048")
                    sorter.execute("CREATE TABLE rows(value TEXT NOT NULL)")
                    count = 0
                    for row in sql.execute(query):
                        values = []
                        for column, value in zip(retained, row):
                            if column.endswith("_json") and value is not None:
                                value = json.loads(value)
                            elif column in foreign and foreign[column][0] in {"symbols", "arch_packages"} and value is not None:
                                value = json.loads(value)
                            values.append(normalize(value))
                        sorter.execute("INSERT INTO rows VALUES (?)", (_json(values),))
                        count += 1
                    digest = hashlib.sha256()
                    for (value,) in sorter.execute("SELECT value FROM rows ORDER BY value COLLATE BINARY"):
                        digest.update(value.encode("utf-8"))
                        digest.update(b"\n")
                    tables[table] = {"columns": retained, "count": count, "sha256": digest.hexdigest()}
                finally:
                    sorter.close()
    finally:
        sql.close()
    return {"policy": "natural-keys-workspace-paths-and-documented-volatiles-v1", "tables": tables,
            "sha256": hashlib.sha256(_json(tables).encode()).hexdigest()}


# Each exact anchor must occur once; drift fails setup instead of timing the wrong path.
# RAII captures early Result returns too. No separate-run time subtraction is used.
TIMER = r'''
struct QualificationTimer {
    phase: &'static str,
    started: std::time::Instant,
    deadline: Option<f64>,
}
impl QualificationTimer {
    fn new(phase: &'static str, deadline: Option<f64>) -> Self {
        eprintln!("TETHYS_QUALIFICATION_V1 {}", serde_json::json!({"event":"start","phase":phase,"deadline_seconds":deadline}));
        Self { phase, started: std::time::Instant::now(), deadline }
    }
}
impl Drop for QualificationTimer {
    fn drop(&mut self) {
        let seconds = self.started.elapsed().as_secs_f64();
        eprintln!("TETHYS_QUALIFICATION_V1 {}", serde_json::json!({"event":"end","phase":self.phase,"seconds":seconds,"deadline_seconds":self.deadline}));
    }
}
'''
ANCHORS = {
    "src/indexing.rs": [
        ("        let discovery = Arc::new(discover_workspace(&request)?);",
         '        let discovery = {\n            let _qualification_timer = QualificationTimer::new("discovery", None);\n            Arc::new(discover_workspace(&request)?)\n        };'),
        ("        let mut stats = run.owner.index_discovered_revision(&options, start)?;",
         '        let mut stats = {\n            let _qualification_timer = QualificationTimer::new("reindex", None);\n            run.owner.index_discovered_revision(&options, start)?\n        };'),
    ],
    "src/discovery/msbuild/host.rs": [
        ("    let output = match run(&mut command, payload, request.options.timeout) {",
         '    let output = match {\n        let _qualification_timer = QualificationTimer::new("evaluation", Some(request.options.timeout.as_secs_f64()));\n        run(&mut command, payload, request.options.timeout)\n    } {'),
    ],
    "src/discovery/msbuild/restore.rs": [
        ("    let output = match host::run(&mut command, Vec::new(), request.options.timeout) {",
         '    let output = match {\n        let _qualification_timer = QualificationTimer::new("restore", Some(request.options.timeout.as_secs_f64()));\n        host::run(&mut command, Vec::new(), request.options.timeout)\n    } {'),
    ],
}


def _timings(stderr, *, expected_success):
    active, frames, elapsed = [], [], {"discovery": [], "reindex": [], "evaluation": [], "restore": []}
    deadlines, restore_deadlines = [], []
    for line in stderr.splitlines():
        if not line.startswith(FRAME):
            require("TETHYS_QUALIFICATION" not in line, f"malformed timing frame: {line}")
            continue
        frame = json.loads(line[len(FRAME):])
        phase, event = frame.get("phase"), frame.get("event")
        require(phase in elapsed and event in {"start", "end"}, f"invalid timing frame: {frame}")
        required = {"event", "phase", "deadline_seconds"} | ({"seconds"} if event == "end" else set())
        require(set(frame) == required, f"unexpected timing fields: {frame}")
        deadline = frame["deadline_seconds"]
        require((phase in {"evaluation", "restore"} and isinstance(deadline, (int, float)) and 0 < deadline <= 60)
                or (phase not in {"evaluation", "restore"} and deadline is None), f"invalid process deadline: {frame}")
        if event == "start":
            active.append((phase, deadline))
            if phase == "evaluation":
                deadlines.append(deadline)
            elif phase == "restore":
                restore_deadlines.append(deadline)
        else:
            require(active and active.pop() == (phase, deadline), f"unbalanced timing frame: {frame}")
            seconds = frame["seconds"]
            require(isinstance(seconds, (int, float)) and math.isfinite(seconds) and seconds >= 0,
                    f"invalid elapsed timing: {frame}")
            elapsed[phase].append(seconds)
        frames.append(frame)
    require(not active, "incomplete timing frames")
    if expected_success:
        require(len(elapsed["discovery"]) == len(elapsed["reindex"]) == 1, "missing/duplicate phase timing")
    require(len(elapsed["discovery"]) <= 1 and len(elapsed["reindex"]) <= 1, "duplicate index phases")
    observed = bool(elapsed["discovery"])
    return {
        "discovery_seconds": sum(elapsed["discovery"]) if elapsed["discovery"] else None,
        "evaluation_seconds": sum(elapsed["evaluation"]) if observed else None,
        "reindex_seconds": sum(elapsed["reindex"]) if elapsed["reindex"] else None,
        "evaluation_invocations": len(deadlines) if observed else None,
        "evaluation_max_seconds": max(elapsed["evaluation"], default=0) if observed else None,
        "evaluation_deadline_max_seconds": max(deadlines, default=None),
        "restore_seconds": sum(elapsed["restore"]) if observed else None,
        "restore_invocations": len(restore_deadlines) if observed else None,
        "restore_max_seconds": max(elapsed["restore"], default=0) if observed else None,
        "restore_deadline_max_seconds": max(restore_deadlines, default=None),
        "timing_status": "measured" if observed else "unavailable: discovery not reached",
        "timing_frames": frames,
    }


def _tree_identity(root):
    """Hash installed file bytes under stable relative names, without buffering them."""
    root = Path(root).resolve(strict=True)
    aggregate = hashlib.sha256()
    count = total = 0
    if root.is_file():
        paths = [(root.parent, [], [root.name])]
        base = root.parent
    else:
        paths = os.walk(root, followlinks=False)
        base = root
    for directory, directories, files in paths:
        directories.sort()
        for name in directories:
            require(not (Path(directory) / name).is_symlink(),
                    "identity closure contains an unobserved symlink directory")
        for name in sorted(files):
            path = Path(directory) / name
            require(path.resolve(strict=True).is_relative_to(base),
                    "identity closure file symlink escapes its observed root")
            require(path.is_file(), "identity closure contains a nonregular file")
            before = path.stat()
            digest = _digest(path)
            after = path.stat()
            require((before.st_size, before.st_mtime_ns, before.st_ino) ==
                    (after.st_size, after.st_mtime_ns, after.st_ino),
                    "identity input changed while hashing")
            record = [path.relative_to(base).as_posix(), before.st_size, digest]
            aggregate.update(_json(record).encode() + b"\n")
            count += 1
            total += before.st_size
    return {"files": count, "bytes": total, "sha256": aggregate.hexdigest()}


def _runner_identity(psutil, environment):
    observed = platform.uname()
    cpu = platform.processor() or None
    cpu_status = "observed: platform.processor" if cpu else "unavailable: CPU model not reported"
    if sys.platform.startswith("linux"):
        try:
            with Path("/proc/cpuinfo").open() as stream:
                for line in stream:
                    key, separator, value = line.partition(":")
                    if separator and key.strip() in {"model name", "Hardware"} and value.strip():
                        cpu, cpu_status = value.strip(), "observed: /proc/cpuinfo"
                        break
        except OSError:
            pass  # The explicit unavailable/platform status remains.
    elif sys.platform == "darwin":
        result = subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True,
                                text=True, env=environment, timeout=10)
        if result.returncode == 0 and result.stdout.strip():
            cpu, cpu_status = result.stdout.strip(), "observed: sysctl machdep.cpu.brand_string"
    elif os.name == "nt":
        import winreg
        try:
            with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE,
                                r"HARDWARE\DESCRIPTION\System\CentralProcessor\0") as key:
                cpu, _ = winreg.QueryValueEx(key, "ProcessorNameString")
                cpu, cpu_status = cpu.strip(), "observed: Windows processor registry"
        except OSError:
            pass  # Keep the explicit platform/unavailable observation.
    image = {key: environment[key] for key in
             ("ImageOS", "ImageVersion", "RUNNER_OS", "RUNNER_ARCH", "RUNNER_ENVIRONMENT")
             if environment.get(key)}
    os_release = None
    if sys.platform.startswith("linux"):
        try:
            release = platform.freedesktop_os_release()
            os_release = {key: release[key] for key in
                          ("ID", "VERSION_ID", "VARIANT_ID", "BUILD_ID", "IMAGE_ID", "IMAGE_VERSION")
                          if key in release}
        except OSError:
            pass  # Missing distro release data remains explicitly unavailable.
    elif sys.platform == "darwin":
        os_release = {"macos_release": platform.mac_ver()[0] or None}
    elif os.name == "nt":
        release, version, service_pack, product_type = platform.win32_ver()
        os_release = {"release": release, "version": version,
                      "service_pack": service_pack, "product_type": product_type}
    return {
        "os": observed.system, "architecture": observed.machine,
        "os_release": os_release, "os_release_status": "observed" if os_release else "unavailable",
        "kernel_release": observed.release, "kernel_version": observed.version,
        "cpu_model": cpu, "cpu_model_status": cpu_status,
        "logical_cpus": psutil.cpu_count(logical=True),
        "physical_cpus": psutil.cpu_count(logical=False),
        "total_ram_bytes": psutil.virtual_memory().total,
        "runner_image": image or None,
        "runner_image_status": "observed environment metadata; not attested" if image else "unavailable",
        "resource_observer": {"name": "psutil", "version": psutil.__version__},
        "boundary": "OS-reported hardware/resources; no cloud image or hardware attestation",
    }


def _toolchain_identity(runtime, source, package, compiler_fields):
    from qualification_environment import environment_root, verify_environment
    frozen = verify_environment(environment_root(runtime.environment))
    # The bootstrap record is historical preparation provenance, not resolver
    # bytes; it may embed temporary workload paths or implementation references.
    frozen = {key: value for key, value in frozen.items() if key != "sdk_bootstrap_sha256"}
    require(runtime.sdk.parent.name.lower() == "sdk",
            "cannot identify installed dotnet root from selected SDK directory")
    installation = runtime.sdk.parent.parent
    muxer = installation / ("dotnet.exe" if os.name == "nt" else "dotnet")
    require(muxer.is_file(), "selected installation has no observed dotnet muxer")
    closures = {"selected-sdk": _tree_identity(runtime.sdk), "muxer": _tree_identity(muxer)}
    for name in ("shared", "host", "packs", "sdk-manifests", "metadata"):
        path = installation / name
        closures[name] = _tree_identity(path) if path.exists() else None
    require(closures["shared"] is not None and closures["host"] is not None,
            "selected .NET runtime/host closure is unavailable")
    if runtime.classic_host is not None:
        closures["classic-host"] = _tree_identity(runtime.classic_host)
    # Observe actual Cargo configuration search inputs without exposing config
    # values or home paths. Their order records outer-to-inner precedence.
    config_roots = [Path(runtime._build_environment.get(
        "CARGO_HOME", str(Path(runtime._build_environment.get("HOME", str(Path.home()))) / ".cargo")))]
    config_roots.extend(parent / ".cargo" for parent in reversed([package.parent, *package.parent.parents]))
    configurations = []
    target = compiler_fields["host"]
    for directory in config_roots:
        for name in ("config.toml", "config"):
            path = directory / name
            if path.is_file():
                content = tomllib.loads(path.read_text())
                configurations.append({"name": name, "sha256": _digest(path)})
                target = content.get("build", {}).get("target", target)
    target = runtime._build_environment.get("CARGO_BUILD_TARGET", target)
    require(isinstance(target, str), "multiple Cargo build targets lack a single qualified caller identity")
    if isinstance(target, str) and target.endswith(".json"):
        target_path = Path(target)
        if not target_path.is_absolute():
            target_path = package.parent / target_path
        target = {"custom_target_sha256": _digest(target_path)}
    # Only hashes of build-affecting values are retained, never environment values.
    overrides = {key: hashlib.sha256(value.encode()).hexdigest()
                 for key, value in runtime._build_environment.items()
                 if key in {"RUSTC", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER",
                            "RUSTC_WORKSPACE_WRAPPER", "RUSTUP_TOOLCHAIN", "CARGO_BUILD_TARGET"}
                 or key.startswith(("CARGO_PROFILE_", "CARGO_TARGET_", "CARGO_BUILD_RUST"))}
    source_manifest = tomllib.loads((source / "Cargo.toml").read_text())
    caller_manifest = tomllib.loads((package / "Cargo.toml").read_text())
    sysroot_query = subprocess.run(
        [runtime._build_environment.get("RUSTC", "rustc"), "--print", "sysroot"],
        cwd=package.parent, env=runtime._build_environment, capture_output=True,
        text=True, timeout=runtime.timeout)
    require(sysroot_query.returncode == 0, "cannot observe Rust compiler sysroot")
    sysroot = Path(sysroot_query.stdout.strip()).resolve(strict=True)
    compiler_binary = sysroot / "bin" / ("rustc.exe" if os.name == "nt" else "rustc")
    support = {}
    for directory in (sysroot / "lib", sysroot / "bin"):
        for path in sorted(directory.iterdir()):
            if path.is_file() and (".so" in path.name or path.suffix in {".dll", ".dylib"}):
                support[path.relative_to(sysroot).as_posix()] = _tree_identity(path)
    host_libraries = _tree_identity(sysroot / "lib/rustlib" / compiler_fields["host"] / "lib")
    rust_closures = {"compiler": _tree_identity(compiler_binary),
                     "compiler_support": support, "host_libraries": host_libraries}
    if target == compiler_fields["host"]:
        rust_closures["target_libraries"] = host_libraries
    elif isinstance(target, str):
        target_libraries = sysroot / "lib/rustlib" / target / "lib"
        rust_closures["target_libraries"] = (_tree_identity(target_libraries)
                                            if target_libraries.is_dir() else None)
    return {
        "dotnet": {"selected_sdk": runtime.sdk.name, "closures": closures},
        "rust": {"compiler": compiler_fields, "target": target, "installed_closures": rust_closures,
                 "profile": {"name": "release", "offline": True,
                             "driver_manifest": caller_manifest.get("profile", {}).get("release", {}),
                             "cli_manifest": source_manifest.get("profile", {}).get("release", {})},
                 "cargo_configuration": configurations, "build_override_value_hashes": overrides},
        "resolver_state": frozen,
        "boundary": "Observed selected installation subtrees and frozen resolver bytes; arbitrary external "
                    "custom-resolver inputs, OS native-library closure and compiler-wrapper behavior are not attested",
    }


class Runtime:
    def __init__(self, repository, output, *, cli, sdk, companion, classic_host=None,
                 environment=None, timeout=3600):
        self.repository = Path(repository).resolve(strict=True)
        self.output = Path(output).resolve()
        self.cli = Path(cli).resolve(strict=True)
        self.sdk = Path(sdk).resolve(strict=True)
        self.companion = Path(companion).resolve(strict=True)
        self.classic_host = Path(classic_host).resolve(strict=True) if classic_host else None
        self.environment = dict(os.environ if environment is None else environment)
        # Build tools retain the invoking user's Cargo/Rustup installation. Never
        # merge this ambient environment into the controlled evaluation process.
        self._build_environment = dict(os.environ)
        require(math.isfinite(timeout) and timeout > 0, "invalid runtime deadline")
        self._deadline = time.monotonic() + timeout
        self.setup_evidence = {"status": "not prepared", "controls": []}
        self._scratch = None
        self._sequence = 0

    @property
    def timeout(self):
        """Remaining budget for the next subprocess; the absolute deadline never moves.

        Recomputing on every read is what bounds each product, control and preparation
        subprocess by the aggregate cadence budget rather than a snapshot taken per lane.
        """
        remaining = self._deadline - time.monotonic()
        require(remaining > 0, "qualification runtime deadline exhausted")
        return remaining

    def prepare(self):
        require(self._scratch is None, "Runtime.prepare may only run once before close")
        try:
            import psutil
        except ImportError as error:
            raise RuntimeError("qualification requires psutil before preparation") from error
        require(isinstance(psutil.__version__, str) and bool(psutil.__version__),
                "psutil version is unavailable")
        self.setup_evidence["sampler"] = {"name": "psutil", "version": psutil.__version__}
        self.output.mkdir(parents=True, exist_ok=True)
        self._scratch = tempfile.TemporaryDirectory(prefix="runtime-", dir=self.output)
        scratch = Path(self._scratch.name)
        source = scratch / "source"
        compiler_command = self._build_environment.get("RUSTC", "rustc")
        compiler = subprocess.run([compiler_command, "--version", "--verbose"], cwd=scratch,
                                  env=self._build_environment, capture_output=True, text=True,
                                  timeout=self.timeout)
        require(compiler.returncode == 0, "cannot identify the preparation Rust compiler")
        compiler_fields = {}
        for line in compiler.stdout.splitlines():
            key, separator, value = line.partition(":")
            if separator and key in {"release", "commit-hash", "commit-date", "host", "LLVM version"}:
                compiler_fields[key] = value.strip()
        require({"release", "host"} <= compiler_fields.keys(), "incomplete Rust compiler identity")
        compiler_path = shutil.which(compiler_command, path=self._build_environment.get("PATH"))
        require(compiler_path is not None, "cannot locate observed compiler launcher")
        compiler_fields["launcher_sha256"] = _digest(Path(compiler_path).resolve(strict=True))
        self.setup_evidence.update(
            rustc=compiler_fields,
            build_profile={"name": "release", "offline": True},
            build_environment_policy="ambient snapshot used only for preparation; values not recorded",
            evaluation_environment_policy="supplied environment used exactly, without ambient merging")
        # Only declared Rust build inputs enter the disposable tree/provenance.
        # The managed companion is supplied prebuilt, so tools/, docs and all
        # issue/private artifacts are deliberately outside this allowlist.
        build_inputs = ["Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml",
                        "build.rs", ".cargo/config", ".cargo/config.toml", "src", "benches", "tests"]
        inventory = subprocess.run(
            ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", *build_inputs],
            cwd=self.repository, env=self._build_environment, capture_output=True, timeout=self.timeout)
        require(inventory.returncode == 0,
                f"cannot enumerate build inputs: {inventory.stderr.decode(errors='replace')}")
        selected = sorted(set(os.fsdecode(name) for name in inventory.stdout.split(b"\0") if name))
        require({"Cargo.toml", "Cargo.lock"} <= set(selected), "required Cargo build inputs are absent")
        source.mkdir()
        excluded_parts = {"target", "obj", "bin", ".git", ".rivets", "__pycache__", "node_modules"}
        for name in selected:
            relative = Path(name)
            require(not relative.is_absolute() and ".." not in relative.parts,
                    f"unsafe build input path: {name}")
            # src/bin contains Rust entrypoints, not managed build output.
            parts = relative.parts[2:] if relative.parts[:2] == ("src", "bin") else relative.parts
            if any(part in excluded_parts for part in parts):
                continue
            original = self.repository / relative
            for parent in [original, *original.parents]:
                if parent == self.repository:
                    break
                require(not parent.is_symlink(), f"symlink build input is unsupported: {name}")
            require(original.resolve(strict=True).is_relative_to(self.repository),
                    f"build input escapes repository: {name}")
            require(original.is_file(), f"build input is not a regular file: {name}")
            destination = source / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(original, destination)
        package = scratch / "caller"
        (package / "src").mkdir(parents=True)
        driver = Path(__file__).with_name("qualification_driver.rs")
        shutil.copyfile(driver, package / "src/main.rs")
        (package / "Cargo.toml").write_text(
            '[package]\nname="tethys-qualification-driver"\nversion="0.0.0"\nedition="2024"\n'
            '[workspace]\n[dependencies]\ntethys={path=' + json.dumps(str(source)) + '}\n'
            'serde={version="1",features=["derive"]}\nserde_json="1"\n')
        shutil.copyfile(source / "Cargo.lock", package / "Cargo.lock")
        provenance = self.output / "instrumentation"
        provenance.mkdir(exist_ok=True)
        files = []
        for path in sorted(source.rglob("*")):
            if path.is_file():
                files.append({"path": path.relative_to(source).as_posix(), "sha256": _digest(path)})
        (provenance / "source-manifest.json").write_text(_json(files))
        self.setup_evidence.update(source_manifest_sha256=_digest(provenance / "source-manifest.json"),
                                   driver_sha256=_digest(package / "src/main.rs"), cli_sha256=_digest(self.cli))
        caller_contract = tomllib.loads((package / "Cargo.toml").read_text())
        caller_contract["dependencies"]["tethys"]["path"] = "$IMPLEMENTATION"
        implementation = {
            "schema": 1,
            "rust_inputs": [entry for entry in files
                            if Path(entry["path"]).parts[0] not in {"tests", "benches", "docs"}],
            "driver": {"path": "qualification_driver.rs", "sha256": self.setup_evidence["driver_sha256"]},
            "caller_manifest": caller_contract,
            "build_contract": {
                "driver": ["cargo", "build", "--offline", "--release", "--manifest-path",
                           "$CALLER/Cargo.toml", "--target-dir", "$BUILD"],
                "cli": ["cargo", "build", "--offline", "--release", "--bin", "tethys",
                        "--manifest-path", "$IMPLEMENTATION/Cargo.toml", "--target-dir", "$BUILD"],
            },
            "companion": _tree_identity(self.companion),
            "scope": "Uninstrumented copied Rust build inputs, public caller/build contract and supplied companion bytes; "
                     "top-level tests/fixtures/benches/docs and temporary build paths excluded",
        }
        self.setup_evidence.update(
            implementation_sha256=hashlib.sha256(_json(implementation).encode()).hexdigest(),
            implementation_inputs=implementation,
            identity_inputs={
                "toolchain": _toolchain_identity(self, source, package, compiler_fields),
                "runner": _runner_identity(psutil, self._build_environment),
            })
        target = scratch / "target"
        command = ["cargo", "build", "--offline", "--release", "--manifest-path", str(package / "Cargo.toml"),
                   "--target-dir", str(target)]
        executable = "tethys-qualification-driver" + (".exe" if os.name == "nt" else "")
        for variant in ("control", "instrumented"):
            if variant == "instrumented":
                patches = []
                for relative, replacements in ANCHORS.items():
                    path = source / relative
                    original = path.read_text()
                    modified = original
                    for before, after in replacements:
                        require(modified.count(before) == 1, f"instrumentation anchor drift: {relative}: {before}")
                        modified = modified.replace(before, after)
                    modified += TIMER
                    path.write_text(modified)
                    saved = provenance / relative
                    saved.parent.mkdir(parents=True, exist_ok=True)
                    saved.write_text(modified)
                    patches.extend(difflib.unified_diff(original.splitlines(True), modified.splitlines(True),
                                                       fromfile=relative, tofile=relative))
                (provenance / "timers.patch").write_text("".join(patches))
                self.setup_evidence["patch_sha256"] = _digest(provenance / "timers.patch")
            build = subprocess.run(command, cwd=scratch, env=self._build_environment, capture_output=True,
                                   text=True, timeout=self.timeout)
            (provenance / f"build-{variant}.log").write_text(build.stdout + build.stderr)
            require(build.returncode == 0, f"{variant} build failed; see {provenance}")
            binary = provenance / f"{variant}-{executable}"
            shutil.copy2(target / "release" / executable, binary)
            setattr(self, "_" + variant, binary)
            self.setup_evidence[variant + "_binary_sha256"] = _digest(binary)
        cli_command = ["cargo", "build", "--offline", "--release", "--bin", "tethys",
                       "--manifest-path", str(source / "Cargo.toml"), "--target-dir", str(target)]
        cli_build = subprocess.run(cli_command, cwd=scratch, env=self._build_environment,
                                   capture_output=True, text=True, timeout=self.timeout)
        (provenance / "build-instrumented-cli.log").write_text(cli_build.stdout + cli_build.stderr)
        require(cli_build.returncode == 0, f"instrumented CLI build failed; see {provenance}")
        cli_name = "tethys" + (".exe" if os.name == "nt" else "")
        self._instrumented_cli = provenance / ("instrumented-" + cli_name)
        shutil.copy2(target / "release" / cli_name, self._instrumented_cli)
        self.setup_evidence.update(instrumented_cli_sha256=_digest(self._instrumented_cli),
                                   cli_build_command=cli_command)
        self.setup_evidence.update(status="prepared; transparency required for every index invocation",
                                   build_command=command, provenance=str(provenance))
        return self.setup_evidence

    def close(self):
        if self._scratch is not None:
            self._scratch.cleanup()
            self._scratch = None

    def _run(self, binary, root, arguments, expected_exit, payload=None):
        capture = {}
        output, measurement = measured_cli(binary, root, arguments, self.environment,
                                           expected_exit, self.timeout, input_text=payload, capture=capture)
        measurement.update(rss_bytes=measurement["process_tree_sampled_peak_rss_bytes"],
                           rss_status=measurement["process_tree_rss_status"],
                           storage_bytes=measurement["index_and_sidecar_sampled_peak_bytes"])
        return output, measurement, capture

    def index(self, root, *, label, mode="batch", operation="index", bypass_cache=False,
              host=None, properties=None, allow_restore=False, expected_exit=0):
        require(self._scratch is not None and hasattr(self, "_instrumented"), "call prepare() first")
        root = Path(root).resolve(strict=True)
        require(mode in {"batch", "stream"} and operation in {"index", "update"}, "unsupported mode/operation")
        frozen_root, environment_checks = None, []
        if allow_restore:
            from qualification_environment import environment_root, verify_environment
            frozen_root = environment_root(self.environment)
            require(not frozen_root.is_relative_to(root) and not root.is_relative_to(frozen_root),
                    "restore replay workload and frozen resolver-state root must be disjoint")
            require(not self.output.is_relative_to(root),
                    "restore replay requires evidence outside the disposable workload")

        def replay(binary):
            if frozen_root is None:
                return self._run(binary, root, [], expected_exit, payload)
            before = verify_environment(frozen_root)
            try:
                return self._run(binary, root, [], expected_exit, payload)
            finally:
                after = verify_environment(frozen_root)
                require(before == after, "restore changed frozen package/home/config bytes")
                environment_checks.append({"binary": binary.name, "before": before, "after": after})
        if operation == "update":
            require(mode == "batch" and host is None and not properties and not bypass_cache and not allow_restore,
                    "public update() cannot accept mode/discovery overrides")
        selected = None if operation == "update" else str(Path(host).resolve(strict=True) if host else self.sdk)
        payload = _json(dict(mode=mode, operation=operation, companion=str(self.companion), host=selected,
                             properties=properties or {}, bypass_cache=bypass_cache, allow_restore=allow_restore))
        self._sequence += 1
        evidence = self.output / f"run-{self._sequence:05d}"
        evidence.mkdir()
        (evidence / "input.json").write_text(payload)
        index = root / ".rivets/index"
        # Replay both binaries against the identical workspace and pre-run DB/cache.
        # Backups and the control are outside the reported workload timing.
        with tempfile.TemporaryDirectory(prefix="control-state-", dir=self._scratch.name) as temporary:
            workspace_backup = Path(temporary) / "workspace"
            if allow_restore:
                shutil.copytree(root, workspace_backup, symlinks=True)
            backup = Path(temporary) / "index"
            existed = index.exists()
            if existed:
                shutil.copytree(index, backup)
            output, measurement, capture = replay(self._instrumented)
            measurement.update(_timings(capture["stderr"], expected_success=expected_exit == 0))
            instrumented = canonical_snapshot(root) if (index / "tethys.db").exists() else None
            report = json.loads(output) if expected_exit == 0 else {"units": []}
            (evidence / "instrumented.stdout").write_text(output)
            (evidence / "instrumented.stderr").write_text(capture["stderr"])
            if allow_restore:
                # Replay the identical same-root workspace, including absent assets.
                # External package/home/config bytes are verified, never reset/reseeded.
                for child in root.iterdir():
                    if child.is_dir() and not child.is_symlink():
                        shutil.rmtree(child)
                    else:
                        child.unlink()
                shutil.copytree(workspace_backup, root, symlinks=True, dirs_exist_ok=True)
            else:
                if index.exists():
                    shutil.rmtree(index)
                if existed:
                    shutil.copytree(backup, index)
            control_output, control_measurement, control_capture = replay(self._control)
            require(FRAME not in control_capture["stderr"], "control build unexpectedly instrumented")
            control = canonical_snapshot(root) if (index / "tethys.db").exists() else None
            (evidence / "control.stdout").write_text(control_output)
            (evidence / "control.stderr").write_text(control_capture["stderr"])
            require(instrumented == control, f"instrumentation changed canonical facts: {label}; {evidence}")
            if expected_exit == 0:
                require(json.loads(control_output) == report, f"instrumentation changed discovery report: {label}")
        transparency = {"label": label, "status": "PASS", "canonical_sha256": control["sha256"] if control else None,
                        "control_measurement": control_measurement, "evidence": str(evidence),
                        "frozen_environment_checks": environment_checks}
        self.setup_evidence["controls"].append(transparency)
        return {"measurement": measurement, "canonical": instrumented, "units": report["units"],
                "exit_code": capture["exit_code"], "report": report, "transparency": transparency}

    def query(self, root, args, *, label, expected_exit=0):
        require(self._scratch is not None and hasattr(self, "_instrumented_cli"), "call prepare() first")
        args = [str(argument) for argument in args]
        require(not any(argument == "--lsp" or argument.startswith("--lsp=") for argument in args),
                "qualification queries must be non-LSP persisted queries")
        root = Path(root).resolve(strict=True)
        before = canonical_snapshot(root)
        output, measurement, capture = self._run(self._instrumented_cli, root, args, expected_exit)
        timing = _timings(capture["stderr"], expected_success=False)
        evaluations = [frame for frame in timing["timing_frames"]
                       if frame["phase"] == "evaluation" and frame["event"] == "end"]
        # A completed, exit-checked instrumented CLI invocation provides complete
        # negative launch evidence too. No discover/index phase is expected here.
        timing.update(evaluation_invocations=len(evaluations),
                      evaluation_seconds=sum(frame["seconds"] for frame in evaluations),
                      evaluation_max_seconds=max((frame["seconds"] for frame in evaluations), default=0),
                      timing_status="measured: complete instrumented CLI invocation")
        measurement.update(timing)
        require(not timing["timing_frames"],
                f"persisted query reached discovery/indexing/evaluation: {label}")
        require(before == canonical_snapshot(root), f"instrumented query changed published facts: {label}")
        control_output, control_measurement, control_capture = self._run(self.cli, root, args, expected_exit)
        require(FRAME not in control_capture["stderr"], "supplied control CLI unexpectedly instrumented")
        require(before == canonical_snapshot(root), f"control query changed published facts: {label}")
        require(control_output == output, f"instrumentation changed query stdout: {label}")
        require(control_capture["exit_code"] == capture["exit_code"], f"instrumentation changed query exit: {label}")
        # Fast single-process queries can finish between samples. Keep the absent
        # sampled-tree observation absent; OS high-water is a different observation,
        # usable here only after non-LSP, zero-evaluator and immutable-query proof.
        native_peak = measurement["wait4_maxrss_native_units"]
        root_peak = None
        root_status = "unavailable: root-process high-water API unsupported on this platform"
        if sys.platform in {"linux", "darwin"} and isinstance(native_peak, (int, float)) and native_peak > 0:
            root_peak = int(native_peak * (1024 if sys.platform == "linux" else 1))
            root_status = "measured: wait4 root-process high-water; not sampled process-tree RSS"
        elif os.name == "nt":
            root_peak = measurement["windows_root_process_peak_working_set_bytes"]
            root_status = measurement["windows_root_process_peak_working_set_status"]
        measurement.update(root_process_high_water_rss_bytes=root_peak,
                           root_process_high_water_rss_status=root_status,
                           rss_scope="sampled-process-tree")
        exited_before_sample = measurement["process_tree_rss_status"] in {
            "unavailable: CLI exited before the first process-tree sample",
            "unavailable: process exited before a positive RSS sample",
        }
        if (measurement["process_tree_sampled_peak_rss_bytes"] is None
                and root_peak is not None and exited_before_sample):
            measurement.update(
                rss_bytes=root_peak,
                rss_scope="root-process",
                rss_status=root_status + "; for non-LSP, evaluator-free persisted query only")
        self._sequence += 1
        evidence = self.output / f"run-{self._sequence:05d}"
        evidence.mkdir()
        for name, text in (("instrumented.stdout", output), ("instrumented.stderr", capture["stderr"]),
                           ("control.stdout", control_output), ("control.stderr", control_capture["stderr"])):
            (evidence / name).write_text(text)
        transparency = {"label": label, "status": "PASS", "canonical_sha256": before["sha256"],
                        "control_measurement": control_measurement, "evidence": str(evidence),
                        "stdout_equal": True, "exit_equal": True, "canonical_unchanged": True}
        self.setup_evidence["controls"].append(transparency)
        return {"measurement": measurement, "output": output, "exit_code": capture["exit_code"],
                "correctness": transparency}
