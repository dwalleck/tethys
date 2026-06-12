# Acceptance record — usgf (C# `using static` static-method disambiguation)

Date: 2026-06-08. Branch: usgf (slices 0–3 committed). Spec rev 2 (signed;
probe narrowed B2 to static methods only — consts/fields/enum members not
indexed, filed tethys-cfme — and revised decision #4 to type-detection, no
schema change). Design: approved, 9 claims (cheapest type-detection
falsifier PASSED on probe data). Plan: 5 slices, two halts, both surfaced
and root-caused.

## Claim ledger (final)

| Claim | Status | Permanent fence |
|---|---|---|
| C1 type-detection | PASSED (probe + falsifier) | `module_resolver` unit tests (7 shapes) |
| C2 colliding method disambiguates | PASSED (probed baseline UNRESOLVED) | `tests/csharp_using_static.rs::static_using_disambiguates_colliding_method` |
| C3 cross-arm collision declines | PASSED | same file, `cross_arm_type_vs_method_collision_declines` |
| C4 prefix-scoping | PASSED | `static_using_scopes_to_the_imported_type` + `search_type_members_by_name` unit tests |
| C5 external static using declines | PASSED | `external_static_using_declines` |
| C6 existing C# monotone-stable | PASSED — csharp-gt refs byte-identical (Assist now via the static arm to the SAME Helper::Assist), xdir byte-identical, 0 target changes/losses | jwf9 fixtures + dump procedure |
| C7 Rust byte-identical | PASSED — self-frozen (16,386 lines) + c6trap, every slice (FirstMatch branch verbatim) | existing pass2/resolver/trap suites |
| C8 seam DB-free | PASSED — `module_resolver.rs` 0 DB references; type-detection is string+map | existing `tests/seam_lint.rs` (unchanged) |
| C9 wall-time ≤ +10% | PASSED — see below | manual (approved at design; csharp-ns C12 / separator-fix C9 precedent) |

Suite: 718 passed, 0 failed. clippy --all-targets: 0 warnings.

## C9 measurement (manual fence)

The C9 fixture is the Rust self-index workspace, which routes entirely
through the UNCHANGED `FirstMatch` glob branch — the union arm and
static-member lookup are C#-only, so the new code is intrinsically dead
weight on this fixture. Wall-time is therefore expected identical; any delta
is measurement noise.

Two interleaved fresh-cold-build runs (frozen slice-0 binary vs branch-tip
binary, both built from the frozen tree):
- Run A (5 reps): frozen min 1818, fresh min 1802 ms → fresh floor faster.
- Run B (9 reps): frozen min 1934, fresh min 1994 ms → +3.1% floor.

Minimums are the robust estimator (OS scheduling noise is strictly
additive); medians were contaminated (Run B max 63,418,802 ms — an overnight
machine suspend). Floor delta across runs is within ±3.1%, well inside the
±10% budget, consistent with the structural argument that the Rust path is
unchanged. **PASS.**

## Halts (both fixture/measurement, not design)

1. **Slice 2**: the per-member test helper re-upserted the file each call,
   and `index_file_atomic` cleared prior symbols (the csharp-ns slice-3
   trap) → rewrote to one-upsert-many-members. Also: an `empty type_name`
   `debug_assert` panicked on the input its load-bearing runtime guard
   handles (a trailing-dot `using static My.Models.;` reaches it with empty
   suffix) → removed the assert, kept the runtime refusal.
2. **Slice 3**: the C5 fixture had ONE workspace `Sqrt` (unique → fallback
   resolved it, masking the external-static behavior) → added a second
   colliding `Sqrt`. Implementation correct; test wrong.

## Deviations log (plan → reality)

1. Member scoping uses EXACT `qualified_name = 'Type::name'` (not the
   design's `LIKE 'Type::%'`) — index-friendly, dodges the underscore-
   wildcard hazard for identifiers with `_`; overloads still surface as
   multiple candidates → decline.
2. `search_unique_symbol_by_name_in_files` deleted — the union replaced its
   only production caller (pre-collapsing defeats cross-arm detection); its
   5 tests migrated to a test-only `unique_in_files` wrapper.
3. Union arm extracted to `resolve_via_union_arm` (the inline arm crossed
   the 100-line lint).

## Closed by this loop

- **tethys-usgf** — scope note: `using static` of METHOD members only.
  Alias usings (tethys-alus), global usings (tethys-glus), and non-method
  members (tethys-cfme) remain open, filed during this loop's spec/probe.

Open follow-ups: tethys-alus, tethys-glus, tethys-cfme, tethys-nnst,
tethys-dsp1, tethys-mpth, tethys-8mze.
