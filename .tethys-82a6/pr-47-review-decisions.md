# PR #47 review decision log — `docs/pr-47-code-review.md`

| | |
|---|---|
| **PR** | #47 — `feat(discovery): publish evaluated workspace revisions` |
| **Head judged** | `cc59647` (`feat/tethys-82a6-publication`) |
| **Base** | `71035d2` |
| **Decision surface** | this file (per-PR file follows the `pr-45-review-decisions.md` precedent in this change dir) |
| **Findings** | 22 + 2 refuted candidates |
| **Method** | six independent read-only verification passes over the finding clusters, then Main re-checked every claim that decides a verdict; probes run by Main: `dotnet msbuild -getItem:Compile` (F1), serde `impl Serialize for Path` source (F2), Rust std Windows prefix parser + its own unit test (F15), exhaustive greps (F17, F21, F11), `git diff` (F22) |
| **Code changes** | none — this pass is verification only |

Evidence states: `Verified` / `Refuted` / `Unverified` / `Not-applicable`. Decisions: `Accept` (reviewer's fix is right), `Modify` (real claim, different/better fix), `Reject` (apply nothing).

## Summary

| id | finding (bug claim / proposed fix) | evidence-state | decision |
|---|---|---|---|
| 1 | Duplicate `Compile` items violate `file_participation` PK, roll back whole revision / dedup the insert | Verified | Modify |
| 2 | Non-UTF-8 path in serde encode aborts whole index run / lossy-encode at the JSON boundary | Verified | Modify |
| 3 | Write lock held across MSBuild evaluate + restore / (no fix) | Verified | Modify |
| 4 | Stale absolute crate paths break Rust attribution / re-derive at open | Verified | Modify |
| 5 | Identity failure deletes live C# facts / (no fix) | Verified | Modify |
| 6 | Untrusted run wipes trusted evaluation cache / carry the cache forward | Verified | Modify |
| 7 | One bad snapshot row locks out every command / tolerate bad rows | Verified | Modify |
| 8 | Any walk issue clears the cache, preventing rebuild / delete the clear | Verified | Reject |
| 9 | Order-sensitive issue compare falsely invalidates all units / order-insensitive compare | Refuted | Reject |
| 10 | Partial sources drive indexing despite Indeterminate / (no fix) | Refuted | Reject |
| 11 | Churn withdrawal clears sources but keeps references / (no fix) | Refuted | Reject |
| 12 | `restore_receipts` skips incomplete-inventory guards / add the guards | Refuted | Reject |
| 13 | Rust-only repo exits 1 on any unreadable directory / stop mapping to FAILURE | Verified | Reject |
| 14 | `--property` name validated trimmed, stored untrimmed / store the trimmed name | Verified | Accept |
| 15 | Windows verbatim prefix mangled on round-trip / normalize symmetrically on read | Verified | Modify |
| 16 | Every CLI command hydrates the full publication / load lazily | Verified | Modify |
| 17 | Two tables written and hydrated, never read / drop both tables | Verified | Modify (revised) |
| 18 | `discover_workspace` adds 2–3 full walks / skip generated dirs | Verified | Modify |
| 19 | `relative_path` canonicalizes `.cs` on hot paths / reorder to lexical-first | Verified | Modify |
| 20 | Neutral-driver fence does not cover `src/lib.rs` / extend the fence | Verified | Modify |
| 21 | `discovery_options()` has no caller / delete it | Verified | Accept |
| 22 | Deletes an unreleased note for shipped #46 behavior / restore the bullet | Verified | Accept |
| R1 | Generated `obj/**.cs` reach the index — refuted candidate | Refuted (refutation sound) | N/A |
| R2 | Machine-absolute paths in `evaluation_inputs` — dropped candidate | Refuted | N/A |

Nine of the review's own CONFIRMED stamps hold; **three of the review's strongest-sounding findings are refuted** (9, 10, 11) and **two more are refuted on harm** (12, and the harm half of 6/8). Four implied fixes are wrong or incomplete (3, 9, 19, 20) and three are incomplete in scope (2, 15, 17).

---

## Detail

### 1. Duplicate `Compile` items violate the participation PK — Verified, Modify

**Evidence (Main, executed).** `dotnet msbuild <proj> -getItem:Compile` on three project shapes, each producing **2 items with one unique `FullPath`**: default glob + explicit `<Compile Include="Program.cs"/>`; two identical explicit includes with `EnableDefaultCompileItems=false`; `Program.cs` + `.\Program.cs`. Code: `write_units` prepares one `INSERT INTO file_participation` per `unit.sources` entry with no dedup (`src/db/discovery.rs:284,302-311`) against `PRIMARY KEY(unit_key, path)` (`src/db/schema.rs:91`); `unit()` pushes one membership per raw item (`src/discovery/msbuild/mod.rs:603-609`); the worker enumerates `GetItems(kind)` verbatim with `IsBuildEnabled = false` (`tools/tethys-msbuild-evaluate/Evaluation.cs:71,86`). The `?` propagates from `replace_discovery_snapshot` (`src/indexing.rs:566`) before `revision.commit()` (`src/indexing.rs:197-198`); `Revision::drop` rolls back, so the whole run is discarded.

**Fix verdict.** The implied "dedup the insert" is the wrong layer. Deduping in `write_units` alone makes the DB disagree with the in-memory snapshot that `discovery_snapshot()` returns (`src/lib.rs:686`), and the reopen-equality tests compare the two (`tests/revision_publication.rs:95-96`). `INSERT OR IGNORE/REPLACE` is also wrong: it silently discards the second item's `link`/`metadata` and collides with `UNIQUE(unit_key, ordinal)`.

**Fix.** Collapse duplicate memberships by resolved normalized path in `unit()` (`mod.rs:603-609`), keeping the first and defining precedence if `link`/`metadata` differ; write ordinals over the deduped vector. Fence: one unit with two identical `Compile` items publishes one membership and reopens equal.

### 2. Non-UTF-8 path in serde encode aborts the run — Verified, Modify

**Evidence.** `encode()` is `serde_json::to_string` mapped to `Error::Internal` (`src/db/discovery.rs:17-20`); serde's `impl Serialize for Path` returns `Err(custom("path contains invalid UTF-8 characters"))` when `to_str()` is `None` (`serde-1.0.219/src/ser/impls.rs:966`, read in the vendored registry source). `CrateInfo` **gains** `Serialize, Deserialize` in this PR (`src/types.rs:174`; base `71035d2:src/types.rs:172` has neither), and it is encoded at `discovery.rs:139`; `IndexError` is encoded at `discovery.rs:190`. The `?` rolls the revision back.

**Corrections to the review.** (a) `IndexError` already derived serde at base (`src/error.rs:71`) — the *new* fatal surface is the new persistence, not the derive. (b) "zero rows in `sqlite_master`" only holds for a `--rebuild` probe; a fresh non-rebuild DB commits `SCHEMA` before the revision (`src/db/mod.rs:126-130`).

**Fix verdict.** Directionally right, incomplete: the fatal surface also includes `ProjectDiscovery.containers`, `RestoreProvenance.inputs`, `EvaluationInput.path`/`canonical_path`, and `DiscoveryDiagnostic.file`. Blind lossy conversion destroys identity for distinct non-UTF-8 paths.

**Fix.** One path policy for all persisted discovery types — a serde adapter encoding bytes losslessly, or an explicitly documented uniform lossy rule — applied to crates, errors, containers, restore inputs and evaluation inputs together, plus a fence that indexes a workspace with a non-UTF-8 filename and asserts a completed revision with a typed diagnostic.

### 3. Write lock held across MSBuild evaluate + restore — Verified, Modify

**Evidence.** `begin_revision` runs `BEGIN IMMEDIATE` (`src/db/revision.rs:45-49`) at `src/indexing.rs:187`, `discover_workspace` runs at `:191`, and `revision.commit()` only at `:198`. `busy_timeout` is 30 s (`revision.rs:37`), and the repo's own fence proves `DatabaseBusy` while a revision is open (`revision.rs:281-299`).

**Corrections.** The *scope* pattern is **not new**: base `71035d2:src/indexing.rs:180` already took `begin_revision` before its walk/parse; this PR's contribution is the **duration** (external process work inside the window). A concurrent writer **queues** for up to 30 s and then gets `SQLITE_BUSY` — the review's "rather than queueing" is wrong, and the "-wal cannot checkpoint" clause is overstated (readers are unaffected; WAL reset/truncation is deferred). The 99.8 %/8.24 s figures are the reviewer's measurements and were not reproduced here; the PR's own 377.8 s body figure is corroborating and exceeds the timeout.

**Fix verdict.** No fix proposed. The naive "move `begin_revision` below `discover_workspace`" **breaks `--rebuild` on a legacy index**: `begin_revision(true)` is what installs the fresh schema (`revision.rs:53-58`) before `discovery_cache()` — an unguarded `SELECT ... FROM evaluation_cache` (`discovery.rs:37-56`) — runs.

**Fix.** In `index_in_revision`, read the cache (`Vec::new()` when `rebuild`), build the request and run discovery **before** `begin_revision`, then keep the existing `DiscoveryRun`/commit sequence. Whole-run atomicity is preserved (every DB write still happens inside the revision); the write-lock window shrinks to the write phase.

### 4. Stale absolute crate paths — Verified, Modify

**Evidence.** `open_workspace` hydrates the snapshot when not rebuilding and falls back to `cargo::discover_crates` only when no publication exists (`src/lib.rs:196-205`); `CrateInfo::path` is absolute/canonical (`src/cargo.rs:178-186,237-238`); `CrateIndex::crate_for` walks `abs.ancestors()` (`src/indexing.rs:57-72`). `tests/revision_publication.rs:85` pins the persisted value.

**Impact overstated (refuted sub-claims).** `RustModuleResolver::file_anchor` **never returns `None`** — it falls back to `file.parent()`/workspace root (`src/languages/module_resolver.rs:243-255,277-285`); `compute_module_path_for_file`, `build_file_crate_map` and `run_architecture_phase` are **index-time only** and always see fresh crates; the orphan fallback is `orphan:<first path component>`, not `orphan:<filename>`; and `find_crate_for_file`/`get_crate_name_for_file` do not exist. Real blast radius: `tethys unused-imports` (`src/unused_imports.rs:181-182`) and the public `crates()`/`get_crate_for_file` API — it self-heals on the next index.

**Fix verdict.** "Re-derive at open" is **not acceptable**: the persisted-crates behavior is the invariant this PR establishes and its tests pin (`tests/revision_publication.rs:84-85`, `tests/discovery_cli.rs:121`, `tests/msbuild_discovery.rs:95`).

**Fix.** After hydrating, repair only genuinely stale state: if any crate path is outside the current canonical root or missing, replace `discovery.crates` with `cargo::discover_crates(&workspace_root)`. Robust follow-up: persist crate paths workspace-relative (or persist the indexing root) and re-absolutize at hydration.

### 5. Identity failure deletes live C# facts — Verified, Modify

**Evidence.** `discover_files` is handed `&mut errors` (`src/indexing.rs:217,221-225`); the `Physical` arm pushes an `IndexError` and `continue`s on `canonicalize()` failure (`:1238-1251`); the same vec feeds `remove_failed_parse_facts` (`:340`, `:418`), which deletes by id with **no** `classify_indexed_file` recheck (`:596-604`), unlike `purge_orphan_files` (`src/reindex.rs:284-292`). The file never enters `source_files`, and the revision still commits. Only `CSharpModuleResolver` returns `Physical` (`src/languages/module_resolver.rs:363-366`), so it is C#-only.

**Correction.** "Transient" is overstated: a failure that self-heals on another occurrence is retried and survives; loss needs the failure to persist for the whole run.

**Fix verdict.** No fix proposed; the implied "transplant the `classify_indexed_file` guard" is **wrong** — `remove_failed_parse_facts` exists to drop stale facts for files that are on disk but failed to parse, which `classify_indexed_file` reports as `Modified`.

**Fix.** Separate the channels: give `discover_files` its own diagnostic sink (used for `source_diagnostics`/publication) and pass only parse failures to `remove_failed_parse_facts`; make the `Err` (I/O) arm diagnostic-only, keeping the containment-withdrawal (`Ok`-but-outside) case as the only deletable one. Fence: index a C# file, make `canonicalize` fail for a whole run, assert symbols survive and the next run re-parses.

### 6. Untrusted index wipes the trusted evaluation cache — Verified, Modify

**Evidence.** `replace_discovery_snapshot` unconditionally `DELETE`s `evaluation_cache` and reinserts only `snapshot.cache` (`src/db/discovery.rs:135,168-171`); `snapshot.cache` is populated only under `trust_msbuild` (`mod.rs:57-58,311,386,425`); defaults are untrusted (`src/types.rs:1059-1068`); `update()` has no options variant (`src/reindex.rs:39-41,296-306`).

**Correction.** The review's "the incoming cache is loaded and then thrown away" is **wrong**: `request.cache` feeds `Cache::entries` and drives reuse decisions (`cache.rs:219-223,256-274`); it is simply not re-published unless the project is re-evaluated in this invocation.

**Fix verdict.** Blanket carry-forward contradicts the documented invocation-envelope decision (`mod.rs:204-205`) and would desynchronize DB and memory.

**Fix.** (1) Add `update_with_options`/a discovery-options setter beside `update()` so library consumers can keep the trust grant, and document that untrusted `index()`/`update()` republishes source-only. (2) Only if receipts must survive an untrusted run: merge validated prior entries into `snapshot.cache` **before** publication (keeping DB == memory) and rely on the existing reuse-time revalidation.

### 7. One bad snapshot row locks out every command — Verified, Modify

**Evidence.** `decode` maps any serde error to `Error::Internal("corrupt discovery metadata: …")` (`src/db/discovery.rs:22-25`) and `open_workspace` propagates it (`src/lib.rs:196-206`); every command opens through `Tethys::new`, including plain `index` (`src/cli/index.rs:70-75`); only `--rebuild` skips the read. The author fenced the hard-failure policy deliberately (`discovery.rs:689-700`).

**Fix verdict.** "Tolerate/skip bad rows" contradicts that fence and could serve a partial publication to queries. Split by command role.

**Fix.** On the non-rebuild **index** path, treat an undecodable publication as absent (warn, continue) because the run replaces it anyway; keep read-only queries hard-failing but make the error name `tethys index --rebuild`. Longer term, version the persisted vocabulary so a newer binary cannot render an older one unusable at the same `SCHEMA_VERSION`.

### 8. One unreadable dir prevents cache rebuild — Verified mechanism, **Reject**

**Evidence.** `if !before.issues.is_empty() || !after.issues.is_empty() { snapshot.cache.clear(); }` (`mod.rs:165-167`) runs after the project loop, and publication then empties the table (`discovery.rs:135,168-171`). The new non-destructive guard is real (`cache.rs:227-228,262-264` → bypass reason at `mod.rs:397-400`, then a fresh evaluation).

**Why Reject.** The causal attribution is wrong: with a persistent unreadable directory, **the new guard alone** blocks evaluation reuse even if the clear is deleted, so the stated harm ("re-evaluates every project on every index") survives the proposed fix. The clear's actual effect is that restore receipts are not reused either — which is consistent with the deliberate "no receipt from a moving tree" policy (`mod.rs:204-205`). The reported slowdown is a property of the intended incomplete-coverage policy, not a defect, and the walk issues are surfaced to the user through the diagnostics path. Optional follow-up (not a fix): if the team wants restore receipts to survive an incomplete inventory, delete the clear and re-validate the restore-reuse fences.

### 9. Order-sensitive issue compare falsely invalidates all units — **Refuted, Reject**

**Evidence (Main, re-verified).** The premise "one issue per failing dirent all keyed on the same parent directory" is **false**: `WorkspaceInventory::observe` is called with `&directory` only for `fs::read_dir` and the `ReadDir` item error (`candidates.rs:278,282`); every per-entry arm — exclusion check, `file_type()`, `metadata()`, symlink ancestry, generated check — passes the **entry path** (`candidates.rs:290,299,310,322,342`). Per-entry failures in one directory therefore have distinct paths, and the directory-keyed arms yield at most one issue per directory (`ReadDir` ends the stream on error). With all issue paths distinct, the path-sorted `Vec` comparison (`mod.rs:151`, sort at `candidates.rs:357-359`) is a deterministic set comparison. The duplicate-`discovery_issues` claim also fails: the merge loop consumes one predecessor per path (`mod.rs:154-164`), and a same-path/different-message pair is a genuinely different observation, not a duplicate.

**Residual.** The comparison still depends on the undocumented unique-path invariant; replacing it with a multiset comparison is defensible hardening, but it fixes no reachable failure and a regression test would have to synthesize a state the walk cannot produce.

### 10. Partial sources drive indexing despite Indeterminate — **Refuted, Reject**

**Evidence.** `unit()` does collect partial sources while downgrading standing (`mod.rs:603-620`), and `discover_files` consumes them with no standing filter (`src/indexing.rs:1213-1218`) — but both failure arms are non-indexable by construction (`Ok(_)` = not a regular file; `Err` = unresolvable into the workspace, `candidates.rs:92-143`), so `sources` holds exactly the resolvable existing regular-file `Compile` items. `discover_files` is a **union** (`:1229`) over the full walk, retained paths and evaluated sources, so in-tree C# is indexed regardless and evaluated sources only add authored files under walk-excluded directories — the PR's stated design and the behavior `tests/msbuild_discovery.rs:414-450` asserts. The implied fix (refuse sources from non-Confirmed units) would drop genuine memberships.

### 11. Churn withdrawal clears sources but keeps references — **Refuted, Reject**

**Evidence.** The asymmetry is real (`mod.rs:198-203` clears `sources` only, guarded by `== Confirmed`), but **nothing consumes** `project_references`/`assembly_references`: a tree-wide grep finds only construction (`mod.rs:601-656`), hydration (`db/discovery.rs:245-263`) and persistence (`db/discovery.rs:286-329`). No dependency graph is built from them, and the unit is published `Indeterminate`. The behavior is also identical at base (`71035d2:src/discovery/msbuild/mod.rs:170-172`). The `== Confirmed` guard is correct — dropping it would clear sources on already-Indeterminate units and discard valid memberships. If the tables ever gain a consumer, gate consumption on standing.

### 12. `restore_receipts` skips incomplete-inventory guards — **Refuted, Reject**

**Evidence (Main, re-verified).** The guard asymmetry is real (`cache.rs:407-429` vs `262-273,326-328,346-348`), but the asserted harm is falsified by `current_inputs` (`restore.rs:1408-1435`): a prior receipt is honored only when `receipt_digest(&files) == receipt`, where the digest is content-based over `asset_inputs` (project file, `Directory.*`, `Directory.Packages.props`, `NuGet.Config`, `global.json`, lock files, generated `obj` files and every `expectedPackageFiles` entry — `restore.rs:998-1069,1089-1185,1187-1206`). Editing any of those changes the digest, the prior receipt is dropped, and the real restore path runs; an unreadable restore source makes `restore_sources` return `None` and the receipt is dropped too. So no stale receipt can skip a needed restore.

**Residual (optional policy, not a fix).** If `--no-discovery-cache` is intended to forbid restore reuse as well, `restore_receipts` should return an empty map under `DiscoveryCachePolicy::Disabled`, mirroring `restore_entry` (`cache.rs:433-437`).

### 13. Rust-only repo exits 1 on any unreadable directory — Verified behavior, **Reject as a defect**

**Evidence.** `cli::index::run` returns `FAILURE` whenever `!is_complete()` (`src/cli/index.rs:142-147`); `is_complete()` starts with `issues.is_empty()` (`src/discovery/types.rs:735-741`); `issues` is set before the empty-projects early return (`mod.rs:45,48-50`); and the shipped test pins it (`tests/concurrency_and_filesystem.rs:314-315`).

**Why Reject.** The behavior is the PR's **documented contract**: "Incomplete discovery publishes the available source revision, prints … diagnostics to stderr, and exits with status 1" (`docs/msbuild-evaluation.md:41-45`), restated in `changelog.d/tethys-82a6-publication.added.md:2`. The review is also wrong that the run prints "a full success summary": `print_discovery_failures` emits the diagnostics before returning `FAILURE` (`src/cli/index.rs:149-166`). Two legitimate **design** observations remain and need a product decision, not a bug fix: a project-less workspace inherits walk issues (so a Rust-only repo can exit 1), and the signal is inverted relative to `stats.errors` (a clean walk with parse failures exits 0). If the first is judged wrong, the minimal change is to drop walk issues in the `found.projects.is_empty()` early return — with doc and test updates.

### 14. `--property` name validated trimmed but stored untrimmed — Verified, **Accept**

**Evidence (Main, re-verified).** `parse_property` filters on `!name.trim().is_empty()` but returns `name.to_owned()` (`src/cli/index.rs:20,22`). A padded name therefore defeats `effective_globals`' case-insensitive convenience-selector match (`src/discovery/types.rs:80-92`), the lowercased `targetframework` lookup (`cache.rs:194-202`, `mod.rs:252-258`), and the case-duplicate rejection (`src/discovery/types.rs:357-363`). No test references `parse_property`.

**Fix.** `Ok((name.trim().to_owned(), value.to_owned()))` — trim the name only, never the value. The two side-claims are not defects: exact duplicates last-win matches MSBuild, and case-duplicate rejection as `Error::Config` (exit 1) rather than clap usage (exit 2) is a minor UX point.

### 15. Windows verbatim path prefix mangled on round-trip — Verified, Modify

**Evidence (Main, executed).** `normalize_path` rewrites `\`→`/` under `cfg!(windows)` (`src/db/files.rs:44-51`); `discovery_issues.path` is written through it (`discovery.rs:178`) and read back raw (`discovery.rs:117`). Rust std's own unit test `test_parse_prefix_verbatim_device` asserts `parse_prefix(r"//?/C:/windows/…") == Some(Prefix::UNC(OsStr::new("?"), OsStr::new("C:")))` (`library/std/src/sys/path/windows/tests.rs:102-107`), versus `Prefix::VerbatimDisk('C')` for the original; `Path`/`PathBuf` equality is component-based (`std/src/path.rs:2204-2206,3638-3640`), so the round trip is unequal. All unreadable-directory tests are `#[cfg(unix)]`.

**Scope corrections.** Only `discovery_issues` round-trips; `source_diagnostics` is written normalized but never read back. And `normalize_path` uses `to_string_lossy()`, so the same column is **also** lossy for non-UTF-8 names on every platform — the defect is wider than the Windows-verbatim instance.

**Fix.** Make the round trip separator- and byte-faithful for the absolute-path columns: store the native spelling (keep `normalize_path` for lookup identities) or reconstruct it on read, and add a non-`cfg(unix)` test that round-trips an issue with an absolute path so Windows CI exercises it.

### 16. Every CLI command hydrates the full publication — Verified mechanism, Modify

**Evidence.** `open_workspace` hydrates whenever not rebuilding (`src/lib.rs:196-205`); base re-derived crates every open (`71035d2:src/lib.rs:194`). `Index::discovery_snapshot` issues **9** SELECTs plus per-row decodes (`src/db/discovery.rs:37,64,87,101,113,211,232,245,258`) — the review's "6 SELECTs" understates it. (The 10th `SELECT path, id FROM files` at `discovery.rs:274` belongs to `write_units`, not hydration.) 15 of 16 CLI commands construct `Tethys` and never read discovery (only `index` does, and it replaces the snapshot before use at `src/indexing.rs:190-191`); the sole non-index consumer is `unused-imports` via `crates()` (`src/unused_imports.rs:181-182`).

**Magnitude.** The 23×/41 MB figures are the reviewer's synthetic measurement and were **not** reproduced here; the synthetic publication also had an empty `evaluation_cache`, so real SDK-8 receipts would be larger. Treat the mechanism as verified and the magnitude as unverified.

**Fix.** Hydrate only `evaluation_context.crates_json` at open and move the bulk behind lazy initialization; the constraints are that `discovery_snapshot()` returns a plain reference (`src/lib.rs:686-688`) and `DiscoveryRun::drop` restores the prior snapshot (`src/indexing.rs:190-196`). Cheapest independent subset: drop the `evaluation_inputs` hydration (finding 17).

### 17. Two tables written and hydrated, never read — Verified mechanism, Modify

**Evidence.** `evaluation_inputs` is created (`schema.rs:111-116`), inserted (`discovery.rs:157-165`) and hydrated (`discovery.rs:100-109`); no production consumer exists — the in-memory `inputs` field is populated by discovery (`mod.rs:91`) and read only by tests. `source_diagnostics` is written (`discovery.rs:184-199`) and read **only** by two unit-test assertions (`discovery.rs:545,554`) and by the qualification oracle `.tethys-82a6/oracles/discovery_smoke.py:363-364`, which asserts the rows are empty. Cache validity uses the opaque payload's own `receipt.inputs` (`cache.rs:283,309-317`). Magnitude unverified here.

**Fix verdict.** Correct for `evaluation_inputs` (no reader at all → delete table, insert loop, hydration block, keeping the in-memory field). **Not** safe to drop `source_diagnostics` in the same change: it breaks the smoke oracle and the unit assertions unless those are updated in the same commit.

### 18. `discover_workspace` adds full walks — Verified mechanism, Modify

**Evidence.** `discover_workspace` has no production caller at base and now runs per index (`src/indexing.rs:191`). `candidates::walk` excludes only `.git`/`.rivets` from descent while marking `bin`/`obj`/`target` generated (`candidates.rs:286-289,335-349`), stats each entry and accumulates every path (`candidates.rs:302-310,351,355`); `validate_snapshot` runs a second inventory (`mod.rs:131`) whose invalidation loop iterates `validations`, which is empty on the default untrusted path (`mod.rs:264,291`). The index's own walker skips `target`/`node_modules`/`obj` (`src/indexing.rs:1254-1259,1316-1320`).

**Corrections.** The added walk count is **1–2**, not "2–3", and the walker's generated-dir descent, the second inventory and the path-digest in `Cache::new` all pre-exist in the engine merged at base; this PR adds the call site.

**Fix verdict.** Blanket exclusion of `bin`/`obj` from `candidates::walk` is unsafe — `validate_snapshot` expects to see restore-written `obj/` artifacts and excuses them via `cache::captured_restore_paths` (`mod.rs:131-190`), and `Cache::new` folds the path list into the environment digest.

**Fix.** Skip the second inventory when it cannot mean anything (`validations.is_empty() && hosts.is_empty()` → reuse `before`), which removes a full walk per default-path index. Reduce the first walk only for directories no discovery input can live under, with the discovery-cache/restore fences re-run.

### 19. `relative_path` canonicalizes `.cs` on hot paths — Verified mechanism, Modify

**Evidence.** The `Physical` branch is tested first (`src/lib.rs:294-306`) and canonicalizes (`:317-318`); base put `strip_prefix` first (`71035d2:src/lib.rs:299-318`). Hot callers: `indexing.rs:915,1100`, `resolve.rs:470`, `reindex.rs:236` (from `:80,130,267`), plus many more.

**Refuted sub-claim.** "The pre-check can never hit / the comment is dead" is false in the ordinary case: raw and canonical keys are equal when the root is canonical and no component is symlinked, so `seen.contains(&path)` does hit (`indexing.rs:1223-1227,1257-1259`); the mismatch only bites for symlinked/`./`-spelled paths. The reject-arm sub-claim is real: one unresolvable path emits one error per chain → duplicate `IndexError`/`source_diagnostics` rows and duplicate ids in one `delete_files` call.

**Fix verdict.** The implied "lexical-first reorder" is **wrong** — the `Physical` branch exists so C# publication/freshness identity is the canonical, workspace-contained path; returning un-canonicalized spellings would let one file key differently across discovery, publication and lookup.

**Fix.** Keep the ordering and remove the repetition: dedup on the canonical key in `discover_files` (and record rejected paths so one unresolvable source emits one diagnostic), and memoize `relative_path` for the hot callers.

### 20. Neutral-driver fence does not cover `src/lib.rs` — Verified, Modify

**Evidence (Main, re-verified).** `tests/seam_lint.rs:11-14` includes only `resolve.rs`, `indexing.rs`, `batch_writer.rs`, `module_resolver.rs`; a tree-wide grep finds no other fence that ingests `src/lib.rs`. In `module_shape.py`, `src/lib.rs` is in the approved ledger and the protected list (growth + four forbidden function names + process launching), and the `source_identity` scan fires only on **declarations** (`module_shape.py:122-169`) — so a language-specific *use* in `lib.rs` passes both fences. The strong phrasing "no fence would catch a regression there" is overstated: C13 does catch a `source_path_identity`/`SourcePathIdentity`/`ModuleResolver` declaration, a forbidden function name, a process launch, or >+100 production lines.

**Fix verdict.** The naive "add `lib.rs` to the needle list" **fails today**: production `lib.rs` legitimately contains `CrateInfo` (`:63,677,699`) and the test-only `"crate"` literal at `:1341` (which `include_str!` would ingest).

**Fix.** Two currently-green additions: add `"src/lib.rs"` to the dotted-join driver list (`tests/seam_lint.rs:81-94`; verified no `.join(".")` there today), and add a production-only forbidden-needle rule for `lib.rs` in `module_shape.py`'s protected-path loop (which already skips `#[cfg(test)]`), forbidding `resolve_module_path`, the `"crate"`/`"self"`/`"super"` literals and `.join(".")`. Do **not** forbid `CrateInfo` or the identity call — they are present in production. AGENTS.md should name `src/lib.rs` where it claims the seam is test-enforced.

### 21. `discovery_options()` has no caller — Verified, **Accept**

**Evidence (Main, re-verified).** A whole-tree grep matches exactly one line: the definition (`src/types.rs:1029-1032`). Nothing in `src/`, `tests/`, `tools/`, `.tethys-82a6/` or `docs/` calls it.

**Correction.** The review's justification is partly wrong: tethys's `dead-code` command excludes public symbols by design (`src/lib.rs:1089-1091`, `src/db/dead_code.rs:188`), so it will never report this; only `untested-code` would, and it is not a CI gate.

**Fix.** Delete the method (repo convention: delete weightless code), or keep it and correct the doc comment to say it exists for callers inspecting options before indexing. Not a defect — hygiene.

### 22. Deletes an unreleased note for shipped #46 behavior — Verified, **Accept**

**Evidence (Main, re-verified).** `git diff 71035d2 cc59647 -- changelog.d/` removes the multi-target restore bullet from `changelog.d/tethys-82a6-discovery.added.md` and adds a publication fragment that does not restate it; there is no `CHANGELOG.md` (base or head), and `changelog.d/README.md` states the directory *is* the unreleased changelog. The described behavior is unchanged by this PR (`restore.rs` empty base..head diff; disabled-cache path preserved at `cache.rs:262-264`, `tests/discovery_cache.rs:120`).

**Fix.** Re-add the exact deleted bullet as the final bullet of `changelog.d/tethys-82a6-discovery.added.md` (5 bullets, within the 1–5 limit) and keep the removal of "Existing CLI indexing behavior is unchanged." Do not move it to the publication fragment and do not create/edit `CHANGELOG.md`.

### R1. Generated `obj/**.cs` reaching the index — refutation sound

The companion only evaluates (`Evaluation.cs:71,73,86`: `IsBuildEnabled = false`, `LoadProject`, `GetItems`), so target-produced `obj/**.cs` cannot appear at evaluation time; the SDK's `Compile` glob excludes `obj/**` via `DefaultItemExcludes`; and tethys still excludes `obj/` from the automatic walk (`src/indexing.rs:1344-1349`). The only `obj/` route in is an authored `Compile` include, which the (ignored) test `tests/msbuild_discovery.rs:414-450` asserts. The deleted doc sentence was stale wording, not a lost invariant.

### R2. Machine-absolute paths in `evaluation_inputs` — correctly dropped

No production code reads them, and `.rivets/index/` is gitignored, so there is no behavioral harm; it aggravates findings 16/17 and is part of finding 2's non-UTF-8 fatal surface. Treating it as a standalone defect would invite path rewriting for no behavioral gain.

---

## Status

- **No production code, test, doc or changelog was changed by this pass.** Decisions above are recommendations.
- Applying the `Accept`/`Modify` rows is a bounded repair pass over an already-published PR head; it should be authorized and landed as atomic commits with the findings they address named in each message.
- Two items need a **product decision**, not a fix: the project-less-workspace exit-1 question (finding 13) and whether `--no-discovery-cache` should also forbid restore-receipt reuse (finding 12 residual).

---

# Repair record — implemented on `work/pr47-repairs`

Scope: every `Accept`/`Modify` row above, plus the two extensions found during
verification (non-UTF-8 lossiness of persisted path columns; the wider non-UTF-8
fatal surface). `Reject` rows carry no code change. The two product questions at
the end are deliberately untouched.

## Atomic repairs

| repair | findings | root-cause change | paths |
|---|---|---|---|
| A | F1 | `unit()` collapses memberships by resolved path before publication, keeping the first item's `link`/`metadata`; `file_participation` can no longer violate `PRIMARY KEY(unit_key, path)` | `src/discovery/msbuild/mod.rs`, `tests/msbuild_discovery.rs` |
| B | F2, F15, R2 | new `types::path_wire`: UTF-8 paths keep the existing plain wire shape, non-UTF-8 paths are tagged with a leading NUL and encoded as platform units (Unix bytes / Windows UTF-16), separators never rewritten. Applied to every persisted `PathBuf` (`CrateInfo`, `IndexError`, `ProjectDiscovery.containers`, `RestoreProvenance.inputs`, `SourceMembership.path`, `EvaluationInput.path`/`canonical_path`, `HostProvenance.path`, `DiscoveryDiagnostic.file`, `DiscoveryIssue.path`) and to the `file_participation`, `discovery_issues` and `source_diagnostics` path columns, replacing lossy `normalize_path` there | `src/types.rs`, `src/error.rs`, `src/discovery/types.rs`, `src/db/discovery.rs` |
| C | F3, F4, F7, F16 | discovery (and the evaluation-cache read, skipped on `--rebuild`) now runs before `begin_revision`, so `BEGIN IMMEDIATE` covers only the write phase; opening reads only the published crate list (`Index::discovery_crates`) and hydrates the rest on first `discovery_snapshot()`; crate roots outside the current workspace are re-derived; a corrupt publication no longer blocks opening or indexing and its error names `--rebuild` | `src/lib.rs`, `src/indexing.rs`, `src/db/discovery.rs`, `src/resolve.rs`, `src/unused_imports.rs` |
| D | F5, F19 | `discover_files` writes identity failures to a dedicated diagnostic channel that is merged only after `remove_failed_parse_facts`, so an unresolvable source never deletes live facts; raw spellings are tracked separately from canonical identities and one unresolvable path emits one diagnostic | `src/indexing.rs` |
| E | F6 | `update_with_options` added beside `update`; `update` documented as granting no evaluation | `src/reindex.rs` |
| F | F14 | `parse_property` stores the trimmed name (values untouched) | `src/cli/index.rs` |
| G | F18 | the second inventory in `validate_snapshot` is skipped when no restore scope and no host was evaluated | `src/discovery/msbuild/mod.rs` |
| H | F20 | `src/lib.rs` added to the dotted-join driver fence; `module_shape.py` gains a production-only forbidden-needle rule for `lib.rs`; AGENTS.md names the driver list | `tests/seam_lint.rs`, `.tethys-82a6/oracles/module_shape.py`, `AGENTS.md` |
| I | F21 | `IndexOptions::discovery_options()` deleted (zero callers) | `src/types.rs` |
| J | F22 | the multi-target restore bullet restored to the discovery fragment; `changelog.d/tethys-82a6.fixed.md` records the user-visible repairs | `changelog.d/` |

**F17 revised during implementation.** Removing `evaluation_inputs` persistence
broke the pinned reopen-equality contract: `DiscoverySnapshot::inputs` is part of
the published snapshot compared by `tests/msbuild_discovery.rs` and
`tests/revision_publication.rs`, so dropping the table makes a reopened snapshot
differ from the live one. The hydration *cost* that the finding measured is
removed by repair C (nothing hydrates unless discovery is read); the table and
the field are therefore retained as publication evidence. `source_diagnostics`
stays for the same reason plus the qualification oracle's read.

## Gates (all run in `/home/dwalleck/.cache/pr47-fix`)

| gate | command | result |
|---|---|---|
| Format | `cargo fmt --all -- --check` | PASS |
| Lint | `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| Ordinary tests | `cargo nextest run --all-features` | 1197 passed, 47 skipped |
| Doctests | `cargo test --doc --all-features` | 18 passed, 2 ignored |
| Native roster (SDK 8.0.417) | `cargo nextest run --all-features --lib --test discovery_candidates --test discovery_failures --test discovery_cache --test discovery_runtime --test msbuild_discovery --test discovery_cli --test revision_publication --run-ignored all --no-fail-fast -E 'not test(sdk_classic_metadata) and not test(installed_companion)'` | 707 passed, 2 skipped |
| Ownership | `python3 .tethys-82a6/oracles/module_shape.py --stage S4 --base 71035d2` | C13 PASS |
| Changelog | `cargo nextest run --test changelog_lint` | 2 passed |
| Fence mutation (F20) | injected a production `.join(".")` into `src/lib.rs` | `seam_lint` 1 failed (import_joins_go_through_the_seam); C13 `src/lib.rs:695: forbidden neutral-driver needle '.join(".")'`; restored byte-identical (`sha256` equal) → 5 passed and C13 PASS |

## New permanent fences

- `types::path_wire_tests`: plain wire shape for UTF-8, non-UTF-8 round trip
  through serde, malformed tagged value rejected, verbatim Windows spelling
  (Windows-gated).
- `db::discovery::tests::non_utf8_issue_path_round_trips_through_publication`
  (Unix) and `verbatim_issue_path_round_trips_through_publication` (Windows).
- `tests/msbuild_discovery.rs::duplicate_compile_items_publish_one_membership`
  (ignored native): two `Compile` items for one file publish one membership and
  reopen equal.
- `tests/revision_publication.rs`: legacy index still rebuilds without reading
  the evaluation cache; crate roots outside the workspace are re-derived on
  open; a corrupt publication does not block opening or indexing and its read
  error names `--rebuild`.
- `tests/concurrency_and_filesystem.rs::unresolvable_source_keeps_live_facts_and_reports_once`
  (Unix): a permission-blocked source keeps its symbols and emits exactly one
  diagnostic.

## Not fenced

- F18's inventory skip is behaviourally invisible: with no validated scope and
  no host the second walk can only reproduce the first, and every discovery
  cache/freshness fence still passes. Recorded as `N/A — no observable
  difference; covered by the existing discovery-cache and freshness fences`.
- F3's lock *duration* has no deterministic in-process fence; the ordering is
  covered by repair C's code shape, the legacy-rebuild fence, and the existing
  `competing_writer_is_busy_until_revision_release` exclusivity fence. A
  one-shot prober measurement was not run in this pass.

## Follow-up: the two open decisions

**F13 — project-less workspace exit status: implemented.** `tethys index` now
maps to `FAILURE` only when evaluated coverage is incomplete — any indeterminate
project or unit. Walk issues alone no longer fail a run, so a Rust-only tree with
an unreadable directory publishes, prints the issue, and exits 0, while an
untrusted C# candidate still exits 1. Fences:
`tests/discovery_cli.rs::unreadable_directory_in_a_project_less_workspace_does_not_fail_the_run`
(new; red under the previous mapping, restored green) and the existing
`untrusted_index_publishes_source_only_with_nonzero_status`, which still passes.

**F12 residual — withholding restore receipts under `--no-discovery-cache`:
rejected on evidence; recommendation retracted.** The guard was implemented and
four native restore fences failed
(`authorized_restore_rechecks_metadata_before_confirmation`,
`authorized_restore_corroborates_target_downloads_and_rejects_changed_imports`,
`authorized_restore_preserves_multitarget_assets_with_target_assigned_versions`,
`restore_receipt_property_names_ignore_case_but_values_do_not`), with the
second-run units collapsing to `[]` because the run then reports
`RestoreRequired`.

The evidence shows a restore receipt is not evaluation cache: it records that
restore already produced outputs for these exact input contents
(`restore.rs::current_inputs` revalidates the receipt digest against
`asset_inputs`), which is what lets a later run evaluate after the restore grant
was given once (`tests/discovery_failures.rs`, the `granted_once` control).
Withholding it turns the cache flag into an implicit revocation of the restore
grant: `--no-discovery-cache` without `--allow-restore` would stop evaluating any
package-backed project, and adding `--allow-restore` back would re-run restore —
a side effect that was not requested and that can fail offline. `restore_entry`
refusing to *write* new receipts under `Disabled` is the correct, narrower
policy. The actual gap was documentation, not behaviour, so the flag's meaning is
now stated in `docs/msbuild-evaluation.md` and the retraction is recorded here.

## Product decisions still open
