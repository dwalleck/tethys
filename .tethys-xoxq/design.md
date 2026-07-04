# tethys-xoxq design: visibility tightening (falsifiable-design, 2026-07-04)

## Purpose

`tethys visibility-tightening`: list pub Rust items whose observed use is
consistent with `pub(crate)`, tiered Definite/Maybe under the PRD error
posture (suppressions, not accusations). Probe phase (`findings.md`) proved
the naive refs-only rule is 33% false on real data and validated the
evidence-based rule this design encodes (5/5 grep-oracle agreement).

## Probe constraints the design may not contradict

- Cross-package resolved refs alone are NOT sufficient evidence of non-use:
  53iv steals qualified refs (and destroys their text), ygjx hides macro
  uses, z9mr declines some reexport refs.
- Rescue channels that exist in-index: `imports` rows (source_module carries
  the crate path), unresolved refs' qualified `reference_name` text.
- Cross-package resolution is 83% unique-name-fallback; non-unique names
  cannot be trusted either direction.
- Module symbols carry visibility; root-reachability is computable (C8 ran).
- The self-index is one package (lib+bin merged).

## Architecture

- `src/db/visibility.rs` — query module beside `deprecated.rs`, same shape:
  `VisibilityFinding { name, kind, file, line, tier, demotions }` where
  `demotions` is a (possibly empty) list of kebab-case reasons
  (`shared-name`, `root-reachable`, `glob-reexport-risk`). Definite ⇔ empty.
- Facade method `Tethys::get_visibility_candidates()`; CLI
  `src/cli/visibility_tightening.rs` with table + `--json`
  (`{summary, findings}` envelope, deprecated-callers precedent).
- `--workspace-closed` flag lifts the root-reachability Maybe ceiling (C7).

### Candidate rule (per pub top-level symbol S in package P)

S is a candidate iff NO cross-package evidence exists in ANY channel:
(a) resolved refs from another package; (b) an `imports` row in another
package whose `source_module` first segment is P's crate name (`-`→`_`
normalized) and whose `symbol_name` or `alias` equals S.name; (c) an
unresolved ref in another package whose qualified `reference_name` ends
with `::S.name`. Symbols carrying a `reexport`-kind ref are excluded
entirely (AC2). Candidates start Definite and demote to Maybe per C4/C6/C7.

## Input shapes (step 2)

- **Visibility**: `public` → analyzed; `crate` (already tight), `module`
  (`pub(super)`/`pub(in)`), `private` → excluded (C9). Tightening advice
  for `module`-visibility items is out of scope v1 (settled: pub(crate) is
  not necessarily narrower than pub(in path)).
- **Kind**: in scope: function, struct, enum, trait, type_alias, const.
  Out: method, struct_field, enum_variant, module, macro — tracked at
  **tethys-w1e9** (C9 fences the exclusion).
- **Evidence channels**: each of (a)/(b)/(c) present alone (C1/C2/C3),
  none present (candidate), import via alias (C2 fixture variant),
  hyphenated package name imported with underscores (C2 fixture variant).
- **Re-export shapes**: named `pub use` (reexport ref → C5), glob
  `use m::*` row targeting S's module in-package (C6), no re-export.
- **Name uniqueness**: workspace-unique / shared cross-package / shared
  same-package / cfg-twins in one file — all non-unique shapes demote (C4).
- **Reachability**: root-reachable via all-pub chain / buried under a
  non-pub module (C7/C8/C13); flag on/off (C7).
- **Topology**: multi-package / single-package (C13) / zero candidates (C10).
- **Language**: Rust only; C# parity tracked at **tethys-41lq**.
- **lib+bin single package**: not modelable today — the bin target's files
  are the same package, so bin-only consumption of lib API is invisible
  (probe fact 2). Absorbed by C7's default Maybe ceiling (bin-consumed lib
  items are root-reachable) and documented in the self-index review (C15).

## Subtractive sweep (step 2b)

Purely additive: a new read-only query module and CLI command; no existing
constraint, guard, ordering, or serialization point is removed. (One
sentence, per the skill.)

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| 1 | A pub symbol with a cross-package resolved ref is never reported | two-crate fixture: `b` calls `a::widget()` via import (unique name → resolves) | fixture source hand-read | 20m | pending | integration test `cross_package_ref_excludes` (tests/visibility.rs). Buggy impl caught: evidence sweep missing channel (a) |
| 2 | A pub symbol whose only cross-package use is an `imports` row is never reported — incl. aliased import and hyphenated package (`my-lib` → `my_lib::…`) | fixture: `use a_lib::helper as h;` where `helper` collides with a local name so the ref does NOT resolve; package dir named `a-lib` | fixture source hand-read | 30m | pending | `cross_package_import_excludes` + alias/hyphen asserts. Buggy: refs-only rule (the probe's 33% failure); no `-`→`_` normalization; alias column ignored |
| 3 | A pub symbol whose only cross-package use is an unresolved qualified ref `…::name` is never reported | fixture: `a::helper()` fully qualified, no import, name collides so Pass 2 declines | fixture source hand-read | 20m | pending | `unresolved_qualified_excludes`. Buggy: ignoring `refs.symbol_id IS NULL` rows |
| 4 | A candidate whose name is shared by ANY other indexed symbol is at most Maybe | fixture: unique-name candidate (Definite) + same-name-other-package candidate (Maybe) + cfg-twin pair (Maybe) | fixture source hand-read | 20m | pending | `shared_name_demotes`. Buggy: uniqueness scoped per-package; twins collapsed by GROUP BY name |
| 5 | A symbol carrying a reexport-kind ref is never reported (AC2) | fixture: `pub use inner::item;` at crate root, `item` otherwise unreferenced | fixture source hand-read | 15m | pending | `reexported_item_excluded`. Buggy: reexport refs counted as ordinary same-package refs → item becomes a candidate |
| 6 | A candidate whose module is the source of a same-package glob import row is at most Maybe (pv7w guard) | fixture: `pub use inner::*;` at root + pub `item` in `inner` | fixture source hand-read | 15m | pending | `glob_module_demotes`. Buggy: glob rows (`symbol_name='*'`) skipped because they match no symbol name |
| 7 | A root-reachable candidate defaults to Maybe; `--workspace-closed` allows Definite (AC3) | fixture: pub fn in `pub mod` chain (Maybe by default, Definite with flag) vs pub fn under private mod (Definite by default) | fixture source hand-read | 25m | pending | `root_reachable_ceiling` (both flag states). Buggy: reachability ignoring re-exports' additive effect is separate (C5 excludes those); chain walk stopping at first module |
| 8 | Root-reachability is computable: private-chain symbol → unreachable, all-pub chain → reachable | self-index SQL walk: `crate::db::deprecated` (db, deprecated private) vs `crate::cargo` (pub) | source text `grep "mod db" src/lib.rs` etc. — independent of index | 5m | **passed** (2026-07-04) | C7's fixture re-encodes both chains; unit test on the chain-walk helper. Buggy: module row lookup keyed wrong (name vs module_path confusion) |
| 9 | Non-public visibility and member kinds never appear | fixture with pub(crate)/pub(super)/private items + pub method/field/variant | fixture source hand-read | 15m | pending | `scope_excludes_nonpublic_and_members`. Buggy: kind filter drifts when new kinds added; `module` visibility treated as public |
| 10 | Zero candidates → empty envelope: summary zeros, empty array, exit 0 | fixture where every pub item is cross-package-used | fixture source hand-read | 10m | pending | `empty_envelope` (binary-level `run_cli`). Buggy: early return skipping envelope |
| 11 | Byte-identical JSON across full re-index | index twice, compare bytes | string equality (mechanical) | 10m | pending | `json_deterministic_across_reindex`. Buggy: HashMap iteration order in demotions; row-id-dependent ordering |
| 12 | CLI table and `--json` both render tier; JSON key set fixed | run binary both modes on C7's fixture | `jq` key extraction + string contains | 15m | pending | `cli_tier_visible_both_modes`. Buggy: tier only in JSON; demotions key absent when empty (key-set drift) |
| 13 | Single-package workspace: default run yields no Definite except non-root-reachable items | single-package fixture: root-reachable pub fn (Maybe) + buried pub fn (Definite) | fixture source hand-read | 15m | pending | `single_package_defaults`. Buggy: cross-package evidence check vacuously passing → everything Definite |
| 14 | On q-cli, Definite tier for fig_auth + fig_ipc has zero grep-refuted candidates | run analysis, grep-oracle every Definite survivor (probe3 mechanism); if workspace compiles locally, also `cargo check` after applying `pub(crate)` to survivors | grep over sources (independent); rustc (independent) | 45m | pending (fig_auth slice passed at probe time, 5/5) | one-shot measurement recorded in `.tethys-xoxq/definite-audit.md`; deterministic floor lives in C1–C7 fixtures which encode each refutation class | 
| 15 | Self-index run reviewed by hand; lib/bin merge limitation documented with observed rate (AC6) | run on tethys itself; hand-classify every finding | human review vs source | 30m | pending | **manual** — requires explicit user approval (skill rule); the mechanical floor is C13's fixture |

Cheapest falsifier (C8, 5m): **run and passed** before this design was
presented — see the SQL/grep transcript in the design session; both chains
agree with source text.

Per-claim distinctness: every fixture claim has its own named test; C14's
audit file lists per-symbol verdicts; C8's unit test is separate from C7's
fixture.

## Negative space (what this deliberately does not do)

1. **No substrate fixes**: tethys-53iv, tethys-ygjx, tethys-z9mr,
   tethys-3i35, tethys-pv7w stay open; tiering and multi-channel evidence
   absorb them (C2/C3/C4/C6). When pv7w lands, C6's demotion can narrow —
   pv7w is the tracked trigger.
2. **No member/module-level advice** (methods, fields, `pub mod`): tracked
   at tethys-w1e9. Enum variants have no own visibility (language rule).
3. **No C# analysis**: tracked at tethys-41lq.
4. **No `Cargo.toml` `publish` parsing**: enclosure is the user's call via
   `--workspace-closed` (settled: publishedness is a release-process fact
   tethys cannot verify; a manifest flag can be stale in either direction).
5. **No auto-rewrite**: report-only, like every other tethys analysis.
6. **No `pub(super)`→narrower advice**: `module` visibility excluded (C9);
   pub(crate) is not strictly narrower than pub(in path).

## Open decisions flagged for approval

1. **C15's fence is `manual`** (hand-reviewed self-index run recorded in
   the PR) — the skill requires explicit user approval for a manual fence.
2. CLI command name `visibility-tightening`; flag name `--workspace-closed`.
3. Re-exported items are EXCLUDED from output entirely (AC2 "not flagged"),
   not listed as Maybe — the alternative (list-as-Maybe) was rejected as
   noise: re-export is affirmative API-surface intent.
