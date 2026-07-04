# tethys-haw5 S8 regression measurement (claim C11), 2026-07-03

Workspace: rand-0.8.5 (real crate from the cargo registry; 4 genuine
`#[deprecated]` items, 2 with in-crate callers — the probe3 corpus).

Old binary: built from `main` @ fa1b93e in a throwaway worktree.
New binary: feat/tethys-haw5-csharp-obsolete @ S8.

| Measurement | Result |
|---|---|
| Human mode, same index, old vs new binary | **byte-identical** |
| `--json`, same index, normalized (`del(.deprecated[].symbol.error)`) | **empty diff** |
| `--json`, new binary's own rebuilt index vs frozen design-time baseline (`baseline-rand-deprecated.json`) | **empty normalized diff** |
| Raw JSON delta | exactly 4 × `"error": null` insertions (one per symbol) + their trailing commas |

Conclusion: Rust deprecated-callers behavior is unchanged (AC5) modulo the
approved always-null `error` key. Deterministic CI fences for this claim:
`cli_json_key_set_identical_across_languages` (key set incl. `error`),
`detects_all_kinds` + `cli_json_envelope_stable_and_parseable` (Rust
since/note values pinned — any parser-dispatch corruption fails these).
