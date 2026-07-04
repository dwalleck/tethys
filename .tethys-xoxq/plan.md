# tethys-xoxq plan: visibility tightening (budgeted-plan, 2026-07-04)

Design: `.tethys-xoxq/design.md` (approved; C8 falsifier passed). Claim →
slice coverage: C1,C9→S1; C2→S2; C3→S3; C5,C6→S4; C4→S5; C8→S6; C7,C13→S7;
C10,C12→S8; C11→S9; C14→S10; C15→S11.

Conventions: one commit per slice; module-registration and clap-dispatch
one-liners (src/db/mod.rs, src/cli/mod.rs, src/main.rs) don't count toward
the 2-file limit; all fixtures build their own tempdir index via
`workspace_with_files` and include real `Cargo.toml` files so
`arch_packages` gets manifest attribution (S1 validates this mechanism —
if directory-fallback attribution fires instead, S1's fixture fails
honestly). Output streams: findings (table/JSON) → stdout with the
`ignore_broken_pipe` guard (deprecated-callers precedent); `tracing`
diagnostics → stderr. Expected fixture outputs below are written BEFORE
implementation.

## Slice 1: candidate selection + resolved-refs evidence channel

**Claim:** C1 (cross-package resolved ref excludes) + C9 (non-public
visibility and member kinds never appear).
**Oracle:** fixture source hand-read (expected candidate list written here).
**Stress fixture:** two-crate workspace (`crates/a-lib`, `crates/b-app`,
hyphenated dir now so later slices reuse it): `a_lib` has `pub fn used_fn`
(called from `b-app` via `use a_lib::used_fn; used_fn();` — workspace-unique
so Pass 2 resolves), `pub fn lonely_fn` (no refs anywhere), `pub(crate) fn
tight_fn`, `pub(super)`-style `pub(in crate) fn scoped_fn` inside a module,
`pub struct Widget` with `pub fn method(&self)` and a pub field. Expected
candidates: exactly `[lonely_fn, Widget]` — `used_fn` excluded by channel
(a); `tight_fn`/`scoped_fn` excluded by visibility; `method` and the field
excluded by kind. Bug classes: same-package refs miscounted as evidence
(would drop every internally-used symbol — assert `Widget` present even if
`a_lib` uses it internally… add one internal `Widget` use), `module`
visibility treated as public, member kinds leaking.
**Loop budget:** one SQL statement (symbols ⋈ files ⋈ arch_file_packages,
xref CTE over refs) — O(refs + symbols) inside SQLite with existing
indexes; refs ≈ 10^7, symbols ≈ 10^6 production: single pass each, no
Rust-side nested loop.
**Wall budget:** n/a (on-demand command, not always-on).
**Files:** `src/db/visibility.rs` (new; `VisibilityFinding`,
`Tier`, `get_visibility_candidates` skeleton on `Index`),
`tests/visibility.rs` (new) [+ 1-line mod registration].

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixture produces expected candidate list exactly
- [ ] probe3 oracle mechanism unaffected (no product code shared yet)
- [ ] Budgets hold

## Slice 2: imports evidence channel (hyphen + alias hardened)

**Claim:** C2.
**Oracle:** fixture source hand-read.
**Stress fixture:** extend S1 workspace: `a_lib` gains `pub fn helper`;
`b-app` gains `pub fn helper` too (private would also work — collision
defeats unique-name fallback so the imported bare call stays UNRESOLVED,
forcing channel (b) to do the work — this is the probe's `is_amzn_user`
shape) plus `use a_lib::helper; … helper();`. Also `pub fn mixin` in
`a_lib` imported as `use a_lib::mixin as mx;` with a colliding `mixin`
elsewhere. Expected: `a_lib::helper` and `a_lib::mixin` NOT candidates
(import evidence); `b-app::helper` IS a candidate (Maybe once S5 lands;
tier not asserted in this slice). Bug classes killed: refs-only rule (the
probe's 33% failure), missing `-`→`_` normalization (`a-lib` package name
vs `a_lib::` path), keying on `alias` instead of `symbol_name`.
**Loop budget:** one SQL pass over imports (i ≈ 10^5 production) building a
HashSet<(normalized_pkg, symbol_name)> in Rust — O(i) build, O(1) lookup
per candidate (d ≈ 10^3) → ≪ 10^6 ops.
**Wall budget:** n/a.
**Files:** `src/db/visibility.rs`, `tests/visibility.rs`.

**Verification:** same four boxes.

## Slice 3: unresolved-qualified evidence channel

**Claim:** C3.
**Oracle:** fixture source hand-read.
**Stress fixture:** `a_lib` gains `pub mod detail { pub fn helper2() {} }`
(pub chain irrelevant here) with a colliding `helper2` elsewhere; `b-app`
calls `a_lib::detail::helper2();` fully qualified with NO use statement →
ref stays unresolved with qualified text (probe's `refresh_token` shape).
Decoy: `b-app` also calls `xhelper2()` — suffix boundary must not match
`helper2`. Expected: `a_lib::detail::helper2` NOT a candidate; the decoy
changes nothing. Bug classes: ignoring `symbol_id IS NULL` rows, suffix
match without `::` boundary (jdly-proven bug class), scanning refs without
the partial index.
**Loop budget:** jdly Path B mechanics — one pass over
`idx_refs_unresolved` (u ≈ 10^6) with last-segment HashMap lookup against
candidates (d ≈ 10^3): O(u + d), never O(u × d).
**Wall budget:** n/a.
**Files:** `src/db/visibility.rs`, `tests/visibility.rs`.

**Verification:** four boxes; EXPLAIN QUERY PLAN fence may assert the
partial index (reuse jdly's `path_b_uses_partial_unresolved_index`
pattern) if the query is a separate statement.

## Slice 4: reexport exclusion + glob demotion

**Claim:** C5 (reexport ref → excluded entirely) + C6 (same-package glob
import of the candidate's module → Maybe, `glob-reexport-risk`).
**Oracle:** fixture source hand-read.
**Stress fixture:** `a_lib` root: `pub use detail2::item;` where `pub fn
item` in `mod detail2` is otherwise unreferenced → reexport-kind ref
exists (v1w8) → `item` absent from output entirely. And `pub use
inner2::*;` at root with `pub fn glob_item` in `mod inner2` → `glob_item`
listed, tier Maybe, demotion `glob-reexport-risk`. The glob import's
`source_module` is stored RELATIVE (`inner2`), while `glob_item.module_path`
is `crate::inner2` — the fixture is designed so an exact-equality match
implementation FAILS. Bug classes: reexport refs treated as ordinary
same-package refs (item would surface as candidate), glob rows skipped
because `symbol_name='*'` matches no symbol, relative-vs-absolute
source_module mismatch.
**Loop budget:** reexport check rides the existing refs SQL (kind filter);
glob check is O(g × d) with g = glob-import rows in the candidate's
package (g ≈ 10^2, d ≈ 10^3 → 10^5 worst case, within budget; suffix
comparison is O(path length)).
**Wall budget:** n/a.
**Files:** `src/db/visibility.rs`, `tests/visibility.rs`.

**Verification:** four boxes; C5 and C6 get distinct test fns.

## Slice 5: shared-name demotion

**Claim:** C4 (any same-named indexed symbol → at most Maybe;
`shared-name`).
**Oracle:** fixture source hand-read.
**Stress fixture:** three shapes in one workspace: (1) `lonely_fn`
(workspace-unique) stays Definite-eligible; (2) `pub fn dup_fn` in `a_lib`
+ `fn dup_fn` (private!) in `b-app` → Maybe (visibility of the collider
must not matter); (3) cfg-twin pair — two `pub fn twin()` declarations in
ONE `a_lib` file under `#[cfg]` gates → Maybe (COUNT(*) sees 2 rows even
though name+file collide). Expected demotions lists exact. Bug classes:
uniqueness scoped per-package or to pub-only symbols, twins collapsed by
DISTINCT/dedup before counting.
**Loop budget:** one SQL `GROUP BY name HAVING COUNT(*) > 1` over symbols
(≈ 10^6) → single pass in SQLite; Rust-side HashSet lookup O(1) × d.
**Wall budget:** n/a.
**Files:** `src/db/visibility.rs`, `tests/visibility.rs`.

**Verification:** four boxes.

## Slice 6: root-reachability helper

**Claim:** C8 (chain walk classifies buried vs reachable correctly) — the
design-time falsifier passed against the self-index; this slice makes it a
deterministic unit fence.
**Oracle:** hand-computed expectations on a synthetic modules map (and the
two self-index chains from the design transcript re-encoded as fixture
rows: `db`/`deprecated` private → unreachable; `cargo` public → reachable).
**Stress fixture:** unit tests on the helper with a hand-built modules
map: (1) all-pub chain → reachable; (2) private mid-chain → unreachable;
(3) item at crate root (`module_path == "crate"` or empty) → reachable;
(4) MISSING module row mid-chain → treated reachable (documented
conservative choice: unknown ⇒ Maybe ceiling applies — suppression-safe);
(5) duplicate module key where one row is public and one private → public
wins (any-public rule, same rationale). Bug classes: keying confusion
between a module's own path and its parent `module_path` (the C8 buggy
impl from the design), walk stopping after the first segment, missing-row
panic.
**Loop budget:** modules map build O(m) (m ≈ 10^4 production, one SQL
pass); per-candidate walk O(depth ≤ 10) → ≤ 10^4 ops total.
**Wall budget:** n/a.
**Files:** `src/db/visibility.rs` (helper + `#[cfg(test)]` unit table).

**Doc-comment contract:** the helper documents "missing module rows are
treated as reachable" — sanity-classified behavior choice asserted by unit
test (4); no runtime precondition.

**Verification:** four boxes.

## Slice 7: reachability ceiling + `workspace_closed` + facade

**Claim:** C7 (root-reachable defaults to Maybe with `root-reachable`
demotion; flag allows Definite) + C13 (single-package workspace defaults).
**Oracle:** fixture source hand-read.
**Stress fixture:** SINGLE-package workspace (also serves C13): `pub mod
api { pub fn exposed() {} }` (root-reachable, unused) and `mod internal {
pub fn buried() {} }` (unused, pub-in-private-module). Expected with
default flags: `exposed` → Maybe/`root-reachable`, `buried` → Definite.
With `workspace_closed = true`: `exposed` → Definite (demotion absent),
`buried` unchanged. Single-package topology proves the evidence sweep
doesn't vacuously exclude everything (C13's buggy impl) and that no
Definite appears for reachable items by default. Bug classes: flag
inverted, demotion left in place when flag set, ceiling applied to buried
items.
**Loop budget:** no new loops (composes S6 helper).
**Wall budget:** n/a.
**Files:** `src/db/visibility.rs` (+ `src/lib.rs` facade
`get_visibility_candidates(workspace_closed: bool)` + re-exports),
`tests/visibility.rs`.

**Verification:** four boxes.

## Slice 8: CLI command, envelope, tier rendering

**Claim:** C10 (empty envelope: summary zeros, empty array, exit 0) + C12
(tier visible in table AND JSON; fixed JSON key set incl. always-present
`demotions`).
**Oracle:** binary-level `run_cli` output vs hand-written expectations;
`serde_json` key extraction (mechanical, independent of render code).
**Stress fixture:** (a) the S7 fixture through the real binary — table
mode must show tier text and demotion reasons; JSON must have key set
{name, kind, file, line, tier, demotions} on every finding, `demotions:
[]` present-not-absent for Definite; (b) a workspace whose only pub item
is cross-package-used → zero candidates → `{summary: zeros, findings: []}`
exit 0. Bug classes: `skip_serializing_if` on demotions (key-set drift —
haw5 C10 lesson), tier rendered only in JSON, early-return skipping the
empty envelope.
**Loop budget:** render loops O(findings) — trivially within budget.
**Wall budget:** n/a.
**Files:** `src/cli/visibility_tightening.rs` (new),
`tests/visibility.rs` [+ dispatch one-liners in src/main.rs,
src/cli/mod.rs].
**Output streams:** findings table/JSON → stdout (single guarded write,
`ignore_broken_pipe`); `debug!`/`trace!` → stderr via tracing. No other
writes.

**Verification:** four boxes.

## Slice 9: determinism

**Claim:** C11 (byte-identical JSON across full re-index; deterministic
ordering incl. tie-breaks and demotion order).
**Oracle:** string equality between two runs (mechanical).
**Stress fixture:** fixture with (1) two pub symbols declared on the SAME
line (`pub struct Aa; pub struct Bb;`) so the name tie-break must fire —
a fixture with unique primary sort keys would let a missing tie-break
pass; (2) one finding carrying TWO demotions (shared-name +
root-reachable) so demotion-vec ordering is exercised. Index → run →
re-index → run; bytes must match. Bug classes: row-id-dependent ordering,
HashSet iteration order leaking into demotions.
**Loop budget:** no new loops (sorts are O(d log d), d ≈ 10^3).
**Wall budget:** n/a.
**Files:** `tests/visibility.rs` (+ ordering fixes in
`src/db/visibility.rs` if the fence catches any).

**Verification:** four boxes.

## Slice 10: q-cli Definite audit (C14, one-shot measurement)

**Claim:** C14 (zero grep-refuted Definite candidates for fig_auth AND
fig_ipc on the real workspace).
**Oracle:** probe3's grep pipeline (independent of the index); if the
workspace happens to `cargo check` locally, additionally apply
`pub(crate)` to survivors and compile (rustc oracle) — best-effort, not
gating.
**Stress fixture:** the real 43-package workspace IS the stress input
(scale + name collisions + cfg twins + re-exports all occur naturally).
Expected: every Definite survivor for both packages has zero external
qualified uses under grep; any refutation = STOP per checkpointed-build
drift rules (substrate bug or design error — do not paper over).
**Loop budget:** none (runs the shipped binary + shell).
**Wall budget:** one-shot audit; minutes, unmeasured.
**Files:** `.tethys-xoxq/definite-audit.md` (audit record; the
deterministic floor for this claim lives in S1–S7 fences per the design's
Regression-fence column).

**Verification:**
- [ ] Audit records per-symbol verdicts for both packages
- [ ] Zero refuted Definite findings (else STOP)
- [ ] probe3 re-run agrees with the binary's Definite set for fig_auth

## Slice 11: self-index review + docs (C15, manual fence — user approved)

**Claim:** C15 (hand-reviewed self-index run recorded; lib/bin merge
limitation documented with observed rate).
**Oracle:** human review against tethys sources (no automatic oracle
exists — rustc has no such lint; manual fence explicitly approved in the
design sign-off).
**Stress fixture:** n/a (review artifact); the mechanical floor is C13's
fixture (S7).
**Loop budget:** n/a. **Wall budget:** n/a.
**Files:** `.tethys-xoxq/self-index-review.md`, `AGENTS.md` (command list
mention) [+ clap help text riding in the S8 file if any wording change
falls out of review].

**Verification:**
- [ ] Every self-index finding hand-classified (true candidate /
      bin-consumed / other-false-positive, with counts)
- [ ] lib/bin merge rate documented and linked from the PR body
- [ ] AGENTS.md command list updated

## Plan Self-Review

1. **Loops:** S1 SQL single-pass O(refs+symbols); S2 O(i)+O(d) lookups;
   S3 O(u+d) via partial index (jdly-proven); S4 O(g×d) ≈ 10^5 worst;
   S5 SQL GROUP BY single pass; S6 O(m)+O(d×depth) ≈ 10^4; S8/S9 render
   and sort loops O(d log d). All ≪ 10^6 ops; no always-on phase exists
   (on-demand CLI command), so no wall budgets apply. No gaps.
2. **Fixtures:** every logic slice names its bug classes: S1
   same-package-evidence miscount / member-kind leak / `module`-visibility
   promotion; S2 refs-only rule, hyphen normalization, alias keying; S3
   unresolved-rows ignored, `::` suffix boundary; S4 reexport-as-plain-ref,
   `'*'` rows skipped, relative-vs-absolute module path; S5 per-package
   uniqueness, twin collapse; S6 key confusion, walk truncation,
   missing-row panic; S7 flag inversion, ceiling on buried items; S8
   key-set drift, tier-only-in-JSON, skipped empty envelope; S9 unfired
   tie-break, nondeterministic demotions. S10's input is real-scale data;
   S11 is a review artifact (approved manual). No happy-path-only
   fixtures. No gaps.
3. **Doc-comment preconditions:** S6's "missing module rows treated as
   reachable" — behavior choice, unit-asserted (test 4). Crate-name
   normalization assumes non-empty `arch_packages.name` — schema-enforced
   (NOT NULL UNIQUE, manifest-parsed); classified sanity-hint, gets a
   `debug_assert!` in S2. No load-bearing precondition ships
   enforcement-free. No gaps.
4. **Write targets:** stdout = findings table/JSON (data, pipe-safe with
   `ignore_broken_pipe`); stderr = tracing diagnostics; audit markdown
   files written by hand in S10/S11, not by the binary. No gaps.
5. **Tracker references:** member/module kinds → tethys-w1e9 (verified,
   filed this session); C# → tethys-41lq (verified, filed this session);
   glob-demotion narrowing trigger → tethys-pv7w (verified open);
   substrate bugs absorbed by tiering → tethys-53iv / tethys-ygjx /
   tethys-z9mr / tethys-3i35 (all verified open). No uncited deferrals.
   No gaps.

Hard-gate check: all slices have all mandatory fields; every loop has a
complexity statement; every slice has a stress fixture (or an approved
manual/measurement rationale); claim coverage C1–C15 complete; tracker
references resolve.
