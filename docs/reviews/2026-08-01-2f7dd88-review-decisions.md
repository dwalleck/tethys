# Review-feedback decisions — 2f7dd88 (feat(impact): return depth-accurate file impact)

Two-axis review (`/code-review xhigh PR 37`, standards + spec sub-agents).
Seven findings; each verified at its site before any change was applied. The
user directed: resolve all findings, allow the scope-creep items.

| # | Finding (one line) | Axis | Category | Verified? | Decision | Note |
|---|---|---|---|---|---|---|
| 1 | `FileImpact` mirrors `SymbolImpact` method-for-method beyond the shared `depth_one_prefix_len` | Standards | Design (smell: Duplicated Code) | Yes — src/graph/types.rs | Reject change | Rule of three: a generic depth-partitioned wrapper would erase the glossary's domain naming (callers vs dependents) for a speculative abstraction. Revisit if a third depth-partitioned result type appears (natural checkpoint: tethys-71if contract phase) |
| 2 | `DEPENDENT_TREE_CTE` doc omits the `?2 >= 1` depth-zero guard; inert-for-`DEFAULT_MAX_DEPTH` consumers is unstated | Standards | Polish | Yes — src/db/graph.rs base case | Accept | `0962dcf` — guard and inertness documented on the CTE constant |
| 3 | workflows.md "neither projection invents reference counts" drifts from the glossary's "result fabricates" | Standards | Convention (CONTEXT.md vocabulary) | Yes — workflows.md vs CONTEXT.md Impact entry | Accept | `0962dcf` — aligned to glossary wording |
| 4 | CONTEXT.md Impact entry line runs 92 columns against the file's wrap | Standards | Style (cosmetic) | Yes | Accept | `0962dcf` — rewrapped |
| 5 | `dependent_depths` helper local to one test, inlined in another | Standards | Polish | Yes — tests/graph.rs | Accept | `eec3a72` — hoisted to module scope, both tests use it |
| 6 | "Test-topology tests cover the completed behavior" met only by pre-existing tests; nothing new asserts affected-tests invariance | Spec | Test coverage | Yes — no test_topology.rs hunk in 2f7dd88 | Accept | `eec3a72` — `deep_transitive_chain_preserves_membership_and_line_order`: depth-3 chain pins membership and `(file_id, line)` ordering |
| 7 | Depth zero verified at the Tethys seam but not through the CLI | Spec | Test coverage | Yes — CLI render test covered depths 1 and 2 only | Accept | `eec3a72` — depth-0 case added to the CLI render test |

Spec-axis scope-creep items — the transitive header correction ("N files
beyond direct") and the forced deletions of `Impact`/`Dependent` and
`format_callers_by_file` — were explicitly allowed by the user and left as-is.
