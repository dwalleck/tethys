# Review Notes

Consistency and completeness review of the generated documentation for tethys.
This file records gaps, language-support limitations, and recommendations.

## Consistency Check

No contradictions were found across the generated documents. The following
facts were cross-checked against source/config and are used consistently:

- **Supported languages**: Rust and C# only — consistent in codebase_info.md,
  architecture.md, dependencies.md, data_models.md (matches `Language` enum and
  `get_language_support`).
- **CLI commands** (10): `index`, `search`, `callers`, `impact`, `coupling`,
  `cycles`, `stats`, `reachable`, `affected-tests`, `panic-points` — match
  `src/main.rs` and the README.
- **DB location** `.rivets/index/tethys.db` — matches `Tethys::new`.
- **Schema tables/view** — match `src/db/schema.rs` exactly.
- **Default depths** — `impact` default 50, `reachable` default 10,
  `search --limit` default 20 — match `src/main.rs`.
- **Coupling instability** computed in Rust, not SQL — matches both
  `db/schema.rs` view comment and `db/architecture.rs`.

## Completeness Gaps & Limitations

### Language-support asymmetry (Rust vs C#)
- **Attribute extraction is Rust-only.** `languages/common.rs` states attributes
  are "Currently populated by the Rust extractor only; C# extraction will
  follow." The `attributes` table and the `panic-points` / test-detection
  features are therefore richer for Rust. C# test detection still works (it
  keys off `[Test]`/`[Fact]`/`[Theory]`/`[TestMethod]`), but C# symbol
  attributes are not stored. Documented in data_models.md indirectly; flagged
  here explicitly.
- **C# dependency resolution differs from Rust.** C# uses namespace/using
  corroboration and a namespace map rather than Rust's `crate::`/`super::`
  module-path resolution. Tests (`csharp_*` files) suggest C# file-deps are
  treated more conservatively ("L2 used-only", using-corroboration required).
  The exact precision tradeoffs were not exhaustively documented.

### LSP coverage
- The CLI `--lsp` flag and `check_lsp_availability` are hard-wired to
  **rust-analyzer** (`src/cli/mod.rs` uses `RustAnalyzerProvider`). A
  `CSharpLsProvider` exists in the library (`lsp/provider.rs`), but it was not
  confirmed whether C# LSP refinement is reachable from the CLI. Treated as
  "provider exists" rather than "fully wired" in the docs.

### Areas described from structure, not full read
The following were documented primarily from the codebase overview, module
docs, headers, and symbol lists rather than a line-by-line read of every
implementation:
- The "k-hybrid" cross-crate call-edge corroboration logic (`db/call_edges.rs`)
  — described at a conceptual level only.
- Exact recursive-CTE SQL in `db/graph.rs` and `db/architecture.rs`.
- `cargo.rs` glob-member/virtual-workspace edge cases.
These are accurate at the described level of abstraction; deeper claims should
be verified against source before relying on them.

### Not documented (out of scope / historical)
- `docs/` contains design docs, plans, and spikes (e.g. the architecture
  analysis plan, LSP session-result design). These are historical/explanatory
  and were not folded into the generated docs.
- The `.separator-fix/` and `.csharp-ns/` directories contain task-specific
  spec/probe/baseline artifacts (shell scripts, dumps) for past bug-fix efforts,
  not part of the shipping product. Intentionally omitted.
- `KIROGRAPH-COMPARISON.md` and `BENCHMARKS.md` exist at the root but were not
  summarized; benchmark numbers are intentionally excluded as volatile.

## Recommendations

1. **Close the Rust/C# attribute gap** or document it in user-facing docs so
   users know `panic-points`/attribute features are Rust-first.
2. **Clarify C# LSP support** — either wire `CSharpLsProvider` into the CLI or
   note that `--lsp` is Rust-only.
3. **Keep schema docs in sync** — `data_models.md`'s ER diagram is the most
   change-sensitive document; regenerate it whenever `src/db/schema.rs` changes.
4. **Regenerate after structural changes** — module moves/renames will stale
   components.md and the source anchors in index.md.

## Verification Status

- Verified against source: `Cargo.toml`, `src/main.rs`, `src/lib.rs` (header +
  API list), `src/indexing.rs` (header + entry methods), `src/db/schema.rs`
  (full), `src/languages/mod.rs`, `src/languages/common.rs`, `src/graph/mod.rs`,
  `src/lsp/mod.rs`, `src/cli/mod.rs`, `.github/workflows/ci.yml`, `deny.toml`,
  `rust-toolchain.toml`, README.
- Not independently run: build, tests, or the CLI (documentation task only).
  No code was modified.
