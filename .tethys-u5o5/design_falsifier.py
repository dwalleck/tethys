#!/usr/bin/env python3
"""Cheapest falsifier for path-based start-min cycle enumeration."""

NAMES = {1: "a.rs", 2: "b.rs", 3: "c.rs", 4: "d.rs", 5: "self.rs"}
EDGES = {
    1: (2, 3),  # a-b-c-a and a-c-a overlap
    2: (3, 1),
    3: (1, 2),
    4: (),
    5: (5,),
}


def canonical(path):
    pivot = min(range(len(path)), key=lambda index: NAMES[path[index]])
    return tuple(path[pivot:] + path[:pivot])


def exhaustive(node, start, path, found):
    for nxt in EDGES[node]:
        if nxt == start:
            found.add(canonical(path))
        elif nxt not in path:
            exhaustive(nxt, start, path + [nxt], found)


def start_min_cycles():
    found = set()
    for start in sorted(NAMES, key=NAMES.__getitem__):
        def walk(node, path):
            for nxt in EDGES[node]:
                if nxt == start:
                    found.add(tuple(path))
                elif nxt not in path and NAMES[nxt] >= NAMES[start]:
                    walk(nxt, path + [nxt])
        walk(start, [start])
    return found

oracle = set()
for start in NAMES:
    exhaustive(start, start, [start], oracle)
expected = {
    ("a.rs", "b.rs"),
    ("a.rs", "b.rs", "c.rs"),
    ("a.rs", "c.rs"),
    ("a.rs", "c.rs", "b.rs"),
    ("b.rs", "c.rs"),
    ("self.rs",),
}
oracle_paths = {tuple(NAMES[node] for node in cycle) for cycle in oracle}
candidate_paths = {
    tuple(NAMES[node] for node in cycle) for cycle in start_min_cycles()
}
assert oracle_paths == expected, (oracle_paths, expected)
assert candidate_paths == expected, (candidate_paths, expected)
print("PASS: exhaustive oracle and start-min candidate agree on 6 cycles")
