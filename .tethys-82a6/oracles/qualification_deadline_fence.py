#!/usr/bin/env python3
"""Deterministic C12 fence: the runtime deadline is absolute and recomputed per subprocess.

A per-lane snapshot lets a corpus lane run many multiples of the cadence cap before the
post-lane check rejects it. Named mutation: make the deadline effectively infinite
(`self._deadline = time.monotonic() + 1e9` in qualification_runtime.Runtime.__init__),
which reproduces that snapshot defect and turns the shrinking/exhaustion cases red.
"""

from __future__ import annotations

import json
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
from qualification_runtime import Runtime  # noqa: E402

SDK = Path("/home/dwalleck/.local-dotnet/sdk/10.0.102")
COMPANION = ROOT / "target/worker-dist/msbuild-evaluate"
CLI = ROOT / "target/debug" / ("tethys.exe" if sys.platform == "win32" else "tethys")


def runtime(output, timeout):
    return Runtime(ROOT, output, cli=CLI, sdk=SDK, companion=COMPANION, timeout=timeout)


def main():
    outcomes, failures = [], []

    def check(name, condition, detail):
        outcomes.append({"case": name, "ok": bool(condition), "detail": detail})
        if not condition:
            failures.append(name)

    with tempfile.TemporaryDirectory(prefix="deadline-fence-") as temporary:
        output = Path(temporary) / "runtime"
        subject = runtime(output, 3.0)
        first = subject.timeout
        time.sleep(0.6)
        second = subject.timeout
        check("deadline-shrinks-with-clock", second < first - 0.4,
              {"first_seconds": first, "second_seconds": second})
        try:
            subject.timeout = 60.0
            assignment = "accepted"
        except AttributeError:
            assignment = "rejected"
        check("no-per-lane-snapshot", assignment == "rejected", assignment)
        check("remaining-is-within-original-budget", second <= 3.0, second)

        expired = runtime(Path(temporary) / "expired", 0.3)
        time.sleep(0.4)
        try:
            expired.timeout
            exhausted = "returned"
        except Exception as error:
            exhausted = f"raised {type(error).__name__}"
        check("expired-deadline-refuses", exhausted.startswith("raised"), exhausted)

        # Multi-invocation proof: the second subprocess sees the reduced budget, not a fresh one.
        work = Path(temporary) / "work"
        work.mkdir()
        sleepy = Path(temporary) / "sleepy"
        sleepy.write_text("#!/bin/sh\nsleep 1.4\n", encoding="utf-8")
        sleepy.chmod(0o755)
        budget = runtime(Path(temporary) / "invocations", 2.0)
        budget._run(sleepy, work, [], 0)
        try:
            budget._run(sleepy, work, [], 0)
            multi = "completed"
        except Exception as error:
            multi = f"raised {type(error).__name__}"
        check("second-invocation-uses-remaining-budget", multi.startswith("raised"), multi)

    print(json.dumps({"scope": "absolute-deadline arithmetic only; no build, lane or native evaluation",
                      "status": "fail" if failures else "pass", "failures": failures, "outcomes": outcomes}, indent=2))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
