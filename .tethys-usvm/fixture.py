#!/usr/bin/env python3
"""Generate an N-file single-cycle Rust workspace and index it with tethys.

Names are zero-padded so lexicographic path order equals numeric order --
`enumerate_cycles` canonicalizes on path order, so the closed-form visit
count in probe.py only holds if the two orders coincide.
"""
import shutil
import subprocess
import sys
from pathlib import Path

TETHYS = Path(__file__).resolve().parents[1] / "target/release/tethys"


def build(root: Path, n: int) -> Path:
    """Write n files where f{i} uses f{i+1}, and f{n-1} uses f0."""
    if root.exists():
        shutil.rmtree(root)
    src = root / "src"
    src.mkdir(parents=True)
    (root / "Cargo.toml").write_text(
        '[package]\nname = "cyc"\nversion = "0.1.0"\nedition = "2021"\n'
    )
    width = len(str(n - 1))
    for i in range(n):
        nxt = (i + 1) % n
        (src / f"f{i:0{width}d}.rs").write_text(
            f"use crate::f{nxt:0{width}d}::Item{nxt};\n\n"
            f"pub struct Item{i};\n\n"
            f"pub fn use_it{i}(_x: Item{nxt}) {{}}\n"
        )
    return root


def index(root: Path) -> None:
    subprocess.run(
        [str(TETHYS), "--workspace", str(root), "index", "--rebuild"],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


if __name__ == "__main__":
    n = int(sys.argv[1])
    root = Path(sys.argv[2])
    build(root, n)
    index(root)
    print(f"built and indexed {n} files at {root}")
