# Codebase Information: tethys

## Overview

**tethys** is a code-intelligence cache and query interface. It indexes source
code using [tree-sitter](https://tree-sitter.github.io/) and caches the results
in a SQLite database, then exposes fast queries for symbols, references, call
graphs, and dependency/architecture analysis. It targets CI integration and
local development workflows.

Design philosophy (from `src/lib.rs`):

- **Cache, not analyzer** — tethys indexes and caches; LSPs do the hard semantic work.
- **Layered accuracy** — fast approximate results from tree-sitter, optional precision via LSP integration.
- **Language extensible** — Rust + C# today, designed for adding more.
- **Embeddable** — library first, CLI second.
- **Intelligence, not policy** — reports facts ("12 callers"), not judgments ("too risky").

## Project Type

A single Cargo crate that builds both:

- A **library** (`src/lib.rs`) exposing the `Tethys` struct as the primary API.
- A **binary** (`src/main.rs`) — the `tethys` CLI built with `clap`.

## Technology Stack

| Concern | Technology |
|---------|-----------|
| Language | Rust (edition 2024, MSRV `1.94.0`) |
| Parsing | `tree-sitter` 0.24, `tree-sitter-rust` 0.23, `tree-sitter-c-sharp` 0.23 |
| Storage | `rusqlite` 0.32 (`bundled` SQLite) |
| Parallelism | `rayon` 1.10 |
| Serialization | `serde` / `serde_json` 1.0 |
| Errors | `thiserror` 2.0 |
| Logging | `tracing` 0.1, `tracing-subscriber` 0.3 |
| LSP | `lsp-types` 0.97, `percent-encoding` 2.3 |
| Manifest parsing | `cargo_toml` 0.22 |
| CLI | `clap` 4.5 (derive + cargo), `colored` 3 |
| Dev / test | `tempfile`, `rstest`, `proptest`, `criterion`, `tracing-test` |

## Supported Languages (for indexing)

- **Rust** — full extraction (symbols, references, imports, attributes, module-path resolution).
- **C#** — symbols, references, using directives, namespace-based dependency resolution.

Other languages are **not supported** for indexing and are skipped during file
discovery. Adding a language requires implementing two traits (`LanguageSupport`
and `ModuleResolver`) — see `src/languages/mod.rs`.

## Repository Layout

```text
tethys/
├── src/
│   ├── lib.rs            # Library entry point; `Tethys` struct + public API
│   ├── main.rs           # CLI binary (clap parser, command dispatch)
│   ├── types.rs          # Domain model: Symbol, Reference, Span, metrics, enums
│   ├── error.rs          # Error / IndexError / IndexErrorKind
│   ├── indexing.rs       # Indexing pipeline orchestration (on Tethys)
│   ├── batch_writer.rs   # Streaming-mode batched DB writer
│   ├── parallel.rs       # Parallel parse data structures (rayon)
│   ├── reindex.rs        # Incremental reindex / staleness detection
│   ├── resolve.rs        # Cross-file reference resolution (language-neutral driver)
│   ├── resolver.rs       # Rust module-path → file resolution
│   ├── cargo.rs          # Cargo workspace/crate discovery (public)
│   ├── cli/              # CLI command implementations (one module per command)
│   ├── db/               # SQLite persistence layer (Index struct + submodules)
│   ├── graph/            # Graph operation traits + DTO types
│   ├── languages/        # Per-language extraction + module resolution
│   └── lsp/              # LSP client transport + providers
├── tests/                # Integration tests (incl. seam_lint.rs invariants)
├── benches/              # Criterion benchmarks (indexing, queries)
├── docs/                 # Design docs, plans, spikes (historical/explanatory)
├── Cargo.toml            # Crate manifest, lints, profiles
├── deny.toml             # cargo-deny: licenses, advisories, bans
├── rust-toolchain.toml   # Pinned toolchain 1.94.0
└── .rustfmt.toml         # Formatting config
```

## Module Map (crate roots)

| Module | Visibility | Responsibility |
|--------|-----------|----------------|
| `cargo` | `pub` | Discover Cargo crates/workspaces, compute module paths |
| `db` | private | SQLite schema + all persistence (`Index`) |
| `error` | private (re-exported) | Error types |
| `graph` | private | `SymbolGraphOps` / `FileGraphOps` traits + DTOs |
| `indexing` | private | Indexing pipeline methods on `Tethys` |
| `languages` | private | `LanguageSupport` + `ModuleResolver` per language |
| `lsp` | `pub` | LSP client + provider abstraction |
| `parallel` | private | Owned data for parallel parsing |
| `reindex` | private | Incremental update / staleness |
| `resolve` | private | Cross-file reference resolution driver |
| `resolver` | private | Rust module-path resolution |
| `types` | private (re-exported) | Domain model |

## Database

- Location: `.rivets/index/tethys.db` (under the workspace root).
- Engine: SQLite via bundled `rusqlite`, foreign keys enabled.
- Schema defined in `src/db/schema.rs` as a single SQL string.
- Tables: `files`, `symbols`, `refs`, `file_deps`, `imports`, `call_edges`,
  `attributes`, `arch_packages`, `arch_file_packages`, `arch_package_deps`,
  plus the `arch_coupling` view.

## CLI Commands

`index`, `search`, `callers`, `impact`, `coupling`, `cycles`, `stats`,
`reachable`, `affected-tests`, `panic-points`. Global flags: `--workspace/-w`,
`--verbose/-v` (repeatable). See `interfaces.md` for full details.

## Tooling & Quality Gates

- **Lints** (`Cargo.toml`): `unsafe_code = "forbid"`, `missing_docs = "warn"`,
  clippy `all` + `pedantic` = `warn` (CI runs clippy with `-D warnings`).
- **CI** (`.github/workflows/ci.yml`): commit-message lint (conventional
  commits), `cargo fmt --check`, clippy, test (cargo-nextest, multi-OS, stable
  + beta) + doctests, release build with artifact upload, `cargo-deny`, and
  coverage via `cargo-tarpaulin` → Codecov.
- **Release profile**: fat LTO, single codegen unit, stripped symbols.
