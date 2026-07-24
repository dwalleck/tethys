//! Cross-file reference resolution (Pass 2) and LSP-based resolution (Pass 3).
//!
//! This module contains the resolution methods on [`Tethys`] that handle:
//! - Import-based cross-file symbol resolution
//! - LSP `goto_definition` resolution
//! - LSP `find_references` for caller discovery

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tracing::{debug, info, trace, warn};

use crate::Tethys;
use crate::error::{Error, Result};
use crate::graph;
use crate::languages::get_language_support;
use crate::languages::module_resolver::{
    GlobPolicy, ModuleContext, ModuleResolver, NamespaceMap, get_module_resolver,
};
use crate::lsp::{self, LspProvider};
use crate::types::{
    CallEdgeSelection, Caller, DEFAULT_LSP_TIMEOUT_SECS, FileId, Import, Language,
    LspCompletedSession, LspOutcome, LspSessionResult, Reference, ReferenceKind,
    ResolutionStrategy, Symbol, SymbolId, SymbolKind, UnresolvedRefForLsp,
};

/// Whether a reference of `ref_kind` is allowed to bind to a symbol of
/// `symbol_kind`.
///
/// A macro invocation (`foo!()`) lives in Rust's macro namespace and must
/// resolve only to a macro definition — never to a same-named fn/type/const.
/// Without this gate, `write!(...)` resolves by bare name to a workspace
/// `fn write`, forging a phantom call edge that corrupts
/// callers/reachable/impact/coupling.
///
/// Symmetrically, a call or construct must never bind a data member
/// (property, event, field): without this, `new Exception("timeout")` in a
/// file whose result type declares `public Exception Exception { get; }`
/// binds the BCL constructor to the property, forging a phantom edge —
/// caught by the tethys-xebx corpus audit. `Delegate` stays bindable
/// (`new Transform(Method)` is a real construct). The general kind-aware
/// binding work is tracked at tethys-0aqj.
///
/// Every other reference kind is unconstrained here (a kind-mismatched
/// candidate is simply skipped, so resolution falls through to the next
/// strategy; a ref with no kind-valid candidate stays unresolved).
fn ref_binds_to_symbol_kind(ref_kind: &ReferenceKind, symbol_kind: SymbolKind) -> bool {
    match ref_kind {
        ReferenceKind::Macro => symbol_kind == SymbolKind::Macro,
        // MacroCall joins Call/Construct: a call-shaped token must not bind a
        // data member (tethys-8ym0; the `_ => true` default would silently
        // allow it).
        ReferenceKind::Call | ReferenceKind::Construct | ReferenceKind::MacroCall => {
            !symbol_kind.is_data_member()
        }
        // A hierarchy edge names a TYPE container (tethys-j2r1): binding a
        // same-named fn or field would fabricate an edge.
        ReferenceKind::Inherit => symbol_kind.is_container(),
        _ => true,
    }
}

/// Per-file context used during cross-file reference resolution (Pass 2).
///
/// Bundles the import tables and path information that stay constant while
/// resolving every reference within a single file.
pub(crate) struct ResolveContext<'a> {
    pub(crate) explicit_imports: &'a HashMap<&'a str, (&'a str, &'a str)>,
    pub(crate) glob_imports: &'a [&'a str],
    pub(crate) current_file_path: Option<&'a Path>,
    pub(crate) file_id: FileId,
    /// Per-language module resolution (the [`ModuleResolver`] seam),
    /// selected by the file's language.
    pub(crate) resolver: &'a dyn ModuleResolver,
    pub(crate) module_ctx: &'a ModuleContext<'a>,
}

impl Tethys {
    /// Resolve cross-file references against the symbol database (Pass 2).
    ///
    /// After all files are indexed (Pass 1), this method resolves unresolved
    /// references by matching them to symbols discovered in other files via
    /// the imports table.
    ///
    /// Returns the number of references successfully resolved.
    pub(crate) fn resolve_cross_file_references(&self) -> Result<usize> {
        let unresolved = self.db.get_unresolved_references()?;
        if unresolved.is_empty() {
            return Ok(0);
        }

        debug!(
            unresolved_count = unresolved.len(),
            "Starting cross-file reference resolution (Pass 2)"
        );

        // Namespace→files map for the C# using-arm, built once per resolve
        // run (one Module-kind query; empty for Rust-only workspaces).
        let namespace_map = self.build_namespace_map()?;

        // Group by file for efficiency - avoids repeated import lookups
        let mut by_file: HashMap<FileId, Vec<Reference>> = HashMap::new();
        for ref_ in unresolved {
            by_file.entry(ref_.file_id).or_default().push(ref_);
        }

        // Resolutions are collected during the scan and applied in ONE
        // transaction afterwards (idxperf claim C7). No resolution lookup
        // reads the refs table mid-pass (probe-verified), so deferring the
        // writes cannot change any outcome — and the batched commit lands
        // before populate_call_edges and Pass 3 read refs.
        let mut resolutions: Vec<(i64, SymbolId, ResolutionStrategy)> = Vec::new();
        for (file_id, refs) in by_file {
            self.resolve_refs_for_file(file_id, refs, &namespace_map, &mut resolutions)?;
        }

        let resolved_count = resolutions.len();
        self.db.apply_resolutions(&resolutions)?;

        Ok(resolved_count)
    }

    /// Resolve references for a single file using its imports.
    ///
    /// The per-file anchor comes from [`ModuleResolver::file_anchor`] (for
    /// Rust, the crate's source root) so that `crate::*` paths in a
    /// sub-crate file resolve under that crate's own source root rather
    /// than the workspace root. For orphan files (anchor falls back to the
    /// file's parent), `crate::*` becomes a semantic no-op as in Rust
    /// itself; `self::`/`super::` arms keep working off `current_file`
    /// directly, and the path-agnostic `fallback_symbol_search` still has a
    /// chance to resolve qualified references via
    /// `get_symbol_by_qualified_name`.
    fn resolve_refs_for_file(
        &self,
        file_id: FileId,
        refs: Vec<Reference>,
        namespace_map: &NamespaceMap,
        resolutions: &mut Vec<(i64, SymbolId, ResolutionStrategy)>,
    ) -> Result<()> {
        let imports = self.db.get_imports_for_file(file_id)?;
        // Do NOT short-circuit on imports.is_empty(): try_resolve_reference's
        // fallback_symbol_search (same-crate prefix + unscoped unique lookup) and
        // get_symbol_by_qualified_name paths resolve workspace-internal refs
        // without needing any `use` statement (rivets-dn35). The explicit/glob
        // import paths become no-ops on empty maps, which is correct.

        // Get the current file's path for relative path resolution
        let Some(file_record) = self.db.get_file_by_id(file_id)? else {
            warn!(
                file_id = %file_id,
                "File not found during reference resolution - possible database inconsistency"
            );
            return Ok(());
        };
        let current_file_path = self.workspace_root.join(&file_record.path);

        // Per-language module resolution, selected by the file's language.
        let module_resolver = get_module_resolver(file_record.language);
        let anchor =
            module_resolver.file_anchor(&current_file_path, &self.workspace_root, self.crates());
        let module_ctx = ModuleContext {
            current_file: &current_file_path,
            crates: self.crates(),
            anchor,
            // Namespace-import languages get the map; Rust contexts stay None.
            namespaces: (file_record.language == Language::CSharp).then_some(namespace_map),
        };

        // Build import structures
        let (explicit_imports, glob_imports) = Self::build_import_maps(&imports);

        let ctx = ResolveContext {
            explicit_imports: &explicit_imports,
            glob_imports: &glob_imports,
            current_file_path: Some(&current_file_path),
            file_id,
            resolver: module_resolver,
            module_ctx: &module_ctx,
        };

        // Memoize outcomes by the FULL reference_name string within this
        // file (idxperf claim C6): resolution depends only on the file's
        // import context (constant here) and the name, so the first
        // outcome — including a negative one — holds for every duplicate.
        // Keying by anything shorter (e.g., the name's tail) would collapse
        // `alpha` and `Holder::alpha`, which legitimately resolve
        // differently.
        let mut memo: HashMap<String, Option<(SymbolId, ResolutionStrategy)>> = HashMap::new();

        for mut ref_ in refs {
            // Move the owned name out of the ref: it is used only as the memo
            // key and as the lookup string. `try_resolve_reference` does not
            // read `ref_.reference_name`, so taking it here lets a memo miss
            // hand ownership to the map instead of cloning the String.
            let Some(ref_name) = ref_.reference_name.take() else {
                continue;
            };

            // Macro invocations bypass the name-keyed memo: a `write!()` macro
            // and a `write()` call share `reference_name` but resolve in
            // different namespaces (see `ref_binds_to_symbol_kind`), so a shared
            // memo entry would cross-contaminate them. Macro refs are rare, so
            // resolving them fresh costs little and keeps the memo sound.
            let outcome = if matches!(ref_.kind, ReferenceKind::Macro) {
                self.try_resolve_reference(&ref_, &ref_name, &ctx)?
            } else if let Some(cached) = memo.get(ref_name.as_str()) {
                *cached
            } else {
                let outcome = self.try_resolve_reference(&ref_, &ref_name, &ctx)?;
                memo.insert(ref_name, outcome);
                outcome
            };

            if let Some((symbol_id, strategy)) = outcome {
                resolutions.push((ref_.id, symbol_id, strategy));
            }
        }

        Ok(())
    }

    /// `UniqueAcrossAll` union arm (C#): collect candidate symbols across the
    /// file's plain namespace usings (types) AND `using static Type;`
    /// directives (Type's methods), then resolve iff the deduped union is
    /// exactly one symbol (spec decision #3).
    ///
    /// The types arm caps at 2; the static-member arm caps at 2 per `using
    /// static` directive — two distinct candidates already force a decline, so
    /// exact bounds don't matter, only "unique vs not". The dedup is keyed on
    /// symbol id and is INTRA-arm: the type and member kind-sets are disjoint,
    /// so one symbol can never appear in both arms (no cross-arm collision is
    /// possible). It is defensive against the member arm surfacing the SAME
    /// symbol twice — e.g. two distinct `using static` directives that resolve
    /// to the same type+file (a file with multiple namespace blocks) — so such
    /// a self-collision collapses to one candidate instead of false-declining.
    fn resolve_via_union_arm(
        &self,
        ref_name: &str,
        glob: &crate::languages::module_resolver::GlobResolution,
        ctx: &ResolveContext<'_>,
    ) -> Result<Option<Symbol>> {
        // Types arm: namespace usings → files → type symbols by name.
        let mut candidate_files = Vec::new();
        for source_module in ctx.glob_imports {
            candidate_files.extend(
                ctx.resolver
                    .resolve_import_files(source_module, ctx.module_ctx),
            );
        }
        let mut candidates =
            self.db
                .search_symbols_by_name_in_files(ref_name, glob.kinds, &candidate_files, 2)?;

        // Static-member arm: `using static Ns.Type;` → Type's methods scoped
        // to the namespace's files.
        if let Some(member_kinds) = glob.member_kinds {
            for source_module in ctx.glob_imports {
                if let Some(smi) = ctx
                    .resolver
                    .static_member_import(source_module, ctx.module_ctx)
                {
                    candidates.extend(self.db.search_type_members_by_name(
                        ref_name,
                        &smi.type_name,
                        &smi.files,
                        member_kinds,
                        2,
                    )?);
                }
            }
        }

        candidates.sort_by_key(|s| s.id.as_i64());
        candidates.dedup_by_key(|s| s.id.as_i64());
        match candidates.len() {
            1 => Ok(candidates.pop()),
            0 => Ok(None),
            n => {
                // Restore the ambiguity trail the pre-union primitive emitted,
                // and keep parity with the other refuse-ambiguity paths in
                // db/symbols.rs which still `debug!` on multi-candidate decline.
                debug!(
                    ref_name = %ref_name,
                    candidate_count = n,
                    "Refusing ambiguous using-arm match (multiple candidates across types / static-member arms)"
                );
                Ok(None)
            }
        }
    }

    /// Build lookup maps from imports for reference resolution.
    fn build_import_maps(imports: &[Import]) -> (HashMap<&str, (&str, &str)>, Vec<&str>) {
        let mut explicit_imports: HashMap<&str, (&str, &str)> = HashMap::new();
        let mut glob_imports: Vec<&str> = Vec::new();

        for imp in imports {
            if imp.symbol_name == "*" {
                glob_imports.push(&imp.source_module);
            } else {
                let lookup_name = imp.alias.as_deref().unwrap_or(&imp.symbol_name);
                if let Some((prev_symbol, prev_module)) =
                    explicit_imports.insert(lookup_name, (&imp.symbol_name, &imp.source_module))
                {
                    trace!(
                        lookup_name = %lookup_name,
                        prev_symbol = %prev_symbol,
                        prev_module = %prev_module,
                        new_symbol = %imp.symbol_name,
                        new_module = %imp.source_module,
                        "Import name collision: overwriting previous import"
                    );
                }
            }
        }

        (explicit_imports, glob_imports)
    }

    /// Try to resolve a single reference using imports and fallback search.
    ///
    /// Pure lookup: returns the resolved target's `SymbolId` (or `None`)
    /// without writing — the caller collects outcomes and applies them in
    /// one batched transaction. The outcome depends only on the file's
    /// import context and `ref_name`, which is what makes per-file
    /// memoization sound; `ref_` is used for trace logging only, so a memo
    /// hit skipping this fn loses nothing but a duplicate trace line.
    fn try_resolve_reference(
        &self,
        ref_: &Reference,
        ref_name: &str,
        ctx: &ResolveContext<'_>,
    ) -> Result<Option<(SymbolId, ResolutionStrategy)>> {
        let is_qualified = ref_name.contains("::");

        // Try explicit imports
        if let Some(symbol) = self
            .resolve_via_explicit_import(ref_name, ctx, is_qualified)?
            .filter(|s| ref_binds_to_symbol_kind(&ref_.kind, s.kind))
        {
            trace!(
                ref_id = ref_.id,
                ref_name = %ref_name,
                symbol_id = %symbol.id,
                "Resolved reference via explicit import"
            );
            return Ok(Some((symbol.id, ResolutionStrategy::ExplicitImport)));
        }

        // Try glob imports — consumption semantics are declared by the
        // language's resolver (GlobResolution).
        let glob = ctx.resolver.glob_resolution();
        match glob.policy {
            GlobPolicy::FirstMatch => {
                // Pre-seam behavior, verbatim: iterate stored order, first
                // match wins, any symbol kind.
                for source_module in ctx.glob_imports {
                    if let Some(symbol) = self
                        .resolve_symbol_in_module(ref_name, source_module, ctx, is_qualified)?
                        .filter(|s| ref_binds_to_symbol_kind(&ref_.kind, s.kind))
                    {
                        trace!(
                            ref_id = ref_.id,
                            ref_name = %ref_name,
                            symbol_id = %symbol.id,
                            "Resolved reference via glob import"
                        );
                        return Ok(Some((symbol.id, ResolutionStrategy::GlobImport)));
                    }
                }
            }
            // C# using-arm: UNION candidates across all usings, then one
            // unique-or-decline. Simple names only; qualified refs keep their
            // pre-existing fallback path. See [`Self::resolve_via_union_arm`].
            GlobPolicy::UniqueAcrossAll if !is_qualified => {
                if let Some(symbol) = self.resolve_via_union_arm(ref_name, &glob, ctx)? {
                    trace!(
                        ref_id = ref_.id,
                        ref_name = %ref_name,
                        symbol_id = %symbol.id,
                        "Resolved reference via namespace / static-member imports"
                    );
                    return Ok(Some((symbol.id, ResolutionStrategy::ImportUnion)));
                }
            }
            // Qualified ref under UniqueAcrossAll: decline here; the
            // qualified fallback arms below handle it exactly as before.
            GlobPolicy::UniqueAcrossAll => {}
        }

        // Fallback search differs for qualified vs simple names. The caller's
        // file path scopes simple-name lookups to the same crate first; without
        // that scope, names like `Error` resolve to the first matching symbol
        // workspace-wide (rivets-0gom).
        if let Some((symbol, sub_path)) = self
            .fallback_symbol_search(ref_name, is_qualified, ctx.current_file_path)?
            .filter(|(s, _)| ref_binds_to_symbol_kind(&ref_.kind, s.kind))
        {
            trace!(
                ref_id = ref_.id,
                ref_name = %ref_name,
                symbol_id = %symbol.id,
                strategy = sub_path.as_str(),
                "Resolved reference via fallback search"
            );
            return Ok(Some((symbol.id, sub_path)));
        }

        // Qualified-path module fallback (rivets-044i). Only fires for refs that
        // both (a) contain `::` and (b) survived every prior path. Interprets the
        // prefix as a module path, looks the tail up in the resolved file.
        if is_qualified
            && let Some(symbol) = self
                .qualified_module_fallback(ref_name, ctx)?
                .filter(|s| ref_binds_to_symbol_kind(&ref_.kind, s.kind))
        {
            trace!(
                ref_id = ref_.id,
                ref_name = %ref_name,
                symbol_id = %symbol.id,
                "Resolved reference via qualified module fallback"
            );
            return Ok(Some((
                symbol.id,
                ResolutionStrategy::QualifiedModuleFallback,
            )));
        }

        trace!(
            ref_name = %ref_name,
            file_id = %ctx.file_id,
            "Reference remains unresolved (likely external crate)"
        );
        Ok(None)
    }

    /// Resolve a qualified reference via the file's [`ModuleResolver`]
    /// candidate enumeration (rivets-044i).
    ///
    /// Used after every import-based and same-crate fallback has missed. The
    /// literal `get_symbol_by_qualified_name` match cannot resolve refs like
    /// `helper::do_thing` from an import-less file because stored
    /// `symbols.qualified_name` is module-stripped (free fns store `name`;
    /// methods store `parent_name::name`). The resolver bridges that gap by
    /// enumerating prefix-splits (longest first), each carrying candidate
    /// files in priority order — for Rust, the implicit-crate interpretation
    /// before the as-written one; see
    /// [`RustModuleResolver::qualified_splits`](crate::languages::module_resolver::RustModuleResolver).
    ///
    /// Driver contract (claim C6, fenced by the `qualified_split_trap`
    /// integration test): within a split, the first candidate file present
    /// in the index claims it; if the tail symbol is then missing in that
    /// file, the split is abandoned — remaining candidates are NOT tried.
    /// This preserves the pre-seam interleaving of candidate generation and
    /// index lookup, including its bounded-ambiguity acceptance (design-v3
    /// C5/C6): a phantom resolution still requires both a colliding file
    /// path and a colliding tail symbol.
    ///
    /// Returns `Ok(None)` for unqualified names (the resolver yields no
    /// splits), external-crate prefixes, and refs whose tail matches no
    /// symbol in any claimed file.
    fn qualified_module_fallback(
        &self,
        ref_name: &str,
        ctx: &ResolveContext<'_>,
    ) -> Result<Option<Symbol>> {
        for split in ctx.resolver.qualified_splits(ref_name, ctx.module_ctx) {
            let mut claimed = None;
            for file in &split.files {
                let relative = self.relative_path(file);
                if let Some(file_id) = self.db.get_file_id(&relative)? {
                    claimed = Some(file_id);
                    break;
                }
            }
            let Some(file_id) = claimed else { continue };

            if let Some(sym) = self
                .db
                .search_symbol_by_qualified_name_in_file(&split.tail, file_id)?
            {
                return Ok(Some(sym));
            }
            // Tail miss: abandon this split without trying its remaining
            // candidates (driver contract, claim C6).
        }

        trace!(
            ref_name = %ref_name,
            "qualified_module_fallback: no prefix split resolved"
        );
        Ok(None)
    }

    /// Resolve a reference via explicit import lookup.
    ///
    /// For qualified references like `Index::open`, looks up the first segment (`Index`)
    /// and searches for the full qualified name in that module.
    fn resolve_via_explicit_import(
        &self,
        ref_name: &str,
        ctx: &ResolveContext<'_>,
        is_qualified: bool,
    ) -> Result<Option<Symbol>> {
        let lookup_name = if is_qualified {
            ref_name
                .split_once("::")
                .map_or(ref_name, |(first, _)| first)
        } else {
            ref_name
        };

        let Some((symbol_name, source_module)) = ctx.explicit_imports.get(lookup_name) else {
            return Ok(None);
        };

        // For qualified refs, build the full qualified name using the imported symbol
        let search_name = if is_qualified {
            if let Some((_, rest)) = ref_name.split_once("::") {
                format!("{symbol_name}::{rest}")
            } else {
                (*symbol_name).to_string()
            }
        } else {
            (*symbol_name).to_string()
        };

        self.resolve_symbol_in_module(&search_name, source_module, ctx, is_qualified)
    }

    /// Resolve a symbol within a specific module (source path).
    ///
    /// Translates the module path to a file path, then searches for the symbol.
    /// Uses qualified name matching for qualified references, simple name for others.
    fn resolve_symbol_in_module(
        &self,
        symbol_name: &str,
        source_module: &str,
        ctx: &ResolveContext<'_>,
        use_qualified_search: bool,
    ) -> Result<Option<Symbol>> {
        let Some(target_file_id) = self.resolve_module_to_file_id(source_module, ctx)? else {
            return Ok(None);
        };

        if use_qualified_search {
            self.db
                .search_symbol_by_qualified_name_in_file(symbol_name, target_file_id)
        } else {
            self.db.search_symbol_in_file(symbol_name, target_file_id)
        }
    }

    /// Translate a stored import module path (e.g., `crate::db`) to a file ID
    /// via the file's [`ModuleResolver`].
    fn resolve_module_to_file_id(
        &self,
        source_module: &str,
        ctx: &ResolveContext<'_>,
    ) -> Result<Option<FileId>> {
        let Some(resolved_file) = ctx.resolver.resolve_import(source_module, ctx.module_ctx) else {
            trace!(
                source_module = %source_module,
                "Cannot resolve module: path resolution failed (likely external crate)"
            );
            return Ok(None);
        };

        let relative_path = self.relative_path(&resolved_file);
        let file_id = self.db.get_file_id(&relative_path)?;

        if file_id.is_none() {
            trace!(
                source_module = %source_module,
                resolved_file = %resolved_file.display(),
                "Cannot resolve module: target file not indexed"
            );
        }

        Ok(file_id)
    }

    /// Fallback symbol search when import-based resolution fails.
    ///
    /// For qualified names, searches by exact `qualified_name` match.
    /// For simple names, prefers a same-crate match (using `caller_file_path`
    /// to identify the caller's containing crate). Only when no same-crate
    /// symbol of that name exists does this fall back to the unscoped
    /// workspace-wide search.
    fn fallback_symbol_search(
        &self,
        ref_name: &str,
        is_qualified: bool,
        caller_file_path: Option<&Path>,
    ) -> Result<Option<(Symbol, ResolutionStrategy)>> {
        if is_qualified {
            return Ok(self
                .db
                .get_symbol_by_qualified_name(ref_name)?
                .map(|s| (s, ResolutionStrategy::QualifiedExact)));
        }

        // Same-crate first: cheap, deterministic, and almost always correct.
        if let Some(path) = caller_file_path {
            if let Some(crate_info) = self.get_crate_for_file(path) {
                let prefix = self.relative_path(&crate_info.path);
                let prefix_str = prefix.to_string_lossy();
                if let Some(symbol) = self
                    .db
                    .search_symbol_by_name_in_path_prefix(ref_name, &prefix_str)?
                {
                    if self.db.get_file_by_id(symbol.file_id)?.is_some() {
                        return Ok(Some((symbol, ResolutionStrategy::SameCrate)));
                    }
                    // Same-crate symbol exists but its file record is gone. DB is
                    // inconsistent; falling through to the unscoped search would
                    // silently mask the inconsistency by returning a different
                    // crate's symbol. Refuse with a warn instead.
                    warn!(
                        ref_name = %ref_name,
                        symbol_id = %symbol.id,
                        file_id = %symbol.file_id,
                        "Same-crate symbol found but file record missing - returning None to avoid masking DB inconsistency"
                    );
                    return Ok(None);
                }
            } else {
                // Caller's file is in the workspace but not in any indexed crate.
                // Most likely: stale DB entry for a deleted file (rivets-lcb6),
                // or an orphan file under a non-member directory (rivets-fayv).
                // Either way the same-crate path silently disables — log it so
                // operators can correlate unresolved refs to orphan files.
                debug!(
                    caller_file = %path.display(),
                    ref_name = %ref_name,
                    "Caller file has no containing crate; same-crate scoping skipped"
                );
            }
        }

        // Unscoped fallback. `search_unique_symbol_by_name` returns None on
        // genuine ambiguity (≥2 workspace candidates), so this only resolves
        // when exactly one workspace-wide candidate exists.
        let Some(symbol) = self.db.search_unique_symbol_by_name(ref_name)? else {
            return Ok(None);
        };
        if self.db.get_file_by_id(symbol.file_id)?.is_some() {
            Ok(Some((symbol, ResolutionStrategy::UniqueWorkspace)))
        } else {
            warn!(
                ref_name = %ref_name,
                symbol_id = %symbol.id,
                file_id = %symbol.file_id,
                "Symbol found but file record missing - database may be inconsistent"
            );
            Ok(None)
        }
    }

    /// Resolve references using LSP `goto_definition` (Pass 3).
    ///
    /// After tree-sitter resolution (Pass 2), some references may still be unresolved
    /// (e.g., external crate symbols, complex type inference). This pass uses the
    /// language server to resolve them. Queries start only after the
    /// server's readiness signal (see [`lsp::ReadinessWait`]) — both
    /// supported servers load their workspace asynchronously, and
    /// pre-readiness queries silently return nothing.
    ///
    /// # Design
    ///
    /// - LSP is spawned lazily (only if there are unresolved refs for this language)
    /// - LSP stays alive for batch queries (amortizes startup cost)
    /// - Shutdown on completion
    /// - Matches LSP definition locations to symbols by file path + line number
    ///
    /// # Arguments
    ///
    /// * `provider` - The LSP provider to use (e.g., `RustAnalyzerProvider`)
    /// * `language` - The language to filter references by (e.g., `Language::Rust`)
    ///
    /// # Returns
    ///
    /// The number of references successfully resolved via LSP.
    #[expect(
        clippy::too_many_lines,
        reason = "LSP resolution involves multiple sequential protocol steps"
    )]
    pub(crate) fn resolve_via_lsp(
        &self,
        provider: &dyn lsp::LspProvider,
        language: Language,
        lsp_timeout_secs: u64,
    ) -> Result<LspSessionResult> {
        // Get unresolved references with file path information, filtered by language
        let all_unresolved = self.db.get_unresolved_references_for_lsp()?;
        let unresolved: Vec<_> = all_unresolved
            .into_iter()
            .filter(|r| {
                r.file_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| Language::from_extension(ext) == Some(language))
            })
            .collect();

        if unresolved.is_empty() {
            debug!(
                language = ?language,
                "No unresolved references for LSP resolution"
            );
            return Ok(LspSessionResult {
                language,
                outcome: LspOutcome::NothingToResolve,
            });
        }

        debug!(
            language = ?language,
            unresolved_count = unresolved.len(),
            "Starting LSP resolution pass (Pass 3)"
        );

        // Start LSP lazily - only if there are refs to resolve
        let mut client = match lsp::LspClient::start(provider, &self.workspace_root) {
            Ok(c) => c,
            Err(e) => {
                // User explicitly requested LSP with --lsp flag, so log at error level
                tracing::error!(
                    error = %e,
                    language = ?language,
                    install_hint = %provider.install_hint(),
                    "LSP server failed to start - LSP resolution skipped. \
                     {} Or remove --lsp flag.",
                    provider.install_hint()
                );
                return Ok(LspSessionResult {
                    language,
                    outcome: LspOutcome::ServerUnavailable {
                        reason: format!("LSP server for {language:?} failed to start: {e}"),
                        install_hint: provider.install_hint().to_string(),
                    },
                });
            }
        };

        let mut resolved_count = 0;
        let mut lsp_error_count: usize = 0;
        let mut lsp_error_messages: Vec<String> = Vec::new();
        let mut opened_files: HashSet<PathBuf> = HashSet::new();
        let mut did_open_warned = false;

        // Language::as_str() returns the LSP language identifier ("rust", "csharp")
        let language_id = language.as_str();

        // Pre-open all unique files for servers like csharp-ls that need time to process
        let unique_files: HashSet<_> = unresolved
            .iter()
            .map(|r| self.workspace_root.join(&r.file_path))
            .collect();

        debug!(
            language = ?language,
            file_count = unique_files.len(),
            "Pre-opening files for LSP"
        );

        for file_path in &unique_files {
            match std::fs::read_to_string(file_path) {
                Ok(content) => match client.did_open(file_path, &content, language_id) {
                    Ok(()) => {
                        opened_files.insert(file_path.clone());
                    }
                    Err(e) => {
                        if did_open_warned {
                            trace!(
                                file = %file_path.display(),
                                error = %e,
                                "Failed to send didOpen notification"
                            );
                        } else {
                            warn!(
                                file = %file_path.display(),
                                error = %e,
                                "Failed to send didOpen notification"
                            );
                            did_open_warned = true;
                        }
                    }
                },
                Err(e) => {
                    if did_open_warned {
                        trace!(
                            file = %file_path.display(),
                            error = %e,
                            "Failed to read file for LSP pre-opening"
                        );
                    } else {
                        warn!(
                            file = %file_path.display(),
                            error = %e,
                            "Failed to read file for LSP pre-opening"
                        );
                        did_open_warned = true;
                    }
                }
            }
        }

        // Both supported servers load their workspace asynchronously after
        // initialize; queries sent before the load completes return empty
        // results (indistinguishable from "no definition") or transient
        // `-32801` errors, so an ungated Pass 3 silently resolves nothing
        // on a cold workspace (probed for rust-analyzer — see
        // .tethys-2mjj/findings.md). Wait for the readiness signal the
        // provider declares for its server before the query loop.
        let timeout = std::time::Duration::from_secs(lsp_timeout_secs);
        match client.wait_until_ready(provider.readiness_wait(), timeout) {
            Ok(true) => {
                debug!(language = ?language, "LSP server ready");
            }
            Ok(false) => {
                warn!(
                    language = ?language,
                    "LSP readiness not detected or timed out, queries may fail"
                );
            }
            Err(e) => {
                warn!(
                    language = ?language,
                    error = %e,
                    "Error while waiting for LSP readiness"
                );
            }
        }

        for unresolved_ref in &unresolved {
            match self.resolve_single_ref_via_lsp(
                &mut client,
                unresolved_ref,
                &mut lsp_error_count,
                &mut lsp_error_messages,
                &mut opened_files,
                language_id,
                &mut did_open_warned,
            ) {
                Ok(true) => resolved_count += 1,
                Ok(false) => {}
                Err(e) => {
                    warn!(
                        ref_id = %unresolved_ref.ref_id,
                        error = %e,
                        "Database error during LSP resolution"
                    );
                }
            }
        }

        if lsp_error_count > LspCompletedSession::MAX_ERROR_MESSAGES {
            info!(
                total_errors = lsp_error_count,
                "Additional LSP errors suppressed"
            );
        }

        debug!(
            language = ?language,
            resolved_count = resolved_count,
            total_unresolved = unresolved.len(),
            lsp_errors = lsp_error_count,
            "LSP resolution pass complete"
        );

        // Graceful shutdown — capture exit code for the session result
        let exit_code = match client.shutdown() {
            Ok(code) => code,
            Err(e) => {
                warn!(error = %e, "LSP shutdown failed");
                None
            }
        };

        Ok(LspSessionResult {
            language,
            outcome: LspOutcome::Completed(LspCompletedSession {
                resolved_count,
                unresolved_attempted: unresolved.len(),
                error_count: lsp_error_count,
                errors: lsp_error_messages,
                server_exit_code: exit_code,
            }),
        })
    }

    /// Attempt to resolve a single reference via LSP.
    ///
    /// Returns `Ok(true)` if resolved, `Ok(false)` if not resolved (but no error),
    /// or `Err` for database errors.
    #[expect(
        clippy::too_many_lines,
        reason = "didOpen warn/trace branching adds lines but keeps logic cohesive"
    )]
    #[expect(
        clippy::too_many_arguments,
        reason = "mutable resolution state is threaded through; bundling into a struct is a future cleanup"
    )]
    fn resolve_single_ref_via_lsp(
        &self,
        client: &mut lsp::LspClient,
        unresolved_ref: &UnresolvedRefForLsp,
        lsp_error_count: &mut usize,
        lsp_error_messages: &mut Vec<String>,
        opened_files: &mut HashSet<PathBuf>,
        language_id: &str,
        did_open_warned: &mut bool,
    ) -> Result<bool> {
        // Construct absolute file path for LSP
        let file_path = self.workspace_root.join(&unresolved_ref.file_path);

        // Ensure the file is opened in the LSP server (required by some servers like csharp-ls)
        if !opened_files.contains(&file_path) {
            // Read file content
            let content = match std::fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(e) => {
                    if *did_open_warned {
                        trace!(
                            file = %file_path.display(),
                            error = %e,
                            "Failed to read file for LSP didOpen"
                        );
                    } else {
                        warn!(
                            file = %file_path.display(),
                            error = %e,
                            "Failed to read file for LSP didOpen"
                        );
                        *did_open_warned = true;
                    }
                    return Ok(false);
                }
            };

            // Send didOpen notification
            if let Err(e) = client.did_open(&file_path, &content, language_id) {
                if *did_open_warned {
                    trace!(
                        file = %file_path.display(),
                        error = %e,
                        "Failed to send didOpen notification"
                    );
                } else {
                    warn!(
                        file = %file_path.display(),
                        error = %e,
                        "Failed to send didOpen notification"
                    );
                    *did_open_warned = true;
                }
                // Continue anyway - some servers might work without it
            }
            opened_files.insert(file_path.clone());
        }

        // LSP uses 0-indexed positions, our DB uses 1-indexed
        let lsp_line = unresolved_ref.line.saturating_sub(1);
        let lsp_col = unresolved_ref.column.saturating_sub(1);

        // Call goto_definition - we wait once after initialization for solution loading,
        // so no per-query retries needed here
        let definition = match client.goto_definition(&file_path, lsp_line, lsp_col) {
            Ok(Some(loc)) => loc,
            Ok(None) => {
                trace!(
                    ref_id = %unresolved_ref.ref_id,
                    ref_name = %unresolved_ref.reference_name,
                    "LSP returned no definition"
                );
                return Ok(false);
            }
            Err(e) => {
                *lsp_error_count += 1;
                // Collect first few error messages for the caller
                if *lsp_error_count <= LspCompletedSession::MAX_ERROR_MESSAGES {
                    lsp_error_messages.push(format!(
                        "goto_definition failed for '{}': {e}",
                        unresolved_ref.reference_name,
                    ));
                }
                // Log first error at warn level so users see something went wrong
                if *lsp_error_count == 1 {
                    warn!(
                        ref_id = %unresolved_ref.ref_id,
                        error = %e,
                        "LSP goto_definition failed (further errors logged at trace level)"
                    );
                } else if *lsp_error_count <= LspCompletedSession::MAX_ERROR_MESSAGES {
                    trace!(
                        ref_id = %unresolved_ref.ref_id,
                        error = %e,
                        "LSP goto_definition failed"
                    );
                }
                return Ok(false);
            }
        };

        // Extract file path from LSP URI and convert to relative path
        let Some(def_path) = Self::uri_to_path(definition.uri.as_str()) else {
            trace!(
                uri = definition.uri.as_str(),
                "Cannot parse LSP definition URI"
            );
            return Ok(false);
        };

        // Make the path relative to workspace root
        let Ok(relative_def_path) = def_path.strip_prefix(&self.workspace_root) else {
            trace!(
                def_path = %def_path.display(),
                "Definition outside workspace, skipping"
            );
            return Ok(false);
        };

        // Look up the file in our DB
        let Some(def_file_id) = self.db.get_file_id(relative_def_path)? else {
            trace!(
                def_path = %relative_def_path.display(),
                "Definition file not in index"
            );
            return Ok(false);
        };

        // LSP returns 0-indexed, convert to 1-indexed for DB lookup.
        //
        // ENCODING FENCE: only the LINE of the incoming range is used —
        // match-back is line-granular and line numbers are identical in every
        // position encoding. If match-back ever becomes column-sensitive,
        // incoming columns are in the negotiated encoding (see
        // src/lsp/encoding.rs) and need the inverse conversion to bytes.
        let def_line = definition.range.start.line + 1;

        // Find the symbol at that line
        let Some(symbol) = self.db.find_symbol_at_line(def_file_id, def_line)? else {
            trace!(
                def_path = %relative_def_path.display(),
                def_line = def_line,
                "No symbol found at definition line"
            );
            return Ok(false);
        };

        // Resolve the reference
        self.db.resolve_reference(
            unresolved_ref.ref_id.as_i64(),
            symbol.id,
            ResolutionStrategy::Lsp,
        )?;

        trace!(
            ref_id = %unresolved_ref.ref_id,
            symbol_id = %symbol.id,
            symbol_name = %symbol.name,
            "Resolved reference via LSP"
        );

        Ok(true)
    }

    /// Convert a file URI to a filesystem path.
    ///
    /// Handles `file://` URIs from LSP responses, including percent-encoded
    /// characters (e.g., `%20` for spaces).
    pub(crate) fn uri_to_path(uri: &str) -> Option<PathBuf> {
        use percent_encoding::percent_decode_str;

        // Strip file:// prefix
        let path_str = uri.strip_prefix("file://")?;

        // Decode percent-encoded characters (%20 -> space, etc.)
        let decoded = percent_decode_str(path_str).decode_utf8().ok()?;

        // On Unix, paths start with /, so we have file:///path
        // On Windows, paths start with drive letter, so we have file:///C:/path
        #[cfg(windows)]
        {
            // Remove leading / before drive letter: /C:/path -> C:/path
            let path_str = decoded.strip_prefix('/').unwrap_or(&decoded);
            Some(PathBuf::from(path_str))
        }

        #[cfg(not(windows))]
        {
            Some(PathBuf::from(decoded.as_ref()))
        }
    }

    /// Locate the declared identifier for an LSP position.
    ///
    /// Indexed symbol coordinates point at the declaration node, while
    /// `textDocument/references` must target its `name` field. Falls back to
    /// the indexed coordinates when no syntax span or source is available.
    fn lsp_symbol_name_position(file: &Path, symbol: &Symbol, language: Language) -> (u32, u32) {
        let fallback = (
            symbol.line.saturating_sub(1),
            symbol.column.saturating_sub(1),
        );
        let Some(span) = symbol.span else {
            return fallback;
        };
        let Ok(source) = std::fs::read(file) else {
            return fallback;
        };
        get_language_support(language)
            .definition_name_position(&source, span)
            .map_or(fallback, |(line, column)| {
                (line.saturating_sub(1), column.saturating_sub(1))
            })
    }

    /// Start an LSP client and wait for its provider-specific readiness signal.
    fn start_ready_caller_lsp(
        &self,
        provider: lsp::AnyProvider,
        symbol_name: &str,
        language: Language,
    ) -> Option<lsp::LspClient> {
        let mut client = match lsp::LspClient::start(&provider, &self.workspace_root) {
            Ok(client) => client,
            Err(error) => {
                warn!(
                    error = %error,
                    symbol = %symbol_name,
                    ?language,
                    install_hint = %provider.install_hint(),
                    "LSP server failed to start — returning indexed callers only \
                     (results may be incomplete). {} Or remove --lsp.",
                    provider.install_hint()
                );
                return None;
            }
        };

        let timeout = std::time::Duration::from_secs(DEFAULT_LSP_TIMEOUT_SECS);
        let ready = match client.wait_until_ready(provider.readiness_wait(), timeout) {
            Ok(ready) => ready,
            Err(error) => {
                warn!(
                    error = %error,
                    symbol = %symbol_name,
                    "LSP readiness check failed"
                );
                false
            }
        };
        if ready {
            return Some(client);
        }

        warn!(
            symbol = %symbol_name,
            timeout_secs = DEFAULT_LSP_TIMEOUT_SECS,
            "LSP server was not ready — returning indexed callers only"
        );
        if let Err(error) = client.shutdown() {
            warn!(error = %error, "LSP shutdown failed");
        }
        None
    }

    /// Get symbols that call/use the given symbol, with LSP refinement.
    ///
    /// Combines results from the tree-sitter index with references found by the
    /// language server. This catches callers that tree-sitter could not resolve
    /// during indexing (e.g., through complex type inference).
    ///
    /// # Design
    ///
    /// 1. Get callers from the index
    /// 2. Find the symbol's definition location
    /// 3. Call LSP `find_references` at that location
    /// 4. For each LSP reference, find its containing symbol
    /// 5. Merge with indexed callers, deduplicating by symbol ID
    ///
    /// # Fallback Behavior
    ///
    /// If LSP fails to start or returns errors, falls back to indexed results
    /// and logs a warning.
    pub(crate) fn get_lsp_refined_callers(
        &self,
        qualified_name: &str,
        symbol: &Symbol,
    ) -> Result<Vec<graph::CallerInfo>> {
        // Step 1: Get callers from the index
        let indexed_callers = self.db.get_callers(symbol.id, CallEdgeSelection::All)?;

        // Step 2: Get the symbol's definition file path
        let symbol_file = self
            .db
            .get_file_by_id(symbol.file_id)?
            .ok_or_else(|| Error::NotFound(format!("file id: {}", symbol.file_id)))?;
        let symbol_file_path = self.workspace_root.join(&symbol_file.path);

        // Step 3: Spawn LSP and call find_references
        // Select the appropriate LSP provider based on the symbol's file language
        let provider = lsp::AnyProvider::for_language(symbol_file.language);
        let Some(mut lsp_client) =
            self.start_ready_caller_lsp(provider, qualified_name, symbol_file.language)
        else {
            return Ok(indexed_callers);
        };

        // LSP uses 0-indexed positions, our DB uses 1-indexed
        let (lsp_line, lsp_col) =
            Self::lsp_symbol_name_position(&symbol_file_path, symbol, symbol_file.language);

        let lsp_refs = match lsp_client.find_references(&symbol_file_path, lsp_line, lsp_col) {
            Ok(refs) => refs,
            Err(e) => {
                // User explicitly requested --lsp, so log at error level
                tracing::error!(
                    error = %e,
                    symbol = %qualified_name,
                    "LSP find_references failed - returning indexed callers only"
                );
                if let Err(shutdown_err) = lsp_client.shutdown() {
                    warn!(error = %shutdown_err, "LSP shutdown failed");
                }
                return Ok(indexed_callers);
            }
        };

        // Graceful shutdown
        if let Err(e) = lsp_client.shutdown() {
            warn!(error = %e, "LSP shutdown failed");
        }

        let indexed_len = indexed_callers.len();
        debug!(
            symbol = %qualified_name,
            indexed_callers = indexed_len,
            lsp_refs = lsp_refs.len(),
            "Merging indexed and LSP caller results"
        );

        // Steps 4-5: resolve references to containing symbols and merge
        let merged = self.merge_lsp_reference_callers(indexed_callers, lsp_refs)?;

        info!(
            symbol = %qualified_name,
            indexed_callers = indexed_len,
            lsp_additional = merged.len() - indexed_len,
            "Caller merge complete"
        );
        Ok(merged)
    }

    /// Merge LSP reference locations into indexed caller findings.
    ///
    /// Resolves each location to its innermost containing indexed symbol and
    /// appends callers the index did not already report, deduplicating by
    /// caller symbol id. Locations outside the workspace or absent from the
    /// index are skipped.
    pub(crate) fn merge_lsp_reference_callers(
        &self,
        indexed_callers: Vec<graph::CallerInfo>,
        lsp_refs: Vec<lsp_types::Location>,
    ) -> Result<Vec<graph::CallerInfo>> {
        // Symbol IDs the index already reported.
        let mut known_symbol_ids: HashSet<SymbolId> = indexed_callers
            .iter()
            .map(|caller| caller.caller.symbol.id)
            .collect();
        let mut additional_callers: Vec<graph::CallerInfo> = Vec::new();

        for loc in lsp_refs {
            // Extract file path from LSP URI and convert to relative path
            let Some(ref_path) = Self::uri_to_path(loc.uri.as_str()) else {
                trace!(uri = loc.uri.as_str(), "Cannot parse LSP reference URI");
                continue;
            };

            // Make the path relative to workspace root
            let Ok(relative_ref_path) = ref_path.strip_prefix(&self.workspace_root) else {
                trace!(
                    ref_path = %ref_path.display(),
                    "Reference outside workspace, skipping"
                );
                continue;
            };

            // Look up the file in our DB
            let Some(ref_file_id) = self.db.get_file_id(relative_ref_path)? else {
                trace!(
                    ref_path = %relative_ref_path.display(),
                    "Reference file not in index"
                );
                continue;
            };

            // LSP returns 0-indexed, convert to 1-indexed for DB lookup.
            // ENCODING FENCE: line-granular match-back — see the fence note
            // in resolve_single_ref_via_lsp before adding column use here.
            let ref_line = loc.range.start.line + 1;

            // Find the symbol that contains this reference location
            let Some(containing_symbol) =
                self.db.find_symbol_containing_line(ref_file_id, ref_line)?
            else {
                trace!(
                    ref_path = %relative_ref_path.display(),
                    ref_line = ref_line,
                    "No symbol found at reference line"
                );
                continue;
            };

            // Skip if the index already reported this caller.
            if known_symbol_ids.contains(&containing_symbol.id) {
                continue;
            }

            // Add a caller found only through LSP refinement.
            known_symbol_ids.insert(containing_symbol.id);
            additional_callers.push(graph::CallerInfo {
                caller: Caller {
                    symbol: containing_symbol,
                    file: crate::db::normalize_path(relative_ref_path).into(),
                },
                reference_count: 1,
            });
        }

        Ok(indexed_callers
            .into_iter()
            .chain(additional_callers)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileId, Import};

    // ========================================================================
    // ref_binds_to_symbol_kind Tests
    // ========================================================================

    /// `MacroCall` must gate like `Call` (tethys-8ym0): binds functions and
    /// structs (tuple ctors), refuses data members; the `Macro` gate is
    /// untouched (macro-name refs still bind only macro symbols).
    #[test]
    fn macro_call_gates_like_call() {
        use crate::types::SymbolKind;
        let mc = ReferenceKind::MacroCall;
        assert!(ref_binds_to_symbol_kind(&mc, SymbolKind::Function));
        assert!(ref_binds_to_symbol_kind(&mc, SymbolKind::Struct));
        assert!(!ref_binds_to_symbol_kind(&mc, SymbolKind::StructField));
        assert!(!ref_binds_to_symbol_kind(
            &ReferenceKind::Macro,
            SymbolKind::Function
        ));
    }

    // ========================================================================
    // build_import_maps Tests
    // ========================================================================

    #[test]
    fn build_import_maps_empty_imports() {
        let (explicit, glob) = Tethys::build_import_maps(&[]);
        assert!(explicit.is_empty());
        assert!(glob.is_empty());
    }

    #[test]
    fn build_import_maps_explicit_import() {
        let imports = vec![Import {
            file_id: FileId::from(1),
            symbol_name: "HashMap".to_string(),
            source_module: "std::collections".to_string(),
            alias: None,
        }];

        let (explicit, glob) = Tethys::build_import_maps(&imports);

        assert!(glob.is_empty());
        assert_eq!(explicit.len(), 1);
        let (symbol_name, source_module) = explicit["HashMap"];
        assert_eq!(symbol_name, "HashMap");
        assert_eq!(source_module, "std::collections");
    }

    #[test]
    fn build_import_maps_glob_import() {
        let imports = vec![Import {
            file_id: FileId::from(1),
            symbol_name: "*".to_string(),
            source_module: "crate::prelude".to_string(),
            alias: None,
        }];

        let (explicit, glob) = Tethys::build_import_maps(&imports);

        assert!(explicit.is_empty());
        assert_eq!(glob, vec!["crate::prelude"]);
    }

    #[test]
    fn build_import_maps_aliased_import_uses_alias_as_key() {
        let imports = vec![Import {
            file_id: FileId::from(1),
            symbol_name: "HashMap".to_string(),
            source_module: "std::collections".to_string(),
            alias: Some("Map".to_string()),
        }];

        let (explicit, _) = Tethys::build_import_maps(&imports);

        assert!(!explicit.contains_key("HashMap"));
        assert_eq!(explicit.len(), 1);
        let (symbol_name, source_module) = explicit["Map"];
        assert_eq!(symbol_name, "HashMap");
        assert_eq!(source_module, "std::collections");
    }

    #[test]
    fn build_import_maps_mixed_explicit_and_glob() {
        let imports = vec![
            Import {
                file_id: FileId::from(1),
                symbol_name: "Index".to_string(),
                source_module: "crate::db".to_string(),
                alias: None,
            },
            Import {
                file_id: FileId::from(1),
                symbol_name: "*".to_string(),
                source_module: "crate::prelude".to_string(),
                alias: None,
            },
            Import {
                file_id: FileId::from(1),
                symbol_name: "Error".to_string(),
                source_module: "crate::error".to_string(),
                alias: None,
            },
        ];

        let (explicit, glob) = Tethys::build_import_maps(&imports);

        assert_eq!(explicit.len(), 2);
        assert!(explicit.contains_key("Index"));
        assert!(explicit.contains_key("Error"));
        assert_eq!(glob, vec!["crate::prelude"]);
    }

    #[test]
    fn build_import_maps_duplicate_name_keeps_last() {
        let imports = vec![
            Import {
                file_id: FileId::from(1),
                symbol_name: "Error".to_string(),
                source_module: "std::io".to_string(),
                alias: None,
            },
            Import {
                file_id: FileId::from(1),
                symbol_name: "Error".to_string(),
                source_module: "crate::error".to_string(),
                alias: None,
            },
        ];

        let (explicit, _) = Tethys::build_import_maps(&imports);

        assert_eq!(explicit.len(), 1);
        let (_, source_module) = explicit["Error"];
        assert_eq!(source_module, "crate::error");
    }
}

#[cfg(test)]
mod memo_tests {
    use crate::Tethys;
    use rusqlite::params;

    /// Plan slice 5 stress fixture. One file makes:
    /// - three calls to `target_fn` (unique cross-file name) — memo hits
    ///   must yield the SAME resolved target for all three;
    /// - one call to `alpha` (ambiguous: free fn + method share the name)
    ///   — must DECLINE (unresolved);
    /// - one call to `Holder::alpha` (qualified) — must resolve to the
    ///   method. A buggy memo keyed by the name's tail would collapse this
    ///   with `alpha`'s negative outcome and lose the resolution;
    /// - two calls to `no_such_thing` — negative outcome cached, both stay
    ///   unresolved.
    #[test]
    fn memo_preserves_per_name_outcomes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("toml");
        std::fs::write(root.join("src/lib.rs"), "pub mod util;\npub mod caller;\n").expect("lib");
        std::fs::write(
            root.join("src/util.rs"),
            "pub fn target_fn() {}\n\
             pub fn alpha() {}\n\
             pub struct Holder;\n\
             impl Holder {\n    pub fn alpha(&self) {}\n}\n",
        )
        .expect("util");
        std::fs::write(
            root.join("src/caller.rs"),
            "pub fn go() {\n    target_fn();\n    target_fn();\n    target_fn();\n    alpha();\n    Holder::alpha();\n    no_such_thing();\n    no_such_thing();\n}\n",
        )
        .expect("caller");

        let mut tethys = Tethys::new(root).expect("Tethys::new");
        tethys.index().expect("index");

        let conn = tethys.db.connection().expect("conn");
        let target_at = |line: i64| -> Option<String> {
            conn.query_row(
                "SELECT s.qualified_name FROM refs r
                 LEFT JOIN symbols s ON s.id = r.symbol_id
                 JOIN files f ON f.id = r.file_id
                 WHERE f.path = 'src/caller.rs' AND r.line = ?1",
                params![line],
                |row| row.get::<_, Option<String>>(0),
            )
            .expect("ref lookup")
        };

        // Lines 2-4: target_fn ×3 — all resolved, all to the same target.
        for line in [2, 3, 4] {
            assert_eq!(
                target_at(line).as_deref(),
                Some("target_fn"),
                "target_fn ref at line {line} must resolve to util's target_fn"
            );
        }
        // Line 5: bare `alpha` is ambiguous (free fn + method) — declined.
        assert_eq!(
            target_at(5),
            None,
            "ambiguous simple name must remain unresolved"
        );
        // Line 6: qualified `Holder::alpha` resolves to the method. A memo
        // keyed by the tail would have returned line 5's negative outcome.
        assert_eq!(
            target_at(6).as_deref(),
            Some("Holder::alpha"),
            "qualified ref must resolve independently of the bare name's outcome"
        );
        // Lines 7-8: unknown name — negative outcome cached, both unresolved.
        for line in [7, 8] {
            assert_eq!(
                target_at(line),
                None,
                "unknown name at line {line} must remain unresolved"
            );
        }
    }
}

/// CI-safe fences for [`Tethys::merge_lsp_reference_callers`]: the merge and
/// dedup behavior is exercised with fabricated LSP locations, so no language
/// server is required (the real-server path stays in the ignored
/// `lsp_callers` integration tests).
#[cfg(test)]
mod lsp_caller_merge_tests {
    use std::path::{Path, PathBuf};

    use crate::types::CallEdgeSelection;

    /// Fixture crate for the merge seam: `indexed_caller` produces a call
    /// edge to `target`; `lsp_only_caller` never calls it, so only a
    /// fabricated LSP reference can surface it as a caller.
    const MERGE_FIXTURE: &str = "\
pub fn target() -> bool {
    true
}

pub fn indexed_caller() -> bool {
    target() // indexed call site
}

pub fn lsp_only_caller() -> bool {
    true // lsp-only reference site
}
";

    fn caller_merge_fixture() -> (tempfile::TempDir, crate::Tethys) {
        let dir = tempfile::tempdir().expect("create temp dir");
        std::fs::create_dir_all(dir.path().join("src")).expect("create src dir");
        std::fs::write(dir.path().join("src/lib.rs"), MERGE_FIXTURE).expect("write lib.rs");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"merge_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write Cargo.toml");
        let mut tethys = crate::Tethys::new(dir.path()).expect("create Tethys");
        tethys.index().expect("index fixture");
        (dir, tethys)
    }

    /// Zero-indexed (LSP convention) line of the first fixture line
    /// containing `marker`.
    fn fixture_lsp_line(marker: &str) -> u32 {
        let line = MERGE_FIXTURE
            .lines()
            .position(|line| line.contains(marker))
            .expect("marker present in fixture");
        u32::try_from(line).expect("fixture line fits u32")
    }

    fn lsp_location_at(file: &Path, lsp_line: u32) -> lsp_types::Location {
        let uri: lsp_types::Uri = format!("file://{}", file.display())
            .parse()
            .expect("valid file URI");
        let position = lsp_types::Position::new(lsp_line, 4);
        lsp_types::Location {
            uri,
            range: lsp_types::Range::new(position, position),
        }
    }

    /// CI-safe fence for the LSP caller merge: no language server involved.
    /// A reference overlapping an indexed caller must not duplicate it, and
    /// a reference inside a symbol the index reported no edge for must be
    /// added as a caller with the indexed file attached.
    #[test]
    fn merge_lsp_reference_callers_dedups_overlap_and_adds_novel_caller() {
        let (dir, tethys) = caller_merge_fixture();
        let workspace = dir.path().canonicalize().expect("canonical workspace");
        let lib_rs = workspace.join("src/lib.rs");

        let symbol = tethys
            .db
            .get_symbol_by_qualified_name("target")
            .expect("symbol query")
            .expect("target symbol indexed");
        let indexed = tethys
            .db
            .get_callers(symbol.id, CallEdgeSelection::All)
            .expect("indexed callers");
        let indexed_names: Vec<_> = indexed
            .iter()
            .map(|info| info.caller.symbol.name.clone())
            .collect();
        assert_eq!(
            indexed_names,
            ["indexed_caller"],
            "fixture precondition: the index reports exactly the direct caller"
        );

        let overlap = lsp_location_at(&lib_rs, fixture_lsp_line("indexed call site"));
        let novel = lsp_location_at(&lib_rs, fixture_lsp_line("lsp-only reference site"));

        let merged = tethys
            .merge_lsp_reference_callers(indexed, vec![overlap, novel])
            .expect("merge succeeds");

        let mut merged_names: Vec<_> = merged
            .iter()
            .map(|info| info.caller.symbol.name.clone())
            .collect();
        merged_names.sort_unstable();
        assert_eq!(
            merged_names,
            ["indexed_caller", "lsp_only_caller"],
            "overlapping reference deduplicated, novel caller added exactly once"
        );

        let lsp_only = merged
            .iter()
            .find(|info| info.caller.symbol.name == "lsp_only_caller")
            .expect("lsp-only caller present");
        assert_eq!(lsp_only.reference_count, 1);
        assert_eq!(lsp_only.caller.file, PathBuf::from("src/lib.rs"));
    }

    /// References the merge cannot attribute — outside the workspace or on a
    /// line no indexed symbol contains — are skipped, not errors.
    #[test]
    fn merge_lsp_reference_callers_skips_unattributable_references() {
        let (dir, tethys) = caller_merge_fixture();
        let workspace = dir.path().canonicalize().expect("canonical workspace");

        let outside = lsp_location_at(Path::new("/definitely/not/indexed.rs"), 0);
        // Blank separator line between fixture functions: in-workspace but
        // contained by no symbol span.
        let uncontained = lsp_location_at(&workspace.join("src/lib.rs"), 3);

        let merged = tethys
            .merge_lsp_reference_callers(Vec::new(), vec![outside, uncontained])
            .expect("merge succeeds");
        assert!(
            merged.is_empty(),
            "unattributable references must be skipped: {merged:?}"
        );
    }
}
