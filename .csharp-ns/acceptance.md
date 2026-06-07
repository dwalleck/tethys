# Acceptance record — csharp-ns (jwf9 + nmsp)

Date: 2026-06-07. Branch: csharp-ns (slices 0–7 committed). Spec rev 2
(signed; probe falsified rev 1's first-time-resolution premise — the
delivered capability is collision disambiguation). Design: approved, 12
claims (C2 ran-failed at design time → nested namespaces descoped to
tethys-nnst). Plan: 9 slices, two halts, both surfaced and root-caused.

## Claim ledger (final)

| Claim | Status | Permanent fence |
|---|---|---|
| C1 flat-key namespace map | PASSED | indexing.rs unit tests (4 stress shapes incl. sort determinism, nested exclusion, Rust-module exclusion) |
| C2 nested reconstruction impossible | RAN-FAILED at design (by intent) | descoped: tethys-nnst |
| C3 collision → used namespace | PASSED (probed baseline: UNRESOLVED) | tests/csharp_using_disambiguation.rs |
| C4 arm-order safety (no target flips) | PASSED | same fence, distinct assert + monotone join |
| C5 members not disambiguated | PASSED | same fence, distinct assert |
| C6 monotone-stable C# refs | PASSED — refs sections byte-identical on csharp-gt AND xdir (0 changes, 0 losses) | fixture fences + dump procedure |
| C7 Rust byte-identical | PASSED — self-frozen (16,059 lines) + c6trap, every slice | existing pass2/trap suites + FirstMatch-verbatim branch |
| C8 L2 delta exact | PASSED — E2 predicted at slice 0 matched row-for-row (4→3, two absences, 2→1, 1→1) | tests/csharp_l2_file_deps.rs (full-set equality) |
| C9 cross-dir deps survive | PASSED — xdir byte-identical to original baseline (count 1, source flipped post-pass→corroborated call edge) | tests/csharp_cross_dir_deps.rs (3 distinct asserts incl. negative + same-bucket controls) |
| C10 post-pass deleted | PASSED — grep 0 in src/ | seam_lint needle (TDD-inversion: pre-deletion tree fails) |
| C11 DB-free/stateless seam | PASSED — map via ctx; seam_lint unchanged | existing tests/seam_lint.rs |
| C12 wall-time ≤ +10% | **PASSED: −2.9%** (median 1832→1779 ms; fresh cold-target builds both sides, frozen input, ABAB ×5) | manual (approved at design; criterion bench available) |

Suite: 702 passed, 0 failed. clippy --all-targets: 0 warnings.

## Halts (both resolved as fixture/measurement issues, not design)

1. **Slice 5**: cross-dir fence's negative control failed — root cause:
   `workspace_with_files` auto-injects a root `[package]` Cargo.toml,
   collapsing all files into one crate bucket and bypassing the
   cross-bucket arm. Reproduced bare: implementation correct. Fence now
   pins a virtual-workspace manifest and documents the trap.
2. **Slice 6** (anticipated): `mixed_language_dispatch`'s positive control
   encoded the superseded L1 semantics (edge from an UNUSED using);
   re-encoded to L2 (fixture gains a used reference). Also exposed
   `Tethys.parser` as dead (the post-pass was its last consumer) — field
   deleted.

## Deviations log (plan → reality)

1. DB helper takes file PATHS not ids (files-table subquery) — removes
   per-ref translation queries.
2. C# using-arm fires for simple names only; qualified refs keep the
   pre-existing fallback path (planned at build critique — structurally
   excludes resolution-order target flips).
3. K-hybrid corroboration data built inside db/call_edges.rs via two
   C#-scoped SQL queries — no signature change, Rust edges structurally
   unaffected.
4. Slice-3 kind-filter test fixture rebuilt after discovering
   upsert_file→index_file_atomic clears prior symbols.

## Closed by this loop

- **tethys-jwf9** — C# using-directives resolve through
  CSharpModuleResolver (namespace map, types-only, unique-or-decline).
- **tethys-nmsp** — the namespace-map post-pass is deleted; its mechanism
  lives behind the seam (map → Pass-2 glob arm + K-hybrid corroboration).

Open follow-ups (tracked): tethys-usgf (using static / alias / global),
tethys-nnst (nested block namespaces), tethys-dsp1, tethys-8mze,
tethys-mpth.
