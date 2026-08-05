#!/usr/bin/env python3
"""Probe: what does cycle enumeration cost on a single-cycle workspace?

Smallest question: on an N-file graph holding exactly ONE directed cycle,
how many node visits does `tethys cycles` make?

Probe  -- the real release binary over a real indexed workspace; the
          `visits` counter is read out of its own debug log.
Oracle -- closed-form arithmetic derived by hand, independent of the
          implementation: start f_k is confined to nodes >= f_k in path
          order, walks the tail f_k..f_{N-1}, and is turned back at the
          f_{N-1} -> f0 edge because f0 < f_k. So start f_k costs N-k
          visits and only f0 closes a cycle: sum(N-k) = N(N+1)/2.

The SCC-restricted enumeration this issue asks for should visit exactly N.
"""
import os
import re
import subprocess
import sys
import time
from pathlib import Path

import fixture

ROOT = Path(__file__).resolve().parents[1]
TETHYS = ROOT / "target/release/tethys"
VISITS = re.compile(r"visits=(\d+)")
COUNT = re.compile(r"cycle_count=(\d+)")


def measure(root: Path) -> tuple[int, int, float]:
    """Return (visits, cycles_found, wall_seconds) for `tethys cycles`."""
    env = {**os.environ, "RUST_LOG": "tethys=debug", "NO_COLOR": "1"}
    start = time.perf_counter()
    proc = subprocess.run(
        [str(TETHYS), "--workspace", str(root), "cycles"],
        env=env,
        capture_output=True,
        text=True,
        check=True,
    )
    elapsed = time.perf_counter() - start
    log = proc.stderr
    return int(VISITS.search(log)[1]), int(COUNT.search(log)[1]), elapsed


def oracle(n: int) -> int:
    """Hand-derived visit count for the unrestricted search: N(N+1)/2."""
    return n * (n + 1) // 2


if __name__ == "__main__":
    sizes = [int(a) for a in sys.argv[1:]] or [10, 50, 100, 200, 400]
    scratch = Path(os.environ["SCRATCH"])
    print(f"{'files':>7} {'probe visits':>13} {'oracle':>10} {'cycles':>7} {'secs':>8}")
    ok = True
    for n in sizes:
        root = scratch / f"cyc{n}"
        fixture.build(root, n)
        fixture.index(root)
        visits, cycles, secs = measure(root)
        want = oracle(n)
        agree = visits == want and cycles == 1
        ok &= agree
        flag = "" if agree else "   <== DISAGREE"
        print(f"{n:>7} {visits:>13} {want:>10} {cycles:>7} {secs:>8.2f}{flag}")
    if not ok:
        raise SystemExit("probe/oracle disagreement")
    print("\nprobe and oracle agree: cost is N(N+1)/2, output is 1 cycle")
