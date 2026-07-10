# tethys-53iv budgeted plan — receiver-gated method-call resolution

2026-07-09. Implements the approved `.tethys-53iv/design.md` (D1: targets-
unchanged AC3, band shifts accepted; D2: annotations-only derivation — both
confirmed by dwalleck at the design pause). Pre-feature baselines committed
at plan time: `baseline-self-call-refs.txt` (11659),
`baseline-self-noncall-refs.txt` (7038), `baseline-self-call-edges.txt`
(3023), `baseline-csharp-all-refs.txt` (4997). Repro pre-state:
`probe1-output.txt`.

Every slice gate: `cargo nextest run`, clippy pedantic `-D warnings`,
`cargo fmt --check`, doctests — real exit codes. **Resolver carve-out
applies** (AGENTS.md): slices edit Pass-1/extraction, so grep + the
committed probes are the oracle, never tethys-on-itself.

## Slice 1: `ExtractedReferenceKind::Method` + Pass-1 routing (inert)

**Claim:** C11 (no DB surface change) + the routing half of C7.
**Oracle:** existing suite (886 tests) — the slice's claim IS behavioral
inertness until an emitter exists; plus `SELECT DISTINCT kind FROM refs`
unchanged on a fixture.
**Stress fixture:** lives in Slice 2 (`unknown_receiver_skips_pass1_...`)
which exercises this routing the moment Method refs exist — noted
explicitly per the combine-with-next rule; Slice 1's own check is the full
suite passing bit-identical (any behavior change falsifies inertness).
**Loop budget:** none (one `match` branch in the insert loop, O(1)/ref).
**Files:** `src/languages/common.rs`, `src/db/files.rs`

`Method` variant documented as "method call whose callee is a
`field_expression` — never Pass-1 bare-name bound (tethys-53iv), stored as
`'call'` via `to_db_kind`". files.rs: Method refs skip both `name_to_id`
lookups (bare AND qualified — qualified Pass-1 keys are symbol NAMES, so a
`T::m` lookup there is vacuous anyway; Pass 2 owns qualified matching).

**Verification:**
- [ ] Unit tests pass (suite bit-identical)
- [ ] Stress fixture: deferred to Slice 2 (noted)
- [ ] Oracle agrees (no kind string changes)
- [ ] Budgets hold

## Slice 2: rust.rs emits `Method` for field-expression callees

**Claim:** C7 (Pass-1 skip → Pass-2 unique-or-decline), C8 (plain fn calls
untouched), C3's mechanism (`t.unwrap()` → `unique_workspace`).
**Oracle:** probe1 re-run (section A: line-11 bind becomes
`unique_workspace`, same target) + grep-derived fixture expectations.
**Stress fixture:** one file with `impl A { fn probe(&self){} }` + an
unknown-receiver `x.probe()` in the SAME file, plus a second file with
`impl B { fn probe(&self){} }` (ambiguous twin) → the call must NOT bind
(`same_file` bind = Pass-1 leak; any bind = ambiguity leak). Control in
the same fixture: `fn free()` + `free()` still binds `same_file` (C8),
and a workspace-UNIQUE method + unknown receiver binds `unique_workspace`
(C3's shape). Bug classes: routing not consulted by the emitter; skip
accidentally applied to identifier callees.
**Loop budget:** none new (existing traversal).
**Files:** `src/languages/rust.rs`, `tests/method_calls.rs` (new)

Expected collateral: existing tests pinning `same_file` for method calls
(e.g. `tests/graph.rs` intra-file, `tests/strategy.rs`) fail HERE by
design (D1) — each updated deliberately with a comment citing 53iv, and
the full list recorded in the slice commit message. Binding TARGETS must
not change in any updated test; only labels/edges explicitly adjudicated.

**Verification:**
- [ ] Unit tests pass (incl. deliberately-updated label pins)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees (probe1 §A/§B/§D shapes)
- [ ] Budgets hold

## Slice 3: panic-points last-segment matching

**Claim:** D4 (the AC2 enabler).
**Oracle:** SQL fixture rows + grep of the fixture source.
**Stress fixture:** refs with `reference_name` ∈ {`unwrap`,
`Option::unwrap`, `a::b::expect`} must ALL report; decoys
`T::not_unwrap`, `unwrap_or`, `expected` must NOT (bug classes: suffix
match without the `::` anchor; `LIKE` matching mid-name).
**Loop budget:** none (SQL predicate widened).
**Files:** `src/db/panic_points.rs` (query + inline unit tests)

**Verification:**
- [ ] Unit tests pass (`panic_points_matches_qualified_last_segment`)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees
- [ ] Budgets hold

## Slice 4: self-receiver derivation

**Claim:** C4 (`self.m()` → `T::m` via `qualified_exact`).
**Oracle:** hand-derived targets per fixture (rustc semantics); probe3's
true-bind site class (`!self.is_empty()`) re-checked in Slice 8 audit.
**Stress fixture:** ONE file, two inherent impls (`impl A`, `impl B`) each
with method `m` and a `self.m()` call inside each — each call must bind
its OWN impl's `A::m`/`B::m` (bug class: file-global instead of
enclosing-impl attribution); a trait impl (`impl Run for C`) whose
`self.go()` binds `C::go`; a free fn with no impl context leaves an
identifier receiver unknown (no panic, no derivation).
**Loop budget:** enclosing-impl lookup is O(ancestor depth) per method
call, threaded during the existing walk — O(AST nodes) total, ~10^5.
**Files:** `src/languages/rust.rs`

Derivation target = the impl's `type` field, generics stripped to base
name, path types by last segment (matches how `qualified_name` is built
for the symbols themselves — verified premise).

**Verification:**
- [ ] Unit tests pass (`self_receiver_binds_qualified_same_file`, trait + nested variants)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees
- [ ] Budgets hold

## Slice 5: local type map (params + single-binding lets)

**Claim:** C5's substrate (the map itself, unit-tested in isolation).
**Oracle:** hand-written expected maps per snippet (rustc name-resolution
semantics, trivially derivable for these shapes).
**Stress fixture:** map-builder unit table: typed param `x: &Thing` →
`Thing`; `let y: lib::Widget` → `Widget`; `let z: Gauge<f64>` → `Gauge`;
SHADOWED `let s: Thing; let s = other();` → absent (bug class: last-wins
instead of unique-only); closure `|s: String|` param counts as a second
binding of `s` → absent; destructured `(a, b): (T, U)` → absent (no
per-identifier annotation); `let x: Vec<i32>` present as `Vec` (external
names stay IN the map — externality is decided at bind time, not here).
**Loop budget:** one pass over each fn body's pattern/let nodes —
O(AST nodes) total, ~10^5 at corpus scale.
**Files:** `src/languages/rust.rs` (builder + inline unit tests)

Shape mirrors ygjx's `collect_local_bindings`, upgraded from a set to
`ident → Option<TypeBase>` where a second binding stores `None`
(poisoned). Doc contract "call once per fn body" is a sanity hint
(`debug_assert!` on fn-item node kind).

**Verification:**
- [ ] Unit tests pass (`local_type_map_*` table)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees
- [ ] Budgets hold

## Slice 6: identifier-receiver derivation wired

**Claim:** C5 (annotated receivers bind qualified), C6 (known-external
annotated receivers decline).
**Oracle:** hand-derived per-shape targets; repro semantics for the
external case.
**Stress fixture:** integration matrix in `tests/method_calls.rs`: each
annotation shape from Slice 5 driving a real bind to an in-crate
`Widget::m` (same- AND cross-file); the ADVERSARIAL twin — same-named
in-crate method on another type — must not steal any of them; shadowed
receiver falls back to unknown (binds `unique_workspace` when unique —
observable difference from the qualified strategy); `let v: Vec<i32>;
v.contains(x)` with in-crate `contains` produces NO bind (C6; bug class:
externality check binding "any method named m" instead of `Vec::contains`
exactly).
**Loop budget:** map lookup O(1) per method call.
**Files:** `src/languages/rust.rs`, `tests/method_calls.rs`

**Verification:**
- [ ] Unit tests pass (`annotated_receiver_matrix`)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees
- [ ] Budgets hold

## Slice 7: repro end-to-end fences + deprecated-callers fence

**Claim:** C1, C2, C3 (the three ACs as permanent tests), C13's new fence.
**Oracle:** the ticket's rustc-semantics repro, hand-derived (already
recorded pre-feature in `probe1-output.txt`).
**Stress fixture:** the repro fixture verbatim (`Thing::unwrap` +
annotated `Option`) — asserts: line-7 ref unresolved with
`reference_name = 'Option::unwrap'`; panic-points == exactly
`src/lib.rs:7`; line-11 ref bound to `Thing::unwrap` with
`unique_workspace`; call edge set == {`use_internal → Thing::unwrap`}.
Plus: `#[deprecated] impl method` called through an annotated EXTERNAL
receiver stays unresolved and surfaces as a deprecated-callers Path-B
Maybe site (bug class: qualified decline dropping the name Path B needs).
**Loop budget:** none (tests only).
**Files:** `tests/method_calls.rs`, `tests/deprecated_callers.rs`

**Verification:**
- [ ] Unit tests pass (`annotated_external_receiver_does_not_bind`, `panic_points_sees_annotated_external_unwrap`, `underived_receiver_still_resolves_unique`, `deprecated_method_declined_call_is_path_b_site`)
- [ ] Stress fixture produces expected outcome
- [ ] Oracle agrees (probe1 expected end-state)
- [ ] Budgets hold

## Slice 8: corpus audit → audit.md

**Claim:** C9 (phantoms gone, diffs adjudicated), C10 (non-method + C#
freeze), C12 (reindex idempotency), C11 re-check (kind strings).
**Oracle:** the four committed pre-feature baselines + probe3's phantom
list (7 `is_empty` sites) + probe1 pre-state.
**Stress fixture:** the production corpora themselves; written-first
expectations: zero remaining `is_empty`/`as_str` binds with non-self
receivers; `types.rs:1224` true bind present as `qualified_exact`;
`baseline-self-noncall-refs.txt` and `baseline-csharp-all-refs.txt`
diffs EMPTY; call-ref diff nonempty but 100% adjudicated into
(phantom-removed | same-target-relabel | declined-ambiguous |
qualified-upgrade); reindex diff empty. STOP-on-drift on any
unadjudicated row.
**Loop budget:** audit SQL O(refs) ≈ 1.2×10^4 — trivial.
**Files:** `.tethys-53iv/audit.md` (+ audit queries recorded inline)

**Verification:**
- [ ] Unit tests pass (full suite — C10's fence)
- [ ] Stress fixture produces expected outcome (all enumerations close)
- [ ] Oracle agrees (baseline diffs as written)
- [ ] Budgets hold

## Slice 9: docs + final gates

**Claim:** repo-truth hygiene: AGENTS.md dogfood note (`--exclude-
speculative` semantics shift: name-only method binds now live in the
speculative band) and any stale "name-only resolution" gotcha text;
CONTEXT.md untouched unless vocabulary drifted (expected: none).
**Oracle:** grep for stale claims (`same_file` method-call examples,
53iv-as-open-gap phrasing) — zero hits after the slice.
**Stress fixture:** n/a — docs + full-gate slice; the check is the grep
sweep plus the final integration run (every fence + every probe re-run).
**Loop budget:** none.
**Files:** `AGENTS.md` (+ `CLAUDE.md` only if a session gotcha earned it)

**Verification:**
- [ ] Unit tests pass (full suite, doctests)
- [ ] Stress fixture n/a (grep sweep is the check)
- [ ] Oracle agrees (no stale text)
- [ ] Budgets hold

## Plan Self-Review

1. **Loops:** three new loop sites — Pass-1 routing branch (O(1)/ref),
   enclosing-impl ancestor walk (O(depth), summed O(AST)), local type map
   build (O(AST)); all ~10^5 at corpus scale, no always-on phase. No
   `O(?)`.
2. **Fixtures:** every logic slice names its bug class — Pass-1 leak +
   ambiguity leak + identifier-callee overreach (S2), suffix-anchor abuse
   (S3), file-global impl attribution (S4), shadow last-wins + closure
   rebinding + destructuring (S5), adversarial same-named twin + external
   exact-match (S6), Path-B name loss (S7), unadjudicated corpus drift
   (S8). S1's fixture is explicitly deferred to S2 (combine rule) with
   inertness as its own check; S9 is docs-only.
3. **Doc-comment preconditions:** one new contract — the type-map
   builder's "call on a fn item" (sanity hint → `debug_assert!`); the
   Method-variant doc states routing behavior enforced by the files.rs
   branch itself. No load-bearing precondition lacking a runtime check.
4. **Write targets:** no new output streams; audit numbers to committed
   markdown; tests via harness.
5. **Tracker references:** tethys-k543 (LSP tier), tethys-0aqj (C# facet /
   kind-aware binding), tethys-bvgb (duplicate qualified names),
   tethys-9l27 (macro-context refs), tethys-z9mr (adjacent) — all shown to
   exist with covering descriptions during design; no new deferrals
   introduced by this plan.

Claim coverage: C1(S7,S8) C2(S3,S7) C3(S2,S7) C4(S4) C5(S5,S6) C6(S6)
C7(S1,S2) C8(S2) C9(S8) C10(S8) C11(S1,S8) C12(S8) C13(S7) — all 13.
