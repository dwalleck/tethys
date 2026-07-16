# tethys

Code-intelligence CLI and library that indexes Rust and C# source with tree-sitter
into a SQLite cache and answers symbol, reference, call-graph, and dependency
queries. This glossary fixes the terms that are easy to conflate — especially the
several distinct operations that all get called "resolve."

## Language

### The index

**Index**:
The SQLite store of symbols, references, imports, call edges, and file dependencies
for one workspace, at `.rivets/index/tethys.db` — a *rebuildable cache* of parsed
source (derived, disposable via `--rebuild`, never the source of truth). "Index" is
the canonical noun; also used as a verb (building it).
_Avoid_: database (when you mean the tethys index specifically)

**Workspace**:
The root directory tethys indexes; for Rust, the Cargo workspace and its member
crates. Every indexed path is stored relative to it.

**Crate**:
Cargo's unit of compilation, discovered from a `Cargo.toml`. Prefer "crate" over
"package" — the latter is only the `[package]` table; the resolved unit is a crate.
_Avoid_: package, module (for a whole compilation unit)

### Code entities

**Symbol**:
A named definition in source — function, method, struct, class, trait, enum,
module, and the like — normalized across languages into a fixed set of kinds. A
definition, never a usage.
_Avoid_: definition, declaration, entity

**Reference**:
A site where a symbol is *used* (a call, import, type mention, field access, …), as
opposed to where it is defined. Carries a reference kind and is unresolved
(`symbol_id` is NULL) until reference resolution binds it.
_Avoid_: usage, mention, occurrence

**Import**:
A `use` (Rust) or `using` (C#) statement recorded per file. The corroboration input
that reference resolution and cross-crate call edges are checked against.
_Avoid_: use statement, using directive

**Re-export reference**:
The reference kind emitted at a `pub use` site (Rust), one per named non-glob leaf,
binding the re-exported symbol through the same explicit-import machinery as a bare
usage. It carries no containing symbol (`in_symbol_id` NULL), so call edges and
panic points never see it, yet it still counts as an inbound reference — which is
what stops a symbol consumed only via re-export from looking dead. A plain
(non-`pub`) `use` emits no reference.
_Avoid_: re-export (bare, when you mean the reference)

**Macro-token call reference**:
The `macro_call` reference kind emitted for a bare call-shaped identifier token
inside a macro invocation's token tree (`assert_eq!(helper(), 1)` → `helper`).
Distinct from the `macro` kind, which binds the macro NAME itself (`assert_eq`).
Token-soup provenance: rows that reference resolution cannot bind are dropped,
and resolved rows never enter call edges — suppression consumers (dead code,
untested code, deprecated callers) read them from refs.
_Avoid_: macro ref (ambiguous between the two kinds)

**Indexed file**:
A source file recorded in the index with its language, mtime, and size. The unit
that reindexing and staleness track.
_Avoid_: document, source file

**Panic point**:
An `.unwrap()` or `.expect()` call site — a place that can panic at runtime.

### A symbol's name

**Name**:
A symbol's bare identifier, with no qualification (`save`).

**Qualified name**:
A symbol's hierarchy path *within its file* (`IssueStorage::save`) — enclosing
types and scopes, but not the module. For a method in `impl Trait for Type`,
the qualifier is the IMPLEMENTING TYPE (`Type::method`, never `Trait::method`)
— the same identity the reference side derives for receivers, so
receiver-typed references can bind exactly.

**Hierarchy edge / inherit reference**:
The `inherit` reference kind (tethys-j2r1), emitted at TWO granularities:
a *type-level edge* (`impl Trait for Type`, supertrait bounds, C# base
lists — `in_symbol` is the subtype, the named target is the supertype)
and a *method-level marker* (each method in a Rust trait-impl block —
"this method implements a trait member", the dead-code suppression
channel). Unresolved inherit references are RETAINED: external supertypes
(`Display`) are the majority and are themselves the suppression signal.
Never in call edges — implementing a trait is not a call.
_Avoid_: conflating the edge (type-to-type) with the marker (method-scoped)

**Parent linkage**:
The insert-time step that sets `symbols.parent_symbol_id` from the extracted
container name: a member (method, struct field, enum variant, C# class
member) is linked to the SAME-FILE container symbol its name came from, or
left NULL when the container lives in another file or the name collides.
Same-file by construction — distinct from all three resolution operations
above (no imports, no module paths, no cross-file search).
_Avoid_: "resolving the parent" (reserve resolve for the three operations)

**Module path**:
The chain of modules leading to a symbol (`crate::storage::issue`) — where it
lives, kept separate from what it is called so "exports from module" is queryable.

**Full path**:
`module_path` joined to `qualified_name`
(`crate::storage::issue::IssueStorage::save`). Computed on read, never stored.

### Resolution

Three distinct operations share the word "resolve." Always qualify which one.
_Avoid_: bare "resolve" / "resolver" / "resolution" without a qualifier.

**Reference resolution**:
Binding each reference to the symbol it names — the Pass 2 step that fills in a
reference's `symbol_id`. Language-neutral; runs behind the drivers in `resolve.rs`.

**Module-path resolution**:
Mapping a module-path string (`crate::a::b`, `MyApp.Services`) to the file or crate
it denotes on disk. Language-specific, behind the `ModuleResolver` trait
(`resolver.rs` handles Rust).

**LSP resolution**:
An opt-in fallback pass (`--lsp`) that asks a language server (`goto_definition`)
to bind references tree-sitter could not resolve on its own.

**Resolution strategy**:
The factual label recording *which mechanism* bound a resolved reference
(`same_file`, `explicit_import`, `glob_import`, `import_union`,
`qualified_exact`, `same_crate`, `unique_workspace`,
`qualified_module_fallback`, `lsp`) — stored as text on `refs` at bind time;
NULL means unresolved. See ADR-0003.
_Avoid_: calling the strategy a "confidence" — it is a code path, not a score.

**Confidence band**:
`high` / `medium` / `speculative`, derived from the resolution strategy in the
query surface (one view `CASE`), never stored. Remeasuring moves strategies
between bands without re-indexing.
_Avoid_: numeric confidences; persisting the band.

**Speculative edge**:
A resolved reference whose strategy bands `speculative` — kept so
recall-side consumers (dead code) can treat it as a suppression, while
precision-side consumers (callers, impact, panic-points) can exclude it.
_Avoid_: "phantom edge" as a synonym — a phantom is a *wrong* binding; a
speculative edge is an *unverified* one.

### Indexing lifecycle

**Indexing**:
The pipeline that parses source, extracts symbols and references, stores them, and
runs reference resolution.

**Extraction**:
Pulling symbols and references out of one parsed file for one language, behind the
`LanguageSupport` trait. Together with module-path resolution, the only
language-aware step of indexing.

**Pass 1 / Pass 2**:
The two phases of indexing. Pass 1 stores symbols and unresolved references; Pass 2
performs reference resolution. A reference carries a NULL `symbol_id` between them.

**Pending dependency**:
A cross-file dependency not yet resolvable, queued and retried until a pass makes
no further progress. How indexing tolerates forward and circular references.

**Reindex**:
Re-running indexing over only the files whose mtime changed since the last index,
rather than the whole workspace.
_Avoid_: incremental index (say "reindex" or "incremental update")

**Staleness**:
Divergence between the index and the filesystem, reported in three buckets —
modified, added, deleted. Drives what a reindex must touch.

**Orphan file**:
An indexed file whose on-disk counterpart has been deleted since its last
index — staleness's "deleted" bucket. An orphan-cleanup pass purges its rows
(FK cascades take the dependents) at the start of every non-rebuild index
run; left in place, its stale imports and references feed phantom edges to
dependency queries.
_Avoid_: stale file (staleness also covers modified and added), deleted file
(ambiguous with a file the purge already removed)

**Streaming mode**:
An indexing mode that writes parsed files to SQLite incrementally via a background
writer thread, bounding memory to the batch size. The contrast is **batch mode**,
the default, which accumulates all data in memory before writing.

### The seam

**Seam**:
The enforced boundary that keeps the indexing and reference-resolution drivers
language-neutral; all language-specific module semantics live behind
`ModuleResolver`. Policed by `tests/seam_lint.rs`.
_Avoid_: boundary, interface (when you mean this specific invariant)

**LanguageSupport**:
The trait a language implements to extract symbols and references from its parse
trees.

**ModuleResolver**:
The trait a language implements for module-path resolution. Must not touch the
database — enforced by the seam lint.

### Graph analyses

**Impact**:
Given a target file or symbol, the files and symbols that depend on it — direct and
transitive dependents. Answers "what could break if I change this."
_Avoid_: blast radius

**Reachability**:
Call-graph traversal from a symbol. Forward reachability follows callees (what this
can reach); backward follows callers (what can reach this).

**Callers**:
The symbols that call a given symbol, directly or transitively — a backward
call-graph query at symbol granularity.

**Affected tests**:
The test symbols whose reachable set touches a set of changed files — the tests
worth running for those changes.

**Coupling**:
Per-crate afferent (Ca) and efferent (Ce) dependency counts, plus the derived
instability metric `Ce / (Ca + Ce)`.

**Cycle**:
A circular dependency among files.

**Call edge**:
A symbol-to-symbol call relationship retained in the index. Cross-crate edges are
kept only when corroborated by an import ("k-hybrid"); a raw call reference is the
pre-corroboration signal, a call edge is the retained fact.
_Avoid_: call, call reference (when you mean the retained edge)

### Query standing

**Confirmed / Indeterminate (query standing)**:
Whether the index can stand behind an analysis result as a whole. A result is
*confirmed* when every input file is indexed and fresh; *indeterminate* when an
input is missing from the index or stale on disk — or when an analysis's root
set is empty (untested-code with zero test roots indexed: every product symbol
is trivially unreachable, so the analysis reports nothing rather than accusing
everything). An indeterminate empty result means "cannot see", never "clean" —
it must surface distinguishably, not as silence.
_Avoid_: treating an empty result as clean without checking standing; conflating
standing with confidence bands (bands grade individual edges; standing grades a
query's inputs).
