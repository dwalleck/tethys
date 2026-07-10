# tethys-53iv corpus audit (checkpointed-build slice 8, 2026-07-09)

Diffs against the four pre-feature baselines committed at plan time, plus
the repro pre-state (`probe1-output.txt`). Every changed row adjudicated.

## Headline: the ticket repro (probe1 re-run, post-build)

| Check | Pre | Post |
|---|---|---|
| `x.unwrap()` (annotated `Option`) | bound `Thing::unwrap` (`same_file`) | **unresolved, `reference_name = Option::unwrap`** (AC1) |
| call edges into `Thing::unwrap` | `use_external` + `use_internal` | **`use_internal` only** (fabricated edge dead) |
| panic-points | 0 (false negative) | **1: `src/lib.rs:7 in use_external()`** (AC2) |
| `t.unwrap()` (underivable) | bound (`same_file`) | **bound `Thing::unwrap` (`unique_workspace`)** (AC3, target unchanged) |

## Freeze checks

- **Non-call refs** (7038 rows): diff vs baseline **EMPTY** (C10).
- **C# corpus** (4997 rows, Tethys.Results): diff **EMPTY** (C10/D5).
- **`SELECT DISTINCT kind FROM refs`**: unchanged set (`type, call,
  construct, macro, value, reexport`) — no new wire string (C11).
- **Reindex idempotency**: second index run over the self-corpus produced a
  bit-identical call-ref dump (C12).

## Call-ref diff (11659 baseline rows; 896 changed), fully adjudicated

| n | Category | Adjudication |
|---|---|---|
| 509 | unresolved → unresolved, name bare → qualified (`m` → `T::m`) | Derived receivers on calls that never resolved; Path-B/panic-points visibility improves. Benign relabel |
| 101 | `same_file` → `unique_workspace`, same target | The D1-approved band demotion (name-only binds are speculative; tethys-k543's LSP re-verify population) |
| 88 | `same_file` → declined | ALL are workspace-ambiguous names (mechanically: the name arms are unique-or-decline, and a same-file candidate existed, so decline ⇒ ≥2 candidates). Dominant shape: the `Index::m` / `Tethys::m` facade twins, receivers = underivable test-mod locals. D1/D3-approved trade; recovery is k543's tier |
| 76 | `same_file` → `qualified_exact`, same target | `self`/annotated receivers upgraded to type-anchored binds |
| 1 | `same_file` → `qualified_exact`, **target changed** | `types.rs:1224` (`!self.is_empty()`): the old last-wins bind hit the WRONG same-file twin (`ReachabilityResult::is_empty`); derivation binds the enclosing impl's `StalenessReport::is_empty`. A hidden phantom the probe's hand-read had misclassified as true — fixed |
| 51 + 10 | `glob_import`/`unique_workspace` → `explicit_import`, same target | Derived receiver paths (`Tethys::m`, `FileId::as_i64`) whose first segment is explicitly imported route through the import-corroborated arm — type- AND import-anchored |
| 21 | `unique_workspace` → `qualified_exact`, same target | Derivation upgrades |
| 20 + 19 | unresolved → `qualified_exact`/`explicit_import` (**new binds**) | Previously-unresolvable calls now bound via derivation — e.g. the multiline `self\n.get_symbol_by_id(...)` chains in `db/graph.rs` (facade-twin names that used to decline). Samples verified correct |

**Phantom checks (probe3's list):** zero `is_empty`/`as_str` binds with
non-self receivers remain. Remaining binds: `StalenessReport::is_empty` ×1
and `ReachabilityResult::is_empty` ×1 (each impl's own self-call,
`qualified_exact`) and `Language::as_str` ×3 (annotated receivers via
`explicit_import`) — all verified true.

## Call edges

−77 / +38 (3023 → 2984): removals = the 88 ambiguous declines' edges plus
the phantom class; additions = the 39 new derived binds (previously
invisible true edges, e.g. `Index::get_symbol_by_id` callers). Consistent
with the ref-level adjudication; no unexplained rows.

## Corrections to earlier records

- probe3/findings.md called `types.rs:1224` "1 true bind of 8" — the audit
  shows it bound the wrong twin; the honest score was 8/8 wrong (7 external
  phantoms + 1 wrong-twin). Derivation fixed all 8.
- The design's C3 predicted `unique_workspace` for the repro's AC3 bind —
  correct on the repro; on richer corpora the same tier also surfaces as
  `same_crate`. Fixtures assert tier membership, not one label.

## Post-review-fix delta (bot findings, 2026-07-09)

Three verified gemini findings (nested-fn map leak; `let_condition`
scrutinee over-poisoning; `mut_pattern` losing derivation) applied after
the first audit. Corpus re-run: non-call refs and C# still frozen; 103
call-ref rows shifted — 101 bare→qualified `reference_name` upgrades on
still-unresolved refs (`Path::join`, `HashMap::entry`, `Option::is_some`
— mut/if-let receivers now derive and decline with type-anchored names)
and 2 same-target `unique_workspace` → `qualified_exact` upgrades. Zero
bind-target changes.
