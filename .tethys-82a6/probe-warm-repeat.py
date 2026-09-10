#!/usr/bin/env python3
"""One diagnostic fresh-index repeat; neither calibration nor suite acceptance."""
import hashlib
import json
from pathlib import Path
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / ".tethys-82a6/oracles"))
from discovery_smoke import measured_cli
from qualification_environment import controlled_environment, verify_environment
from qualification_runtime import _timings

previous = ROOT / "target/qualification/baseline-f34-v1"
workspace = previous / "work/fixtures/fixture-baseline-workspace"
binary = previous / "runtime/instrumentation/instrumented-tethys-qualification-driver"
payload = (previous / "runtime/run-00001/input.json").read_text()
original = json.loads((previous / "runtime/run-00001/instrumented.stdout").read_text())
assert len(original["projects"]) == 240 and len(original["units"]) == 401
output = ROOT / "target/qualification/performance-premises/warm-full-repeat"
output.mkdir()
frozen = ROOT / "target/qualification/fixture-environment"
sdk = Path("/home/dwalleck/.local-dotnet/sdk/10.0.102")
environment = controlled_environment(frozen, sdk)
before = verify_environment(frozen)

def inputs():
    rows = []
    for path in sorted(workspace.rglob("*")):
        relative = path.relative_to(workspace)
        if relative.parts[0] == ".rivets" or not path.is_file():
            continue
        stat = path.stat()
        rows.append({"path": str(relative), "length": stat.st_size, "mtime_ns": stat.st_mtime_ns,
                     "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    return rows

source = inputs()
report = {"scope": "n=1 warm fresh-index repeat of retained 240-project/401-unit workload; no calibration or cadence acceptance",
          "status": "running", "workspace": str(workspace), "command": [str(binary), "-w", str(workspace)],
          "payload": json.loads(payload), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
          "frozen_environment_before": before, "source_before": source}
(output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
index, backup = workspace / ".rivets/index", output / "original-index"
existed = index.exists()
if existed:
    index.rename(backup)
capture = {}
started = time.monotonic()
try:
    stdout, measurement = measured_cli(binary, workspace, [], environment, 0, 900,
                                       input_text=payload, capture=capture)
    report.update(measurement=measurement, phases=_timings(capture["stderr"], expected_success=True))
    actual = json.loads(stdout)
    report.update(discovery_output_equal=actual == original, projects=len(actual["projects"]), units=len(actual["units"]))
    assert report["discovery_output_equal"], "Warm repeat changed complete discovery output"
    assert report["projects"] == 240 and report["units"] == 401
    report["status"] = "pass"
except BaseException as error:
    report.update(status="fail", error=repr(error))
    raise
finally:
    report["probe_wall_seconds"] = time.monotonic() - started
    for name in ("stdout", "stderr"):
        (output / f"instrumented.{name}").write_text(capture.get(name, ""))
    if index.exists():
        index.rename(output / "repeated-index")
    if existed:
        backup.rename(index)
    report["frozen_environment_after"] = verify_environment(frozen)
    report["source_inputs_unchanged"] = inputs() == source
    if report["frozen_environment_after"] != before or not report["source_inputs_unchanged"]:
        report["status"] = "fail"
        report["input_error"] = "Frozen environment or workload bytes/mtimes changed"
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({key: value for key, value in report.items() if key not in {"source_before", "frozen_environment_before", "frozen_environment_after"}}, indent=2))
assert report["status"] == "pass"
