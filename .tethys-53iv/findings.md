# tethys-53iv probe findings (prove-it-prototype, 2026-07-09)

Probes ran against the ticket's exact repro (indexed with the real binary)
and against **real production data**: tethys's own source (4612 method
calls), copied to scratchpad and self-indexed. Probe scripts: `probe1.sh`
(repro, end to end), `probe2/` (standalone tree-sitter receiver-shape
classifier, pinned to the exact grammar versions in Cargo.lock),
`probe3.sh` (what the index actually binds). Outputs committed alongside.

## The probe

1. **probe1** — the ticket repro: `Thing::unwrap` + `Option::unwrap` in one
   file. Result: BOTH `unwrap` calls bind `same_file` to `Thing::unwrap`;
   fabricated `use_external → unwrap` call edge; `callers Thing::unwrap`
   over-attributes (2 callers, 1 real); `panic-points` reports **0** (the
   genuine `x.unwrap()` invisible). The ticket reproduces exactly.
2. **probe2** — receiver-shape classifier over `src/` (raw tree-sitter walk,
   no tethys code): every `recv.m(...)` classified by receiver kind and
   local type-derivability, cross-referenced against in-crate method names.
3. **probe3** — self-index bind reality: strategy distribution for
   call→method binds, std-collision-name bind targets, panic-points counts.

## Oracle (independent)

Raw-text `grep -rn` over the source. Agreement achieved:

- **Repro slice**: rustc semantics hand-derived (2 `.unwrap()` sites; the
  line-7 call is `Option::unwrap`, external; `Thing::unwrap`'s only true
  caller is `use_internal`) — the probe's output matches the ticket's
  claimed misbehavior item by item.
- **`self`-receiver slice (probe2)**: probe 174 vs grep 165, every
  discrepancy diagnosed to the oracle — one grep hit inside a comment
  (`call_edges.rs:116`), the rest multiline chains (`self\n.method(...)`)
  that line-based grep structurally misses.
- **Bound-site slice (probe3)**: the 8 same-file `is_empty` binds hand-read
  against source — receivers identified per line (see Measurements).

## What I learned that I did not know before running the probe

> **Rust method-call extraction discards the receiver entirely
> (`rust.rs:550-556`: `x.unwrap()` → bare `name: "unwrap", path: None`),
> which is the exact asymmetry that makes C# conservative and Rust
> phantom-prone — C# folds the receiver into a qualified name that
> declines; Rust's bare name matches every name-only arm. The bug is in
> extraction's information loss, not in the resolver's arms.**

Also non-obvious:

- **The phantom rate is concentrated in Pass-1 same-file binds**: of the 8
  same-file `is_empty` binds on tethys itself, **7 are phantom** (receivers
  `&OsStr`, `String`, `Vec` — std types bound to an in-crate method); only
  `!self.is_empty()` is true. Cross-file `unique_workspace` binds sample
  **4/4 true** (`session.has_errors()`, `target.src_root()`,
  `d.metrics.instability()`, `index.insert_file_dependency(...)`) — so
  blanket-declining variable receivers would kill ~400 mostly-true binds
  and violate AC3, while leaving same-file bare-name binding untouched
  keeps the worst phantom channel.
- **Workspace ambiguity already protects the cross-file tier for collision
  names**: `is_empty` has 2 in-crate declarations → `unique_workspace`
  declines its ~77 cross-file calls today. The corpus's phantom channel is
  same-file Pass-1 (and would be `unique_workspace` only for names both
  workspace-unique AND std-colliding).
- **panic-points is blind through TWO mechanisms**: (1) the ticket's —
  resolution NULLs `reference_name` and `panic_points.rs:51` filters the
  raw column; (2) newly discovered — calls inside macro args are token
  trees, never extracted: 728 grep sites vs 661 stored refs on tethys src
  (filed as **tethys-9l27**, related to ygjx's value-ref token-tree work).
- **`self` receivers carry derivable type info without LSP**: the enclosing
  `impl` block names the type at extraction time — `self.m()` could emit
  `Type::m` qualified. Similarly `let x: T` annotations and constructor
  lets are in the same function node.

## Measurements (design-driving)

| Fact | Value |
|---|---|
| Method calls in tethys `src/` (probe2) | 4612 |
| Receiver shapes | `call_result` 1485, `ident_unknown` 1474, `ident_annotated` 853, `field_recv` 366, `other` 178, `self` 174, `ident_constructed` 81 |
| Locally-derivable receivers (self + annotated + constructed) | 1108 (24%) |
| At-stake calls (name collides with an in-crate method) | 822 |
| Self-index call→method binds by strategy | `same_file` 419, `unique_workspace` 400, `explicit_import` 124, `glob_import` 62, `qualified_exact` 34, `qualified_module_fallback` 10 |
| Same-file `is_empty` binds that are phantom | **7 of 8** (receivers: `&OsStr`, `String`×2, `Vec`×4) |
| `unique_workspace` bind sample | 4/4 true (in-crate receivers) |
| `unwrap`/`expect`: grep sites vs stored refs (src/) | 728 vs 661 (68 = macro-context, tethys-9l27) |
| In-crate `unwrap`/`expect` methods in tethys | 0 (tethys's own panic-points unaffected; the repro fixture carries the collision) |

## prove-it-prototype hard gate

- [x] Probe written and runs against the real codebase (repro through the
  real binary; probe2/probe3 against tethys's own 4612-call source)
- [x] Oracle defined and produces output (grep + rustc semantics; item-by-
  item joins on the self and is_empty slices)
- [x] Probe and oracle agree on non-trivial slices (repro behavior exact;
  self slice 174 = 165 + diagnosed misses; is_empty sites hand-classified)
- [x] Learned something new (extraction discards the receiver — the
  C#/Rust conservatism asymmetry; phantom concentration in Pass-1;
  the macro-context blindness, filed as tethys-9l27)
