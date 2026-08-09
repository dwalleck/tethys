#!/usr/bin/env python3
"""Compare tethys-7a6a public reachability with an independent raw-SQLite BFS.

IDs, depths, paths, discovery order, and real ``symbols.is_test`` values must
match exactly. ``--legacy-is-test`` reproduces the pre-cutover audit: forward
wrappers decoded ``call_edges.call_count`` as ``is_test`` under tethys-6bui,
and every difference must be explained by that mechanism.

Exit code 0 means full agreement under the selected projection contract.

Usage: compare.py [--legacy-is-test] <probe.out> <oracle.out>
"""
import re
import sys

legacy_is_test = "--legacy-is-test" in sys.argv[1:]
paths = [argument for argument in sys.argv[1:] if argument != "--legacy-is-test"]
if len(paths) != 2:
    raise SystemExit("usage: compare.py [--legacy-is-test] <probe.out> <oracle.out>")
probe_file, oracle_file = paths
PAT = re.compile(
    r"ENTRY (\S+) seq=(\d+) id=(\d+) depth=(\d+) is_test=(\S+) qn=(\S+) "
    r"path=\[([^\]]*)\](?: edge_count=(\S+))?"
)


def load(path):
    meta = {}
    entries = {}
    tag = None
    for line in open(path):
        line = line.strip()
        if line.startswith("META "):
            tag = line.split()[1]
            meta[tag] = line
        elif line.startswith("ENTRY "):
            m = PAT.match(line)
            assert m, f"unparseable: {line}"
            t, seq, i, d, it, qn, p, ec = m.groups()
            entries.setdefault(t, []).append(
                {
                    "seq": int(seq),
                    "id": int(i),
                    "depth": int(d),
                    "is_test": it,
                    "qn": qn,
                    "path": tuple(int(x) for x in p.split(",")) if p else (),
                    "edge_count": int(ec) if ec and ec != "None" else None,
                }
            )
    return meta, entries


pm, pe = load(probe_file)
om, oe = load(oracle_file)

ok = True
# Every META slice must be present on both sides with matching declared counts
# (pins empty slices such as depth-0, which have no ENTRY lines).
for tag in sorted(set(pm) | set(om)):
    if tag not in pm or tag not in om:
        print(f"[{tag}] FAIL: META present in probe={tag in pm} oracle={tag in om}")
        ok = False
        continue
    pc = int(re.search(r"count=(\d+)", pm[tag]).group(1))
    oc = int(re.search(r"count=(\d+)", om[tag]).group(1))
    if pc != oc or pc != len(pe.get(tag, [])):
        print(f"[{tag}] FAIL: count probe={pc} oracle={oc} parsed_probe={len(pe.get(tag, []))}")
        ok = False
    elif tag not in pe:
        print(f"[{tag}] PASS: empty slice (count=0) on both sides")
for tag in sorted(set(pe) | set(oe)):
    if tag not in pe or tag not in oe:
        print(f"[{tag}] FAIL: present in probe={tag in pe} oracle={tag in oe}")
        ok = False
        continue
    probe, oracle = pe[tag], oe[tag]
    traversal = [
        (e["id"], e["depth"], e["path"]) for e in probe
    ] == [(e["id"], e["depth"], e["path"]) for e in oracle]
    if not traversal:
        print(f"[{tag}] FAIL: traversal mismatch")
        for i, (a, b) in enumerate(
            zip(
                [(e["id"], e["depth"], e["path"]) for e in probe],
                [(e["id"], e["depth"], e["path"]) for e in oracle],
            )
        ):
            if a != b:
                print(f"  first divergence seq={i}: probe={a} oracle={b}")
                break
        ok = False
    else:
        print(f"[{tag}] PASS: {len(probe)} entries, ids/depths/paths/order identical")

    # Cross-cutting invariants on both sides of every slice.
    for side, rows in (("probe", probe), ("oracle", oracle)):
        ids = [e["id"] for e in rows]
        assert len(set(ids)) == len(ids), f"{side}/{tag}: duplicate target ids"
        for e in rows:
            assert e["path"] or e["depth"] == 0, f"{side}/{tag}: empty path at depth>0"
            assert len(e["path"]) == e["depth"], (
                f"{side}/{tag}: path.len {len(e['path'])} != depth {e['depth']}"
            )

    # Projection audit. The default is the post-cutover canonical contract;
    # legacy mode retains the pre-cutover defect evidence.
    if legacy_is_test and tag.startswith("fwd"):
        mech_ok = all(
            (p["is_test"] == "true") == (o["edge_count"] != 0)
            for p, o in zip(probe, oracle)
        )
        flipped = sum(
            p["is_test"] != o["is_test"] for p, o in zip(probe, oracle)
        )
        real_non_test_mislabeled = sum(
            o["is_test"] == "false" and p["is_test"] != o["is_test"]
            for p, o in zip(probe, oracle)
        )
        if not mech_ok:
            print(f"[{tag}] FAIL: legacy is_test differences not explained by call_count")
            ok = False
        else:
            print(
                f"[{tag}] legacy is_test audit: {flipped} flips, "
                f"{real_non_test_mislabeled} real non-test targets mislabeled "
                f"as test; every flip == (edge_count != 0) PASS"
            )
    elif [p["is_test"] for p in probe] != [o["is_test"] for o in oracle]:
        print(f"[{tag}] FAIL: is_test differs from real symbols.is_test")
        ok = False
    else:
        print(f"[{tag}] is_test matches real symbols.is_test PASS")

# Self-loop / cycle fact: the source must never be returned as its own target.
for tag in sorted(pe):
    for side, rows in (("probe", pe[tag]), ("oracle", oe.get(tag, []))):
        src_id = int(re.search(r"source_id=(\d+)", pm[tag]).group(1))
        if src_id in [e["id"] for e in rows]:
            print(f"[{tag}] FAIL: {side} returned the source as a target")
            ok = False
print("source never returned as its own target (self-loop/cycle fact): PASS")

print("RESULT:", "AGREE" if ok else "DISAGREE")
sys.exit(0 if ok else 1)
