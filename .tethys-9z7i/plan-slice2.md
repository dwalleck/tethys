# tethys-9z7i slice 2 plan (budgeted-plan, 2026-07-04)

Design: `.tethys-9z7i/design-slice2.md` (approved, Option C). Build slices
are B1–B8 (the epic's own "slices" are deliverables; B-numbers avoid the
collision). Claim → slice: C8,enum→B1; C7→B2; C2→B3; C1,C4,C9→B4;
C3,C5→B5; C6+helper→B6; C11→B7; C10,C13→B8. C12 passed at design time
(existing refs_named fences hold throughout).

Conventions: ship §Conventions bind (real-exit-code gates, commitlint
single scope, one commit per B-slice). New integration fences live in
`tests/strategy.rs`. No new loops anywhere — every change rides existing
per-ref passes; budgets state the per-ref delta.

## B1: schema column + ResolutionStrategy enum

**Claim:** C8 fence half (fresh DB has the column) + the enum the rest builds on.
**Oracle:** `PRAGMA table_info(refs)` via rusqlite in schema_tests (raw SQL, not analysis code).
**Stress fixture:** open a fresh DB in a tempdir; assert `strategy` present AND nullable AND the column count matches the CREATE TABLE (catches a stray second ALTER-style addition). Enum: `as_str()` round-trip table for all 9 variants pinning the ADR spellings (kills a variant/string mismatch — serde isn't used here, the string IS the wire format).
**Loop budget:** none (schema text + enum).
**Wall budget:** n/a.
**Files:** `src/db/schema.rs` (column + schema_tests), `src/types.rs` (enum + as_str) [+ lib.rs re-export line].

**Verification:** unit tests pass; fixture exact; no oracle drift possible yet; budgets n/a.

## B2: open-guard — outdated schema errors clearly (C7)

**Claim:** C7 — opening a pre-column DB fails with an error naming `tethys index --rebuild`; fresh DBs open fine.
**Oracle:** error string + `PRAGMA table_info` via raw sqlite on a hand-built pre-column DB (probe1.sh(a) shape).
**Stress fixture:** schema_tests builds a db via raw SQL with the OLD refs CREATE TABLE (no strategy), then `Index::open` → expect Err whose message contains "--rebuild"; open a fresh db → Ok; ALSO a db missing the refs table entirely (brand-new path) must still open fine — the guard must only fire when refs EXISTS without the column (kills an inverted or over-eager check).
**Loop budget:** one PRAGMA query at open — O(columns) ≈ 11, once per process.
**Wall budget:** open path: << 1ms added.
**Files:** `src/db/mod.rs` (guard in `open`), `src/db/schema.rs` (schema_tests).
**Doc contract:** the guard's doc states "fires only when refs exists without strategy" — load-bearing (silent wrong direction would brick fresh DBs); enforced by the fixture's three cases, runtime behavior IS the check.

**Verification:** four boxes.

## B3: Pass 1 stamps same_file (C2)

**Claim:** C2 — every insert-time bind stamps `same_file`; label count == same-file spatial JOIN count at fixture scale.
**Oracle:** the spatial join (`refs r JOIN symbols s ON r.symbol_id=s.id AND r.file_id=s.file_id`) — independent of the label (probe's 1564==1564 reconciliation, in-DB).
**Stress fixture:** new `tests/strategy.rs`: fixture file with (a) a same-file call bound by the last-wins map, (b) an unresolved cross-file ref (NULL until B4 — assert NULL here), (c) a macro invocation bound via the macro map (must also stamp same_file — kills a stamp wired only into the general map arm).
**Loop budget:** zero new loops — one more bind on the existing per-ref INSERT (refs ≈ 10^7 production: +1 param each, no measurable term).
**Wall budget:** n/a (covered by C13's measurement).
**Files:** `src/db/files.rs`, `tests/strategy.rs` (new).

**Verification:** four boxes.

## B4: Pass 2 + LSP threading through the widened seam (C1, C4, C9)

**Claim:** C1 (NULL ⇔ unresolved, both directions, multi-round), C4 (memo fans strategy to duplicates), C9 (reexport refs stamp).
**Oracle:** raw SQL counts for C1; RUST_LOG trace events vs DB rows for C4 (one arm event, N stamped rows); trace + readback for C9.
**Stress fixture:** extend `tests/strategy.rs`: (a) forward-reference fixture forcing ≥2 Pass-2 rounds, then assert zero `symbol_id NOT NULL AND strategy IS NULL` rows AND zero `symbol_id IS NULL AND strategy IS NOT NULL` rows (two separate asserts — direction-distinct); (b) one file calling an imported fn 3× → all three rows `explicit_import` (kills memo caching SymbolId only); (c) `pub use inner::item` with unique name → reexport-kind row stamped non-NULL.
**Loop budget:** memo value widens `Option<SymbolId>` → `Option<(SymbolId, ResolutionStrategy)>` — Copy-able 1-byte enum, same O(unique names per file) memo, no new loops; resolutions vec gains a field, same O(resolved).
**Wall budget:** n/a (C13).
**Files:** `src/resolve.rs` (arms return tuples; memo; resolutions; the :1019 LSP call site), `src/db/references.rs` (RESOLVE_REFERENCE_SQL + apply_resolutions + resolve_reference signatures). Biggest slice — mechanical signature threading; if it exceeds ~40 minutes, stop and reassess per checkpointed-build.
**Doc contract:** `resolve_reference`'s doc gains "strategy records which mechanism bound it" — sanity-level (callers are in-crate); no runtime check beyond the type system (the enum is total).

**Verification:** four boxes (oracle = trace/DB comparisons above).

## B5: per-arm labels + macro bypass fences (C3, C5)

**Claim:** C3 — each of the 7 Pass-2 labels stamps from its own arm; C5 — macro refs (memo bypass) stamp correctly.
**Oracle:** RUST_LOG trace events name the arm per (file,name) — matched against per-ref DB labels.
**Stress fixture:** mixed workspace firing every arm: explicit import; Rust glob (`use m::*` + call); C# using (import_union); qualified exact (`Widget::method` shape); same-crate simple name; unique-workspace name (cross-crate, no import, unique); `mod::fn()` qualified_module_fallback shape; PLUS `write!()` macro + `write()` fn in ONE file (kills memo cross-contamination and the bypass missing the stamp — the macro row and the fn row must carry independent strategies). Expected label per ref written in the test BEFORE the build reaches B5 (they are; see asserts). Arm-collapse kill: the three fallback sub-labels must all appear — an implementation returning one label for the whole fallback fails three distinct asserts.
**Loop budget:** none (fence-only slice).
**Wall budget:** n/a.
**Files:** `tests/strategy.rs`.

**Verification:** four boxes.

## B6: test-helper widening + LSP fence (C6)

**Claim:** C6 — the single-row LSP path stamps `lsp` through the same widened SQL.
**Oracle:** SQL readback after driving `resolve_reference(ref, sym, Lsp)` directly (no LSP server needed — the seam is what's under test; Pass-3 wiring to it was updated in B4 and is compile-checked).
**Stress fixture:** via the widened `InsertReferenceParams` (gains `strategy: Option<&str>`): insert an unresolved ref, resolve it with `Lsp`, read back `strategy='lsp'` AND `reference_name IS NULL` (the seam's existing null-out must still happen — kills a forked single-row SQL).
**Loop budget:** none.
**Wall budget:** n/a.
**Files:** `src/db/references.rs` (helper field + unit test), test fixture construction sites the field addition breaks (`src/db/panic_points.rs` tests + any grep hits — mechanical, impact-listed in the commit).

**Verification:** four boxes.

## B7: golden fence extension (C11)

**Claim:** C11 — the canonical dump gains the strategy field; expected literals updated so the diff vs pre-change dumps is strategy-only; batch and streaming variants both pin values (this IS the epic's "fence pins strategy values on a fixture crate" AC).
**Oracle:** the dump text itself diffed pre/post (existing golden machinery); the fixture's known shapes (same-file call, cross-file import call, duplicated fn name) make expected strategies derivable by hand — write them into the literals BEFORE running (RED expected: old literals fail with exactly the strategy field differing).
**Stress fixture:** the existing golden fixture already contains the name-collision (`caller` duplicated across files) and cross-file shapes; the extension must show DIFFERENT strategies across rows (`same_file` vs an import/fallback label) — a dump printing one constant label for all rows fails.
**Loop budget:** none (test-only).
**Wall budget:** n/a.
**Files:** `tests/idxperf_golden.rs`.

**Verification:** four boxes.

## B8: audits — C10 byte-identical outputs, C13 perf (manual, approved)

**Claim:** C10 — deprecated-callers and visibility-tightening `--json`/human outputs byte-identical old-vs-new binary on the same fixture; C13 — self-index wall-clock delta ≤5%.
**Oracle:** byte diff between binaries built from main vs branch (mechanical); hyperfine ≥10 runs (external tool).
**Stress fixture:** the real self-index (--rebuild each side) + the golden fixture workspace; any diff or >5% delta = STOP per checkpointed-build.
**Loop budget:** n/a (measurement).
**Wall budget:** the measurement itself ~5 min.
**Files:** `.tethys-9z7i/perf-slice2.md` (audit record; C10's deterministic floor = the existing CLI fences that pinned outputs all along; C13 fence = manual, approved at the design pause).

**Verification:** recorded audit; zero output diffs; delta ≤5%; existing suite green.

## Plan Self-Review

1. **Loops:** none introduced anywhere; per-ref deltas are single extra
   binds/fields on existing passes (stated in B3/B4); B2 adds one PRAGMA
   per open. No gaps.
2. **Fixtures:** B1 column-count + round-trip table (string drift); B2
   three-way guard cases (inverted/over-eager check); B3 macro-map stamp
   (partial wiring); B4 direction-split NULL asserts, 3×-duplicate
   fan-out, reexport kind; B5 all-seven-labels incl. fallback collapse
   kill and write!/write collision; B6 forked-SQL kill via
   reference_name null-out assert; B7 multi-label dump requirement; B8
   real-scale audit. No happy-path-only fixtures. No gaps.
3. **Doc contracts:** B2's guard condition (load-bearing — enforced by
   the runtime check itself + three-case fixture); B4's strategy param
   doc (sanity — type-system-total). No unenforced contracts. No gaps.
4. **Write targets:** no new program output; trace/debug diagnostics
   stay on the tracing layer (stderr); audit markdowns written by hand.
   No gaps.
5. **Tracker references:** tethys-xvlw (closes with this PR — verified
   open, updated this session); tethys-9z7i slices 3/4 (in-epic);
   tethys-q8qw, tethys-53iv/msn0/3i35 (verified open, referenced as
   non-goals). No uncited deferrals. No gaps.

Hard gate: all slices have mandatory fields; claim coverage C1–C13
complete (C12 design-time-passed with existing fences); fixtures
adversarial; tracker clean.
