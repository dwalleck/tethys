"""Controlled resolver state on Linux/macOS and clean-profile Windows runners.

Authority: NuGet.Client 5fe0c128b2d58335a60161c5141064be42dd8a6b,
NuGetEnvironment.cs, PluginDiscoverer.cs and ConfigurationDefaults.cs. CoreCLR
uses DOTNET_CLI_HOME/APPDATA; desktop NuGet uses actual Windows special folders.
Separator-only NETCORE/NETFX_PLUGIN_PATHS explicitly select an empty plugin list
(Split(RemoveEmptyEntries)), rather than empty values which enable discovery.
"""
import hashlib
import json
import os
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
from urllib.parse import urlsplit

CONFIG = '<configuration><packageSources><clear/><add key="nuget.org" value="https://api.nuget.org/v3/index.json"/></packageSources></configuration>\n'
AUTHORITY = "https://github.com/NuGet/NuGet.Client/tree/5fe0c128b2d58335a60161c5141064be42dd8a6b/src/NuGet.Core"


def require(condition, message):
    if not condition:
        raise RuntimeError("C12 environment: " + message)


def windows_folders():
    import ctypes
    folders = {}
    for name, number in (("APPDATA", 26), ("LOCALAPPDATA", 28), ("USERPROFILE", 40),
                         ("PROGRAMFILES", 38), ("PROGRAMFILES(X86)", 42), ("PROGRAMDATA", 35), ("SystemRoot", 36)):
        buffer = ctypes.create_unicode_buffer(32768)
        result = ctypes.windll.shell32.SHGetFolderPathW(None, number, None, 0, buffer)
        require(result == 0 and buffer.value, "cannot resolve actual Windows known folder: " + name)
        folders[name] = Path(buffer.value)
    return folders


def safe_config(path):
    """Accept only unauthenticated public/offline source declarations; emit hashes."""
    require(not path.is_symlink() and path.is_file(), "non-regular external NuGet config")
    raw = path.read_bytes()
    require(b"<!DOCTYPE" not in raw.upper() and b"<!ENTITY" not in raw.upper(), "external XML declarations in NuGet config")
    tree = ET.fromstring(raw)
    require(tree.tag == "configuration", "unknown NuGet configuration document")
    for section in tree:
        require(section.tag in ("packageSources", "disabledPackageSources"),
                "credential-bearing or uncontrolled NuGet configuration section: " + section.tag)
        for item in section:
            require(item.tag in ("add", "clear", "remove"), "unknown NuGet config operation")
            require(set(item.attrib) <= {"key", "value", "protocolVersion"}, "uncontrolled NuGet config attributes")
            if item.tag != "add":
                continue
            value = item.get("value", "")
            if section.tag == "disabledPackageSources":
                require(value.lower() in ("true", "false"), "invalid disabled-source setting")
                continue
            url = urlsplit(value)
            local = Path(value)
            allowed_public = (url.scheme == "https" and url.hostname == "api.nuget.org" and
                              not url.username and not url.password and not url.query and not url.fragment and
                              url.path.rstrip("/") == "/v3/index.json")
            allowed_offline = local.is_absolute() and not value.startswith(("\\\\", "//")) and local.is_dir()
            require(allowed_public or allowed_offline, "uncontrolled external NuGet source; clean runner required")
    return hashlib.sha256(raw).hexdigest()


def ancestor_preflight(root):
    records = []
    for depth, parent in enumerate(Path(root).resolve().parents):
        for name in ("NuGet.Config", "NuGet.config", "nuget.config"):
            path = parent / name
            if path.is_file():
                records.append({"ancestor_depth": depth, "name": name, "sha256": safe_config(path)})
    return records


def platform_preflight(root):
    require(os.name == "nt" or sys.platform.startswith("linux") or sys.platform == "darwin", "unqualified operating system")
    scopes, plugins = [], []
    if os.name == "nt":
        folders = windows_folders()
        scopes = [("windows-user", folders["APPDATA"] / "NuGet"),
                  ("windows-profile", folders["USERPROFILE"] / ".nuget/NuGet"),
                  ("windows-machine-x86", folders["PROGRAMFILES(X86)"] / "NuGet"),
                  ("windows-machine", folders["PROGRAMFILES"] / "NuGet")]
        plugins = [folders["USERPROFILE"] / ".nuget/plugins", folders["LOCALAPPDATA"] / "NuGet/plugins-cache"]
    else:
        scopes = [("unix-machine", Path("/Library/Application Support/NuGet") if sys.platform == "darwin" else Path("/etc/opt/NuGet"))]
    records = []
    for label, directory in scopes:
        if not directory.exists():
            continue
        require(not directory.is_symlink(), "external NuGet directory is symlinked")
        for parent, dirs, files in os.walk(directory, followlinks=False):
            dirs.sort()
            for name in dirs:
                require(not (Path(parent) / name).is_symlink(), "external NuGet directory is symlinked")
            for name in sorted(files):
                if name.lower().endswith(".config"):
                    path = Path(parent) / name
                    records.append({"scope": label, "name": path.relative_to(directory).as_posix(), "sha256": safe_config(path)})
    for directory in plugins:
        require(not directory.exists() or (directory.is_dir() and not any(directory.iterdir())),
                "ambient Windows NuGet plugin/profile state requires a fresh clean CI profile")
    return {"platform": sys.platform, "authority": AUTHORITY, "safe_configs": records,
            "ancestor_configs": ancestor_preflight(root), "plugins": "explicit empty protocol-plugin lists; Windows actual-profile plugin preflight"}


def controlled_environment(root, sdk):
    root, sdk = Path(root).resolve(), Path(sdk).resolve(strict=True)
    require((sdk / "MSBuild.dll").is_file(), "SDK must contain MSBuild.dll")
    platform_preflight(root)
    directories = {name: root / name for name in ("home", "dotnet-home", "packages", "http-cache", "xdg-config", "tmp", "appdata", "localappdata", "common-data")}
    for path in directories.values():
        path.mkdir(parents=True, exist_ok=True)
        require(not path.is_symlink(), "resolver directories cannot be symlinks")
    # Both native desktop HOME and CoreCLR DOTNET_CLI_HOME are represented; CoreCLR
    # Windows reads APPDATA. Do not rely on HOME to redirect desktop known folders.
    for config in (directories["home"] / ".nuget/NuGet/NuGet.Config",
                   directories["dotnet-home"] / ".nuget/NuGet/NuGet.Config",
                   directories["appdata"] / "NuGet/NuGet.Config"):
        config.parent.mkdir(parents=True, exist_ok=True)
        if config.exists():
            require(config.read_text(encoding="utf-8") == CONFIG, "isolated NuGet configuration changed")
        else:
            config.write_text(CONFIG, encoding="utf-8")
    environment = {
        "HOME": str(directories["home"]), "DOTNET_CLI_HOME": str(directories["dotnet-home"]),
        "DOTNET_ROOT": str(sdk.parent.parent), "DOTNET_MULTILEVEL_LOOKUP": "0",
        "DOTNET_SKIP_FIRST_TIME_EXPERIENCE": "1", "DOTNET_CLI_TELEMETRY_OPTOUT": "1",
        "DOTNET_NOLOGO": "1", "DOTNET_CLI_WORKLOAD_UPDATE_NOTIFY_DISABLE": "1",
        "NUGET_PACKAGES": str(directories["packages"]), "NUGET_HTTP_CACHE_PATH": str(directories["http-cache"]),
        "NUGET_SCRATCH": str(directories["tmp"] / "nuget-scratch"),
        "NUGET_NETCORE_PLUGIN_PATHS": os.pathsep, "NUGET_NETFX_PLUGIN_PATHS": os.pathsep,
        "NUGET_CREDENTIALPROVIDERS_PATH": str(directories["tmp"] / "no-credential-providers"),
        "XDG_CONFIG_HOME": str(directories["xdg-config"]), "XDG_DATA_HOME": str(directories["localappdata"]),
        "NUGET_COMMON_APPLICATION_DATA": str(directories["common-data"]),
        "TMPDIR": str(directories["tmp"]), "TEMP": str(directories["tmp"]), "TMP": str(directories["tmp"]),
        "LANG": "en_US.UTF-8" if sys.platform == "darwin" else "C.UTF-8",
        "LC_ALL": "en_US.UTF-8" if sys.platform == "darwin" else "C.UTF-8",
    }
    if os.name == "nt":
        folders = windows_folders()
        environment.update({key: str(folders[key]) for key in ("PROGRAMFILES", "PROGRAMFILES(X86)", "PROGRAMDATA", "SystemRoot")})
        environment.update({"APPDATA": str(directories["appdata"]), "LOCALAPPDATA": str(directories["localappdata"]),
                            "USERPROFILE": str(directories["home"]), "ComSpec": str(folders["SystemRoot"] / "System32/cmd.exe")})
        environment["PATH"] = os.pathsep.join(map(str, (sdk.parent.parent, folders["SystemRoot"] / "System32", folders["SystemRoot"])))
    else:
        environment["PATH"] = os.pathsep.join((str(sdk.parent.parent), "/usr/bin", "/bin", "/usr/sbin", "/sbin"))
    return environment


def environment_fingerprint(root):
    """Stream resolver bytes and safe external-config hashes; never emit config values."""
    root = Path(root).resolve(strict=True)
    aggregate = hashlib.sha256()
    count = size = 0
    for name in ("packages", "home", "dotnet-home", "xdg-config", "appdata", "localappdata", "common-data"):
        directory = root / name
        require(directory.is_dir(), "controlled environment has not been prepared")
        for parent, dirs, files in os.walk(directory, followlinks=False):
            dirs.sort()
            for name in dirs:
                require(not (Path(parent) / name).is_symlink(), "symlink in resolver state")
            for name in sorted(files):
                path = Path(parent) / name
                require(not path.is_symlink() and path.is_file(), "non-regular resolver input")
                content = hashlib.sha256()
                length = 0
                with path.open("rb") as stream:
                    while block := stream.read(1024 * 1024):
                        content.update(block)
                        length += len(block)
                record = [path.relative_to(root).as_posix(), length, content.hexdigest()]
                aggregate.update(json.dumps(record, separators=(",", ":")).encode() + b"\n")
                count += 1
                size += length
    sdk_checkpoint = root / "sdk-bootstrap.json"
    return {"schema": 1, "scope": "package-content-and-isolated-home-config; excludes HTTP cache, temporary files and mtimes",
            "sdk_bootstrap_sha256": hashlib.sha256(sdk_checkpoint.read_bytes()).hexdigest() if sdk_checkpoint.is_file() else None,
            "files": count, "bytes": size, "sha256": aggregate.hexdigest(), "preflight": platform_preflight(root)}


def freeze_environment(root):
    root = Path(root).resolve(strict=True)
    marker = root / "frozen-environment.json"
    require(not marker.exists(), "environment already frozen; use a new environment for explicit bootstrap")
    fingerprint = environment_fingerprint(root)
    marker.write_text(json.dumps(fingerprint, indent=2) + "\n", encoding="utf-8")
    return fingerprint


def verify_environment(root):
    root = Path(root).resolve(strict=True)
    marker = root / "frozen-environment.json"
    require(marker.is_file(), "missing frozen SDK cache; explicitly bootstrap first")
    expected = json.loads(marker.read_text(encoding="utf-8"))
    actual = environment_fingerprint(root)
    require(actual == expected, "frozen package/config content drift; automatic rebaseline forbidden")
    return actual


def environment_root(environment):
    root = Path(environment["NUGET_PACKAGES"]).resolve(strict=True).parent
    require(Path(environment["HOME"]).resolve(strict=True) == root / "home" and
            Path(environment["DOTNET_CLI_HOME"]).resolve(strict=True) == root / "dotnet-home" and
            Path(environment["NUGET_HTTP_CACHE_PATH"]).resolve(strict=True) == root / "http-cache",
            "Runtime must use the declared controlled environment")
    return root
