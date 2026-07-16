# tethys-dvsw — budgeted plan

Design: `.tethys-dvsw/design.md` (APPROVED). Claims C1-C13.

**Step 0 (before slice 1):** capture C13 baselines — run the CURRENT
release binary (built at branch point, code-identical to main) on the
fresh self-index: `unused-imports`, `visibility-tightening`,
`untested-code`, `deprecated-callers`, `panic-points` (each `--json`
where available) into `.tethys-dvsw/baselines/`. S6 diffs the head
binary against these.

Claim→slice map: C1(S1,S2,S5) C2(S1) C3(S1) C4(S2) C5(S2) C6(S3)
C7(S6) C8(S5) C9(S2,S5) C10(S1,S6) C11(S4,S5) C12(S5) C13(S6).

---

## Slice 1: db funnel core — candidacy + ref-evidence channels

**Claim:** C1 (visibility/is_test/language-aware kind filters), C2
(resolved non-self suppression incl. speculative), C3 (unresolved
name-match, bare + `::`-suffix), C10 (db half: language-aware kinds).
**Oracle:** in-module tests over DIRECTLY SEEDED rows (symbols/refs
inserted with explicit strategy/kind/language — ground truth by
construction, independent of the resolver).
**Stress fixture:** seeded Index containing: private fn with zero refs
(reported); fn whose ONLY ref has a speculative strategy (suppressed);
fn whose only ref is self-originated `in_symbol_id = self` (REPORTED);
fn matched by unresolved bare name (suppressed); fn matched by
unresolved `crate::mod::name` (suppressed); **suffix trap**: unresolved
`crate::foobar` must NOT suppress symbol `bar` (kills naive
`ends_with(name)` matching); `#[test]`-flagged fn (excluded); public
dead fn (excluded); Rust `struct_field` (excluded) vs seeded
`struct_field` row in a `language='csharp'` file (candidate).
**Loop budget:** funnel query O(c·log r), c≈820 candidates, r≈21.7k
refs → ~12k ops. Unresolved-name set: one O(u) scan (u≈16k, uses
`idx_refs_unresolved`), Rust-side `HashSet` of full names + last
`::`-segments; membership O(1)/candidate. Total ≪10^6. No LIKE-suffix
SQL (would be O(c·u) ≈ 13M string ops — over budget, rejected).
**Wall budget:** n/a (on-demand command).
**Files:** `src/db/dead_code.rs` (new), `src/db/mod.rs` (register).

Code (advisory): `pub(crate) struct ZeroEvidenceCandidate { id, name,
qualified_name, kind, visibility, file path, line, end_line }`;
`Index::dead_code_zero_evidence() -> Result<Vec<...>>` = one SQL pass
(candidacy WHERE + NOT EXISTS resolved-non-self) then Rust-side
unresolved-name filter. Per-language kind lists as consts
(CANDIDATE_KINDS_SQL precedent in db/visibility.rs).

**Verification:**
- [ ] Unit tests pass (each channel its own test)
- [ ] Stress fixture rows produce the exact expected candidate set
- [ ] Budgets hold (no per-candidate LIKE scan)

## Slice 2: inherit markers, container liveness, entry points

**Claim:** C4 (marker suppression, external-trait-proof), C5 (container
transitive liveness incl. is_test descendants), C9 (entry-point rule),
completing C1.
**Oracle:** seeded-row tests as S1.
**Stress fixture:** marker ref with `symbol_id = NULL` (external trait)
and `in_symbol_id = method` — method suppressed (kills the
`symbol_id`-join bug); struct → child method with a ref (struct
suppressed); **depth-2 chain** grandparent→parent→child where only the
child is live (grandparent suppressed — kills direct-children-only);
container whose only descendant is `is_test=1` (suppressed); dead
struct with dead children (reported); `main` in `src/main.rs` and
`crates/x/src/bin/tool.rs` (excluded) vs private `fn main` in a LIB
file (candidate — kills path-blind name matching); Rust method named
`Main` (candidate — kills language-blind C# rule); C# `Main` method
(excluded).
**Loop budget:** liveness = one upward parent-chain walk from every
live/is_test symbol: O(s·depth) ≈ 2669×3 ≈ 8k. Entry-point check O(c)
pure string tests on workspace-relative paths.
**Wall budget:** n/a.
**Files:** `src/db/dead_code.rs` (extend; `is_entry_point` as pure fn
with unit tests).

**Verification:**
- [ ] Unit tests pass per channel
- [ ] Stress fixtures produce expected sets
- [ ] Budgets hold

## Slice 3: textual tier scan + report types + facade

**Claim:** C6 (textual demotion; span exclusion; word boundaries).
**Oracle:** `probe3.py` — extend probe2 with the three approved
refinements (self-ref exclusion, span exclusion, entry-point rule) in
SQL/python; predict and explain every divergence from probe2's 37-Maybe
list (expected: minus 2 entry-point `main`s; plus any self-ref-only
symbols). Binary-vs-probe3 exact match runs at S4 (needs CLI).
**Stress fixture:** candidate mentioned only in ANOTHER file's macro
arg → Maybe; recursive fn whose name appears only in its own span →
Definite (span exclusion); **substring trap** `foo` vs `foobar`
mention → Definite (word boundary); mention in a comment → Maybe
(textual is text; documented); candidate with `end_line = NULL` →
line-only exclusion fallback (safe direction: extra hits → Maybe).
**Loop budget:** one pass over indexed files: O(total source bytes)
tokenization (self-index ≈ 2.5MB) + O(tokens) hash lookups (≈300k).
Justification for scale: one-shot command, linear in corpus, same cost
class as `tethys index` parsing the identical corpus.
**Wall budget:** `dead-code` end-to-end on self-index < 500ms (measure;
expect ≪).
**Files:** `src/dead_code.rs` (new: scan + `DeadCodeReport`/`Finding`/
`Tier` types + module docs incl. the FP-source list tethys-9l27,
tethys-0nar, tethys-7dqj, tethys-ewa7, tethys-wbrh, tethys-i09d,
tethys-0aqj and the m7zm test-only-liveness note), `src/lib.rs`
(`find_dead_code(&self, limit: Option<usize>)` + re-exports).

Doc-comment preconditions: `end_line` nullable → runtime fallback (not
assert — safe-direction degrade, documented); name non-empty → schema
NOT NULL + extractor guarantee, `debug_assert!`.

**Verification:**
- [ ] Unit tests pass
- [ ] Stress fixtures produce expected tiers
- [ ] probe3 runs; divergence from probe2 fully explained
- [ ] Wall budget measured

## Slice 4: CLI subcommand + dispatch + docs

**Claim:** C11 (surface: `dead-code [--limit N] [--json]`, sort, limit
semantics, empty-clean posture).
**Oracle:** probe3-vs-binary EXACT match on the fresh self-index
(names, files, tiers) — the prove-it oracle at full strength.
**Stress fixture:** self-index run; `--limit 0` (empty findings, full
summary); `--json | python -c json.load` round-trip.
**Loop budget:** display O(f) over findings.
**Wall budget:** covered by S3 measurement.
**Files:** `src/cli/dead_code.rs` (new), `src/main.rs` + `src/cli/mod.rs`
(registration), `AGENTS.md` command list + `CONTEXT.md` glossary entry
(doc-maintenance rule: same commit). >2 files justified: clap
registration is unavoidably 3-file; docs ride per repo rule.

Output streams: table + JSON → stdout (pipeable data); scan warnings
(unreadable file) → tracing/stderr. JSON envelope mirrors
untested-code's `{findings, summary}`; workspace-wide envelope
convergence is tracked at tethys-zwaz (not this PR's scope).

**Verification:**
- [ ] Unit/existing tests pass
- [ ] probe3 == binary on self-index, item-exact
- [ ] JSON round-trips; --limit 0 and no-limit behave per design

## Slice 5: Rust integration fences at the ratified seam

**Claim:** C1 (end-to-end), C8 (seeded dead Definite), C9, C11, C12.
**Oracle:** `cargo check`-style ground truth embedded in fixture
construction: every seeded-dead item is one rustc's `dead_code` lint
would flag (verified once by hand during authoring); fixture indexes
built from scratch per test (never ambient DB).
**Stress fixture (tests/dead_code.rs, local `workspace_with_files`
copy — consolidation tracked at tethys-dzn8):** C1 mega-fixture (pub
dead fn, `#[test]` fn, struct_field, module decl, `src/main.rs` main —
none reported; private dead fn reported); C8 seeded workspace (dead fn,
dead struct, dead const, RECURSIVE dead fn → all Definite; decoy
mentioned in another file's macro → Maybe); C9 bin-only crate with
unmentioned main (absent); C11 sort fixture with two files AND a
same-file line tie-break, `--limit 1`, zero-candidate (all-pub)
workspace → empty findings + exit 0; C12 run-twice byte-diff.
**Loop budget:** test-time only.
**Wall budget:** n/a.
**Files:** `tests/dead_code.rs` (new).

**Verification:**
- [ ] All fences pass; each fence fails under its named buggy impl
      (spot-check C2's by predicate inversion during authoring)
- [ ] Fixtures build their own indexes

## Slice 6: C# fence, self-index CI fence, C13 audit

**Claim:** C10 (C# funnel), C7 (self-index zero Definite — the PRD
fence), C13 (no existing analysis changes).
**Oracle:** C7: rustc warning-free compilation of this repo (external
ground truth); C13: baselines captured at step 0 from the pre-change
binary.
**Stress fixture:** C# workspace fixture — internal class with unused
private method (Definite), called method (absent), nested inner class
whose method is used (BOTH inner and outer suppressed — depth-2
recursion in production shape), unused private property (reported),
`Main` (absent); mixed-workspace variant with one Rust file to pin
cross-language textual scan. C7: index CARGO_MANIFEST_DIR workspace in
a test, assert zero Definite. C13: head-binary outputs diffed against
`.tethys-dvsw/baselines/` — byte-identical required, result recorded in
`.tethys-dvsw/audit.md`.
**Loop budget:** C7 test indexes the repo once (~0.5s, idxperf-golden
precedent).
**Wall budget:** C7 test < 30s in CI (dominated by index build).
**Files:** `tests/dead_code.rs` (extend), `.tethys-dvsw/audit.md`.

**Verification:**
- [ ] C# fence passes; C7 self-index fence passes
- [ ] C13 diff empty, recorded
- [ ] Full gate: nextest, clippy pedantic -D warnings, fmt, doctests

---

## Plan Self-Review

1. **Loops:** funnel SQL O(c·log r) ~12k; unresolved-set O(u) ~16k;
   liveness walk O(s·depth) ~8k; textual scan O(bytes)+O(tokens) ~10^6
   one-shot justified; display O(f). No unbounded loop; no gap.
2. **Fixtures:** every slice's fixture names its kill-target bug
   (suffix trap, self-ref, symbol_id-join, direct-children-only,
   path-blind main, language-blind Main, substring boundary, NULL
   end_line, limit-before-sort via tie-break, empty input, run-twice).
   No happy-path-only fixture.
3. **Preconditions:** `end_line` NULL → runtime fallback (safe
   degrade, documented); non-empty name → `debug_assert!` (schema
   enforces). No unenforced documented precondition.
4. **Write targets:** stdout = findings/JSON (data); stderr/tracing =
   scan warnings (diagnostic). No other writes.
5. **Tracker refs:** tethys-dzn8, tethys-zwaz, tethys-3b06, tethys-m7zm,
   tethys-o4re, tethys-9l27, tethys-0nar, tethys-7dqj, tethys-ewa7,
   tethys-wbrh, tethys-i09d, tethys-0aqj — all verified existing this
   session (ready-list / `rivets show`). No uncited deferral.

Claim coverage matches the design's C1-C13 (map at top). No gaps.
