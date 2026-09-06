#!/usr/bin/env python3
"""Exercise the real CLI, independent SQL observer, rollback, rebuild and resource limits."""
import argparse
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/debug/tethys")
    parser.add_argument("--files", type=int, default=40)
    parser.add_argument("--streaming-driver", help="Standalone driver built from revision_stream.rs")
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())
    reports = []
    with tempfile.TemporaryDirectory(prefix="tethys-revision-smoke-") as directory:
        root = Path(directory)
        source = root / "src"
        source.mkdir()
        (root / "Cargo.toml").write_text('[package]\nname="revision_smoke"\nversion="0.1.0"\nedition="2021"\n')
        (source / "lib.rs").write_text("pub fn before() {}\n")
        for index in range(args.files - 1):
            (source / f"f{index}.rs").write_text(f"pub fn symbol_{index}() {{}}\n")

        def run(label, command, expected):
            started = time.monotonic()
            argv = [binary, "-w", str(root), *command]
            if args.streaming_driver and command[0] == "index":
                argv = [str(Path(args.streaming_driver).resolve()), str(root), *command[1:]]
            with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
                child = subprocess.Popen(argv, stdout=stdout, stderr=stderr)
                while True:
                    pid, status, usage = os.wait4(child.pid, os.WNOHANG)
                    if pid:
                        break
                    if time.monotonic() - started > 1800:
                        child.kill()
                        _, status, usage = os.wait4(child.pid, 0)
                        child.returncode = os.waitstatus_to_exitcode(status)
                        raise TimeoutError(f"C1 {label}: corpus wall deadline exceeded")
                    time.sleep(0.01)
                child.returncode = os.waitstatus_to_exitcode(status)
                stdout.seek(0)
                stderr.seek(0)
                process = subprocess.CompletedProcess(
                    argv, child.returncode, stdout.read().decode(), stderr.read().decode()
                )
            elapsed = time.monotonic() - started
            assert process.returncode == expected, (label, process.returncode, process.stdout, process.stderr)
            assert usage.ru_maxrss <= 6 * 1024 * 1024, f"C1 {label} exceeds RSS cap"
            reports.append({"phase": label, "wall_seconds": elapsed,
                            "cpu_seconds": usage.ru_utime + usage.ru_stime,
                            "peak_rss_kib": usage.ru_maxrss, "exit": process.returncode})
            return process

        run("initial", ["index"], 0)
        db = root / ".rivets/index/tethys.db"
        with sqlite3.connect(db, isolation_level=None) as observer:
            assert observer.execute("SELECT count(*) FROM files").fetchone()[0] == args.files
            first_revision = observer.execute("SELECT revision FROM index_revision").fetchone()[0]
            observer.execute("CREATE TRIGGER reject_arch BEFORE INSERT ON arch_packages BEGIN SELECT RAISE(ABORT,'smoke failure'); END")
            (source / "lib.rs").write_text("pub fn after() {}\n")
            run("failed_publication", ["index"], 1)
            assert observer.execute("SELECT name FROM symbols WHERE name IN ('before','after')").fetchall() == [("before",)], "C1 stale/new mixture"
            assert observer.execute("SELECT revision FROM index_revision").fetchone()[0] == first_revision
            observer.execute("DROP TRIGGER reject_arch")
            run("successful_reindex", ["index"], 0)
            assert observer.execute("SELECT name FROM symbols WHERE name IN ('before','after')").fetchall() == [("after",)]
            observer.execute("DROP TABLE index_revision")
            refusal = run("old_schema_refusal", ["stats"], 1)
            assert "--rebuild" in refusal.stderr + refusal.stdout
            assert observer.execute("SELECT count(*) FROM sqlite_master WHERE name='index_revision'").fetchone()[0] == 0
            run("explicit_rebuild", ["index", "--rebuild"], 0)
            assert observer.execute("SELECT count(*) FROM files").fetchone()[0] == args.files
            assert observer.execute("SELECT name FROM symbols WHERE name IN ('before','after')").fetchall() == [("after",)]
            byte_count = sum(path.stat().st_size for path in db.parent.glob("tethys.db*"))
            assert byte_count <= 10 * 1024 ** 3, "C1 index/sidecars exceed cap"
            print(json.dumps({"claims": ["C1", "C2"], "result": "PASS", "files": args.files,
                              "index_and_sidecar_bytes": byte_count, "runs": reports}, indent=2))


if __name__ == "__main__":
    main()
