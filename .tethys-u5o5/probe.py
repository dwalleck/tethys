#!/usr/bin/env python3
"""Probe the real tethys workspace through the public cycles CLI."""
from pathlib import Path
import os
import sqlite3
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
ENV = {**os.environ, "NO_COLOR": "1"}

subprocess.run(
    ["cargo", "run", "--quiet", "--", "--workspace", str(ROOT), "index", "--rebuild"],
    cwd=ROOT,
    env=ENV,
    check=True,
    stdout=subprocess.DEVNULL,
)
output = subprocess.check_output(
    ["cargo", "run", "--quiet", "--", "--workspace", str(ROOT), "cycles"],
    cwd=ROOT,
    env=ENV,
    text=True,
)
cli_cycles = [
    tuple(line.strip().split(" → ")[:-1])
    for line in output.splitlines()
    if " → " in line
]
target = ("src/cargo.rs", "src/lib.rs")
sut_matches = [cycle for cycle in cli_cycles if cycle == target]

connection = sqlite3.connect(ROOT / ".rivets" / "index" / "tethys.db")
files = {
    path: file_id
    for file_id, path in connection.execute(
        "SELECT id, path FROM files WHERE path IN (?, ?)", target
    )
}
edges = set(
    connection.execute(
        "SELECT from_file_id, to_file_id FROM file_deps WHERE from_file_id IN (?, ?) AND to_file_id IN (?, ?)",
        (files[target[0]], files[target[1]], files[target[0]], files[target[1]]),
    )
)
expected_edges = {
    (files[target[0]], files[target[1]]),
    (files[target[1]], files[target[0]]),
}
oracle_matches = [target] if edges == expected_edges else []

print(f"SUT canonical two-file cycles: {sut_matches}")
print(f"Oracle direct-edge cycles:      {oracle_matches}")
if sut_matches != oracle_matches:
    print(f"CLI output contained {len(cli_cycles)} cycles", file=sys.stderr)
    raise SystemExit("probe/oracle disagreement")
