#!/usr/bin/env python3
"""Derive proposed reviewed corpus standing from the committed native captures.

This is a *proposal generator*, not an approver. It maps native evidence to the
approved rvr5 reason taxonomy so a maintainer can review and sign each repository.
Nothing here writes a `review: reviewed` manifest; the runner refuses proposals.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = ROOT / "tests/fixtures/msbuild/qualification/corpus.json"

# Native code -> approved reason. Evaluation failures are mapped separately from
# restore failures because the product reaches them at different stages.
SDK_CODES = {"MSB4236", "MSB4242", "NETSDK1045"}
TOOLCHAIN_CODES = {"NETSDK1147", "NETSDK1080"}


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def load(path):
    raw = Path(path).read_bytes()
    return json.loads(gzip.decompress(raw) if str(path).endswith(".gz") else raw)


def code_reason(codes):
    if any(code in SDK_CODES for code in codes):
        return "sdk-unresolved"
    if any(code in TOOLCHAIN_CODES for code in codes):
        return "toolchain-unavailable"
    return "evaluation-failed"


def excerpt(record, limit=240):
    text = (record.get("stderr") or "").strip().splitlines()
    return (text[-1] if text else "")[:limit]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--output", type=Path, default=ROOT / "target/qualification/corpus-review-proposal")
    parser.add_argument("--host-kind", default="sdk")
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    args.output.mkdir(parents=True, exist_ok=True)
    summary, judgment = {}, []

    for entry in manifest["repositories"]:
        base = args.manifest.parent
        expected = json.loads((base / f"{entry['id']}.{sys.platform}.{args.host_kind}.expected.json").read_text())
        capture = load(base / expected["capture_path"])
        assert digest(capture) == expected["capture_digest"], f"capture digest drift: {entry['id']}"
        restore = capture["restore"]
        authorized = bool(restore["authorized"])
        restore_by_project = {row["project"]: row for row in restore["records"]}
        outer = {row["project"]: row for row in capture["records"] if row["target_framework"] is None}
        inner = {(row["project"], row["target_framework"]): row for row in capture["records"]
                 if row["target_framework"] is not None}

        slots = set()
        for project, row in outer.items():
            if row["exit_code"] != 0:
                continue
            props = row["metadata"]["Properties"]
            selectors = [value.strip() for value in (props.get("TargetFrameworks") or "").split(";") if value.strip()]
            for selector in selectors or [props.get("TargetFramework") or None]:
                slots.add((project, selector))

        units, projects = [], []
        for project in capture["source"]["projects"]:
            restore_row = restore_by_project.get(project)
            restore_failed = authorized and restore_row is not None and restore_row["exit_code"] != 0
            project_slots = sorted(key for key in slots if key[0] == project)
            if not project_slots:
                row = outer.get(project)
                codes = list(row.get("native_codes") or []) if row else []
                reason = code_reason(codes) if row else "evaluation-failed"
                projects.append({
                    "project": project, "standing": "indeterminate", "reason": reason,
                    "evidence": f"direct outer evaluation exit {row['exit_code'] if row else 'absent'}; "
                                f"native codes {codes}; {excerpt(row) if row else 'no record'}",
                    "rationale": f"Native enumeration failed before any framework slot existed ({reason}).",
                    "restore_input_status": "unavailable",
                    "restore_evidence": "no framework slot reached restore",
                })
                judgment.append({"repo": entry["id"], "project": project, "kind": "enumeration-failure",
                                 "native_codes": codes, "proposed_reason": reason})
                continue
            for key in project_slots:
                row = inner.get(key) or outer.get(project)
                codes = list(row.get("native_codes") or [])
                if restore_failed:
                    units.append({
                        "project": key[0], "target_framework": key[1], "standing": "indeterminate",
                        "reason": "restore-failed",
                        "evidence": f"authorized native Restore exit {restore_row['exit_code']}; "
                                    f"codes {restore_row.get('native_codes')}; {excerpt(restore_row)}",
                        "rationale": "Framework metadata was evaluated, but the authorized Restore failed for this project.",
                    })
                elif row["exit_code"] == 0:
                    props = row["metadata"]["Properties"]
                    units.append({
                        "project": key[0], "target_framework": key[1], "standing": "confirmed",
                        "rationale": f"Direct selected-host evaluation exit 0; framework identity "
                                     f"{props.get('TargetFramework') or props.get('TargetFrameworkVersion')}; "
                                     f"{len(row['metadata']['Items']['Compile'])} Compile items.",
                    })
                else:
                    reason = code_reason(codes)
                    units.append({
                        "project": key[0], "target_framework": key[1], "standing": "indeterminate",
                        "reason": reason, "evidence": f"direct inner evaluation exit {row['exit_code']}; "
                                                     f"codes {codes}; {excerpt(row)}",
                        "rationale": f"Direct inner-framework evaluation failed ({reason}).",
                    })
            if restore_failed:
                projects.append({
                    "project": project, "standing": "indeterminate", "reason": "restore-failed",
                    "evidence": f"authorized native Restore exit {restore_row['exit_code']}; "
                                f"codes {restore_row.get('native_codes')}; {excerpt(restore_row)}",
                    "rationale": "Evaluation succeeded, but the authorized Restore failed for this project.",
                    "restore_input_status": "invalid",
                    "restore_evidence": f"native Restore exit {restore_row['exit_code']} with codes "
                                        f"{restore_row.get('native_codes')}",
                })
                judgment.append({"repo": entry["id"], "project": project, "kind": "restore-failure",
                                 "native_codes": list(restore_row.get("native_codes") or []),
                                 "proposed_reason": "restore-failed"})
            else:
                relation = sorted({row["status"] for row in (restore_row or {}).get("final_input_relation", [])})
                status = "current" if restore_row is not None and restore_row["exit_code"] == 0 else "not-required"
                projects.append({
                    "project": project, "standing": "confirmed",
                    "rationale": f"Every enumerated framework evaluated with exit 0"
                                 + (" and the authorized Restore completed with unchanged final inputs."
                                    if status == "current" else "; no Restore was required."),
                    "restore_input_status": status,
                    "restore_evidence": (f"native Restore exit 0; final input relation {relation}"
                                         if status == "current" else "no Restore observation for this project"),
                })

        confirmed_units = sum(1 for row in units if row["standing"] == "confirmed")
        proposed = {
            "schema": 1, "id": entry["id"], "sha": entry["sha"],
            "capture_path": expected["capture_path"], "capture_digest": expected["capture_digest"],
            "review": {"status": "proposed-awaiting-signoff", "reviewer": None, "rationale": None},
            "expected_projects": projects, "expected_units": units,
            "expected_exit": 0 if confirmed_units == len(units) and units else 1,
        }
        target = args.output / f"{entry['id']}.{sys.platform}.{args.host_kind}.expected.proposed.json"
        target.write_text(json.dumps(proposed, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        summary[entry["id"]] = {
            "projects": len(projects), "units": len(units), "confirmed_units": confirmed_units,
            "indeterminate_units": len(units) - confirmed_units,
            "confirmed_projects": sum(1 for row in projects if row["standing"] == "confirmed"),
            "indeterminate_projects": sum(1 for row in projects if row["standing"] == "indeterminate"),
            "expected_exit": proposed["expected_exit"], "proposal": target.name,
        }

    (args.output / "summary.json").write_text(
        json.dumps({"summary": summary, "judgment_cases": judgment}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"summary": summary, "judgment_case_count": len(judgment),
                      "judgment_kinds": sorted({row["kind"] for row in judgment}),
                      "output": str(args.output)}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
