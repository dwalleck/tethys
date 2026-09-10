#!/usr/bin/env python3
"""Deterministic C12 fence: cadence selects work, and a partial selection is not acceptance.

Runs only argument validation and the cheap boundary lane; no runtime lane, build or
native evaluation is started. Named mutation: make the roster checks in qualification.py's
arguments() unconditional acceptance and every rejection case below turns red.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUNNER = Path(__file__).resolve().parent / "qualification.py"
ENVIRONMENT = ROOT / "target/qualification/fixture-environment"
SDK = Path("/home/dwalleck/.local-dotnet/sdk/10.0.102")

# (case, argv tail, expected exit, required stderr/stdout text)
CASES = (
    ("pr-rejects-corpus", ["--corpus", "--cadence", "pr"], 2, "does not own corpus"),
    ("pr-requires-fixtures", ["--check-boundaries", "--cadence", "pr"], 2, "requires fixtures"),
    ("pr-requires-explicit-shapes", ["--fixtures", "--cadence", "pr", "--environment", str(ENVIRONMENT),
                                     "--allow-restore"], 2, "explicit authored --shapes"),
    ("nightly-requires-corpus", ["--cache", "--cadence", "nightly", "--environment", str(ENVIRONMENT),
                                 "--sdk", str(SDK)], 2, "requires corpus"),
    ("nightly-exact-repetitions", ["--corpus", "--cadence", "nightly", "--repeat", "4",
                                   "--environment", str(ENVIRONMENT), "--sdk", str(SDK)], 2, "exactly 3 repetitions"),
    ("nightly-rejects-density", ["--capture-density", "--cadence", "nightly", "--environment", str(ENVIRONMENT),
                                 "--allow-restore"], 2, "does not own capture_density"),
    ("release-requires-corpus", ["--legacy", "--cadence", "release", "--environment", str(ENVIRONMENT),
                                 "--sdk", str(SDK)], 2, "requires corpus"),
)


def main():
    outcomes, failures = [], []
    with tempfile.TemporaryDirectory(prefix="cadence-fence-") as temporary:
        accepted = Path(temporary) / "boundaries.json"
        process = subprocess.run([sys.executable, str(RUNNER), "--check-boundaries", "--output", str(accepted)],
                                 capture_output=True, text=True)
        report = json.loads(accepted.read_text(encoding="utf-8")) if accepted.exists() else {}
        assignment = report.get("cadence_assignment", {})
        outcomes.append({"case": "qualification-accepts-boundaries", "exit": process.returncode,
                         "expected_exit": 0, "matched": "n/a"})
        if process.returncode != 0 or report.get("status") != "pass" or assignment.get("complete") is not True \
                or not assignment.get("accounting_boundary"):
            failures.append("qualification-accepts-boundaries")
        for name, tail, expected, text in CASES:
            process = subprocess.run([sys.executable, str(RUNNER), *tail], capture_output=True, text=True)
            blob = process.stdout + process.stderr
            matched = text in blob
            outcomes.append({"case": name, "exit": process.returncode, "expected_exit": expected, "matched": matched})
            if process.returncode != expected or not matched:
                failures.append(name)
    print(json.dumps({"scope": "cadence roster rejection only; no runtime lane, build or native evaluation",
                      "status": "fail" if failures else "pass", "failures": failures, "outcomes": outcomes}, indent=2))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
