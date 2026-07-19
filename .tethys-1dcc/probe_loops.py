#!/usr/bin/env python3
"""Probe extension (tethys-1dcc): find #[test] fns that hand-roll
parameterization as a for-loop over a literal table with asserts inside.

Reuses probe.py's string-stripping + extraction. Heuristic: body contains
`for <pat> in` over a literal array/vec (`[`, `vec![`, `&[`) AND an
`assert` after the loop head. Table size = top-level element count of the
literal (commas at bracket depth 1), a rough lower bound.
"""
import re

from probe import ROOT, FILES, strip_strings, fn_bodies

LOOP = re.compile(r"for\s+.+?\s+in\s+(?:&\s*)?(?:vec!\s*)?\[", re.S)

def table_size(body, start):
    depth, i, commas = 0, start, 0
    while i < len(body):
        c = body[i]
        if c in "[(":
            depth += 1
        elif c in "])":
            depth -= 1
            if depth == 0:
                break
        elif c == "," and depth == 1:
            commas += 1
        i += 1
    return commas + 1

hits = 0
for f in FILES:
    for name, body in fn_bodies(strip_strings(f.read_text())):
        m = LOOP.search(body)
        if m and "assert" in body[m.start():]:
            size = table_size(body, body.index("[", m.start()))
            print(f"{f.relative_to(ROOT)}  {name}  table≈{size}")
            hits += 1
print(f"\nloop-table test fns: {hits}")
