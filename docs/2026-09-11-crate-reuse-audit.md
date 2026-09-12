# Crate-reuse audit — 2026-09-11

Where does tethys hand-roll code that an existing crate does better — either a
crate already in the dependency graph, or one we could add?

Scope: all of `src/` (32,617 lines across 66 files). Method: seven read-only
slices (cargo/resolution, indexing walk, LSP transport, MSBuild discovery,
`src/db`, CLI/presentation, language extractors), each required to cite
`file:line` and a verbatim quote for every claim. Headline numbers and
external-API claims were re-verified by reading the dependency's own source.

## Constraint frame

A candidate is only real if it survives all four:

- `deny.toml` license allow-list (MIT, Apache-2.0, Apache-2.0 WITH
  LLVM-exception, BSD-2/3-Clause, ISC, MPL-2.0, Unicode-3.0/DFS-2016).
- No `unsafe` in tethys (`unsafe_code = "forbid"`), and no crate that forces it
  on callers.
- The hand-rolled code reimplements a *solved, standard* problem rather than
  encoding tethys policy. Policy is a reason to **keep** code — resolution
  provenance (ADR-0003 bands), the K-hybrid import corroboration
  (`src/db/call_edges.rs`), MSBuild trust/evaluation boundaries, and
  whole-run publication atomicity all read as reinvention to a naive scan and
  are none of them bugs.
- ADRs are **priors, not law**. ADR-0002 (`docs/adr/0002-sql-ctes-not-petgraph.md`)
  was written against 2026-01-22's API landscape; where its stated reasoning has
  aged out, this audit says so rather than citing it as a blocker.

## Findings

| Tracker | # | Location | Hand-rolled | Replace with | New crates | Verdict |
|---|---|---|---|---|---|---|
| `tethys-g0ej` | 1 | `src/cargo.rs:188-243` | Cargo `autolib`/`autobins` target inference probed off disk | `cargo_toml::complete_from_path` output | 0 (direct dep) | **STRONG** |
| `tethys-ixvj` | 2 | `src/db/symbols.rs:78,190,270-277` | LIKE patterns built with unescaped metacharacters | SQL `ESCAPE` + ~4-line escaper | 0 | **MODERATE** (correctness) |
| `tethys-pyxd` | 3 | `src/main.rs:332` + `colored` | stderr colorization decided from stdout | `anstream`/`anstyle` | 0 (both in lock) | **MODERATE** (correctness) |
| `tethys-nk8x` | 4 | `src/cargo.rs:411-441` | `prefix/*`-only glob expansion | `glob 0.3.3` | 0 (in lock via `rstest_macros`) | **MODERATE** |
| `tethys-n7di` | 5 | `src/discovery/msbuild/candidates.rs:422-485` | 64-line quick-xml event machine for `<Project Path=…>` | quick-xml `serialize` feature | 0 (feature flag) | **MODERATE** |
| `tethys-bn7l` | 6 | `src/discovery/msbuild/candidates.rs:209-342` | DFS walker, frame stack, symlink-cycle guard | `walkdir 2.5` | 0 lock entries (dev→runtime) | **MODERATE** |
| `tethys-41fs` | 7 | `src/db/graph.rs:596-940` | Johnson enumeration + iterative Tarjan SCC | `rustworkx-core::connectivity::johnson_simple_cycles` | **6-7 mandatory** | **MODERATE** (live decision) |
| — | 8 | `src/db/graph.rs:569-583` vs `:490-497` | duplicated adjacency load + two BFS cores | std-only consolidation | 0 | **WEAK** |
| — | 9 | `src/discovery/msbuild/host.rs:402-419` | `executable()` — a `which` | `which` 7.x | small | **WEAK** |

Issue column: `—` marks a finding recorded here but deliberately not filed (both are WEAK, and filing them would add tracking overhead without an actionable decision).

### 1. Cargo target inference — STRONG

`Manifest::from_path` (`src/cargo.rs:47`, `:114`) calls
`from_path_with_metadata` → `complete_from_path`, whose own documentation says:

> `Cargo.toml` doesn't contain explicit information about `[lib]` and `[[bin]]`,
> which are inferred based on files on disk. … This scans the disk to make the
> data in the manifest as complete as possible.

It fills `lib.path` (`cargo_toml-0.22.3/src/cargo_toml.rs:508-513`), autosets
`src/bin/x.rs` / `src/bin/x/main.rs` (`:620-655`), and adds the `src/main.rs`
product (`:530-550`). tethys then re-derives all of it:

```rust
let inferred = PathBuf::from(format!("src/bin/{name}.rs"));
let full_path = crate_path.join(&inferred);
```

**Three behaviors, only two of them bugs** — and the flags are what separate
them. cargo_toml gates inference:

```rust
if !has_path && (package.autolib || self.lib.is_some()) && src.contains("lib.rs") {
```
```rust
if package.autobins {
```

tethys's fallbacks react to files on disk and cannot see either flag:

- **A. `manifest.lib == None` → probe `src/lib.rs`. DELETE.** After
  completion, `lib: None` is already exact: no `[lib]`, and either autolib is
  off or there is no `src/lib.rs`.
- **B. `bin_paths.is_empty()` → synthesize `src/main.rs`. DELETE.** Per the
  Cargo book, "automatic target discovery can be disabled so that only manually
  configured targets will be built"; disabled discovery must yield zero bins.
  tethys currently recreates them.
- **C. declared `[[bin]]` with `path: None` → infer `src/bin/{name}.rs`. KEEP.**
  This is *not* disabled auto-discovery: the book states the rule
  unconditionally — "If not specified, the inferred path is used based on the
  target name" — and a `[[bin]]` is a manually configured target, built even
  under `autobins = false`. cargo_toml's `autoset`, which would fill exactly
  that path, sits inside the `if package.autobins` block, so on this path the
  completed manifest legitimately carries `path: None`.

Residual judgment call in C, separate from the flag question: it pushes the
inferred path even when the file does not exist (only a `debug!`), where Cargo
fails the build. That is the phantom-entry question, not an autobins question.

Also verify on the change: bin *order* (`sort_products` sorts by `(name, path)`;
tethys keeps manifest order and `entry_point_file()` takes `bin_paths.first()` —
observable only for multi-bin crates, where bare `crate` already declines), and
keep `sanitize_target_path` applied to the completed paths (cargo_toml passes
explicit manifest paths through raw; sanitizing is our trust policy).

### 2. LIKE metacharacters — correctness, no crate involved

```rust
let like_pattern = format!("{bounded_prefix}%");            // :270
let qualified_pattern = format!("%::{name}");              // :190
```

No `ESCAPE` clause at any of `:84`, `:194`, `:276`. `_` is a LIKE wildcard and
is ordinary in both crate directory names (`my_app`) and Rust identifiers
(`my_field`), so a prefix or name containing one over-matches: silently widened
scope at `:276`, silently wrong bind at `:194`. The doc at `:248-250` asserts
prefixes "don't contain those characters" — false — while the sibling at
`:414-416` names this exact hazard as its reason for using an EXACT match.

`:78`'s `format!("%{query}%")` is a different question: wildcard behavior in a
user's *search* string may be intended, but it is undocumented either way.

Fix is std-only: SQL `ESCAPE '\'` plus a ~4-line escaper, applied at every LIKE
site or the sites diverge.

### 3. `colored` decides from the wrong stream

`colored-3.1.1/src/control.rs:108` gates colorization on
`io::stdout().is_terminal()`, but `src/main.rs:332` renders diagnostics through
`eprintln!`:

```rust
eprintln!("{}: {e}", "error".red().bold());
```

So `tethys … 2>errors.txt` with a TTY stdout writes escape codes into the
capture, and `tethys … | cat` with a TTY stderr gets no color on errors.
`anstream` does per-stream detection and `anstyle` is the style-value crate;
both are already in `Cargo.lock` (via `clap_builder`/`anstream`). This is a
dependency *removal* (`colored` out) plus a correctness fix.

### 4-6. Mechanical expansions

- `glob_member` documents its own deferral: "Full glob support (via the `glob`
  crate) could be added if needed." `glob 0.3.3` is `MIT OR Apache-2.0` with
  zero runtime deps and already in `Cargo.lock`. Deltas: literal member paths
  gain metacharacter semantics (`foo[1]` stops matching literally); non-UTF-8
  roots need an explicit decline; `glob` does not implement Cargo's
  `exclude`/`!`-negation, so this is glob parity, not full member-set parity.
  Keep the `is_dir() && Cargo.toml.exists()` filter — that is tethys policy.
- `slnx_paths` is a quick-xml *event* state machine (depth counter, manual
  attribute decode, manual DTD/entity/CDATA/text rejection) whose whole product
  is each `<Project>`'s `Path`. quick-xml's `serialize` feature collapses it —
  a feature flag, not a new crate. Not a pure deletion: strictness
  (`UnknownField` on `<Folder>`, single-root, no-DTD, no CData) must be
  re-established explicitly.
- `candidates.rs::walk` → `walkdir`. `has_symlinks` is a **cache-qualification
  key** (`cache.rs:130-140` → `unqualified_symlink_inventory`), so any drift in
  symlink semantics disqualifies caching differently rather than merely
  changing output. `tests/symlink_boundary.rs` must re-fence the swap; take
  this one last, or not at all.

### 7. Cycle enumeration and SCC — a live decision, not a settled one

`docs/adr/0002` Consequences invited exactly this: *"Algorithms that don't
express cleanly as CTEs (e.g. Tarjan SCC for rich cycle grouping, weighted
shortest path) could justify petgraph for those specific operations."* The
2026-08-06 amendment (tethys-usvm) declined and narrowed the trigger:
*"needing Tarjan is not on its own a reason to take petgraph. The reason would
be needing several such algorithms, or needing one whose correct implementation
is genuinely hard to get right."*

Re-derived against today's APIs:

- **petgraph has no simple-cycle enumerator**, in 0.6.5 or 0.8.3 (the versions
  verified; no feature flag adds one). Its `algo` surface is `tarjan_scc`,
  `kosaraju_scc`, `johnson` (all-pairs *shortest paths*, unrelated despite the
  name), `all_simple_paths`, `toposort`, `condensation`,
  `is_cyclic_directed/undirected`, `feedback_arc_set`. The tracker has one
  adjacent request (issue #367, largest simple cycle, open, no PR). So petgraph
  offers nothing for enumeration.
- **`rustworkx-core 0.18.1` does.** `connectivity::johnson_simple_cycles` is
  Johnson's algorithm "based on the non-recursive implementation found in
  NetworkX", and it is non-recursive throughout: `process_stack` is an explicit
  `Vec<(NodeIndex, IndexSet<…>)>` loop and `unblock` an explicit stack loop. It
  performs its own SCC restriction internally, so `strongly_connected_components`
  (`graph.rs:756-843`, 70 non-comment lines), `hosts_cycle`, `run_cycle_search`,
  `CycleSearch`, `visit` and `unblock` all become redundant — measured at 195
  non-comment lines across `graph.rs:646-991`.
  **Adopting it closes the open tethys-qqbi** (unbounded recursion in
  `visit`/`unblock`) by construction, since the recursive functions cease to
  exist.
- **The bridge cost is lower than ADR-0002 assumes.** `kosaraju_scc` and both
  DFS drivers are generic over traversal traits (`Dfs`/`DfsPostOrder` are
  explicitly non-recursive, `visit/traversal.rs:15,129`), and petgraph ships the
  induced-subgraph view as a stock adaptor: `visit/filter.rs`'s
  `NodeFiltered`, whose `FilterNode<N>` is implemented for `&HashSet<N, S>` —
  exactly the `allowed: HashSet<FileId>` set. Marshal-in/marshal-back per pass
  is *not* intrinsic. The ADR's reason (a) describes the 2026-01-22 spike's
  hybrid design, not what the current trait API would cost.

**Costs to weigh, verified:**

| Cost | Detail |
|---|---|
| Self-loops | The crate does not reliably report them: "the underlying algorithm is not able to consistently detect self cycles." Pre-collect, strip the self edges from a clone, pass them in; they yield as 1-element `Vec`s. tethys already represents a self-loop cycle the same way (`graph.rs:1242-1275`). Mechanical. |
| Output order | Documented as unspecified — "not guaranteed to have a particular order for either the cycles or the indices in each cycle." tethys buys determinism by pre-sorting its adjacency (`graph.rs:857`), which cannot reach through the crate, so canonicalize-and-sort the *output* instead. Note this is a documented-unspecified contract, not evidence of cross-process reordering: `IndexSet` iteration order "does not depend on the values or the hash function at all" (`indexmap-2.14.0/src/set.rs:35-50`), so the `foldhash::fast::RandomState` in the crate's `IndexSet` is not itself a reordering mechanism. The set of cycles is deterministic either way; the ordering must come from our own comparators. |
| Dependencies | All mandatory in 0.18.1 (docs.rs: "This release does not have any feature flags"): `petgraph`, `fixedbitset`, `priority-queue`, `rand_distr`, `rand_pcg`, `rayon-cond`, `ndarray` are **new**; `foldhash`, `indexmap`, `num-traits`, `rayon`, `hashbrown` are already resolved (several dev-only today). |
| ADR consequence | petgraph arrives as a hard transitive dependency, so "petgraph is not a dependency" becomes false without anyone deciding to adopt petgraph. |

That last row is the real decision, and neither option is free. Neither column
below has been prototyped: no diff exists for either route and nothing in this
audit was run, so every effort figure is an **unvalidated estimate** to check
before committing to a route, not a settled cost.

| | Option A: adopt the crate | Option B: port the shape in place |
|---|---|---|
| Build / dependency | 6-7 new mandatory crates; `petgraph` becomes a hard transitive dependency | none |
| ADR | requires an ADR-0002 amendment | none |
| Implementation effort | self-loop pre-collection + output canonicalization — **unmeasured** | rewrite `visit`/`unblock` as explicit stacks — **unvalidated estimate, not a diff** |
| Ongoing maintenance | upstream owns the algorithm; we track its releases, MSRV and behavior changes | we own the algorithm indefinitely — the shape that produced tethys-usvm and tethys-qqbi |
| Regression validation | re-fence semantics against a new implementation; existing counters degrade to boundary assertions | counters keep meaning something, but only if the port is behavior-preserving, which has to be proven rather than assumed |
| Correctness risk | transferred to a maintained library, with its documented self-loop caveat | back with us; the recursion bug class can return in a later edit |

"Zero new dependencies" and "zero cost" are not the same claim, and the
maintenance and regression-validation rows are the ones a dependency-appetite
argument usually omits. The in-algorithm work counters
(`CycleSearchOutcome::{visits, scc_passes, component_work}`, fenced by name in
`enumerate_cycles_*`) are the concrete stake on the Option B side.

On those counters: they are a *substitute for trust*. Hand-rolling an algorithm
means owning its correctness **and** its cost, so both must be fenced — no
output-based test can see tethys-usvm's 9.4 s / 50,005,000 visits for one
correct answer. Adopting a library means inheriting those guarantees, and the
fence legitimately degrades to a boundary assertion over our corpus. Note also
that `assert_eq!(outcome.visits, NODES)` pins Johnson's blocking specifically: a
different, equally fast enumerator would fail it. Tolerable only because we own
the implementation it pins.

### 8-9. Weak

- The same `file_deps → HashMap<FileId, Vec<FileId>>` load is written twice
  (`:569-583` unvalidated, `:490-497` endpoint-validated) and a parent-map BFS
  core twice (`get_reachable`, `bfs_shortest_ids`). The two BFSs differ
  deliberately (one stops at target and returns one path, one materializes every
  path with per-node depths), so a merge must preserve both contracts.
- `host.rs:402-419` is a `which` without an executable-bit check or PATHEXT
  handling; callers are only `host.rs:516-519` and `restore.rs:1332`. `which`
  requires an executable file, so a non-executable first PATH hit currently
  resolves and would instead be skipped — a behavior improvement, but small.

## Refuted, or deliberately kept

Checked because they look like reinvention; each is policy or already correct:

- **LSP file URIs.** The obvious hypothesis is wrong twice over: `lsp-types
  0.97`'s `Uri` is `fluent_uri::Uri`, not `url::Url` (`url` is absent from
  `Cargo.lock`), and fluent-uri's encoder is behind its `unstable` feature.
  `PATH_PERCENT_ENCODE_SET` (`transport.rs:868-905`) is exactly
  `unreserved(A-Za-z0-9-._~) + / + :` — complete, and *safer* than `url`, which
  under-encodes `[ ] ^ |` and would make `Uri::from_str` reject those filenames.
- **LSP framing and server frameworks.** `lsp-server` is a server scaffold;
  `jsonrpc-core` is unmaintained and server-oriented; `tower-lsp`/`async-lsp`
  force an async runtime onto a synchronous client. The hand-rolled
  Content-Length framing (`transport.rs:258-268`, `281-286`, `364-395`) is
  ~45 lines with tethys-specific semantics (skip notifications, ack server
  requests with null, timeout checked between messages). Keep.
- **UTF-16 column conversion** (`lsp/encoding.rs:45-60`): no crate analog in
  lsp-types, ropey or text-size. Keep.
- **`src/languages/*` (6.3k lines) vs tree-sitter `Query`.** Zero `QueryCursor`
  uses repo-wide; the imperative traversal is load-bearing because it threads
  per-ancestor state a declarative query cannot supply (`containing_span`,
  `local_bindings`, `ReceiverCtx`, `in_invocation_callee`), and two walks use
  node relations queries cannot express (macro token-tree shape, preceding-
  attribute sibling scan). Only two 17-22 line find-all walks are
  query-expressible and a cached `Query` is not smaller there. No hand-rolled
  byte↔char math exists — all positions come from tree-sitter's own
  `start_position`/`end_position`.
- **MSBuild discovery.** quick-xml, serde_json, sha2 and `command-group` are
  used correctly; NuGet ranges are bracketed grammar semver cannot express
  (`[1.0.3, )`); `project.assets.json` is deliberately read as `serde_json::Value`
  so unknown fields fail *currentness*, not parsing; config/home layout is
  NuGet policy, not generic home-dir resolution; nothing in the directory writes
  files, so `tempfile`/atomic-write has no target; the `ps -o stat=` call is
  inside `#[cfg(test)]`. `.sln` text parsing and `--list-sdks` line parsing have
  no license-clean standard crate.
- **`src/resolver.rs`, `src/languages/module_resolver.rs`.** rustc module-file
  resolution plus the ADR-0001 seam (fenced by `tests/seam_lint.rs`), with
  design-pinned decline rules. `cargo_metadata` is a documented non-goal
  (`docs/design/tethys-architecture-analysis.md:33-34`) and would spawn `cargo`
  at index time.
- **`src/db` policy.** Raw `BEGIN IMMEDIATE`/`COMMIT` rather than
  `rusqlite::Transaction` (the revision must outlive connection-lock releases);
  typed corruption-raising row parsers rather than `serde`/`strum`; hand-written
  attribute-literal lexing in `deprecated.rs` (must degrade on C# shapes `syn`
  does not cover). All correct `rusqlite` usage otherwise: every value is bound;
  only const column lists, fixed fragments and `?` runs are interpolated, and
  `rusqlite` fails placeholder drift with `InvalidParameterCount`.
- **CLI presentation.** No hand-rolled ANSI escapes anywhere; all `--json`
  paths use serde derive + `serde_json`; `coupling.rs` is the only real table
  and its column math is data-derived. `comfy-table`/`tabled` would relocate
  that math, add a dependency, and break alignment tests that pin the layout.
  The `{:<28}`/`{:<20}` literals across six modules are label blocks, not
  tables. `error.rs` already uses `thiserror`; the manual `Display` impls cover
  the serde-shaped `IndexError` records and the tethys-09wx exit-code contract.

## Verification log

Read by the audit itself: `Cargo.toml`, `Cargo.lock` (199 packages, reverse-
dependency walk to separate runtime from dev-only), `deny.toml`, all three ADRs,
the amendment history of ADR-0002, `src/cargo.rs:1-265`, `src/indexing.rs:1227-1300`,
`src/db/symbols.rs:74-300`, `src/db/files.rs:30-70`, `src/db/graph.rs:596-940`,
`src/lsp/transport.rs:246-400,840-1000`, `src/lsp/encoding.rs`, `src/error.rs`,
`src/cli/display.rs`, `src/discovery/msbuild/host.rs:400-530`.

Dependency sources read to price candidates (not docs prose): `cargo_toml
0.22.3/src/cargo_toml.rs` (completion + `complete_from_path`), `glob 0.3.3`,
`quick-xml 0.41.0`, `walkdir 2.5.0`, `petgraph 0.6.5` and `0.8.3` (`algo/mod.rs`,
`scc/tarjan_scc.rs`, `scc/kosaraju_scc.rs`, `visit/traversal.rs`, `visit/filter.rs`,
`visit/mod.rs`), `foldhash 0.1.5`, `indexmap 2.14.0`, `colored 3.1.1`,
`anstream 1.0.0`, `url 2.5.8`, `rustworkx-core 0.18.1` (source + `Cargo.toml`),
`rusqlite 0.32.1`. Cargo book cross-checked for `autolib`/`autobins` and the
`path` field's inference rule.

## Corrections to this audit's own earlier claims

Recorded because each was wrong, and because the pattern (asserting an external
API without reading it) is the failure mode to watch in later audits:

1. **"petgraph::algo would replace [SCC and enumeration] nearly line-for-line."**
   Never verified — petgraph is not in `Cargo.lock`, so no slice could have read
   it. Wrong in both directions: no enumerator exists in the versions checked,
   and the SCC alternative is `kosaraju_scc` (iterative) or `tarjan_scc`
   (recursive — the property tethys-usvm deliberately removed).
2. **"Keep a narrow name-derived fallback for the `autobins = false` case."**
   Inverted. Disabled auto-discovery means *no* inferred targets; keeping the
   fallback preserves the bug. The fallback that must survive is the
   *pathless-but-declared* `[[bin]]` inference, which is a different case.
3. **"No library counterpart at any version."** Overstated; the verified claim
   is narrower. And the SCC verdict was a flat REJECT where the honest answer was
   a live, near-break-even option.
4. **Enumeration "stays — absence of an alternative."** Wrong; `rustworkx-core`
   provides one, and its non-recursive shape retires tethys-qqbi.
5. **"Existing expected output vectors must change" / RandomState implies
   cross-process reordering.** `IndexSet` iteration order "does not depend on the
   values or the hash function at all." The crate's *documented unspecified*
   order still justifies output normalization, but not that claim.

## Suggested order

1. **#2 LIKE escaping** — smallest change, real wrong-answer class.
2. **#3 stderr detection** — removes a dependency and fixes a visible defect.
3. **#1 Cargo target inference** — deletes a duplicate owner of Cargo's
   convention; take the corrected A/B-delete, C-keep split.
4. **#7 graph decision** — pick: `rustworkx-core` (deps) vs. porting the
   non-recursive shape (no deps). Whichever wins also closes tethys-qqbi.
5. **#4/#5/#6** — mechanical; #6 last, and only with the symlink fence
   re-established.
