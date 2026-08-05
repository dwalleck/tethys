# tethys

Code intelligence from the command line.

Tethys indexes your source code using [tree-sitter](https://tree-sitter.github.io/) and provides fast queries for symbols, references, call graphs, and dependency analysis. It's designed for CI integration and local development workflows.

## Installation

```bash
cargo install tethys
```

## Quick Start

```bash
# Index the current workspace
tethys index

# Search for symbols
tethys search AuthService

# Find all callers of a function
tethys callers "AuthService::authenticate"

# Analyze impact of changes to a file
tethys impact src/auth/mod.rs

# Find tests affected by changed files (great for CI)
tethys affected-tests src/auth/login.rs src/auth/session.rs --names-only

# View per-crate coupling metrics
tethys coupling

# Sort alphabetically
tethys coupling --sort name

# Drill into one package
tethys coupling --package my-crate

# JSON for tooling
tethys coupling --json
```

## Commands

| Command | Description |
|---------|-------------|
| `affected-tests` | Find tests affected by file changes |
| `callers` | Show callers of a symbol (with `--transitive` for call chains) |
| `coupling` | Per-crate coupling metrics (Ca, Ce, instability) |
| `cycles` | Detect circular dependencies |
| `deprecated-callers` | List reference sites of `#[deprecated]` symbols (Rust; C# `[Obsolete]` pending) |
| `impact` | Analyze impact of changes to a file or symbol |
| `index` | Index source files in the workspace |
| `panic-points` | Find `.unwrap()` and `.expect()` calls |
| `reachable` | Analyze symbol reachability (forward/backward traversal) |
| `search` | Search for symbols by name |
| `stats` | Show index statistics |
| `unused-imports` | Find imports whose names are never referenced (Rust) |

## Language Support

- Rust
- C#

## LSP Integration

For enhanced reference resolution, tethys can integrate with language servers:

```bash
# Index with rust-analyzer support
tethys index --lsp

# Use LSP for caller analysis
tethys callers "MyStruct::method" --lsp
```

## CI Integration

The `affected-tests` command outputs test names suitable for filtering test
runs, and its exit code says whether the index can stand behind the answer
(*query standing*):

- **exit 0** — confirmed. Empty stdout means confirmed **no** affected tests:
  safe to skip the suite.
- **exit 2** — indeterminate. The index could not vouch for the result
  (a changed file is unindexed or stale, or the workspace changed since
  indexing). stdout still lists whatever tests *were* found; one
  machine-readable reason per line goes to stderr, grep-able as
  `^indeterminate: ` (kinds: `unindexed`, `stale`, `stale-index`).
- **exit 1** — tooling error.

The mechanism lives in the tool; the skip/run policy belongs to the consumer:

```bash
# Index right before querying so standing reflects this checkout.
tethys index

CHANGED=$(git diff --name-only main)
TESTS=$(tethys affected-tests $CHANGED --names-only)
case $? in
    0) [ -n "$TESTS" ] && cargo test $TESTS ;; # confirmed; empty = safe skip
    2) cargo test ;;                           # indeterminate: fail open
    *) exit 1 ;;                               # tooling error
esac
```

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
