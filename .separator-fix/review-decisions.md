# Review-feedback decisions — PR #1 (multi-agent review, 2026-06-06)

Every finding verified before any change was applied (gilfoyle
assessing-review-feedback). Verification evidence inline.

| # | Finding (one line) | Reviewer | Category | Verified? | Decision | Note |
|---|---|---|---|---|---|---|
| I1 | indexing.rs:1255 joins namespace segments on hard-coded `.`, bypassing the seam | type-design-analyzer | Design/bug | **Yes** — fn exists, join confirmed; crucial provenance: `resolve_csharp_dependencies` is PRE-EXISTING (outside this PR's diff) and load-bearing (it produced the `Auth.cs→Hasher.cs` dep in the C# baseline). The seam doc's literal claim (stored `source_module` format) is not contradicted — :1255 joins for namespace-map lookup, not storage — but the spirit (one owner for the import format) is. | **Modify** | Join now routes through new `join_import()` (behavior-identical: same constant); fn doc explains the mechanism + its planned unification with the seam. Structural unification deferred: **tethys-nmsp** (depends on tethys-jwf9). |
| I2 | Streaming-mode C# separator path (batch_writer store_imports) has no C# test | pr-test-analyzer | Test gap | **Yes** — `IndexOptions::default().use_streaming == false`; CLI uses default; every committed C# assertion and the loop's dump oracle ran batch-only | **Accept** | `mixed_language_dispatch` parameterized batch+streaming via rstest (pattern from file_deps_idempotency.rs:101); dotted `source_module` storage asserted under both modes |
| I3 | compute_dependencies_from_stored (streaming) has no C# coverage; phantom-dep guarantee untested there | pr-test-analyzer | Test gap | **Yes** — same root as I2 (streaming-only code path, batch-only C# tests) | **Accept** | Same parameterized test covers the phantom-dep fence under streaming |
| S1 | seam_lint needles narrower than advertised: `"self"` missing (C4), `&Index` dropped vs design.md falsifier (C10) | pr-test-analyzer + comment-analyzer | Consistency | **Yes** — `"self"` omission traces to spec B3's own grep pattern (the spec was narrower than the code it described); `&Index` was dropped during the false-positive fix without documented reason. Both needles verified false-positive-free against current sources | **Accept** | Both needles added |
| S2 | No Rust-side decline tests for cross-separator / degenerate inputs | pr-test-analyzer | Test gap | Plausible (only C# had symmetric negatives) | **Accept** | Added: `MyApp.Models`, `crate::`, `::db`, `""` against the Rust resolver — all decline |
| S3 | compute_dependencies doc describes anchor contract in Rust-only terms | comment-analyzer | Doc | **Yes** — second sentence states `self::`/`super::` behavior unconditionally for a function that now runs for every language | **Accept** | Reworded conditionally; decline-languages named explicitly |
| S4 | Replace `import_separator()` with `join_import(segments)` | type-design-analyzer | Design | Partially — the join direction is the hole, but the separator getter also serves the SPLIT direction (`resolve_import` parses stored strings); replacing would break parsing | **Modify** | `join_import` ADDED as a provided method (default: `segments.join(self.import_separator())`); all three import-format joins route through it; `import_separator()` retained for the parse direction and storage-format documentation |
| S5 | Add separator-join lint (`.join(".")` / `.join("::")` outside seam) | type-design-analyzer | Design | Verified the proposed pattern over-matches: `.join("::")` is legitimate at batch_writer.rs:361, indexing.rs:830, cargo.rs:295 (canonical qualified-name/module-path joins — a different concept from import format, per spec decision #5) | **Modify** | Lint added for `.join(".")` only, over the three embedded driver files; would have caught indexing.rs:1255 (verified: pre-fix content fails it) |
| S6 | ModulePath newtype, ModuleContext::for_file, sealed trait | type-design-analyzer | Design (long-term) | n/a (architecture, not defect) | **Reject (defer)** | Filed **tethys-mpth** with the reviewer's type ratings preserved |

## Gate note

Post-fix verification: full suite + clippy --all-targets + fmt; dump oracle
re-run on the C# probe and c6trap fixtures (byte-identical — the changed
paths are exactly what those fixtures exercise: import storage, C# deps,
file_deps). The frozen self-index worktree from the build phase was
retired at acceptance; not recreated for these changes since none touch
Rust-only resolution arms (I1 join is constant-identical, rest are tests/docs).

## Scorecard

3 Important: 1 Modify, 2 Accept. 6 suggestions: 3 Accept, 2 Modify, 1 Reject(defer).
Every deferral carries a tracker ID (tethys-nmsp, tethys-mpth). No finding
applied unverified; two reviewer fixes were re-shaped after verification
(S4 would have broken the parse direction; S5 would have false-positived
on canonical joins).
