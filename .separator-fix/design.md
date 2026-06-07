# Design: ModuleResolver seam (separator-fix loop)

Spec: `.separator-fix/spec.md` (rev 2, signed). Probe basis: `.separator-fix/probe-findings.md`.
This design extends what the probe proved and contradicts none of it.

## Purpose

Extract a per-language `ModuleResolver` trait that owns module-path→file resolution
and the parsing of each language's import-path storage format. Rust semantics
(`crate`/`self`/`super`, Cargo crates, implicit-crate retry) move into the Rust
implementation; C# gets an explicit declining stub (tethys-jwf9 unchanged). Strict
behavior neutrality for both languages, verified by a byte-identical natural-key
dump oracle (`.separator-fix/dump.sh`, determinism proven — claim C1).

## Architecture

New module: `src/languages/module_resolver.rs`.

```rust
pub(crate) struct ModuleContext<'a> {
    pub current_file: &'a Path,       // absolute path of the referencing file
    pub workspace_root: &'a Path,
    pub crates: &'a [CrateInfo],      // workspace packages; empty for C#-only
}

/// One prefix-split of a qualified reference: candidate files in priority
/// order, plus the symbol tail to look up. Within a split, the FIRST candidate
/// that exists in the index claims the split — if the tail then misses, the
/// split is abandoned (remaining candidates are NOT tried). This encodes the
/// exact interleaving of today's resolve.rs:312-342 (verified: C6 fixture).
pub(crate) struct QualifiedSplit {
    pub files: Vec<PathBuf>,
    pub tail: String,
}

pub(crate) trait ModuleResolver: Send + Sync {
    /// Separator used in this language's stored import source_module strings
    /// ("::" Rust, "." C#). Single source of truth for the three storage-side
    /// matches (batch_writer.rs:378, indexing.rs:852, indexing.rs:1047).
    fn import_separator(&self) -> &'static str;

    /// Segments → defining file on disk (filesystem probing allowed, NO DB).
    fn resolve_import_segments(&self, segments: &[String], ctx: &ModuleContext<'_>)
        -> Option<PathBuf>;

    /// Stored source_module string → file. Provided: split on
    /// import_separator(), delegate to resolve_import_segments.
    fn resolve_import(&self, source_module: &str, ctx: &ModuleContext<'_>)
        -> Option<PathBuf> { ... }

    /// Qualified-name ("::"-canonical) fallback candidates, longest prefix
    /// first. Owns language-specific interpretations (Rust: implicit-crate
    /// retry then as-written). Empty for languages without module semantics.
    fn qualified_splits(&self, ref_name: &str, ctx: &ModuleContext<'_>)
        -> Vec<QualifiedSplit>;
}

pub(crate) fn get_module_resolver(lang: Language) -> &'static dyn ModuleResolver;
```

**RustModuleResolver**: absorbs `resolver::resolve_module_path` (moved, not
rewritten) and the `src_root_for_file` anchor computation (becomes a free fn taking
`(file, crates, workspace_root)`; the impl recomputes it per call — see C9).
`qualified_splits` reproduces resolve.rs:299-343: for each split longest→shortest,
files = [implicit-crate path?, as-written path?] (implicit-crate omitted when
prefix[0] ∈ {crate, self, super}).

**CSharpModuleResolver**: `import_separator() = "."`;
`resolve_import_segments` → `None`; `qualified_splits` → `vec![]`. Doc comment
cites tethys-jwf9 as the issue that fills this in.

**Call-site changes (all language-neutral after the change):**

| Site | Today | After |
|---|---|---|
| `resolve.rs` resolve_module_to_file_id (:424) | splits on `"::"`, calls resolve_module_path | `resolver.resolve_import(source_module, ctx)` → relative_path → get_file_id |
| `resolve.rs` qualified_module_fallback (:286) | inline splits + crate/self/super + dual interpretation + interleaved DB lookups | neutral driver over `resolver.qualified_splits()`: per split, first indexed file claims it; tail miss abandons split |
| `resolve.rs` resolve_refs_for_file (:83) | computes src_root via Tethys::src_root_for_file | builds ModuleContext; resolver fetched per file's language |
| `indexing.rs` :950, :1112 | resolve_module_path direct | `resolver.resolve_import_segments(&import_stmt.path, ctx)` |
| `batch_writer.rs` :378, `indexing.rs` :852, :1047 | `match language { Rust => "::", CSharp => "." }` ×3 | `get_module_resolver(lang).import_separator()` |

Unchanged: `is_qualified = ref_name.contains("::")` (canonical storage separator,
decision #5 — not per-language knowledge); fallback_symbol_search's same-crate
scoping via `get_crate_for_file` (package scoping, not module resolution — it
already degrades gracefully for crate-less files; spec B3's grep does not cover it
textually because the call is `self.get_crate_for_file(...)`); LSP Pass 3;
`cargo.rs` discovery; DB schema; storage formats.

## Input shapes (step-2 enumeration)

- **Language**: Rust (full impl) | CSharp (stub) — claims C2/C6 vs C3/C7.
- **Import segments**: single segment (workspace-crate name; hyphen↔underscore) |
  multi-segment | first ∈ {crate, self, super} | external (std::*, System) |
  glob (skipped at indexing site; glob-import arm in resolve.rs unchanged) |
  aliased — C2/C5/C8.
- **Ref name**: simple (no `::`, untouched path) | 2-segment | ≥3-segment |
  first ∈ {crate, self, super} (implicit-crate retry suppressed) | trap shape:
  both interpretations land on indexed files (C6).
- **current_file**: present | None (synthetic refs — decline preserved by code
  motion; observable inside C2's dump).
- **crates**: empty (C# probe WS — C3) | single (self-index — C2) | multiple
  (C6 trap fixture) | nested longest-prefix (unchanged code, existing unit test
  `get_crate_for_file_prefers_longest_prefix_match` — out of new-claim scope).
- **Strings/paths**: hyphenated crate names (existing resolver tests, code moved
  not rewritten); non-UTF8/spaces out of scope: unchanged PathBuf handling.

## Claims and falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|---|---|---|---|---|---|
| C1 | The natural-key dump is deterministic across from-scratch re-indexes (else C2/C3 are noise) | index C# WS and tethys self-index twice each from scratch, diff dumps | `diff(1)` | 5m | **PASSED** (15,759-line + 24-line dumps identical) | new integration test indexes a fixture twice and compares dumps |
| C2 | Rust resolution byte-identical post-extraction | pre/post-binary dumps on self-index + C6 fixture | `diff(1)` | 15m | pending | existing `pass2_qualified_paths`, `resolver_routing`, `module_path_integration`, `pass2_no_imports` + new fixture test |
| C3 | C# resolution byte-identical post-extraction | pre/post dumps on C# probe WS | `diff(1)` | 10m | pending | new C# fixture test: `Hasher::Hash`/`Outer::Inner` resolved, `Console::WriteLine` unresolved |
| C4 | resolve.rs free of Rust module semantics | `rg 'resolve_module_path|CrateInfo|"crate"|"super"' src/resolve.rs` | `rg` exit code | 1m | pending | source-lint `#[test]` greps resolve.rs |
| C5 | indexing.rs import-dep arms routed through the trait | `rg 'resolve_module_path' src/indexing.rs` = 0 AND `## file_deps` dump section identical | `rg` + diff section | 5m | pending | lint test + file_deps asserts in fixture test |
| C6 | Within-split interleaving preserved: implicit-crate file claiming a split suppresses as-written even on tail miss | trap fixture (`app/src/helper.rs` without `do_thing`; crate `helper` with it): `helper::do_thing` must remain UNRESOLVED | SQL on refs (single distinct row) | 10m | **baseline PASSED** (current binary: UNRESOLVED, verified) | new integration test `qualified_split_abandon_trap` embedding the fixture |
| C7 | C# stub declines: resolve_import → None, qualified_splits → empty | unit tests on the stub | unit test asserts | 5m | pending | same unit tests |
| C8 | import_separator() replaces all 3 storage-side separator matches with identical constants | `## imports` dump section identical on both workspaces | diff section | 5m | pending | imports asserts in fixture tests |
| C9 | Indexing wall-time within ±10% (vtable dispatch + per-call src_root recompute) | hyperfine median-of-3, pre vs post, self-index | `hyperfine` | 20m | pending | **manual** (criterion bench `benches/indexing` exists; CI perf gates are flaky) — requires explicit user approval |
| C10 | Resolver impls are DB-free (interleaved-lookup divergence class structurally excluded) | `rg 'use crate::db|&Index' src/languages/module_resolver*` = 0 | `rg` | 1m | pending | source-lint test |

Named buggy implementations per falsifier (non-vacuity): C1 — dump keyed on
autoincrement ids under rayon ordering (would fail today); C2/C6 — flat candidate
list resolving the trap ref through interpretation B; C3 — resolver dispatched by
workspace-majority language instead of per-file; C4 — qualified_module_fallback
left inline; C5 — indexing.rs call sites forgotten (behaviorally invisible, hence
the grep); C7 — stub as `todo!()` or returning a garbage path; C8 — swapped
separator constants (C# imports would store `MyApp::Models`); C9 — per-call crate
scan ballooning on large workspaces; C10 — ctx growing a DB handle.

## Negative space

This design deliberately does NOT:
1. Implement C# namespace→file resolution (tethys-jwf9; the stub's doc cites it).
2. Change any storage format, schema, or stored separator (decision #5).
3. Re-spell qualified names for display (tethys-dsp1, filed this loop).
4. Extract package discovery (`cargo.rs`) or touch the architecture phase.
5. Add languages (tethys-8mze consumes the seam later).
6. Touch LSP Pass 3 or fallback_symbol_search's same-crate scoping.

## Approval

Design approved by requester 2026-06-06. C9's manual regression fence explicitly
approved by requester 2026-06-06 (one-shot hyperfine measurement + criterion
bench for ad-hoc re-runs; no CI perf gate).

## Tracker references

tethys-jwf9 (verified, open), tethys-8mze (verified, open), tethys-dsp1 (filed
2026-06-06 during this design). No other deferrals exist in this document.
