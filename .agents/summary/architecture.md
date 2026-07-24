# Architecture

## System Overview

tethys is a layered code-intelligence system. Source files are parsed with
tree-sitter, extracted into a normalized domain model, persisted to SQLite, and
queried through graph operations and architecture metrics. The CLI is a thin
presentation layer over the `Tethys` library API.

```mermaid
graph TB
    CLI["CLI (src/main.rs + src/cli/*)"] --> LIB["Tethys library API (src/lib.rs)"]
    LIB --> IDX["Indexing pipeline (indexing.rs)"]
    LIB --> GRAPH["Graph ops (graph/ + db/graph.rs)"]
    LIB --> ARCH["Architecture metrics (db/architecture.rs)"]

    IDX --> LANG["Language extraction (languages/)"]
    IDX --> RESOLVE["Reference resolution (resolve.rs + resolver.rs)"]
    IDX --> DB[("SQLite index (db/)")]
    IDX -.optional.-> LSP["LSP refinement (lsp/)"]

    LANG --> TS["tree-sitter parsers"]
    GRAPH --> DB
    ARCH --> DB
    RESOLVE --> DB
    LIB --> CARGO["Crate discovery (cargo.rs)"]
```

## Layers

```mermaid
graph TD
    subgraph Presentation
        A1["src/main.rs — clap CLI"]
        A2["src/cli/* — per-command rendering"]
    end
    subgraph API
        B1["src/lib.rs — Tethys struct"]
    end
    subgraph Domain
        C1["src/types.rs — Symbol, Reference, Span, metrics"]
        C2["src/graph/types.rs — Caller/Callee/Impact DTOs"]
    end
    subgraph Logic
        D1["indexing.rs / reindex.rs / batch_writer.rs"]
        D2["resolve.rs / resolver.rs"]
        D3["languages/* (LanguageSupport, ModuleResolver)"]
        D4["cargo.rs"]
    end
    subgraph Persistence
        E1["db/* — Index + SQL"]
    end
    subgraph External
        F1["lsp/* — language servers"]
    end

    A1 --> A2 --> B1
    B1 --> C1 & C2
    B1 --> D1 & D2 & D4
    D1 --> D3
    D1 --> E1
    D2 --> E1
    D1 -.optional.-> F1
```

## Key Design Patterns

### Trait-based language extensibility (Strategy pattern)

Language-specific logic is isolated behind two traits, dispatched by `Language`:

- `LanguageSupport` (`languages/mod.rs`) — extracts symbols, references, and
  imports from a tree-sitter tree. Implemented by `RustLanguage` and
  `CSharpLanguage`.
- `ModuleResolver` (`languages/module_resolver.rs`) — translates module paths to
  files, provides per-file anchors, and defines the stored-import separator
  (`::` for Rust, `.` for C#).

```mermaid
classDiagram
    class LanguageSupport {
        <<trait>>
        +tree_sitter_language()
        +extract_symbols()
        +extract_references()
        +extract_imports()
    }
    class RustLanguage
    class CSharpLanguage
    LanguageSupport <|.. RustLanguage
    LanguageSupport <|.. CSharpLanguage

    class ModuleResolver {
        <<trait>>
        +resolve_import_files()
        +file_anchor()
        +import_separator()
    }
    class RustModuleResolver
    class CSharpModuleResolver
    ModuleResolver <|.. RustModuleResolver
    ModuleResolver <|.. CSharpModuleResolver
```

### The resolution "seam" (language-neutral drivers)

A core architectural invariant: the resolution drivers in `resolve.rs` and
`indexing.rs` must remain **language-neutral**. All language-specific module
semantics live behind `ModuleResolver` implementations. This boundary is
enforced by `tests/seam_lint.rs`, which fails the build if `resolve.rs` or
`indexing.rs` contain Rust/C#-specific module logic, or if `ModuleResolver`
implementations touch the database directly.

### Graph operations via SQL recursive CTEs

Graph traversal (callers, callees, transitive impact, cycle detection, path
finding) is implemented as concrete `db::Index` operations in `db/graph.rs`
using SQLite recursive common table expressions. `Tethys` is the external
graph-analysis seam; there is no speculative adapter trait or in-memory graph.

### Two-pass deferred dependency resolution

Indexing tolerates circular and forward dependencies by deferring unresolved
references. See `workflows.md` for the full sequence.

```mermaid
graph LR
    P1["Pass 1: parse + insert symbols/refs, queue pending deps"] --> P2["Resolution passes: retry pending until no progress"]
    P2 --> CE["Populate call_edges + file_deps"]
    CE --> ARCH["Architecture phase: packages, deps, coupling"]
```

### Architecture metrics (Robert C. Martin's coupling)

`db/architecture.rs` rolls file-level dependencies up to package level and
computes afferent coupling (Ca), efferent coupling (Ce), and instability
(I = Ce / (Ca + Ce)). The `arch_coupling` SQL view computes Ca/Ce; instability
is deliberately computed once in Rust (`CouplingMetrics::instability`) rather
than in SQL to keep the formula in a single place.

### Batch vs. streaming write modes

The indexer supports two write strategies (`IndexOptions`):

- **Batch** (default) — collect parsed data, then write.
- **Streaming** — `BatchWriter` runs a dedicated writer thread consuming parsed
  files in configurable batches, bounding peak memory for large workspaces.

### Error modeling

`error.rs` defines a top-level `Error` plus a structured `IndexError` carrying an
`IndexErrorKind` (input vs. internal categorization). Per-file indexing errors
are collected rather than aborting the whole run.

## Data Flow

```mermaid
sequenceDiagram
    participant FS as Filesystem
    participant IDX as Indexer
    participant TS as tree-sitter
    participant LANG as LanguageSupport
    participant DB as SQLite
    FS->>IDX: discover source files
    IDX->>TS: parse (parallel, rayon)
    TS->>LANG: syntax tree
    LANG-->>IDX: symbols, refs, imports
    IDX->>DB: write symbols/refs/imports
    IDX->>IDX: resolve references (pass 2+)
    IDX->>DB: populate call_edges, file_deps
    IDX->>DB: architecture phase (packages, coupling)
```

## Concurrency Model

- Parsing is parallelized with `rayon` over discovered files; `parallel.rs`
  provides owned, `Send` data structures (`ParsedFileData`, `OwnedSymbolData`)
  so parse results can cross thread boundaries before DB writes.
- SQLite writes are serialized (single connection / writer thread in streaming
  mode).
