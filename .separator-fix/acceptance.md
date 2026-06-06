# Acceptance record — separator-fix (ModuleResolver seam)

Date: 2026-06-06. Branch: separator-fix-seam (slices 0-7 committed).
Spec: rev 2 (signed). Design: approved, 10 claims. Plan: 9 slices, one halt.

## C2/C3 — strict behavior neutrality (byte-identical dumps)

| Workspace | Baseline | Final binary | Result |
|---|---|---|---|
| tethys self-index (frozen worktree @ slice-0 commit, pre-seam-built baseline binary) | baselines/self-frozen.dump (15,759 lines) | identical | **PASS** |
| C# probe workspace | baselines/csharp.dump (24 lines) | identical | **PASS** |
| C6 trap workspace (incl. UNRESOLVED trap row) | baselines/c6trap.dump (14 lines) | identical | **PASS** |

Dump oracle: `.separator-fix/dump.sh` (natural-key projection over files/
symbols/refs/imports/file_deps/call_edges; determinism proven — claim C1).

## C9 — indexing wall-time (manual fence, approved at design)

Methodology note: comparing the long-lived incremental `target/` build
against a fresh build produced a spurious 2.4x "speedup" (103MB vs 86MB
binaries — incremental codegen drift). Authoritative comparison uses
fresh cold-target builds of BOTH binaries against the frozen tree,
interleaved ABAB, 3 rounds:

| Binary | Runs (ms) | Median |
|---|---|---|
| pre-seam (frozen @ slice-0) | 1748, 1749, 1726 | 1748 |
| post-seam (fresh build of branch tip) | 1766, 1836, 1749 | 1766 |

Delta: **+1.0% — PASS** (budget ±10%). Criterion bench `benches/indexing`
remains available for ad-hoc re-measurement; no CI perf gate (approved).

## Suite, lints, fences

- `cargo test`: all targets green, 0 failures (8 ignored = documented
  LSP/glob skip list). cargo-nextest not installed; cargo test is the gate.
- `cargo clippy --all-targets`: 0 warnings (stricter than the plan's
  lib-only requirement).
- Permanent fences, all passing:
  - `tests/seam_lint.rs` (C4/C5/C10 source lints; TDD-inversion verified
    against the slice-3-era file: 9 pattern matches → would fail)
  - `tests/qualified_split_trap.rs` (C6: helper::do_thing UNRESOLVED)
  - `tests/mixed_language_dispatch.rs` (per-file dispatch: zero phantom
    .cs→.rs deps with a Rust crate named `System`)
  - existing `pass2_qualified_paths` / `resolver_routing` /
    `module_path_integration` / `pass2_no_imports` (C2 fences)
  - `module_resolver` unit tests (17: C7 stub + Rust enumeration shapes)

## Claim ledger (final)

| Claim | Status | Permanent fence |
|---|---|---|
| C1 dump determinism | PASSED (design-time + slice 0) | oracle procedure documented; fixture recipes committed |
| C2 Rust byte-identical | PASSED | existing pass2/resolver/module_path integration suites |
| C3 C# byte-identical | PASSED | csharp fixture assertions live in probe + dump recipes (committed) |
| C4 resolve.rs seam grep | PASSED | tests/seam_lint.rs |
| C5 indexing.rs routed | PASSED | tests/seam_lint.rs + tests/mixed_language_dispatch.rs |
| C6 within-split abandonment | PASSED (baseline pre-verified) | tests/qualified_split_trap.rs |
| C7 C# stub declines | PASSED | module_resolver unit tests |
| C8 import_separator unification | PASSED | imports sections of dump baselines; suite |
| C9 wall-time ±10% | PASSED (+1.0%, corrected methodology) | manual (approved) |
| C10 impls DB-free | PASSED | tests/seam_lint.rs |

## Deviations log (plan → reality)

1. Slice 1 halt: self-index oracle was self-referential (live repo as
   input); re-pinned to frozen worktree. (Oracle wrong, not implementation.)
2. Registry + mod.rs checklist moved from slice 1 → slices 2/7 (exhaustive
   Language match needs both impls).
3. Anchor fn lives in module_resolver.rs, not cargo.rs (seam cohesion).
4. ModuleContext.workspace_root field dropped (unread).
5. types.rs:237 untouched — its resolve_module_path citation remains
   factually correct.
6. Tethys::src_root_for_file deleted in slice 5 (zero callers post-rewire).
7. C9 methodology corrected to fresh-built binaries both sides.

Open follow-ups (tracked): tethys-jwf9 (C# namespace resolution behind the
stub), tethys-8mze (Python/TS/Go consume the seam), tethys-dsp1
(per-language display spelling).
