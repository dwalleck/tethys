"""Fail-closed C12 arithmetic; observations, never defaults, supply every metric."""

from __future__ import annotations

import hashlib
import json
import math
import statistics
from collections import defaultdict

GIB = 1024**3
CAPS = {"evaluation_max_seconds": 60.0, "rss_bytes": 6 * GIB, "storage_bytes": 10 * GIB}
CADENCE = {"pr": (2, 600.0), "nightly": (3, 1800.0), "release": (5, 3600.0), "qualification": (2, 3600.0)}
IDENTITY_FIELDS = ("corpus_sha256", "toolchain_sha256", "runner_sha256", "oracle_sha256")


class QualificationFailure(ValueError):
    """Observed proof is absent, invalid, inconsistent, or outside its contract."""


def number(value, name, *, positive=False):
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise QualificationFailure(f"{name}: missing/non-numeric observation")
    if not math.isfinite(value) or value < 0 or (positive and value == 0):
        raise QualificationFailure(f"{name}: expected finite {'positive' if positive else 'non-negative'} observation")
    return value


def at_most(value, cap, name):
    number(value, name)
    number(cap, f"{name} cap", positive=True)
    if value > cap:
        raise QualificationFailure(f"{name}: observed {value} exceeds {cap}")


def check_measurement(measurement, *, indexing=True):
    """Validate one measured invocation, including the at-cap/above-cap boundary."""
    number(measurement.get("wall_seconds"), "wall_seconds", positive=True)
    if not str(measurement.get("rss_status", "")).startswith("measured"):
        raise QualificationFailure("rss_status: complete process-tree measurement required")
    for name, cap in CAPS.items():
        number(measurement.get(name), name, positive=name in {"rss_bytes", "storage_bytes"})
        at_most(measurement[name], cap, name)
    if indexing:
        for name in ("discovery_seconds", "evaluation_seconds", "reindex_seconds"):
            number(measurement.get(name), name)
        count = measurement.get("evaluation_invocations")
        if isinstance(count, bool) or not isinstance(count, int) or count < 0:
            raise QualificationFailure("evaluation_invocations: exact non-negative count required")
        if count == 0 and (measurement["evaluation_seconds"] != 0 or measurement["evaluation_max_seconds"] != 0):
            raise QualificationFailure("evaluation timing contradicts zero invocations")
        if count > 0 and (measurement["evaluation_seconds"] == 0 or measurement["evaluation_max_seconds"] == 0):
            raise QualificationFailure("evaluation invocations lack measured durations")
        if measurement["evaluation_max_seconds"] > measurement["evaluation_seconds"] + 1e-6:
            raise QualificationFailure("evaluation maximum exceeds total evaluation duration")
        if measurement["evaluation_seconds"] > measurement["discovery_seconds"] + 1e-6:
            raise QualificationFailure("evaluation duration exceeds enclosing discovery phase")
        if measurement["discovery_seconds"] + measurement["reindex_seconds"] > measurement["wall_seconds"] + 1e-6:
            raise QualificationFailure("phase durations exceed enclosing invocation")
    return {"hard_caps": "pass"}


def check_elapsed(seconds, cadence):
    if cadence not in CADENCE:
        raise QualificationFailure(f"unknown cadence: {cadence}")
    at_most(number(seconds, "suite_seconds", positive=True), CADENCE[cadence][1], f"{cadence} suite_seconds")


def check_queries(seconds):
    if not seconds:
        raise QualificationFailure("query timings: missing observations")
    ordered = sorted(number(value, "query seconds", positive=True) for value in seconds)
    p95 = ordered[math.ceil(0.95 * len(ordered)) - 1]
    at_most(p95, 1.0, "query p95 seconds")
    at_most(ordered[-1], 2.0, "query maximum seconds")
    return {"count": len(ordered), "p95_seconds": p95, "maximum_seconds": ordered[-1]}


def sha256_value(value, name):
    if (not isinstance(value, str) or len(value) != 64
            or any(character not in "0123456789abcdef" for character in value)):
        raise QualificationFailure(f"{name}: observed/pinned SHA256 required")
    return value


def reviewed_baseline(manifest, *, identity):
    """Check the immutable authority before applying any calibrated ratio."""
    if not isinstance(manifest, dict) or manifest.get("schema") != 1:
        raise QualificationFailure("calibration: unsupported/missing manifest")
    if not isinstance(identity, dict) or set(identity) != set(IDENTITY_FIELDS):
        raise QualificationFailure("calibration: exactly four observed comparable identity hashes required")
    for key in IDENTITY_FIELDS:
        sha256_value(identity[key], f"calibration {key}")
    baseline_implementation = sha256_value(
        manifest.get("implementation_sha256"), "calibration baseline implementation")
    review = manifest.get("review")
    if (not isinstance(review, dict) or not isinstance(review.get("reviewer"), str)
            or not review["reviewer"].strip()):
        raise QualificationFailure("calibration: explicit reviewed evidence required")
    sha256_value(review.get("evidence_sha256"), "calibration reviewed evidence")
    if manifest.get("identity") != identity:
        raise QualificationFailure("calibration: corpus/toolchain/runner identity differs; no automatic rebaseline")
    observations = manifest.get("observations")
    if not isinstance(observations, list) or len(observations) < 14:
        raise QualificationFailure("calibration: fourteen successful observations required")
    ids = set()
    by_workload = defaultdict(list)
    expected_workloads = None
    for observation in observations:
        if not isinstance(observation, dict):
            raise QualificationFailure("calibration: observation must be an object")
        key = observation.get("id")
        if not isinstance(key, str) or not key or key in ids:
            raise QualificationFailure("calibration: distinct observation IDs required")
        ids.add(key)
        if observation.get("status") != "pass" or observation.get("identity") != identity:
            raise QualificationFailure("calibration: failed or differently scoped observation")
        if observation.get("implementation_sha256") != baseline_implementation:
            raise QualificationFailure("calibration: every observation must match the pinned baseline implementation")
        workloads = observation.get("workloads")
        if not isinstance(workloads, dict) or not workloads:
            raise QualificationFailure("calibration: complete workload measurements required")
        if expected_workloads is None:
            expected_workloads = set(workloads)
        elif set(workloads) != expected_workloads:
            raise QualificationFailure("calibration: observations omit or add workloads")
        for name, measurement in workloads.items():
            check_measurement(measurement)
            by_workload[name].append(measurement)
    payload = json.dumps(
        {"identity": identity, "implementation_sha256": baseline_implementation, "observations": observations},
        sort_keys=True, separators=(",", ":"), allow_nan=False,
    ).encode()
    if review["evidence_sha256"] != hashlib.sha256(payload).hexdigest():
        raise QualificationFailure("calibration: reviewed evidence hash does not match retained observations")
    # Recompute from retained observations; do not trust separately copied totals.
    return {
        name: {
            "wall_seconds": statistics.median(row["wall_seconds"] for row in rows),
            "rss_bytes": max(row["rss_bytes"] for row in rows),
            "storage_bytes": max(row["storage_bytes"] for row in rows),
        }
        for name, rows in by_workload.items()
    }


def check_ratios(measurements, baseline, *, cadence):
    if cadence not in {"nightly", "release"}:
        raise QualificationFailure("calibrated ratios apply to nightly/release, not PR subsecond timing")
    required = CADENCE[cadence][0]
    if len(measurements) != required:
        raise QualificationFailure(f"{cadence}: exactly {required} fresh observations required")
    for measurement in measurements:
        check_measurement(measurement)
    wall = statistics.median(row["wall_seconds"] for row in measurements)
    at_most(wall, 2 * number(baseline.get("wall_seconds"), "baseline wall", positive=True), "fresh index median ratio")
    for name in ("rss_bytes", "storage_bytes"):
        cap = 1.5 * number(baseline.get(name), f"baseline {name}", positive=True)
        for measurement in measurements:
            at_most(measurement[name], cap, f"{name} ratio")
    return {"fresh_median_seconds": wall, "ratios": "pass"}


def check_boundaries(path):
    """Exercise resource rejection with independent, explicitly synthetic cases."""
    from pathlib import Path

    fixture = json.loads(Path(path).read_text(encoding="utf-8"))
    if fixture.get("schema") != 1 or fixture.get("kind") != "arithmetic-boundary-fixture-not-runtime-evidence":
        raise QualificationFailure("unsupported boundary fixture")
    outcomes = []
    for case in fixture["cases"]:
        accepted = True
        try:
            if case["operation"] == "measurement":
                measurement = {**fixture["base"], **case.get("set", {})}
                if "remove" in case:
                    measurement.pop(case["remove"])
                if "special" in case:
                    measurement["wall_seconds"] = {"nan": math.nan, "inf": math.inf}[case["special"]]
                check_measurement(measurement)
            elif case["operation"] == "elapsed":
                check_elapsed(case["seconds"], case["cadence"])
            elif case["operation"] == "queries":
                check_queries(case["seconds"])
            elif case["operation"] == "ratios":
                rows = [{**fixture["base"], "wall_seconds": seconds,
                         "rss_bytes": case.get("rss_bytes", 3 * GIB),
                         "storage_bytes": case.get("storage_bytes", int(1.5 * GIB))}
                        for seconds in case["seconds"]]
                check_ratios(rows, {"wall_seconds": 62, "rss_bytes": 2 * GIB,
                                   "storage_bytes": GIB}, cadence=case["cadence"])
            elif case["operation"] == "calibration":
                identity = {key: "a" * 64 for key in IDENTITY_FIELDS}
                baseline_implementation = "b" * 64
                # Synthetic parser controls only: these are never saved as run evidence.
                observations = [{"id": f"boundary-{index}", "status": "pass", "identity": identity,
                                 "implementation_sha256": baseline_implementation,
                                 "workloads": {"fixture": fixture["base"]}}
                                for index in range(case.get("count", 14))]
                mutation = case.get("mutation")
                if mutation == "duplicate":
                    observations[-1]["id"] = observations[0]["id"]
                elif mutation == "failed":
                    observations[-1]["status"] = "fail"
                elif mutation == "missing-workload":
                    observations[-1]["workloads"] = {}
                elif mutation == "implementation-drift":
                    observations[-1]["implementation_sha256"] = "c" * 64
                payload = {"identity": identity, "implementation_sha256": baseline_implementation,
                           "observations": observations}
                digest = hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
                manifest = {"schema": 1, **payload,
                            "review": {"reviewer": "synthetic-boundary-control", "evidence_sha256": digest}}
                if mutation == "unreviewed":
                    manifest["review"] = {}
                elif mutation == "hash-drift":
                    manifest["review"]["evidence_sha256"] = "0" * 64
                elif mutation == "identity-drift":
                    identity = {**identity, "runner_sha256": "b" * 64}
                reviewed_baseline(manifest, identity=identity)
            else:
                raise ValueError(f"unknown boundary operation: {case['operation']}")
        except QualificationFailure:
            accepted = False
        if accepted != case["accept"]:
            raise QualificationFailure(f"boundary {case['name']}: accepted={accepted}, expected={case['accept']}")
        outcomes.append({"name": case["name"], "accepted": accepted})
    return {"status": "pass", "authority": str(path), "cases": outcomes,
            "scope": "arithmetic rejection, not runtime/calibration evidence"}
