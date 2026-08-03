# Probe findings

## Smallest proven question

Does the public cycles command expose the real indexed workspace's known two-file `src/cargo.rs` ↔ `src/lib.rs` dependency cycle as one canonical, non-repeating cycle?

## Probe and independent oracle

- Probe: `.tethys-u5o5/probe.py` rebuilds the real tethys workspace index, invokes `cargo run -- cycles`, and parses the public cycle output.
- Oracle: Python reads the SQLite `files` and `file_deps` tables directly, checks both directed edge rows for the two selected indexed paths, and independently derives the expected canonical pair.

Observed agreement:

```text
SUT canonical two-file cycles: [('src/cargo.rs', 'src/lib.rs')]
Oracle direct-edge cycles:      [('src/cargo.rs', 'src/lib.rs')]
```

The probe indexed 115 files and the current CLI reported 35 cycles before selecting the target pair.

## What I learned

`Cycle.files` already omits the closing repeat at the library boundary; only `src/cli/cycles.rs` appends the first path for display. The current graph path bulk-loads dependency edges, but `ids_to_cycle` performs one `get_file_by_id` query per cycle member, and cycle canonicalization uses numeric file IDs while traversal/result order is HashMap/HashSet-dependent. The requested change therefore needs both a set-oriented file map and path-based deterministic canonicalization, not a new public result shape.

## Scope boundary

This probe proves the existing API's basic directed-cycle projection on real data. It does not claim that the current implementation enumerates every simple cycle, canonicalizes by relative path, or meets the two-query budget; those are the falsifiable claims and regression fences for the implementation.
