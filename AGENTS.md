# AGENTS.md

Navigation guide for AI agents working in the **tethys** repository. tethys is a
Rust code-intelligence CLI + library that indexes Rust and C# source with
tree-sitter into a SQLite cache and answers symbol/reference/call-graph/
dependency queries.

Detailed reference docs live in [`.agents/summary/`](.agents/summary/index.md) —
start there (especially `index.md`) for anything this file does not cover.

## Table of Contents

- [Orientation](#orientation) — where to start reading
- [Directory & Module Map](#directory--module-map) — where code lives
- [Critical Patterns & Invariants](#critical-patterns--invariants) — non-obvious rules
- [Data & Persistence](#data--persistence) — the SQLite index
- [Tooling & CI](#tooling--ci) — what the pipeline enforces
- [Gotchas](#gotchas) — things that will surprise you
- [Custom Instructions](#custom-instructions) — human/agent-maintained

## Orientation

<!-- tags: entry-points, overview -->

- **Library API**: `src/lib.rs` — the `Tethys` struct is the single facade for
  all functionality. Most query methods live here or in `src/indexing.rs`.
- **CLI**: `src/main.rs` defines clap commands and dispatches to `src/cli/<cmd>.rs`.
  Commands: `index`, `search`, `callers`, `impact`, `coupling`, `cycles`,
  `stats`, `reachable`, `affected-tests`, `panic-points`, `deprecated-callers`,
  `unused-imports`.
- **Domain model**: `src/types.rs` — all shared records, IDs, and enums.
- **Domain vocabulary**: `CONTEXT.md` — the canonical glossary. Use these terms
  (and honor the `_Avoid_` lists) in issue titles, PRs, and code; `docs/adr/`
  records *why* the load-bearing decisions were made.
- **Schema**: `src/db/schema.rs` — the full SQLite schema as one SQL string.

For "what is X / how do I call X / how does process Y work", route via
`.agents/summary/index.md`.

## Directory & Module Map

<!-- tags: navigation, modules -->

| Path | What's there |
|------|--------------|
| `src/lib.rs` | `Tethys` facade + public re-exports |
| `src/indexing.rs` | Indexing pipeline orchestration (methods on `Tethys`) |
| `src/reindex.rs` | Incremental reindex / staleness (mtime-based) |
| `src/batch_writer.rs` | Streaming-mode batched DB writer thread |
| `src/parallel.rs` | Owned `Send` parse data for rayon |
| `src/resolve.rs` | **Language-neutral** cross-file reference resolution driver |
| `src/resolver.rs` | Rust module-path (`crate::`/`self::`/`super::`) resolution |
| `src/cargo.rs` | Cargo workspace/crate discovery (public) |
| `src/languages/` | Per-language extraction: `LanguageSupport` + `ModuleResolver` (rust.rs, csharp.rs, module_resolver.rs, common.rs) |
| `src/db/` | SQLite layer: `Index` + submodules (symbols, references, imports, call_edges, file_deps, graph, architecture, panic_points, files, schema, helpers) |
| `src/graph/` | Graph-op traits (`SymbolGraphOps`, `FileGraphOps`) + DTOs |
| `src/lsp/` | LSP client transport + providers (optional refinement) |
| `src/cli/` | One module per CLI command + display/helpers |
| `tests/` | Integration tests, incl. `seam_lint.rs` (architectural invariant) |
| `benches/` | Criterion benchmarks (`harness = false`) |
| `docs/` | Historical design docs, plans, spikes — explanatory, not normative |

## Critical Patterns & Invariants

<!-- tags: patterns, invariants, extension -->

- **The resolution "seam" is enforced by a test.** `src/resolve.rs` and
  `src/indexing.rs` must stay language-neutral; all language-specific module
  semantics belong behind `ModuleResolver` (`src/languages/module_resolver.rs`).
  `tests/seam_lint.rs` fails the build if language-specific logic leaks into the
  drivers, or if `ModuleResolver` impls touch the database. Respect this when
  editing resolution code.
- **Adding a language is a fixed 5-step procedure** documented at the top of
  `src/languages/mod.rs`: add a `Language` variant, create the module, implement
  `LanguageSupport`, register in `get_language_support`, then implement +
  register a `ModuleResolver`. Do not edit the drivers.
- **Graph queries are SQL recursive CTEs.** Traits in `src/graph/mod.rs` are
  implemented on `db::Index` in `src/db/graph.rs`. There is no in-memory graph
  library; reach for SQL, not petgraph (a future swap is noted but not present).
- **Coupling instability is computed in Rust, not SQL.** The `arch_coupling`
  view yields only Ca/Ce; `CouplingMetrics::instability` owns the formula.
  Keep it in one place.
- **Two-pass deferred resolution.** Indexing tolerates forward/circular refs by
  queuing `PendingDependency` and retrying until no progress. Consequence:
  `refs.symbol_id` is **NULL until Pass 2 resolves it** — don't assume refs are
  resolved mid-pipeline.

## Data & Persistence

<!-- tags: database, schema -->

- The index is a SQLite DB at **`.rivets/index/tethys.db`** under the workspace
  root (created by `Tethys::new`). Schema is applied idempotently on open.
- Schema is the source of truth in `src/db/schema.rs`; the ER diagram and table
  semantics are documented in `.agents/summary/data_models.md`.
- `--rebuild` clears the DB and its WAL/SHM sidecars before a full reindex.

## Tooling & CI

<!-- tags: ci, lints, quality-gates -->

Things the pipeline enforces that an agent should not violate
(`.github/workflows/ci.yml`, `Cargo.toml`, `deny.toml`, `rust-toolchain.toml`):

- **`unsafe_code = "forbid"`** crate-wide — no `unsafe`, ever.
- **`missing_docs = "warn"`** and clippy **`all` + `pedantic`**; CI runs clippy
  with **`-D warnings`**, so any clippy/pedantic warning fails CI. New public
  items need doc comments.
- **Conventional commits** are validated in CI (commitlint). Format:
  `<type>(<scope>): <desc>`; types: feat/fix/docs/style/refactor/perf/test/
  build/ci/chore; scopes are lowercase (e.g. `lsp`, `db`, `languages`, `cli`).
- **Tests run under `cargo nextest`** (multi-OS, stable + beta) plus doctests;
  use nextest locally to match CI.
- **`cargo-deny`** restricts licenses to a fixed allow-list and pins sources to
  crates.io — vet new dependencies against `deny.toml` before adding.
- **MSRV `1.94.0`**, pinned in `rust-toolchain.toml` (edition 2024).

## Gotchas

<!-- tags: gotchas, limitations -->

- **C# is a second-class citizen in places.** Symbol *attribute* extraction is
  Rust-only (see `src/languages/common.rs`), and the CLI `--lsp` flag /
  availability check are wired to **rust-analyzer only** (`src/cli/mod.rs`),
  even though a `CSharpLsProvider` exists in the library.
- **C# dependency resolution uses namespace/using corroboration**, not Rust-style
  module paths; its file-deps are treated more conservatively (see the
  `tests/csharp_*` files).
- **`docs/`, `.separator-fix/`, and `.csharp-ns/`** hold historical plans and
  past bug-fix artifacts (specs, probe scripts, baseline dumps). They are not
  the shipping product — don't treat them as current behavior.
- **Cross-crate call edges are corroborated against imports** ("k-hybrid" logic
  in `src/db/call_edges.rs`) before being kept; uncorroborated cross-crate edges
  are dropped.

## Custom Instructions
<!-- This section is for human and agent-maintained operational knowledge.
     Add repo-specific conventions, gotchas, and workflow rules here.
     This section is preserved exactly as-is when re-running codebase-summary. -->

### Agent skills

#### Issue tracker

Issues live in rivets, a local JSONL tracker (`.rivets/issues.jsonl`) driven by the
`rivets` CLI; GitHub is used for PRs only. See `docs/agents/issue-tracker.md`.

#### Triage labels

The five canonical triage roles are used verbatim as rivets labels; `wontfix`
additionally closes the issue with a reason. See `docs/agents/triage-labels.md`.

#### Domain docs

Single-context layout at the repo root: `CONTEXT.md` (the domain glossary) plus
`docs/adr/` (architectural decision records; ADR-0001 is the `ModuleResolver` seam,
ADR-0002 is SQL-CTEs-not-petgraph). See `docs/agents/domain.md`.
