# Review-feedback decisions — fde3638 (feat(callers): return depth-accurate symbol impact)

Two-axis review (`/code-review fde3638`, standards + spec sub-agents) assessed via
`gilfoyle:assessing-review-feedback`. Six findings; each verified against the code
and the rivets tracker before any change was applied.

| # | Finding (one line) | Axis | Category | Verified? | Decision | Note |
|---|---|---|---|---|---|---|
| 1 | AGENTS.md module map still calls graph DTOs "Internal" after `SymbolImpact`/`SymbolImpactCaller` went public | Standards | Convention (documented: AGENTS.md doc-maintenance rule) | Yes — AGENTS.md:59 contradicted lib.rs:59 and AGENTS.md:34 | Accept | `79fec60` — one-line map correction |
| 2 | CONTEXT.md **Impact** entry frames symbol impact as generic "dependents" | Standards | Convention (soft) | Yes — entry not false (callers are dependents) but imprecise for the new depth vocabulary | Modify | `d792869` — one-line clarification; full seam/vocabulary rewrite already tracked at tethys-71if |
| 3 | bool→`CallEdgeSelection` mapping duplicated (cli/callers.rs + lib.rs); facade takes a bare `bool` | Standards | Design | Yes — plus `cli/impact.rs` passed an unreadable bare `false` | Modify | `c28be7e` — took the facade-enum variant over the suggested `From<bool>`: matches the documented explicit-modes seam invariant; ~25 mechanical call sites, fenced by the existing suite |
| 4 | `direct_callers`/`transitive_callers` each recompute `partition_point`; ordering precondition undocumented on `new` | Standards | Polish | Yes | Accept | `3d803b6` — private `direct_end()` + precondition doc on `new`; skipped a `debug_assert` since ordering is already fenced by the depth-contract test |
| 5 | `use cli::callers::run as run_callers` alias inconsistent with the other 15 command dispatches | Standards | Style | Yes — no name collision justifies the alias | Accept | `581f374` — inline `cli::callers::run(...)` |
| 6 | No CLI-level happy-path test for `impact --symbol` depth-partitioned output | Spec | Test coverage | Yes — only the LSP-rejection CLI test existed | Accept | `c02ccb6` — new strategy test asserts direct/transitive sectioning and `--depth 1` trimming on the exclusion fixture |

Spec-axis scope-creep notes (`run_command` extraction; dropped genuine
`ce.call_count` on the direct path) were assessed as benign — no observable
behavior change at the public seam — and left as-is.
