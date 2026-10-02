"""Replay the retained et80 driver pair, preserving both databases for diagnosis."""
import argparse
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import time

REPOSITORY = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPOSITORY / ".tethys-82a6/oracles"))
from qualification_environment import controlled_environment, verify_environment
from qualification_runtime import canonical_snapshot


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


@contextmanager
def preserved_workspace(root, backup):
    """Restore the untouched retained directory even when a diagnostic run fails."""
    root.rename(backup)
    try:
        yield
    finally:
        if root.exists():
            shutil.rmtree(root)  # Only the diagnostic copy, never the retained tree.
        backup.rename(root)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--instrumentation", type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    retained = REPOSITORY / "target/qualification/corpus-qualification-5"
    pristine = retained / "work/corpus/corpus-avalonia-prepared"
    instrumentation = (args.instrumentation or retained / "runtime/instrumentation").resolve()
    payload = (retained / "runtime/run-00002/input.json").read_text()
    request = json.loads(payload)
    frozen = REPOSITORY / "target/qualification/corpus-environment-v5"
    environment = controlled_environment(frozen, request["host"])
    authority = verify_environment(frozen)
    root = retained / "work/corpus/corpus-avalonia"
    backup = output / "untouched-workspace"
    receipt = {
        "source_head": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPOSITORY, text=True).strip(),
        "retained_run": str(retained), "shared_root": str(root),
        "preserved_workspace": str(backup), "path_isolation": "rename and finally restore",
        "replay_sha256": digest(Path(__file__)),
        "projection_sha256": digest(REPOSITORY / ".tethys-82a6/oracles/qualification_runtime.py"),
        "pristine": str(pristine), "request": request,
        "frozen_environment": authority, "runs": {},
    }
    with preserved_workspace(root, backup):
        for label in ("instrumented", "control"):
            if root.exists():
                shutil.rmtree(root)  # Only the disposable copy created below.
            shutil.copytree(pristine, root, symlinks=True)
            binary = instrumentation / f"{label}-tethys-qualification-driver"
            command = [str(binary), "-w", str(root)]
            print(f"Starting {label} at shared root {root}", flush=True)
            start = time.monotonic()
            with (output / f"{label}.stdout").open("w") as stdout, (output / f"{label}.stderr").open("w") as stderr:
                result = subprocess.run(command, input=payload, text=True, env=environment,
                                        cwd=root, stdout=stdout, stderr=stderr, timeout=1800)
            run = {"command": command, "binary_sha256": digest(binary),
                   "seconds": time.monotonic() - start, "exit_code": result.returncode}
            receipt["runs"][label] = run
            (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
            if result.returncode:
                raise RuntimeError(f"{label} failed: see {output / (label + '.stderr')}")
            source = root / ".rivets/index/tethys.db"
            with sqlite3.connect(source.as_uri() + "?mode=ro", uri=True) as src, sqlite3.connect(output / f"{label}.db") as dest:
                src.backup(dest)
            run["canonical"] = canonical_snapshot(root)
            run["stdout_sha256"] = digest(output / f"{label}.stdout")
            run["database_sha256"] = digest(output / f"{label}.db")
            run["frozen_environment_unchanged"] = verify_environment(frozen) == authority
            if not run["frozen_environment_unchanged"]:
                raise RuntimeError("driver changed the frozen resolver inputs")
            (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
            print(f"Captured {label}: {run['seconds']:.3f}s, {run['canonical']['sha256']}", flush=True)
    receipt["original_workspace_restored"] = True
    left, right = (receipt["runs"][label]["canonical"] for label in ("instrumented", "control"))
    receipt["differing_tables"] = [name for name in sorted(left["tables"].keys() | right["tables"].keys())
                                    if left["tables"].get(name) != right["tables"].get(name)]
    receipt["canonical_equal"] = left == right
    receipt["stdout_equal"] = receipt["runs"]["instrumented"]["stdout_sha256"] == receipt["runs"]["control"]["stdout_sha256"]
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({key: receipt[key] for key in ("canonical_equal", "stdout_equal", "differing_tables")}), flush=True)
    return 0 if receipt["canonical_equal"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
