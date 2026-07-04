---
status: accepted
---

# Resolution provenance: store the strategy, derive the band

Every resolved reference records **which mechanism bound it** — a
`strategy` text column on `refs`, written at bind time by every write
path. Confidence **bands** (`high` / `medium` / `speculative`) are
**derived from the strategy in the query surface** (one `CASE` expression
in a view), never stored.

## The strategy enum

Nine values, matching the write paths as they exist in code today (the
originating epic tethys-9z7i predates two of them):

| strategy | where it fires |
|---|---|
| `same_file` | Pass 1 insert-time same-file maps (`src/db/files.rs`; includes the macro-name map) |
| `explicit_import` | Pass 2 `resolve_via_explicit_import` |
| `glob_import` | Pass 2 glob arm, `GlobPolicy::FirstMatch` (Rust) |
| `import_union` | Pass 2 union arm, `GlobPolicy::UniqueAcrossAll` (C# usings / static members) |
| `qualified_exact` | Pass 2 fallback, qualified name matched against `symbols.qualified_name` |
| `same_crate` | Pass 2 fallback, simple name scoped to the caller's crate path prefix |
| `unique_workspace` | Pass 2 fallback, simple name matched workspace-wide (unscoped) |
| `qualified_module_fallback` | Pass 2 prefix-split module enumeration (rivets-044i) |
| `lsp` | Pass 3 `goto_definition` refinement |

Unresolved refs have `strategy` NULL — "unbound" is not a strategy.

## The band mapping (initial; revisable by design)

| band | strategies | rationale |
|---|---|---|
| `high` | `explicit_import`, `lsp` | an import names the exact item; the language server did real semantic work |
| `medium` | `same_file`, `glob_import`, `import_union`, `qualified_exact`, `same_crate` | scoped mechanisms that are usually right but have measured failure modes (same-file last-wins is the tethys-53iv phantom source; glob/name matches are kind-blind) |
| `speculative` | `unique_workspace`, `qualified_module_fallback` | name-shape matching with no scoping corroboration — exactly the arms that fabricate the tethys-53iv / tethys-msn0 / tethys-3i35 phantom class |

Because the band lives in one view definition, remeasuring (e.g. a zbus-
or q-cli-style oracle audit per strategy) can move a strategy between
bands **without re-indexing anything**. That is the point of storing the
label instead of a number.

## Why store text and derive, not store a confidence number

codebase-memory-mcp (the comparative review that motivated this) stores
per-edge numeric confidences (import_map 0.95 … fuzzy 0.40). The numbers
are arbitrary the moment they are written, and frozen into every row. The
strategy label is the real information: it is factual (which code path
ran), stable under remeasurement, and cheap (one small text column,
practically an enum). Consumers that want numbers can map bands to
numbers at the edge.

## Write-path consequences

- Pass 2/3 flow through the **unified UPDATE seam** (`RESOLVE_REF_SQL`,
  `src/db/references.rs` — unified in 4b5e7c4). The seam widens to carry
  strategy; it must not fork.
- Pass 1 binds at **INSERT** time (`src/db/files.rs`), not through the
  UPDATE seam — the refs INSERT carries the strategy column too. Two
  write shapes, one column, both fenced.
- `call_edges` carries **no copy**: `populate_call_edges` derives from
  refs; provenance joins through refs. A copy is a divergence waiting to
  happen (the CBM cautionary tales are exactly this class).
- Golden-content fences (`tests/idxperf_golden.rs`) and the canonical-dump
  oracle change with DB content — they are **regolded deliberately** in
  the write-path slice, never "discovered."

## Consumers (query surface, later slices)

The band is exposed as a view sibling to `refs_named`; precision
consumers (callers, impact, panic-points) gain the ability to exclude
`speculative`; recall consumers (dead code) treat speculative edges as
suppressions — "maybe someone calls this" suppresses a dead-code finding
without polluting caller lists. This epic labels the phantom class; it
does not fix tethys-53iv/msn0/3i35 (their fixes change from
"decline the binding" to "bind but band speculative").
