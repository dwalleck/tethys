#!/usr/bin/env python3
"""Probe (tethys-1dcc): find #[test] fns whose bodies are near-duplicates.

Extraction: regex finds `#[test]` attrs (skipping fns already under #[rstest]),
then brace-matching captures each fn body. Normalization: strip comments,
collapse whitespace, fold string/char/int literals to S/C/N. Clustering:
exact tier = same normalized body; fuzzy tier = difflib ratio >= 0.90
within a file. Output: per-file groups with fn names and sizes.
"""
import re, difflib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FILES = sorted((ROOT / "tests").glob("*.rs")) + sorted((ROOT / "src").rglob("*.rs"))

def strip_strings(text):
    # Fixture code is embedded as string literals; blank them out FIRST so
    # `#[test]` inside r#"..."# is never mistaken for a real test, and braces
    # inside strings cannot break body extraction.
    text = re.sub(r'r#"(?:[^"]|"(?!#))*"#', '"S"', text, flags=re.S)
    return re.sub(r'"(?:[^"\\\n]|\\.)*"', '"S"', text)

def fn_bodies(text):
    out = []
    for m in re.finditer(r"#\[test\]\s*(?:#\[[^\]]*\]\s*)*fn\s+(\w+)", text):
        # skip if the preceding attr block already has #[rstest]/#[case]
        prefix = text[max(0, m.start() - 300):m.start()]
        if "#[rstest]" in prefix.split("\n\n")[-1]:
            continue
        brace = text.find("{", m.end())
        if brace < 0:
            continue
        depth, i = 1, brace + 1
        while depth and i < len(text):
            depth += {"{": 1, "}": -1}.get(text[i], 0)
            i += 1
        out.append((m.group(1), text[brace:i]))
    return out

def normalize(body):
    body = re.sub(r"//[^\n]*", "", body)
    body = re.sub(r"'(?:[^'\\]|\\.)'", "C", body)
    body = re.sub(r"\b\d+\b", "N", body)
    return re.sub(r"\s+", " ", body).strip()

total, groups = 0, []
for f in FILES:
    tests = [(n, normalize(b)) for n, b in fn_bodies(strip_strings(f.read_text()))]
    total += len(tests)
    used = set()
    for i, (n1, b1) in enumerate(tests):
        if i in used:
            continue
        cluster = [n1]
        for j in range(i + 1, len(tests)):
            if j in used:
                continue
            n2, b2 = tests[j]
            if b1 == b2 or difflib.SequenceMatcher(None, b1, b2).ratio() >= 0.90:
                cluster.append(n2)
                used.add(j)
        if len(cluster) >= 2:
            groups.append((str(f.relative_to(ROOT)), cluster))

print(f"parsed #[test] fns (non-rstest): {total}")
collapse = sum(len(c) - 1 for _, c in groups)
print(f"groups >=2 near-identical: {len(groups)}; fns collapsible: {collapse}\n")
for path, cluster in sorted(groups, key=lambda g: -len(g[1])):
    print(f"{path}  [{len(cluster)}]")
    for name in cluster:
        print(f"    {name}")
