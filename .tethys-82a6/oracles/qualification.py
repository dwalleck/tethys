#!/usr/bin/env python3
"""Run the selected C12 evidence lanes; missing authority is never a passing gate."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import sys
import tempfile
import time

from qualification_limits import (
    CADENCE, QualificationFailure, check_elapsed, check_measurement,
    check_queries, check_ratios, reviewed_baseline, sha256_value,
)

ROOT = Path(__file__).resolve().parents[2]

LANE_ORDER = ("capture_density", "fixtures", "cache", "legacy", "corpus")

# Cadence assigns work; it is not a label chosen after the fact. `permitted` bounds what a cadence
# may run, `required` is the roster that must be present for the cadence to mean what it claims, and
# a platform condition (None = every platform) keeps Windows-only classic evidence out of Linux runs.
LANE_ROSTER = {
    "pr": {"permitted": ("fixtures", "cache", "check_boundaries"),
           "required": (("fixtures", None),), "explicit_shapes": True},
    "nightly": {"permitted": ("fixtures", "cache", "legacy", "corpus", "check_boundaries"),
                "required": (("corpus", None), ("legacy", "windows")), "explicit_shapes": False},
    "release": {"permitted": ("fixtures", "cache", "legacy", "corpus", "check_boundaries"),
                "required": (("corpus", None), ("legacy", "windows")), "explicit_shapes": False},
    "qualification": {"permitted": ("capture_density", "fixtures", "cache", "legacy", "corpus", "check_boundaries"),
                      "required": (), "explicit_shapes": False},
}

# Explicit accounting boundary: the cadence cap covers the whole runner invocation after argument
# validation, including preparation, transparency controls and canonical projection. Setup and
# controls are not hidden outside the cap.
AGGREGATE_BOUNDARY = (
    "argument validation through final lane: Runtime.prepare, every measured product invocation, "
    "same-root transparency controls and canonical projection")


def platform_matches(condition):
    if condition is None:
        return True
    if condition == "windows":
        return os.name == "nt"
    raise QualificationFailure(f"unknown roster platform condition: {condition}")


def assignment(args, lanes):
    """Record what this cadence may run and whether its required roster is present."""
    roster = LANE_ROSTER[args.cadence]
    required = [name for name, condition in roster["required"] if platform_matches(condition)]
    selected = [name for name in LANE_ORDER if name in lanes] + (["check_boundaries"] if args.check_boundaries else [])
    missing = [name for name in required if name not in selected]
    return {
        "cadence": args.cadence, "permitted_lanes": list(roster["permitted"]),
        "required_lanes": required, "selected_lanes": selected, "missing_required_lanes": missing,
        "complete": not missing, "explicit_shapes_required": roster["explicit_shapes"],
        "cap_seconds": CADENCE[args.cadence][1], "accounting_boundary": AGGREGATE_BOUNDARY,
    }


def lane_accounting(records, controls, elapsed_seconds):
    """Separate measured product work from transparency replay and runner overhead."""
    product = sum(record["measurement"]["wall_seconds"] for record in records)
    control = sum(row["control_measurement"]["wall_seconds"] for row in controls)
    return {"product_seconds": product, "transparency_control_seconds": control,
            "runner_overhead_seconds": max(0.0, elapsed_seconds - product - control),
            "transparency_replays": len(controls)}


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=path.name + ".", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            json.dump(value, output, indent=2, sort_keys=True, allow_nan=False)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def boundaries(path):
    from qualification_limits import check_boundaries
    return check_boundaries(path)


def file_sha256(path):
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise QualificationFailure(f"identity source is missing or not a regular file: {path}")
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def input_sha256(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     allow_nan=False).encode()).hexdigest()


def oracle_inventory():
    oracle_directory = Path(__file__).resolve().parent
    inventory = [{"name": name, "sha256": file_sha256(oracle_directory / name)} for name in (
        "qualification.py", "qualification_limits.py", "qualification_runtime.py",
        "qualification_environment.py", "qualification_fixtures.py", "qualification_corpus.py",
        "qualification_driver.rs", "discovery_smoke.py")]
    inventory.append({"name": "tests/fixtures/msbuild/qualification/limits.json",
                      "sha256": file_sha256(ROOT / "tests/fixtures/msbuild/qualification/limits.json")})
    return inventory


def observed_identity(args, runtime, lanes):
    """Bind actual selected authorities and loaded oracle sources, never CLI digests."""
    observed = runtime.setup_evidence.get("identity_inputs")
    if not isinstance(observed, dict) or any(
            not isinstance(observed.get(name), dict) or not observed[name]
            for name in ("toolchain", "runner")):
        raise QualificationFailure("Runtime did not provide observed toolchain/runner identity inputs")
    sha256_value(runtime.setup_evidence.get("implementation_sha256"), "current implementation")
    if args.corpus and args.corpus_host_kind == "framework":
        from qualification_runtime import _tree_identity
        host = args.corpus_host.resolve(strict=True)
        closures = observed["toolchain"]["dotnet"]["closures"]
        closures["framework-corpus-host"] = (
            closures["classic-host"] if runtime.classic_host == host else _tree_identity(host))
    corpus = {"schema": 1, "lanes": {}}
    for lane in sorted(lanes):
        corpus["lanes"][lane] = {
            "host_kind": args.corpus_host_kind if lane == "corpus" else
                         "framework" if lane == "legacy" else "sdk",
            "allow_restore": args.allow_restore if lane in {"fixtures", "capture_density", "corpus"} else False,
        }
    if args.fixtures or args.capture_density:
        from qualification_fixtures import MANIFEST
        path = ROOT / MANIFEST
        shapes = json.loads(path.read_text(encoding="utf-8"))
        for lane in ("fixtures", "capture_density"):
            if lane not in lanes:
                continue
            selected = (["baseline", "density-2x"] if lane == "capture_density" else
                        list(shapes["shapes"]) if args.shapes is None else list(args.shapes))
            if not selected or len(selected) != len(set(selected)) or not set(selected) <= set(shapes["shapes"]):
                raise QualificationFailure("identity: shape selection must be nonempty, unique and authored")
            if "density-2x" in selected and "baseline" not in selected:
                selected.append("baseline")
            corpus["lanes"][lane].update(
                selected_shapes=sorted(selected),
                manifest={"name": MANIFEST.as_posix(), "sha256": file_sha256(path)})
    if args.cache:
        # These authored inputs/expectations are code, bound below by oracle bytes.
        corpus["lanes"]["cache"]["authority"] = "qualification_fixtures.run_cache"
    if args.legacy:
        base = ROOT / "tests/fixtures/msbuild/evaluator"
        files = [{"name": "expected.json", "sha256": file_sha256(base / "expected.json")}]
        for name in ("Client40", "Classic472", "Classic48", "Twin", "shared"):
            directory = base / name
            if not directory.is_dir() or directory.is_symlink():
                raise QualificationFailure("identity: missing/nonregular legacy fixture directory")
            for parent, dirs, names in os.walk(directory, followlinks=False):
                dirs.sort()
                if any((Path(parent) / child).is_symlink() for child in dirs):
                    raise QualificationFailure("identity: symlink legacy fixture directory")
                for child in sorted(names):
                    path = Path(parent) / child
                    files.append({"name": path.relative_to(base).as_posix(), "sha256": file_sha256(path)})
        corpus["lanes"]["legacy"]["authored_inputs"] = sorted(files, key=lambda row: row["name"])
    if args.corpus:
        from qualification_corpus import MANIFEST, expectation_name, load_manifest
        path = Path(args.manifest or MANIFEST).resolve(strict=True)
        entries = load_manifest(path)
        authorities = []
        for entry in entries:
            name = expectation_name(entry, args.corpus_host_kind)
            expected_path = path.parent / name
            expected_sha256 = file_sha256(expected_path)
            expected = json.loads(expected_path.read_text(encoding="utf-8"))
            capture_name = expected.get("capture_path")
            if not isinstance(capture_name, str) or Path(capture_name).name != capture_name:
                raise QualificationFailure("identity: native capture authority must be adjacent")
            authorities.append({
                "repository": entry["id"], "pinned_sha": entry["sha"],
                "expected": {"name": name, "sha256": expected_sha256},
                "native_capture": {"name": capture_name, "sha256": file_sha256(path.parent / capture_name)},
            })
        corpus["lanes"]["corpus"].update(
            manifest={"name": "corpus-manifest.json", "sha256": file_sha256(path)},
            platform=sys.platform, authorities=sorted(authorities, key=lambda row: row["repository"]))
    oracle = oracle_inventory()
    identity = {"corpus_sha256": input_sha256(corpus),
                "toolchain_sha256": input_sha256(observed["toolchain"]),
                "runner_sha256": input_sha256(observed["runner"]),
                "oracle_sha256": input_sha256(oracle)}
    return identity, {"corpus": corpus, "oracle": oracle}


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    for lane in ("fixtures", "corpus", "legacy", "cache", "check-boundaries", "capture-density"):
        parser.add_argument("--" + lane, action="store_true")
    parser.add_argument("--repeat", type=int, default=2)
    parser.add_argument("--cadence", choices=CADENCE, default="qualification")
    parser.add_argument("--shapes", nargs="+", help="Explicit authored subset; omitted means every shape")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug" / ("tethys.exe" if os.name == "nt" else "tethys"))
    parser.add_argument("--sdk", type=Path, default=os.environ.get("TETHYS_SDK_MSBUILD_PATH"))
    parser.add_argument("--companion", type=Path, default=os.environ.get("TETHYS_WORKER_DISTRIBUTION", str(ROOT / "target/worker-dist/msbuild-evaluate")))
    parser.add_argument("--classic-host", type=Path, default=os.environ.get("TETHYS_VS_MSBUILD_PATH"))
    parser.add_argument("--environment", type=Path, help="Controlled native environment prepared by corpus capture")
    parser.add_argument("--allow-restore", action="store_true", help="Separately authorize normal fixture/corpus Restore after isolated package preparation")
    parser.add_argument("--corpus-host-kind", choices=("sdk", "framework"), default="sdk")
    parser.add_argument("--corpus-host", type=Path, help="Explicit VS MSBuild directory for framework corpus evidence")
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--calibration", type=Path)
    parser.add_argument("--work-root", type=Path, help="New disposable directory; retained for investigation")
    parser.add_argument("--output", type=Path, default=ROOT / "target/qualification" / time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()) / "report.json")
    result = parser.parse_args()
    if not any(getattr(result, lane) for lane in ("fixtures", "corpus", "legacy", "cache", "check_boundaries", "capture_density")):
        parser.error("select at least one evidence lane or --check-boundaries")
    roster = LANE_ROSTER[result.cadence]
    selected = [name for name in LANE_ORDER if getattr(result, name)] + (["check_boundaries"] if result.check_boundaries else [])
    unpermitted = [name for name in selected if name not in roster["permitted"]]
    if unpermitted:
        parser.error(f"{result.cadence} cadence does not own {', '.join(unpermitted)}; permitted: {', '.join(roster['permitted'])}")
    missing = [name for name, condition in roster["required"]
               if platform_matches(condition) and name not in selected]
    if missing:
        parser.error(f"{result.cadence} cadence requires {', '.join(missing)}; a partial selection is not {result.cadence} acceptance")
    if roster["explicit_shapes"] and "fixtures" in selected and result.shapes is None:
        parser.error(f"{result.cadence} cadence requires an explicit authored --shapes subset; "
                     "the full formula roster belongs to qualification")
    if result.cadence in {"nightly", "release"} and result.repeat != CADENCE[result.cadence][0]:
        parser.error(f"{result.cadence} requires exactly {CADENCE[result.cadence][0]} repetitions "
                     f"(calibrated ratios are computed over that exact count)")
    if result.repeat < CADENCE[result.cadence][0]:
        parser.error(f"{result.cadence} requires at least {CADENCE[result.cadence][0]} repetitions")
    if any(getattr(result, lane) for lane in ("fixtures", "corpus", "legacy", "cache", "capture_density")) and result.environment is None:
        parser.error("every runtime lane requires an explicit frozen --environment")
    if (result.fixtures or result.capture_density) and (result.environment is None or not result.allow_restore):
        parser.error("authored restore-qualified fixtures require --environment and --allow-restore")
    if result.corpus and result.corpus_host_kind == "framework" and result.corpus_host is None:
        parser.error("framework corpus evidence requires --corpus-host")
    if result.capture_density and (result.cadence != "qualification" or result.calibration):
        parser.error("density capture is unreviewed one-off evidence, not calibrated qualification")
    return result


def run(args, report):
    lanes = [name for name in LANE_ORDER if getattr(args, name)]
    report["cadence_assignment"] = assignment(args, lanes)
    if args.check_boundaries:
        report["boundaries"] = boundaries(ROOT / "tests/fixtures/msbuild/qualification/limits.json")
    if not lanes:
        return
    if args.sdk is None:
        raise QualificationFailure("select installed SDK explicitly with --sdk or TETHYS_SDK_MSBUILD_PATH")
    baseline = None
    if not args.calibration and args.cadence in {"nightly", "release"}:
        raise QualificationFailure("nightly/release require reviewed fourteen-observation calibration; capture is not approval")
    from qualification_runtime import Runtime
    from qualification_fixtures import capture_density, run_cache, run_fixtures, run_legacy
    from qualification_corpus import run_corpus
    from qualification_environment import controlled_environment, verify_environment
    oracle_before = oracle_inventory()
    report["stable_inputs"] = {
        "status": "pending", "oracle_before_prepare_sha256": input_sha256(oracle_before),
    }
    if args.environment is None:
        raise QualificationFailure("every runtime lane requires an explicit frozen --environment")
    verify_environment(args.environment)
    environment = controlled_environment(args.environment, args.sdk)
    report["native_environment"] = verify_environment(args.environment)
    classic_host = args.classic_host.resolve(strict=True) if args.classic_host else None
    if args.corpus and args.corpus_host_kind == "framework" and not args.legacy:
        classic_host = args.corpus_host.resolve(strict=True)
    work = (args.work_root or args.output.parent / "work").resolve()
    work.mkdir(parents=True, exist_ok=False)
    report["work_root"] = str(work)
    cap_seconds = CADENCE[args.cadence][1]
    runtime = Runtime(ROOT, args.output.parent / "runtime", cli=args.cli.resolve(strict=True),
                      sdk=args.sdk.resolve(strict=True), companion=args.companion.resolve(strict=True),
                      classic_host=classic_host,
                      environment=environment, timeout=cap_seconds)
    invocation_started = time.monotonic()
    accounting = {"boundary": AGGREGATE_BOUNDARY, "cadence_cap_seconds": cap_seconds,
                  "preparation_seconds": None, "lane_seconds": 0.0, "product_seconds": 0.0,
                  "transparency_control_seconds": 0.0, "transparency_replays": 0,
                  "runner_overhead_seconds": 0.0, "aggregate_seconds": None, "status": "running"}
    report["accounting"] = accounting
    try:
        runtime.prepare()
        accounting["preparation_seconds"] = time.monotonic() - invocation_started
        report["runtime"] = runtime.setup_evidence
        oracle_after_prepare = oracle_inventory()
        report["stable_inputs"]["oracle_after_prepare_sha256"] = input_sha256(oracle_after_prepare)
        if oracle_after_prepare != oracle_before:
            report["stable_inputs"]["status"] = "fail"
            raise QualificationFailure("oracle source bytes changed during Runtime.prepare; execution cannot be relabeled")
        identity, identity_inputs = observed_identity(args, runtime, lanes)
        if identity_inputs["oracle"] != oracle_before:
            report["stable_inputs"]["status"] = "fail"
            raise QualificationFailure("oracle source bytes changed while observing selected identity")
        report["stable_inputs"]["oracle_preparation"] = "pass"
        report["stable_inputs"]["corpus_before_lanes_sha256"] = identity["corpus_sha256"]
        report.update(identity=identity, identity_inputs=identity_inputs,
                      implementation_sha256=runtime.setup_evidence["implementation_sha256"])
        if args.calibration:
            calibration_sha256 = file_sha256(args.calibration)
            manifest = json.loads(args.calibration.read_text(encoding="utf-8"))
            baseline = reviewed_baseline(manifest, identity=identity)
            if file_sha256(args.calibration) != calibration_sha256:
                report["stable_inputs"]["status"] = "fail"
                raise QualificationFailure("calibration authority changed while being read")
            report["calibration"] = {
                "status": "reviewed", "manifest_sha256": calibration_sha256,
                "identity": identity, "implementation_sha256": manifest["implementation_sha256"],
            }
        else:
            report["calibration"] = {"status": "not-established", "ratio_acceptance": False}
        for lane in lanes:
            if cap_seconds - (time.monotonic() - invocation_started) <= 0:
                raise QualificationFailure(f"{args.cadence} suite budget exhausted before lane {lane}")
            # Runtime.timeout derives from an absolute deadline, so every preparation, product and
            # control subprocess recomputes its own remaining budget; no per-lane snapshot exists.
            controls = runtime.setup_evidence["controls"]
            control_start = len(controls)
            start = time.monotonic()
            if lane == "capture_density":
                records = capture_density(runtime, work / lane, repeat=args.repeat, allow_restore=args.allow_restore)
            elif lane == "fixtures":
                records = run_fixtures(runtime, work / lane, repeat=args.repeat, shapes=args.shapes,
                                       allow_restore=args.allow_restore)
            elif lane == "cache":
                records = run_cache(runtime, work / lane)
            elif lane == "legacy":
                records = run_legacy(runtime, work / lane, repeat=args.repeat)
            else:
                records = run_corpus(runtime, work / lane, repeat=args.repeat, manifest_path=args.manifest,
                                     allow_restore=args.allow_restore, host_kind=args.corpus_host_kind,
                                     host=args.corpus_host)
            elapsed = time.monotonic() - start
            for record in records:
                query = bool(record.get("correctness", {}).get("evaluator_free"))
                check_measurement(record["measurement"], indexing=not query)
            lane_totals = lane_accounting(records, controls[control_start:], elapsed)
            for name, value in lane_totals.items():
                accounting[name] = accounting.get(name, 0) + value
            accounting["lane_seconds"] += elapsed
            accounting["aggregate_seconds"] = time.monotonic() - invocation_started
            accounting["runner_overhead_seconds"] = max(
                0.0, accounting["lane_seconds"] - accounting["product_seconds"] - accounting["transparency_control_seconds"])
            # The cap binds the aggregate invocation, not each lane's private clock.
            check_elapsed(accounting["aggregate_seconds"], args.cadence)
            query_times = [row["measurement"]["wall_seconds"] for row in records if row.get("correctness", {}).get("evaluator_free")]
            lane_report = {"status": "pass", "elapsed_seconds": elapsed, "records": records, **lane_totals}
            if lane == "cache":
                lane_report["repeat_policy"] = (
                    "single authored cold/hit/import/forced/query transition sequence; --repeat does not apply; "
                    "records carry no workload id and are excluded from calibrated ratio grouping")
            else:
                lane_report["repeat_applied"] = args.repeat
            if query_times:
                lane_report["query_budget"] = check_queries(query_times)
            report["lanes"][lane] = lane_report
            atomic_json(args.output, report)
        accounting["status"] = "pass"
        if baseline is not None:
            grouped = {}
            for lane in report["lanes"].values():
                for row in lane["records"]:
                    workload = row.get("workload")
                    if workload is not None:
                        grouped.setdefault(workload, []).append(row["measurement"])
            if set(grouped) != set(baseline):
                raise QualificationFailure("calibrated workload set differs from measured workload set")
            report["ratios"] = {key: check_ratios(rows, baseline[key], cadence=args.cadence) for key, rows in grouped.items()}
        oracle_after_lanes = oracle_inventory()
        report["stable_inputs"]["oracle_after_lanes_sha256"] = input_sha256(oracle_after_lanes)
        if oracle_after_lanes != oracle_before:
            report["stable_inputs"]["status"] = "fail"
            raise QualificationFailure("oracle source bytes changed during lane execution")
        final_identity, final_inputs = observed_identity(args, runtime, lanes)
        report["stable_inputs"]["corpus_after_lanes_sha256"] = final_identity["corpus_sha256"]
        if final_inputs != identity_inputs or final_identity != identity:
            report["stable_inputs"]["status"] = "fail"
            raise QualificationFailure("selected authority or identity inputs changed during lane execution")
        if args.calibration and file_sha256(args.calibration) != calibration_sha256:
            report["stable_inputs"]["status"] = "fail"
            raise QualificationFailure("reviewed calibration authority changed during lane execution")
        report["stable_inputs"].update(
            status="pass", oracle_execution="pass", selected_authorities="pass",
            calibration_authority="pass" if args.calibration else "not selected")
    finally:
        runtime.close()


def main():
    args = arguments()
    report = {"schema": 1, "claim": "C12", "status": "running", "cadence": args.cadence,
              "repeat": args.repeat, "platform": platform.platform(), "lanes": {},
              "acceptance_scope": "selected evidence lanes only, checked against the cadence roster; the cadence cap "
                                  "binds the aggregate runner invocation; calibrated ratios require a reviewed "
                                  "fourteen-observation authority; assembled S6 acceptance is separately reviewed"}
    exit_code = 0
    try:
        run(args, report)
        report["status"] = "captured-awaiting-review" if args.capture_density else "pass"
    except Exception as error:
        report["status"] = "fail"
        report["error"] = f"{type(error).__name__}: {error}"
        exit_code = 1
    finally:
        atomic_json(args.output, report)
    print(json.dumps({"status": report["status"], "report": str(args.output), "error": report.get("error")}))
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
