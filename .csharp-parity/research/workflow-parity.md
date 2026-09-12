# Workflow parity inventory — Rust vs C# at the CLI and capability level

Research artifact for ticket `tethys-oxds` ("Inventory approval-time Rust/C# workflow parity").
Branch: `research/csharp-parity-workflow` · Artifact: `.csharp-parity/research/workflow-parity.md` · Captured: 2026-08-09.

## 1. Question / scope

**Question:** For every current CLI command and the public Tethys capability it exercises, what is the observable Rust-vs-C# behavior today — equivalent, degraded, unknown/unproved, Rust-specific/inapplicable candidate, or C#-specific — and which discrepancies are already tracked by title + id in the rivets tracker?

**Scope:** All 16 subcommands of the `tethys` CLI (`src/main.rs` `Commands` enum), their flags, the facade methods they exercise (`src/lib.rs`, `src/indexing.rs`, `src/reindex.rs`), and the cross-cutting public capabilities (LSP refinement, reindex/staleness, import storage, test detection, parent linkage, JSON shapes/exit codes, qualified-name display). Evidence is source, tests, docs, and the local tracker (`.rivets/issues.jsonl`, 183 issues).

**Non-goals (per ticket):** choosing what parity *should* mean; designing fixes; proposing C# branches in indexing/resolution drivers (the language-neutral seam is preserved — see §6); filing new issues; touching anything but this file.

**Sibling artifacts (separate branches):** `.csharp-parity/research/msbuild-discovery.md` (MsbuildResearch) and `.csharp-parity/research/csharp-ls-cli.md` (LspResearch). This document treats `.csproj`/`.sln` discovery and the LSP server lifecycle as *inputs* with pointers, not as its own findings.

## 2. Baseline and method

- **Candidate baseline commit:** `7ca48f881a7ac0ecf669b6c9319ace40c5a0c07b` — `chore(rivets): close tethys-7a6a` (2026-08-09). This worktree's branch `research/csharp-parity-workflow` is created at this commit; it equals `main` HEAD at capture time. All findings describe this tree.
- **Method:**
  1. Enumerate the CLI surface from `src/main.rs` (`Commands` enum → 16 subcommands) and the command modules in `src/cli/`.
  2. Map each command to its facade entry point(s) (`Tethys::*`, `src/lib.rs`; `index_with_options`/`rebuild_with_options` at `src/indexing.rs:164`, `src/reindex.rs:296`).
  3. Locate every language-dependent decision: greps for `Language::`, `f.language`, `'rust'`/`'csharp'` literals across `src/`, then read the matched sites (verified: `src/db/visibility.rs:148-149`, `src/db/dead_code.rs:140-144,190-191`, `src/db/panic_points.rs:24-32`, `src/unused_imports.rs:89-102`, `src/cli/mod.rs:76-104`, `src/indexing.rs:466-496`, `src/resolve.rs:1206`, `src/cli/stats.rs:65-75`, `src/cli/search.rs:114-119`).
  4. Cross-check C# behavior against the fence tests (`tests/seam_lint.rs`, `tests/csharp_*.rs`, `tests/mixed_language_dispatch.rs`, and C# fixtures inside `tests/deprecated_callers.rs`, `tests/dead_code.rs`, `tests/untested_code.rs`, `tests/type_hierarchy.rs`, `tests/test_topology.rs`, `tests/attributes.rs`, `tests/member_reads.rs`, `tests/parent_symbols.rs`, `tests/idxperf_golden.rs`).
  5. Map every discrepancy to the tracker by id + title (statuses as stored in `.rivets/issues.jsonl` at capture time); flag unfiled gaps without filing.
- **Evidence conventions:** `[E]` = observed in source/tests/docs at the cited path:symbol; `[I]` = inference from code shape, not exercised; tracker statuses are snapshot data, not live.
- **Refresh protocol (so the inventory is refreshable at final roadmap approval):**
  1. Re-anchor the baseline: `git rev-parse HEAD` on `main`; record it at the top of this file's §2 and demote the previous snapshot to history (do not edit old rows).
  2. Re-enumerate: count `#[derive(Subcommand)] enum Commands` variants in `src/main.rs` (16 at baseline). Any new command gets a new matrix row.
  3. Re-check language filters: re-run the greps from step 3 above (any new `f.language`/`Language::` match in an analysis or CLI file changes that row's classification).
  4. Re-check tracker statuses: `jq -r '[.id,.status,.title]|@tsv' .rivets/issues.jsonl` for every issue cited in §7; close/defer rows whose issues changed state; re-derive the unfiled-gap list (a gap becomes filed when a new issue title matches it).
  5. Re-scan docs for staleness (§9 is a point-in-time list; re-check `README.md` and `.agents/summary/review_notes.md`).
  6. Recompute classifications and root-cause groups from the re-checked facts; keep the prior snapshot only as history. No fix design, no issue filing, no code changes.

## 3. Classification legend

| Class | Meaning |
|---|---|
| **Equivalent** | Same observable behavior for Rust and C#; verified by shared mechanism and/or a C# fence test. |
| **Degraded** | C# path exists and runs but observably under-delivers (fewer results, wrong metadata, wrong shape, or a gate that blocks it). |
| **Unknown / unproved** | Mechanism is language-neutral or wired, but no C# fixture/test exercises the path; behavior not observed. |
| **Rust-specific / inapplicable candidate** | Analysis or capability is deliberately or structurally Rust-only; a C# analog is either ticketed or unfiled. |
| **C#-specific** | A C#-only behavior/facet (no Rust analog by definition). |

## 4. Command × capability matrix

| # | Command (flags) | Facade exercised | Rust behavior | C# behavior | Class |
|---|---|---|---|---|---|
| 1 | `index` (`--rebuild`, `--lsp`, `--lsp-timeout`) | `index_with_options` (`src/indexing.rs:164`), `rebuild_with_options` (`src/reindex.rs:296`), `remove_index_files` (`src/lib.rs:207`) | Full pipeline: discovery → tree-sitter extraction → Pass-1 same-file → Pass-2 cross-file → optional Pass-3 LSP → call edges/file deps; Cargo crate metadata. | Same neutral pipeline for `.cs` (`Language::from_extension`, `src/types.rs:138-142`); Pass-3 runs `csharp-ls` via `AnyProvider::for_language(CSharp)` (`src/indexing.rs:481-496`); server-missing degrades to `LspOutcome::ServerUnavailable` with install hint (`src/resolve.rs:730-742`). **Facets:** no compilation-unit discovery → `module_path` always empty, `SameCrate` arm unreachable, `orphan:<topdir>` pseudo-crate bucketing (`src/db/call_edges.rs:231-234`); architecture phase yields zero stats for no-crate workspaces (`src/indexing.rs` `run_architecture_phase`); smaller ref vocabulary; symbol metadata gaps. | **Degraded** (pipeline equivalent-by-construction; extraction/metadata facets are open C#-specific gaps). `--lsp` facet: **degraded** — CLI preflight `check_lsp_availability` is `RustAnalyzerProvider`-only (`src/cli/mod.rs:76-104`), so on a csharp-ls-only machine the command fails before the (wired) C# path runs. |
| 2 | `search` (`--kind`, `--limit`) | `search_symbols` (`src/lib.rs:310`) | Name/partial match over all symbols; kind filter. | Same query; `parse_kind` accepts C# kinds `property`/`event`/`delegate`/`struct_field` plus `class`/`interface` (`src/cli/search.rs:114-119`); C# records are stored as `Class` so `--kind class` finds them. **Facet:** clap help text advertises "(function, method, struct, class, enum, trait, interface)" (`src/main.rs:48-51`) — omits the C#-relevant kinds and mixes Rust-only `trait` with C#-only `class`/`interface`. | **Equivalent** (help-text staleness, unfiled minor). |
| 3 | `callers` (`--transitive`, `--depth`, `--lsp`, `--exclude-speculative`) | `get_callers` (`src/lib.rs:440`), `get_symbol_impact`-style transitive walk, LSP refinement `get_lsp_refined_callers` (`src/resolve.rs:1206`) | Indexed callers from refs; `--lsp` refines via rust-analyzer. | Same indexed path for C# symbols; `--lsp` refinement dispatches on `symbol_file.language` → `CSharpLsProvider` (`src/resolve.rs:1206-1209`), falls back to indexed callers with warning if the server won't start (`src/resolve.rs:1134-1141`). **Facets:** CLI `--lsp` gate is rust-analyzer-only (see row 1); C# method calls resolve name-only (receiver-typing is Rust-grammar, tethys-53iv) so same-name collisions are a live hazard (tethys-0aqj). | **Equivalent** (indexed path; C# LSP refinement wired but CLI-gated and untested — see §5 row L1). |
| 4 | `impact` (`--symbol`, `--depth`, `--lsp`) | `get_impact` (`src/lib.rs:425`), `get_symbol_impact` (`src/lib.rs:472`) | File/symbol impact over file deps and refs. | Same neutral traversal over C# file deps (L2 used-only semantics) and refs. `--lsp` same gate + per-language dispatch as `callers`. | **Equivalent** (same `--lsp` caveats as row 3). |
| 5 | `coupling` (`--sort`, `--package`, `--json`) | `get_packages` (`src/lib.rs:1117`), `get_coupling_metrics` (`src/lib.rs:1125`), `get_package_coupling` (`src/lib.rs:1138`) | Packages = Cargo crates (`PackageSource::Manifest`); Ca/Ce/instability over `file_deps`. | Pure-C# workspace: zero packages, empty output / `--package` prints `null` + non-zero exit. Mixed workspace: C# files inside a Cargo crate directory are attributed to the containing crate by path prefix `[I]`; `orphan:` pseudo-crates are *not* packages (`src/db/call_edges.rs` K-hybrid bucket only). | **Rust-specific / inapplicable candidate** (structurally crate-based; root cause G1; tracked: tethys-byie open, tethys-41lq depends on assembly attribution; unfiled `.csproj` discovery). |
| 6 | `cycles` | `detect_cycles` (`src/lib.rs:503`) | Cycle detection over `file_deps`. | Same query over C# file deps (which are produced by the corroborated L2 path — `tests/csharp_l2_file_deps.rs`, `tests/csharp_cross_dir_deps.rs`). Rust-side hazard tethys-qqbi (stack overflow) is language-neutral. | **Equivalent** (by construction; C# dep shapes fenced). |
| 7 | `stats` | `get_stats` (`src/lib.rs:683`) | Per-language file counts rendered with "Rust"/"C#" labels (`src/cli/stats.rs:65-75`). | Same; C# files counted under `C#`. | **Equivalent**. |
| 8 | `reachable` (`--direction`, `--max-depth`) | `get_reachable`/`get_forward_reachable`/`get_backward_reachable` (`src/lib.rs:536-627`) | Ref-based traversal, unified behind the seam (tethys-7a6a closed). | Same traversal over C# refs. **Facet:** no dedicated C# fixture fences this command; C# ref vocabulary is smaller (row 1) so reachable sets are inherently smaller for C#. | **Equivalent** (mechanism); C# e2e coverage **unknown/unproved** (no `.cs` fixture found in reachable tests). |
| 9 | `affected-tests` (`--names-only`; exit 0/2/1) | `get_affected_tests` (`src/lib.rs:727`), `get_affected_tests_with_standing` (`src/lib.rs:757`) | Standing-aware: exit 0 confirmed / 2 indeterminate / 1 error (tethys-09wx closed; contract in `src/cli/affected_tests.rs:1-21`). | Same exit-code contract (language-neutral). C# test roots are detected (`[Test]`/`[Fact]`/`[Theory]`/`[TestMethod]` — `tests/test_topology.rs` `mod csharp_test_detection`). **Facet:** `tests/affected_tests_cli.rs` contains no `.cs` fixture — the CLI-level C# run is never exercised. | **Equivalent** (mechanism + standing); C# CLI e2e **unknown/unproved**. |
| 10 | `panic-points` (`--include-tests`, `--json`, `--file`) | `get_panic_points` (`src/lib.rs:904`), `count_panic_points` (`src/lib.rs:1106`) | Lowercase `unwrap`/`expect` (+ `*::`-qualified) call refs inside callable symbols (`src/db/panic_points.rs:24-32`). | No language filter — a C# method named `unwrap()` (lowercase) *would* be reported `[I]`; idiomatic C# (`Unwrap`, `.First()`, `throw`) never matches, so in practice C# workspaces report nothing. Unfenced either way (no C# panic-point test). | **Rust-specific / inapplicable candidate** (by naming convention, unfenced; C# analog unfiled — flag only). Rust-side under-report tethys-9l27. |
| 11 | `deprecated-callers` (`--json`) | `get_deprecated_symbols` (`src/lib.rs:931`), `get_deprecated_callers` (`src/lib.rs:953`) | `#[deprecated]` (since/note parsed) + tiered sites (tethys-jdly). | `[Obsolete]` with all four spellings + error flag, identical JSON key set across languages (`src/db/deprecated.rs:18-45,119-120`; tethys-haw5 closed); deep C# fences in `tests/deprecated_callers.rs`. **Facet:** obsolete *constructors* read Clean because construct refs bind the type symbol (tethys-9181, C#-sharper); member-read shapes beyond plain access invisible (tethys-5uqz). | **Equivalent** (degraded facets tracked: 9181, 5uqz). |
| 12 | `visibility-tightening` (`--json`, `--workspace-closed`) | `get_visibility_candidates` (`src/lib.rs:984`) | SQL `WHERE s.visibility='public' AND f.language='rust'` (`src/db/visibility.rs:148-151`); help text "List pub Rust items…" (`src/main.rs`). | No C# candidates by construction. | **Rust-specific** (fenced); C# parity open: **tethys-41lq**. Related Rust facets: tethys-w1e9, tethys-aitb. |
| 13 | `unused-imports` (`--json`, `--all`) | `find_unused_imports` (`src/unused_imports.rs:91`) | Rust files only, by extension filter (`src/unused_imports.rs:95-103`). | C# `using` directives are namespace globs and skipped by design, documented at `src/unused_imports.rs:89-90`. | **Rust-specific** (by design, documented); a C# unused-using analog is **unfiled** (flag only). Rust parser facets tracked: tethys-7035, tethys-rylk, tethys-xzdr, tethys-msn0, tethys-m7zm. |
| 14 | `untested-code` (`--json`) | `get_untested_code` (`src/lib.rs:1015`) | Product symbols unreachable from any test root (tethys-y3bx closed). | C# `[Fact]` roots fence: `csharp_fact_roots_cover_their_callees` (`tests/untested_code.rs:193-210`). | **Equivalent**. |
| 15 | `dead-code` (`--limit`, `--json`) | `find_dead_code` (`src/lib.rs:1052`) | Bilingual funnel (`RUST_CANDIDATE_KINDS_SQL`), `main` entry suppression (`src/db/dead_code.rs:140-144,190-191`; tethys-dvsw closed). | `CSHARP_CANDIDATE_KINDS_SQL` (incl. property/event/struct_field via `field_access` channel), `Main` entry suppression; C# fence `tests/dead_code.rs` `csharp_funnel`. **Facet:** interface-member suppression is type-granular only — methods implementing interfaces can read dead (tethys-3b06 open). | **Equivalent** (degraded facet: 3b06). |
| 16 | `hierarchy` (`--direction`, `--json`) | `get_type_hierarchy` (`src/lib.rs:1079`) | Rust trait impl + supertrait edges (tethys-j2r1 closed). | C# base-list inherit edges both directions incl. nested classes: `csharp_base_lists_walk_both_directions` (`tests/type_hierarchy.rs`); records map to Class so they participate via class edges. | **Equivalent**. |

## 5. Public capabilities beyond individual commands

| Id | Capability | Evidence | Class |
|---|---|---|---|
| C1 | File discovery / language dispatch | `Language::from_extension` rs/cs (`src/types.rs:138-142`); `discover_files` walk with `is_excluded_dir` (`src/indexing.rs:1215-1255`): `obj/` always excluded, `bin/` only under `src/` (Rust convention — a .NET `bin/` directly under `src/` would be walked `[I]`). | Equivalent by construction (shared walk). |
| C2 | Compilation-unit / project metadata | `cargo.rs` `discover_crates` → `CrateInfo`; `compute_module_path_for_file` returns `''` outside crates (`src/lib.rs:225-244`); no `.csproj`/`.sln` discovery anywhere; comment `src/db/call_edges.rs:231-234` ("no `.csproj` discovery… mainly future-proofing"). | **C#-specific gap — root cause G1, unfiled.** |
| C3 | Symbol/reference/import extraction | Rust: 10+ symbol kinds, 10 ref kinds (`src/languages/rust.rs`). C#: namespace/class/struct/interface/enum/record→Class/delegate/property/event/field-per-declarator/method/ctor; ref kinds Call/Constructor/FieldAccess/Inherit only (`src/languages/csharp.rs`); attributes incl. `[Obsolete]` (`extract_attributes`); usings stored as `'*'` glob rows (`src/db/files.rs:507-510`). | Degraded for C# (vocabulary + metadata); tracked: tethys-cfme, tethys-itez, tethys-5uqz; **unfiled: C# type-reference extraction (annotations/type uses produce no refs)**; unfiled minor: records→Class (no Record kind). |
| C4 | Parent linkage | Same-file container linkage, kind-gated (`src/db/files.rs:299-335`; tethys-aay4 closed); C# nested classes linked innermost (`tests/parent_symbols.rs:163-190`). | Equivalent. |
| C5 | Module-path resolution seam | `ModuleResolver` trait; `CSharpModuleResolver` (dot separator, namespace map, UniqueAcrossAll types-only + using-static arm) vs `RustModuleResolver` (crate/self/super) (`src/languages/module_resolver.rs:215-218,341+`); seam enforced by `tests/seam_lint.rs` C4/C5/C10 + import-join + DB-free fences. | Equivalent by construction (C# deliberately one-to-many; tracked: tethys-nnst, tethys-alus, tethys-glus, tethys-nvcy). |
| C6 | Call-edge eligibility & file deps | Kind-exclusion filter shared (`src/db/call_edges.rs:107-133`); K-hybrid cross-bucket requires usings∩namespaces corroboration for C# (`src/db/call_edges.rs:144-292`); fences `tests/csharp_l2_file_deps.rs`, `tests/csharp_cross_dir_deps.rs`, `tests/mixed_language_dispatch.rs`. | Equivalent (C# deliberately more conservative — tethys-haw5 posture; tethys-nmsp closed). |
| C7 | Reindex / staleness | `update()` = full re-index both languages (`src/reindex.rs:39-42`); mtime+size `needs_update` (`src/reindex.rs:65`); `purge_orphan_files` (tethys-dhxo closed); C# attribute rows survive reindex (`tests/attributes.rs:263-418`). | Equivalent; polish tracked: tethys-gkt2 (in_progress), tethys-3l14. |
| L1 | LSP transport, readiness, refinement | Language-neutral transport; `RustAnalyzerProvider` (Quiescence) + `CSharpLsProvider` (SolutionLoad) (`src/lsp/provider.rs:118,187-190`); Pass-3 per language (`src/indexing.rs:466-496`); callers refinement per symbol language (`src/resolve.rs:1206`); UTF-16 negotiation (tethys-2d1x closed), solution-load wait (tethys-2mjj closed). **Facets:** CLI `--lsp` preflight checks rust-analyzer only (`src/cli/mod.rs:76-104`); no test exercises csharp-ls e2e (`tests/lsp_resolution.rs`/`lsp_callers.rs` contain no `.cs` fixtures); nightly lsp-smoke installs rust-analyzer only (tethys-xpc4). | Library wiring **equivalent by construction / C# unproved**; CLI gate **degraded (unfiled residual of tethys-6x7g)**; open: tethys-24tj (no read deadline — a silent csharp-ls stalls Pass-3), tethys-k543, tethys-ot0z. |
| L2 | Test detection | `is_test` for both; C# `[Test]`/`[Fact]`/`[Theory]`/`[TestMethod]` (`tests/test_topology.rs`; tethys-o6x7 closed). | Equivalent. |
| L3 | JSON shapes / exit codes / stdout discipline | Shared report writers; stderr diagnostics (tethys-sspl closed); `deprecated-callers` JSON key set identical across languages (`src/db/deprecated.rs:18-45`); affected-tests exit contract (row 9). | Equivalent. |
| L4 | Qualified-name display | Storage canonical `::` for both; C# consumers see `AuthService::Login` (`src/resolve.rs` QualifiedExact canonical names). | C#-specific cosmetic gap: **tethys-dsp1** open. |

## 6. Shared-by-construction vs C#-specific regression fences

**Shared by construction** (language-neutral drivers, no C#-specific test needed for the mechanism): file discovery, Pass-1/2/3 drivers, same-file binding, call-edge eligibility, reindex, search/stats queries, reachable/cycles/impact traversals, affected-tests standing, exit-code and JSON contracts. A change to these paths affects both languages identically.

**C#-specific regression fences** (tests that pin C# behavior and would fail on a C# regression):

| Fence | What it pins |
|---|---|
| `tests/seam_lint.rs` C4/C5/C10 | `src/resolve.rs`/`src/indexing.rs` stay free of Rust module semantics; C# namespace post-pass stays deleted; resolvers DB-free; import joins through the seam. |
| `tests/csharp_l2_file_deps.rs` | C# file deps are L2 used-only (no per-using post-pass). |
| `tests/csharp_cross_dir_deps.rs` | K-hybrid cross-bucket corroboration; uncorroborated C# edges dropped. |
| `tests/csharp_using_static.rs` | `using static` static-member arm (tethys-usgf). |
| `tests/csharp_using_disambiguation.rs` | Types-only UniqueAcrossAll using arm; collision resolution. |
| `tests/mixed_language_dispatch.rs` | C# imports never route through Rust crate routing (adversarial crate named `System`). |
| C# fixtures in `tests/deprecated_callers.rs`, `tests/dead_code.rs`, `tests/untested_code.rs`, `tests/type_hierarchy.rs`, `tests/test_topology.rs`, `tests/attributes.rs`, `tests/member_reads.rs`, `tests/parent_symbols.rs`, `tests/idxperf_golden.rs` | Bilingual behavior of each analysis and the canonical mixed-language DB dump. |

## 7. Discrepancy → issue mapping

**Open tracked issues (id — title — status as of 2026-08-09):**

| Discrepancy | Issue |
|---|---|
| C# visibility parity (analysis is Rust-fenced) | tethys-41lq — "Visibility tightening: C# parity (public types/members that could be internal)" (open, P4) |
| C# dead-code interface-method suppression missing | tethys-3b06 — "C# method-level interface-member suppression requires override resolution" (open, P4) |
| C# member-read shapes invisible (implicit-this, `?.`, indexers, object initializers) | tethys-5uqz — "C# member reads: shapes beyond plain member_access_expression are invisible…" (open, P4) |
| Obsolete/deprecated constructors read Clean | tethys-9181 — "deprecated-callers: obsolete/deprecated CONSTRUCTORS read Clean while callers exist" (open, P3; C#-sharper facet) |
| C# alias usings unresolved | tethys-alus — "C# alias using resolution (using Alias = Namespace.Type)" (open, P4) |
| C# global usings not propagated | tethys-glus — "C# global using propagation (C# 10 global using)" (open, P4) |
| C# nested namespace blocks don't resolve | tethys-nnst — "Nested C# namespace blocks don't participate in namespace resolution" (open, P4) |
| `using static` trailing-dot fence | tethys-nvcy — "C# using static: verify parser emits trailing-dot suffix…" (open, P4) |
| C# const/static-field/enum-member symbols missing | tethys-cfme — "C# extractor doesn't index const / static-field / enum-member symbols (or their refs)" (open, P3) |
| C# unsafe/generics metadata hardcoded | tethys-itez — "C# parser: detect unsafe modifier and extract generics" (open, P4; absorbed tethys-778r, tethys-z45p) |
| C# qualified names displayed with `::` | tethys-dsp1 — "Per-language display spelling for qualified names at query/output boundary" (open, P4) |
| Kind-blind symbol binding (C# property/method collisions) | tethys-0aqj — "Symbol binding is kind-blind…" (open, P4) |
| Duplicate qualified names first-match bind | tethys-bvgb — "qualified_exact binds first match when duplicate qualified names exist across crates" (open, P4; `User::new()` C# class facet) |
| LSP transport read has no deadline (silent csharp-ls stalls Pass-3) | tethys-24tj — "lsp: transport reads have no deadline…" (open, P3) |
| Pass-3 only re-verifies unresolved refs | tethys-k543 — "Pass 3 LSP re-verifies speculative-band resolutions, not just unresolved refs" (open, P3) |
| LSP integration tests don't assert an LSP-contributed bind (no C# LSP e2e) | tethys-ot0z — "test(lsp): make ignored LSP integration tests assert an LSP-contributed bind" (open, P3) |
| Nightly LSP smoke is rust-analyzer-only | tethys-xpc4 — "nightly workflow: LSP ignored tests + criterion bench run-and-record" (open, P3) |
| No per-language corpus fixtures (C# corpus thin) | tethys-m1rz — "consistency fences: strategy-to-band exhaustiveness + per-language corpus fixtures" (open, P3) |
| Architecture/coupling package detection (crate-scoped) | tethys-byie — "Architecture analysis: package detection and coupling metrics (Ca/Ce/instability)" (open, P2) |
| cycles stack overflow (both languages) | tethys-qqbi — "cycles: unbounded recursion in detect_cycles aborts the process…" (open, P3) |
| Staleness/content-hash polish (both languages) | tethys-gkt2 — "Implement proper staleness check" (in_progress, P2); tethys-3l14 — "Compute content hash for indexed files" (open, P3) |
| Rust-side refs inside macro args invisible (hits panic-points, deprecated-callers) | tethys-9l27 — "Rust refs inside macro invocations are invisible…" (open, P3) |
| Visibility-tightening Rust facets | tethys-w1e9 — "Visibility tightening: member- and module-level items…" (open, P4); tethys-aitb — "visibility-tightening channel-(c) fence is vacuous…" (open, P3) |
| unused-imports Rust parser facets | tethys-7035 (wildcard groups, open), tethys-rylk (alias members, open), tethys-xzdr (crate-root `use`, open), tethys-msn0 (phantom file_dep, open), tethys-m7zm (test-code handling, open) |

**Closed issues that established current C# behavior:** tethys-6x7g (Add CSharpLsProvider), tethys-jwf9 (C# namespace resolution for using statements), tethys-nmsp (namespace-map mechanism unified with seam; post-pass deleted + fenced), tethys-usgf (using-form coverage: using static shipped; alus/glus/cfme residuals carved out), tethys-haw5 (C# parity for deprecated-callers: `[Obsolete]` + identical JSON), tethys-xebx (C# member declarations), tethys-j2r1 (type hierarchy incl. C# base lists), tethys-dvsw (dead-code finder, bilingual), tethys-y3bx (untested-code, C# roots), tethys-xov3 (C# nested types), tethys-itz7/tethys-lxbg (imports storage Rust + C#), tethys-o6x7 (test topology), tethys-ugg6 (cross-file integration tests), tethys-aay4 (parent_symbol_id), tethys-9z7i (resolution provenance/bands), tethys-2d1x (UTF-16), tethys-2mjj (solution-load readiness), tethys-7a6a (reachable seam unification), tethys-09wx (affected-tests standing), tethys-sspl (stderr logs).

**Truly unfiled gaps (flagged; deliberately NOT filed by this ticket):**
1. **C# compilation-unit discovery (`.csproj`/`.sln`/assembly)** — the root asymmetry; documented only in source (`src/db/call_edges.rs:231-234`; `src/lib.rs:225-244`). Blocks tethys-41lq, correct C# coupling (tethys-byie), multi-project C# workspaces, csharp-ls solution-load semantics.
2. **CLI `--lsp` preflight gate is rust-analyzer-only** (`src/cli/mod.rs:76-104`) — residual of closed tethys-6x7g; on a csharp-ls-only machine every `--lsp` CLI command fails before the wired C# engine path.
3. **C# affected-tests CLI e2e** — no `.cs` fixture in `tests/affected_tests_cli.rs` (mechanism shared, standing contract language-neutral).
4. **panic-points C# behavior** — unfenced lowercase-name predicate would report a C# `unwrap()` method (`src/db/panic_points.rs:24-32`); no C# test either way.
5. **C# type-reference extraction** — C# annotations/type uses emit no `Type` refs, so type-use sites are invisible to ref-based analyses (distinct from 5uqz, which is member reads).
6. **Minor:** `search --kind` help text omits C# kinds (`src/main.rs:48-51`); records map to `Class` (no Record kind); C# unused-using detection (no analog to row 13).

## 8. Root-cause groups (for scorecard grouping — not one patch per symptom)

- **G1 — No C# compilation-unit discovery (the root).** Spawns: empty `module_path`, unreachable `SameCrate`, `orphan:` pseudo-crate bucketing, zero architecture stats for pure-C#, `coupling` inapplicable, and tethys-41lq blocked (assembly attribution + `InternalsVisibleTo` awareness). (tethys-3b06 — method-level interface-member/override matching — is a separate gap in G2, not an assembly-attribution dependency.) Any fix must enter via the `ModuleResolver` seam / `ModuleContext` (`tests/seam_lint.rs` C4/C5/C10; precedent tethys-nmsp).
- **G2 — C# extractor vocabulary & metadata.** Smaller ref-kind vocabulary (no Type/Value/Macro/Reexport), tethys-cfme (enum members/const), tethys-itez (unsafe/generics), records→Class, tethys-5uqz (read shapes), tethys-3b06 (no impl blocks → method↔interface-member association unknown, so dead-code suppression is type-granular only). Degrades `search`, `hierarchy`, `deprecated-callers`, `impact`/`callers` precision for C#.
- **G3 — C# using-form resolution ladder.** tethys-alus, tethys-glus, tethys-nnst, tethys-nvcy; glob-only import model blocks any C# unused-using analog. Deliberately conservative posture (tethys-haw5 decision).
- **G4 — Analyses deliberately Rust-scoped.** `unused-imports` (documented by design), `visibility-tightening` (fenced; tethys-41lq), `panic-points` (unfenced convention). These are the only three CLI analyses with a language filter.
- **G5 — Presentation / UX.** tethys-dsp1 (`::` display), `search --kind` help text.
- **G6 — Cross-cutting correctness infra, C#-load-bearing.** tethys-0aqj (kind-blind binding), tethys-bvgb (duplicate qualified names), tethys-24tj (LSP read deadline), tethys-k543 (Pass-3 scope), tethys-ot0z + tethys-xpc4 (LSP e2e coverage), tethys-m1rz (corpus fixtures), tethys-qqbi (cycles depth), tethys-gkt2/tethys-3l14 (staleness).
- **G7 — Docs debt.** README and `.agents/summary/review_notes.md` (below).

## 9. Known stale docs

- **`README.md`** — Commands table lists 12 of the 16 subcommands (missing `dead-code`, `hierarchy`, `untested-code`, `visibility-tightening`); `deprecated-callers` row says "(Rust; C# `[Obsolete]` pending)" — false since tethys-haw5 shipped; LSP section framed rust-analyzer-only.
- **`.agents/summary/review_notes.md`** — claims attribute extraction is Rust-only ("C# symbol attributes are not stored") — false (`src/languages/csharp.rs` `extract_attributes`; `tests/attributes.rs`; `tests/deprecated_callers.rs`); claims C# LSP refinement "not confirmed wired" — false (`src/indexing.rs:481-496`, `src/resolve.rs:1206`); lists "CLI commands (10)" — 16 exist.
- **`.tethys-zk2q/plan.md`** (historical epic dir) — Phase-2 "remaining tasks" list is stale; every child closed. Historical artifact, not shipping docs.

## 10. Uncertainty

- **C# LSP end-to-end is wired but never exercised**: no CI installs csharp-ls (tethys-xpc4), `tests/lsp_resolution.rs`/`lsp_callers.rs` contain no `.cs` fixtures, and tethys-6x7g closed without closure evidence. All `--lsp` C# claims above are "code-verified, behavior-unproven"; the CLI preflight gate is source-verified today.
- **C# affected-tests CLI e2e** unproven (no `.cs` fixture); mechanism and exit contract are language-neutral and standing is fenced by tethys-09wx tests (Rust fixtures).
- **`coupling` on mixed workspaces** — C#-inside-Cargo-crate attribution is `[I]` from `get_crate_for_file` path-prefix shape, not exercised.
- **panic-points on lowercase C# methods** — `[I]` from the SQL predicate; no test.
- **Records→Class / dotted-namespace search UX** — source-verified; user-visible output not empirically probed here (no builds/tests run per assignment).
- **Tracker statuses** are the local `.rivets/issues.jsonl` snapshot of 2026-08-09 (183 issues); two issues (7a6a, 71if) show closure activity dated 2026-08-09.
- **No empirical CLI runs** were performed in this ticket; every classification rests on source, tests, docs, and tracker evidence.

## 11. Downstream decision handoff

At baseline `7ca48f8`, tethys delivers 16 CLI commands over one language-neutral pipeline: 10 are equivalent for Rust and C# today (`search`, `callers`, `impact`, `cycles`, `stats`, `reachable`, `deprecated-callers`, `untested-code`, `dead-code`, `hierarchy` — the latter four with C#-fenced parity), 2 are structurally Rust-scoped with the C# analog already ticketed (`visibility-tightening` → tethys-41lq; `unused-imports` by design with no analog filed), `panic-points` is Rust-only by unfenced naming convention, `coupling` is inapplicable to pure-C# workspaces, `index` is degraded by C# extraction/metadata gaps, and `affected-tests` plus all `--lsp` paths are wired but lack C# e2e proof. All but four discrepancies map to existing open issues (tethys-41lq, 3b06, cfme, itez, 5uqz, alus, glus, nnst, nvcy, 9181, dsp1, 0aqj, bvgb, 24tj, k543, ot0z, xpc4, m1rz, byie, qqbi and Rust-side facets); the four unfiled gaps are `.csproj`/`.sln`/assembly discovery (the single root cause behind most C# degradation, incl. 41lq and byie), the rust-analyzer-only CLI `--lsp` preflight gate, missing C# CLI e2e fixtures for `affected-tests`/LSP, and C# type-reference extraction. Scorecard work can group all items into seven root-cause families (G1 compilation-unit discovery → G2 extractor vocabulary → G3 using-form resolution → G4 deliberately Rust-scoped analyses → G5 presentation → G6 cross-cutting infra → G7 docs debt) rather than one patch per symptom, and must respect the `tests/seam_lint.rs` seam when any fix is designed.
