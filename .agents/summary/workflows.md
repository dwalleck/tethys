# Workflows

This document describes the key processes in tethys. They fall into two groups:
the **indexing pipeline** (write path) and **query workflows** (read path).

## Indexing Pipeline

```mermaid
flowchart TD
    Start([tethys index]) --> Discover["Discover files (walk_dir, skip excluded dirs)"]
    Discover --> Filter["Filter to supported extensions (.rs, .cs)"]
    Filter --> Parse["Parse in parallel (rayon + tree-sitter)"]
    Parse --> Extract["Extract symbols / refs / imports (LanguageSupport)"]
    Extract --> Mode{Streaming?}
    Mode -- "batch (default)" --> WriteBatch["Write all parsed files"]
    Mode -- "streaming" --> WriteStream["BatchWriter thread writes in batches"]
    WriteBatch --> Pass1
    WriteStream --> Pass1
    Pass1["Pass 1 complete: symbols + refs stored, deps queued"] --> Resolve["Resolution passes: retry pending deps until no progress"]
    Resolve --> Edges["Populate call_edges + file_deps"]
    Edges --> LSP{"--lsp?"}
    LSP -- yes --> Refine["Refine unresolved refs via LSP"]
    LSP -- no --> Arch
    Refine --> Arch["Architecture phase: packages, package deps, coupling"]
    Arch --> Stats([Return IndexStats])
```

### Stages

1. **Discovery** (`discover_files` / `walk_dir`) — recursively walks the
   workspace, skipping excluded directories (`target`, `node_modules`, `bin`,
   `obj`, `build`, `dist`, `vendor`, `__pycache__`, hidden dirs). Symlinks are
   followed with loop protection (see `tests/symlink_boundary.rs`).
2. **Filtering** — keeps files whose extension maps to a supported `Language`;
   others increment `files_skipped`.
3. **Parallel parsing** — `rayon` parses files concurrently; results become
   `ParsedFileData` (owned, `Send`).
4. **Extraction** — the language's `LanguageSupport` yields symbols, references,
   and imports from each tree.
5. **Persistence** — batch mode writes after parsing; streaming mode hands
   parsed files to a `BatchWriter` thread in configurable batches.
6. **Deferred resolution** — references to not-yet-indexed files are queued as
   `PendingDependency` and retried in resolution passes until no progress is
   made; this tolerates circular and forward references.
7. **Edge population** — `call_edges` (caller→callee) and `file_deps` are
   computed from resolved references. Cross-crate call edges are corroborated
   against imports before being kept.
8. **Optional LSP refinement** — when enabled, unresolved references are
   resolved via a language server.
9. **Architecture phase** — assigns files to packages, rolls `file_deps` up to
   `arch_package_deps`, and prepares coupling metrics.

### Cross-file reference resolution detail

```mermaid
flowchart TD
    Ref["Unresolved reference"] --> Imports["Build import maps for file"]
    Imports --> Explicit{"Matches explicit import?"}
    Explicit -- yes --> ResolveMod["Resolve module → file (ModuleResolver)"]
    Explicit -- no --> Glob{"Matches glob import?"}
    Glob -- yes --> ResolveMod
    Glob -- no --> Qualified{"Qualified path?"}
    Qualified -- yes --> Fallback["Qualified-module fallback (longest prefix)"]
    Qualified -- no --> SameCrate["Fallback: same-crate symbol search"]
    ResolveMod --> Lookup["Look up symbol in target file → symbol_id"]
    Fallback --> Lookup
    SameCrate --> Lookup
    Lookup --> Done([Set refs.symbol_id])
```

The driver (`resolve.rs`) is language-neutral; all module-path semantics come
from `ModuleResolver`. Rust uses `crate::`/`self::`/`super::` resolution
(`resolver.rs`); C# uses namespace/using corroboration with a namespace map.

## Incremental Reindex Workflow

```mermaid
flowchart LR
    A([tethys index]) --> B["get_stale_files (compare mtime vs DB)"]
    B --> C{Changes?}
    C -- none --> D([No-op])
    C -- "added/modified/deleted" --> E["Reindex changed files"]
    E --> F["Re-resolve + recompute edges"]
    F --> G([IndexUpdate])
    H([tethys index --rebuild]) --> I["reset DB + full reindex"]
```

`reindex.rs` classifies each indexed file (`FileChange`: added, modified,
deleted, unchanged) by comparing filesystem mtime to the stored `mtime_ns`.
`--rebuild` clears the database (including WAL/SHM sidecars) and reindexes from
scratch.

## Query Workflows

### Callers / Impact

```mermaid
sequenceDiagram
    actor User
    participant CLI
    participant Tethys
    participant DB as SQLite
    participant LSP
    User->>CLI: tethys callers "Foo::bar" [--lsp | --exclude-speculative]
    CLI->>Tethys: get_callers("Foo::bar", CallerMode)
    Tethys->>DB: query retained call_edges and hydrate caller files
    DB-->>Tethys: indexed caller records
    opt LspRefined
        Tethys->>LSP: find_references at target definition
        LSP-->>Tethys: reference locations
        Tethys->>Tethys: merge and deduplicate by caller symbol
    end
    Tethys-->>CLI: Caller rows
    CLI-->>User: grouped, formatted output
```

Transitive callers remain index-backed through symbol impact. The CLI rejects
`--lsp` with either `--transitive` or `--exclude-speculative`; unsupported
combinations are never silently ignored.

`impact` works the same way at file granularity over `file_deps`, or at symbol
granularity with `--symbol`. `--depth` bounds transitive traversal.

### Reachability

`reachable <symbol> --direction forward|backward --max-depth N` does a BFS over
the call graph (`get_forward_reachable` / `get_backward_reachable`), returning
reachable symbols grouped by depth. Cyclic graphs terminate safely.

### Cycle Detection

`cycles` runs DFS-based cycle detection over file dependencies
(`detect_cycles`), normalizing each cycle (rotated to a canonical start) and
deduplicating.

### Coupling / Architecture

```mermaid
flowchart LR
    A([tethys coupling]) --> B["get_coupling_metrics (arch_coupling view)"]
    B --> C["Compute instability per package (Rust)"]
    C --> D["Sort (instability default, or --sort)"]
    D --> E([Table or --json])
    F([tethys coupling --package X]) --> G["get_package_coupling: neighbors in/out"]
    G --> H([Detail view])
```

### Affected Tests (CI)

```mermaid
flowchart LR
    A["git diff --name-only"] --> B([tethys affected-tests files... --names-only])
    B --> C["Map changed files → dependent test symbols"]
    C --> D["Emit test names"]
    D --> E["cargo test/nextest filter"]
```

This is the primary CI workflow: feed changed files in, get back the test names
that transitively depend on them, and run only those.

### Panic Points

`panic-points` queries symbols/refs for `.unwrap()` / `.expect()` occurrences
(`PanicKind`), optionally including tests, filtering to a file, or emitting JSON.

## Development Workflow (from CI config)

```mermaid
flowchart LR
    Commit["Conventional commit"] --> Fmt["cargo fmt --check"]
    Fmt --> Clippy["clippy -D warnings (all+pedantic)"]
    Clippy --> Test["cargo nextest (multi-OS, stable+beta) + doctests"]
    Test --> Build["release build"]
    Build --> Deny["cargo-deny (licenses/advisories)"]
    Deny --> Cov["tarpaulin coverage → Codecov"]
```
