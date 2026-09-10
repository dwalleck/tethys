# PR #48 review decision log — code review (max, 15 findings)

| | |
|---|---|
| **PR** | #48 — `feat(architecture): expose evidence-aware unit coupling` |
| **Head judged** | `978dbcd` (`feat/tethys-82a6-coupling`) |
| **Base** | `main` = `87e406a` (PR47 was squash-merged; #48 was auto-retargeted to main) |
| **Findings** | 15 + 2 dropped sweep items + 1 evidence-leak census |
| **Code changes** | none — this pass is verification only |

Evidence states: `Verified` / `Refuted` / `Unverified` / `Not-applicable`. Decisions: `Accept` (reviewer's fix is right), `Modify` (real claim, different/better fix), `Reject` (apply nothing).

## Summary

| # | Finding | Evidence | Decision | Fix verdict |
|---|---|---|---|---|
| 1 | FK cascade: `replace_discovery_snapshot` destroys MSBuild arch nodes + edges | Verified | Accept | correct |
| 2 | MSBuild units have no file attribution → fabricated known-zero | Verified | Modify | right layer, larger than minimal |
| 3 | `declared_assembly_references` never read by coupling | Verified | Modify | right only as *evidence*, never as edges |
| 4 | `apply_evidence` overwrites efferent unconditionally | Verified (half) | Modify | consequences half-wrong |
| 5 | Three test guards eroded | Verified (2/3) | Modify | proptest clamp misattributed |
| 6 | `semantic_metadata` leaks absolute paths into `tethys coupling` | Verified (narrowed) | Modify | right shape, exact keys differ |
| 7 | MSBuild vs Cargo package path spellings diverge | Verified | Accept | correct arm to change |
| 8 | Rust-only guard in shared `CrateIndex::crate_for_file` | Verified (contested) | Modify | reverting would re-admit the C9 violation |
| 9 | Workspace-global `incomplete` blanks every unit's Ca | Verified | Modify | needs scoping, not removal |
| 10 | `get_package_coupling` full scan + abort on unrelated corruption | Verified | Accept | correct |
| 11 | Two standing warnings deleted | Verified | Modify | restore observation, not the old text |
| 12 | C# coupling assertions structurally vacuous | Refuted (as stated) | Modify | add a positive control |
| 13 | Ledger `crate_private` became opt-in | Verified | **Modify** | **reviewer's fix breaks the fence** |
| 14 | `path_wire_tests` deleted as move collateral | Verified | Accept | restore all four |
| 15 | Coupling table stopped being a table | Verified | Modify | marker + move the unit block out |
| A | `SCHEMA_VERSION` triplication (dropped sweep) | Verified (mislocated) | Modify | single-source the version |
| B | `module_shape.py --base` escape hatch (dropped sweep) | Verified | Accept | delete or fail closed |
| L | Absolute user paths committed across the repo | Verified | Accept | redact at capture + one-shot sweep |

**Two reviewer fixes are wrong or harmful**: F13 (defaulting the visibility check to `True` would emit 12 false failures on the current tree) and F8 (reverting the guard would restore the Cargo-layout-dependent C# bucketing the C9 fence exists to prevent). Three more findings are materially overstated or wrong in mechanism: F6 (only ProjectReference metadata reaches the CLI, not "each declared reference's metadata" wholesale), F9 (the global `incomplete` is the conservative direction; removing it would fabricate zeros), and F12 (the assertions are falsifiable; the gap is a missing positive control, not vacuity).

---

## Detail

### 1. Cascade data-loss — Verified, Accept

`PRAGMA foreign_keys = ON` is set on every production connection (`src/db/revision.rs:68`, via `configure_connection`). `arch_packages.evaluation_unit_key REFERENCES evaluation_units(unit_key) ON DELETE CASCADE` (`src/db/schema.rs:275`) extends the reach of `replace_discovery_snapshot`'s first statement, `DELETE FROM projects` (`src/db/discovery.rs:176`), through `evaluation_units` into `arch_packages` and on into `arch_file_packages` / `arch_package_deps`. The function's own doc sanctions calling it alone ("Replace active metadata without removing syntax, atomically even when called alone", `src/db/discovery.rs:166`). A Rust neighbour's Ca drops silently because its target node disappears.

Decisive supporting fact the review did not cite: `repopulate_architecture` **already wipes and rebuilds** the whole table (`// 1. Wipe. Cascade clears the two child tables.` then `DELETE FROM arch_packages`, `src/db/architecture.rs:66-67`). So the cascade buys nothing and only widens an existing DELETE's blast radius. Because PR48 is unmerged, the schema text can change without a migration or version bump.

**Fix (accept, with the reason above):** replace `ON DELETE CASCADE` with `ON DELETE SET NULL` (keeps the FK's integrity intent, stops the destruction) or drop the FK action entirely; then add a fence that calls `replace_discovery_snapshot` alone on a fixture carrying an MSBuild arch node plus a Rust neighbour edge and asserts both the node and the neighbour's Ca survive.

### 2. MSBuild units have no file attribution — Verified, Modify

`run_architecture_phase` takes `assignments` only from `cargo::architecture_inputs` (`src/architecture.rs:335-336`); `architecture_inputs` maps files through `CrateIndex::crate_for_file`, which is now Rust-only (`src/cargo.rs:487-491`); `repopulate_architecture` writes `arch_file_packages` only by Cargo crate name (`src/db/architecture.rs:96-99`); `arch_package_deps` is derived solely from `file_deps ⋈ arch_file_packages` on both endpoints (`:112-119`); the view COALESCEs absent joins to 0 (`src/db/schema.rs:314-324`); `read_metrics` wraps those zeros as `Known` (`src/db/architecture.rs:1433-1436`). So no `msbuild:<project>:<unit>` package can ever carry a file, and a Confirmed unit in a complete run with no contributing ProjectReference publishes Ca=Ce=`Known(0)`.

Caveat the review omits: non-Confirmed units and incomplete runs already surface `Indeterminate`, so "every MSBuild unit reports known" is overstated; the fabricated zero applies to the confirmed happy path.

**Fix (modify):** the reviewer's long-term wiring is the right layer and the data already exists (`EvaluationUnit.sources`, `file_participation`, and real C# `file_deps`). But do the convention-compliant half first: mark an MSBuild package's file-graph Ca/Ce as `Indeterminate` instead of letting the structural zero through as `Known` — "publish unknown, never a fabricated zero" is the rule this feature exists to honour. Then extend `assignments` from the persisted membership. Note the `arch_file_packages.file_id` PK means a C# file claimed by two units needs a disambiguation rule.

### 3. Assembly references never read — Verified, Modify

`declared_assembly_references` is created (`src/db/schema.rs:103-107`), written (`src/db/discovery.rs:329-367`), and read exactly once into hydration (`:299-307`). `read_unit_evidence` populates only `declared_references`, and `EvaluationUnitCoupling` has no assembly-reference field (`src/architecture.rs:28-43`); `apply_evidence` consults only project references (`:358-366,381-383`). A classic `<Reference><HintPath>…` unit therefore keeps `Known(0)` efferent.

**Fix (modify):** correct as *evidence*, never as edges — assembly references name external DLLs and can never be `arch_packages` nodes. Carry a per-unit `EXISTS` from `declared_assembly_references` and set efferent `Indeterminate` when a contributing assembly reference exists. Do not resolve `HintPath` into `arch_package_deps`.

### 4. Unconditional efferent overwrite — Verified (half), Modify

The overwrite is real (`src/architecture.rs:381-383`, no read of the prior value, `incomplete` gates only afferent at `:376-380`). But: "contradictory reasons on one row" overstates it — the two axes carry axis-specific reasons, which is defensible. "Silently discards a real measured Ce" is **Refuted for this tree**: Cargo packages are skipped (`:367-370`) and every MSBuild efferent is the COALESCE'd zero, so today the overwrite discards only the fabricated zero it is meant to mask. It becomes a genuine clobber once finding 2's attribution lands.

**Fix (modify):** when `incomplete`, set **both** axes to `Indeterminate(IncompleteDiscovery)`; build the selected-project set from the units and only mark `UnselectedProjectReference` for a contributing reference whose target is genuinely unselected.

### 5. Test-guard erosion — Verified (2/3), Modify

Real: the boundary test lost `to_bits()` + `!is_nan()` (now `assert_eq!` on `MetricEvidence::Known(expected)`, `src/architecture.rs:307-319` — float equality still catches NaN and drift, so only the ±0.0 sign bit escapes); and `index_stats_default_arch_phase_is_none` / `coupling_sort_default_is_instability` are gone with no replacement. **Misattributed:** the proptest `unwrap_or(u32::MAX)` clamp predates PR48 and was moved verbatim (`pr47-fix/src/db/architecture.rs:1455-1467` → `src/architecture.rs:455-467`).

**Fix (modify):** destructure the payload and restore bit-exact + NaN guards; restore the `coupling_sort_default_is_instability` line (user-visible ordering); track the clamp as a pre-existing issue, not a regression of this PR.

### 6. Absolute-path metadata leak — Verified (narrowed), Modify

`semantic_metadata` drops only `ModifiedTime`/`CreatedTime`/`AccessedTime` (`src/discovery/msbuild/mod.rs:780-789`) while the worker emits every evaluated metadata entry plus a `BuiltInMetadata` set (`Evaluation.cs:26-29,88-93`). Absolute machine paths survive in `FullPath`, `RootDir`, `Directory`, `DefiningProjectFullPath`, `DefiningProjectDirectory`. Reach to output is **narrower than the review states**: only the ProjectReference metadata map is printed (text `src/cli/coupling.rs:211-214,427-429`; JSON `:246`). Unit `properties` are persisted but never printed — `EvaluationUnitCoupling` has no properties field and the query reads only `$.AssemblyName` (`src/db/architecture.rs:~1370`).

**Fix (modify):** extend the case-insensitive drop list to exactly `FullPath, RootDir, Directory, DefiningProjectFullPath, DefiningProjectDirectory` (keep `Filename/Extension/RelativeDir/RecursiveDir/Identity/DefiningProjectName/DefiningProjectExtension`). No Rust consumer reads the absolute keys, so stripping breaks nothing. Separately decide whether the persisted-but-unprinted `properties_json` sink (which carries `MSBuildProjectFullPath`, `NuGetPackageRoot`, `MSBuildBinPath`) is acceptable — the reviewer's fix does nothing for it.

### 7. Path spellings diverge — Verified, Accept

`ProjectKey` is separator-normalized at construction (`src/discovery/msbuild/candidates.rs:162-168`), so msbuild rows are forward-slashed on every platform; the Cargo arm stores native separators (`src/cargo.rs:513-518`); both land in `arch_packages.path` and the `packages` JSON array. Windows-only, and the repo's storage convention is already forward-slash (`files.path`, `SourceMembership`, `ProjectKey`). No fence asserts on the value.

**Fix (accept):** normalize the Cargo producer with a one-line `replace(std::path::MAIN_SEPARATOR, "/")`, mirroring `project_key`, and pin `packages[].path` in an existing architecture test.

### 8. Rust-only guard in a shared accessor — Verified (contested), Modify

The guard is real (`src/cargo.rs:487-491`) and shared with `Tethys::build_file_crate_map` (`src/indexing.rs:594-595`). Verified consequence: pre-PR48 a C# file under a root-`[package]` crate ancestor-matched that crate (one bucket → `(Some(a), Some(b)) if a == b => true` kept edges unconditionally, `src/db/call_edges.rs:163`); post-PR48 C# files fall to `orphan:<top-dir>`, so uncorroborated cross-directory C# `file_deps` now require using/namespace corroboration and can be dropped.

**But the "regression" frame is wrong.** `tests/csharp_cross_dir_deps.rs:36-44` documents the C9 invariant — C# files in different top-level directories land in *different* orphan buckets — and its virtual-workspace fixture exists precisely to dodge the root-package bucket merge. Pre-PR48, a root `[package]` violated that invariant; the guard makes C# bucketing depend only on top-level directory, never on Cargo layout, and stops C# files being credited to Cargo crates in `arch_file_packages`.

**Fix (modify):** keep the guard. (1) Correct the now-false comments in `tests/csharp_cross_dir_deps.rs:38-44` and `tests/csharp_l2_file_deps.rs:35-37`. (2) Add a variant fixture *with* the injected root `[package]` asserting C# stays orphan-bucketed — no current test fences the guard. (3) Record the user-visible C# `file_deps` change in the changelog. If the old recall is actually wanted, that is a design decision to make explicitly, not by reverting silently.

### 9. Workspace-global `incomplete` — Verified, Modify

`incomplete` is seeded from `EXISTS(SELECT 1 FROM discovery_issues)` (`src/db/architecture.rs:1355-1357`) and OR-folded over every project and unit standing (`:1364,1373`), then applied to all metrics (`:1459`). One traversal error anywhere withholds afferent for every evaluation unit.

**Fix (modify):** the review proposes scoping but the naive fix is dangerous — dropping the global entirely would publish known zeros while a project may be *missing* from the publication (an unreadable directory can hide a `.csproj`), which is exactly the fabrication findings 2 and 3 complain about. Decide *which* discovery issues invalidate coupling evidence (e.g. ignore issues under generated/excluded directories) rather than removing the term.

### 10. `get_package_coupling` full scan — Verified, Accept

`read_metrics(&tx, Some(name))` passes the selector only to decide whether an unknown `source` is an error or a warn+skip (`src/db/architecture.rs:1421-1431`); the SELECT has **no WHERE clause** (`:1415`) and `read_unit_evidence` decodes every unit (`:1451`), after which the caller does `.find(...)` (`:311-313`). Unrelated corrupt unit evidence — or an orphaned unit key (`:1454-1460`) — aborts a single-package query, breaking the spirit of the error asymmetry documented immediately above (`:293-307`).

**Fix (accept):** restore an indexed single-row fetch for the target and decode only that package's unit evidence.

### 11. Deleted warnings — Verified, Modify

`traced_test` usages: base 2 → head 0, taking `logs_contain("unknown source value")` with them, while the doc still bills the `warn!` channel as the integrity path for truncated neighbour lists. The `rivets-4srr` hand-rolled-JSON notes in `src/cli/coupling.rs` (base 4 occurrences) are likewise gone, in the PR that proved the hazard live by hand-editing both writers.

**Fix (modify):** restore the log-channel assertion (the field name is `source`, not the old text) and the auto-propagation note; `tracing-test` is still an active dev-dependency.

### 12. Vacuous C# coupling assertions — **Refuted as stated**, Modify

The claim is that the `incoming.is_empty()` / `outgoing.is_empty()` assertions (`tests/csharp_coupling.rs:93-100`) are structurally vacuous because the fixture never populates `arch_package_deps`. That is wrong on both halves, and the reviewer's implied remedy is not the right one.

- The assertions are **falsifiable**: `fetch_neighbors` reads `arch_package_deps` (`src/db/architecture.rs:339-351`), so any read-path change that surfaced a declared project reference as a neighbour row would make them fail. They are not unfalsifiable.
- The same loop's count assertions (`(APP, Known(0), Indeterminate(UnselectedProjectReference))`, `:83-91`) catch the other variant — a declaration *inflating* Ca/Ce — so "reintroduce the bug and all four tests stay green" is false for the count-level bug the test is named for.
- What the test genuinely cannot catch is the **write-path** variant (a `run_architecture_phase` that turned declarations into `arch_package_deps` rows): the fixture hand-authors rows and never calls the phase.

So the real gap is not vacuity but a **missing positive control inside this file**: with zero `arch_package_deps` rows, a passing `is_empty()` cannot distinguish "correctly empty" from "the neighbour path returns nothing for this fixture". Controls do exist, but elsewhere — `src/db/architecture.rs:563` (`detail.outgoing.len() == 1`), `:716` and `:826` (neighbour dep counts) — so the neighbour path is proven working, just not in the C# evidence test.

**Fix (modify):** add one real `arch_package_deps` edge to the C# fixture (look up both `arch_packages.id` values, insert the edge) and assert it *does* surface as the expected neighbour, then keep the declaration-emptiness assertion alongside it. Rewriting the tests as "vacuous" would discard assertions that do hold.

### 13. Ledger visibility check became opt-in — Verified, **reviewer's fix wrong**

`module_shape.py:189` gates the visibility rule on `rule.get("crate_private", False)`; only the S4 rule opts in, so the five crate-private S5 symbols (`ArchitecturePackage`, `run_architecture_phase`, `apply_evidence`, `CrateIndex`, `architecture_inputs`) can be promoted to `pub` under a C13 PASS. Severity note: these symbols had *no* visibility fence at base either, so this is an incomplete new fence, not a regression.

**Fix (modify — the proposed default-`True` is incorrect):** the check fires per owned symbol, and 12 of the 15 S5 `architecture.rs` symbols are legitimately `pub` (the crate's public coupling API). Defaulting to `True` would immediately emit 12 false failures and break the fence it claims to repair. Instead **split the ledger rules**: keep the intentionally-`pub` symbols flag-less and add a second rule per owner listing exactly the five crate-private symbols with `"crate_private": true`.

### 14. `path_wire_tests` deleted — Verified, Accept

`mod path_wire_tests` (4 tests) exists at `87e406a:src/types.rs:3054` and is absent at `978dbcd`, not relocated. It also contradicts the PR's own decision record, which still lists `types::path_wire_tests` as a retained fence (`.tethys-82a6/pr-47-review-decisions.md:282-285`). **"No coverage anywhere" is refuted**: the non-UTF-8 (unix) and verbatim (windows) publication round trips survive at `src/db/discovery.rs:738,765` and cover `encode`/`decode` end-to-end. Genuinely uncovered at head: malformed-tag rejection, the plain-UTF-8 wire-shape pin, and non-UTF-8 inside a full `CrateInfo` serde payload.

**Fix (accept):** restore all four (the codec deserves defense in depth). Non-negotiable if deduplicating: `utf8_paths_keep_the_plain_wire_shape` and `malformed_tagged_values_are_reported_not_substituted`.

### 15. Coupling table stopped being a table — Verified, Modify

`CountText` renders `indeterminate (unselected_project_reference)` — 43 characters (`src/cli/coupling.rs:172-183`) — into a `{:>3}` field (`:418-420`), so an indeterminate row's count columns blow out against neighbouring numeric rows. `write_unit_text` runs **inside** the row loop (`:426-429`), interleaving an ~8+ line JSON block per unit between table rows. Both text-mode tests were deleted in the same PR with no replacement.

**Fix (modify):** the reviewer's short marker in the count column is right for the blowout (and correctly avoids changing `Display`, which is byte-identical at this width). It does not fix the interleaving: move the unit detail out of the row loop (emit after the table, or gate it behind a `--detail` flag) and re-add a text-mode fence for the new shape.

### A. `SCHEMA_VERSION` triplication — Verified (mislocated), Modify

All three sites are in `src/db/schema.rs` (`:4` constant, `:41` `INSERT OR IGNORE INTO index_revision`, `:42` `PRAGMA user_version`), not `revision.rs`; `revision.rs` already parameterizes the constant correctly (`:83,138`). "Silent drift" is **refuted**: any pairwise drift fails loudly at the next `Index::open`. Only the installing open skips revalidation.

**Fix (modify):** single-source it — drop both literals from the DDL, and apply both from the constant in one `install_schema(conn)` helper called from all three apply sites.

### B. `--base` escape hatch — Verified, Accept

`--base` re-points the single `base` variable driving the changed-file census, the unchanged-file exemption, and the growth deltas, checked only for existence (`module_shape.py:80,98-99,102,109-113,124-125`). `--base HEAD` makes all three vacuous. Compounding: the discovered `upstream` is never compared to `base` — it appears only in the PASS banner.

**Fix (accept):** delete `--base` and fail closed when upstream cannot be discovered; if a debug override stays, warn on stderr, assert `merge-base --is-ancestor <base> HEAD`, and refuse it under CI.

### Evidence leak census — Verified, Accept

`git grep -c -E '/home/[A-Za-z0-9_.-]|C:(\\)+Users'` at head: **127 tracked files, 635 matching lines, 655 occurrences** — `.tethys-82a6/` accounts for 108 files / 469 lines, and the worst single file carries 63. Leaked forms include `/home/dwalleck/.cargo/…`, `.rustup`, `.claude/tmp`, `.local-dotnet/sdk/8.0.417`, `/home/dwalleck/repos/tethys/…`. Among the 19 non-`.tethys-*` files, most are benign `/home/user/…` placeholders; exceptions are a real username in a windows-gated test fixture (`src/lsp/transport.rs:1164`) and probe scripts that hardcode this machine's paths and are broken elsewhere.

**Fix (accept, two parts):** redact at capture (`$HOME` → `<home>` in evidence writers) so the corpus stops accruing leaks; then one mechanical sweep over `.tethys-*/` evidence and the probe scripts — which also un-breaks those scripts on other machines. The `/home/user` placeholders in `src/` need no change.

---

## Status

No code, test, ledger, changelog or evidence file was changed by this pass. Findings 1–15 and A/B/L all have a decision above; implementation is a separate bounded repair pass, and the two contested fixes (13, 8) should be settled before any of it is applied.

---

# Implementation record

Applied on `work/pr48-repairs` (branch of #48). Every `Accept`/`Modify` row above
is implemented; `Reject` rows are unchanged. Two reviewer fixes were deliberately
not applied as proposed (F13, F8) — see below.

## Code repairs

| Findings | Change |
|---|---|
| F1 | `arch_packages.evaluation_unit_key` now `ON DELETE SET NULL`; a membership-only `replace_discovery_snapshot` detaches the withdrawn unit instead of cascading through the node into `arch_package_deps`. Fence: `db::discovery::tests::metadata_only_replacement_detaches_units_without_destroying_architecture` — **red under the old CASCADE** (arch_packages 1 vs 2), restored green. |
| F2, F3, F4 | `apply_evidence` takes a workspace-wide `DeclaredContext` (selected projects + contributing declaration targets, both queried in SQL so a single-package read sees the whole publication). Both axes are withheld together when standing is unconfirmed or discovery is incomplete; the outgoing reason is only `UnselectedProjectReference` when a contributing target is genuinely unselected; a contributing assembly reference yields the new `UnresolvedAssemblyReference`; an `MsBuild` package's *zero* axis becomes the new `UnattributedEvaluationUnit` while a non-zero graph count stays measured. |
| F5 | Boundary cases assert `to_bits()` equality and `!is_nan()` on the inner `f64` again; `coupling_sort_default_is_instability` and `index_stats_default_arch_phase_is_none` restored. |
| F6 | `semantic_metadata` drops `FullPath`, `RootDir`, `Directory`, `DefiningProjectFullPath`, `DefiningProjectDirectory` in addition to the three volatile timestamps. |
| F7 | `cargo::architecture_inputs` stores `arch_packages.path` with `MAIN_SEPARATOR` replaced by `/`, matching `ProjectKey`. |
| F8 | Guard kept (C# must never be Cargo-attributed); stale fixture comments corrected in `tests/csharp_cross_dir_deps.rs` and `tests/csharp_l2_file_deps.rs`. |
| F9 | Documented as deliberate: `discovery_is_incomplete` explains why the scope is workspace-global (a traversal issue can hide a project no surviving row mentions) and is now counted in SQL. |
| F10 | `collect_metrics` issues an indexed `WHERE p.name = ?1` for a targeted read; unit evidence is loaded only for the requested keys via `read_units(conn, keys)`; incompleteness and the declaration context are computed by aggregate queries instead of decoding every unit. |
| F11 | `#[traced_test]` + `logs_contain("unknown source value")` restored on the corrupt-neighbour fence; the `rivets-4srr` hand-rolled-JSON note restored above `write_detail_json`. |
| F12 | Positive control added: `a_declared_reference_does_not_replace_a_real_edge` inserts a real `arch_package_deps` edge, asserts it surfaces in `incoming`, and only then asserts the declaration adds none — so the emptiness assertions are falsifiable. |
| F13 | Ledger split into four `owned_symbols` rules; the five crate-private S5 symbols now carry `"crate_private": true`. Defaulting the flag to `True` (the reviewer's proposal) was **not** applied: it would emit 12 false failures for the public coupling API. |
| F14 | `mod path_wire_tests` restored verbatim (4 tests). |
| F15 | `TableCount` renders `?` in the count columns while `CountText` keeps the long label for prose/JSON; the evaluation-unit block moved out of the row loop to below the table; both deleted text-mode fences restored. |
| A | `schema::install_schema` installs the schema and stamps `index_revision` + `PRAGMA user_version` from `SCHEMA_VERSION`; the two DDL literals are gone. |
| B | `--base` removed; the baseline must now be an ancestor of the discovered upstream. |

## C13 tripwire

Removing the `--base` escape hatch exposed a real violation that the override had
been hiding: `src/lib.rs` production delta +108 against a +100 tripwire (the
growth came from the merged PR47 repairs, not from #48). Fixed by moving
`crates_are_current` to its owner, `src/cargo.rs`. `module_shape.py --stage S5`
now reports C13 PASS at the pinned baseline `0a2753d`.

## Evidence

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo nextest run --all-features` | 1206 passed, 48 skipped |
| `cargo test --doc --all-features` | PASS |
| `module_shape.py --stage S5` | C13 PASS (baseline 0a2753d) |
| changelog lint | 2 passed |
| F1 mutation | CASCADE → FAIL (arch_packages 1), SET NULL → PASS, restore byte-identical |
| F8 mutation attempt | **removing the language guard changed no asserted behaviour** in any fixture I could build: `file_deps` outcomes and Cargo file attribution were identical, and the fixture's discovered crate list is empty. The reviewer's "dropped edges" harm is therefore **unverified**, the guard stays on semantic grounds, and no fence was added because none I could construct would fail on removal. |

## Deferred (proposed tracker issues — not yet filed)

Publication of the tracker is a direct commit to `main`, and the working
checkout's `.rivets/issues.jsonl` carries uncommitted local drift; filing was
left to the maintainer rather than performed unilaterally. The decision log above
carries the scope for each item.

- MSBuild file attribution (F2's second half): `arch_file_packages.file_id` is a
  primary key, so a C# file claimed by two evaluation units needs a
  disambiguation rule before `assignments` can name `msbuild:` packages.
- The proptest clamp `unwrap_or(u32::MAX)` (F5, pre-existing, not a #48 regression).
- Scoping which discovery issues invalidate coupling evidence (F9).
- Redacting committed evidence: 127 tracked files carry absolute user paths, 108
  of them under `.tethys-82a6/`; capture-time redaction plus a one-shot sweep.
- Refreshing `.tethys-82a6/evidence/s5-qualification.json`: its `source_sha256`
  block pins files this repair changed; the record is prose-only (no script
  verifies it), so it was left untouched rather than restamped with stale proof.
