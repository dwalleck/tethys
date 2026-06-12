# Review-feedback decisions — PR #3 (usgf)

Assessment of the `/pr-review-toolkit:review-pr` multi-agent review (6 reviewers:
code-reviewer, pr-test-analyzer, silent-failure-hunter, type-design-analyzer,
comment-analyzer, rust-code-reviewer) under the `assessing-review-feedback`
discipline: every finding verified against the real `usgf` code before any
change. 8 accepted, 5 rejected/deferred. Deferrals carry tracker IDs.

| # | Finding | Reviewer(s) | Category | Verified | Decision | Note |
|---|---|---|---|---|---|---|
| C1 | Ambiguity `debug!` lost in refactor; union arm declines silently while two siblings (`symbols.rs:284,316`) still log | silent-failure-hunter | Bug | Yes — `resolve.rs` union match had no log | **Accept** | Restored `debug!` on the multi-candidate branch; restructured match to also drop the clone (S6). |
| C2 | Comments say `qualified_name` "begins `Type::`" / "`Sub::%`" but the query is exact `= ?` | comment-analyzer | Bug (doc) | Yes — `symbols.rs:422` exact; the design chose exact to avoid the `LIKE` wildcard | **Accept** | Reworded both comments (`module_resolver.rs` struct doc + over-fire test) to "exactly `Type::<member>`". |
| I1 | `search_type_members_by_name` lacked an `member_kinds.is_empty()` guard → `kind IN ()` syntax error | code-reviewer | Bug (latent) | Yes — guard omitted it; only caller passes non-empty | **Accept** | Added to the guard + doc; extended `type_members_empty_inputs_decline` to fence it. |
| I2 | `LIMIT {limit}` interpolated instead of bound, deviating from in-file convention | rust-reviewer, code-reviewer, silent-failure-hunter | Style | Yes — observation true (`symbols.rs:79,128` bind); **fix not justified** | **Reject** | Safe (usize→digits; SQL text stable since limit is always 2). Reviewers' caching rationale is void (identical SQL string every call); a clean bind through `params_from_iter(Vec<String>)` would need fragile TEXT-coercion of an integer. Correct observation, fix cost > cosmetic benefit. |
| S1 | Doc "resolvable by both arms to the SAME symbol" describes an unreachable case (disjoint kinds) | test-analyzer, comment-analyzer | Bug (doc) | Yes — types `{Class,Struct,Interface,Enum}` ⟂ members `{Function,Method}` | **Accept** | Reworded: cross-arm collision is impossible; dedup is defensive against the member arm surfacing one symbol twice (distinct usings → same type+file). Initial rewrite wrongly blamed "repeated usings" — corrected after finding `imports` PK `(file_id, symbol_name, source_module)` collapses identical usings. |
| S2 | "Each arm caps at 2" imprecise — static arm is 2×N | comment-analyzer | Bug (doc) | Yes — per-directive loop | **Accept** | Folded into the S1 doc rewrite. |
| S3a | `StaticMemberImport.type_name` non-empty invariant not type-encoded | type-design-analyzer | Design | Yes — trailing-dot → empty, caught only downstream | **Reject (defer)** | Absorbed into **tethys-mpth** (item 4). Code is correct + guarded + tested; smart-constructor is a refactor, not a defect. |
| S3b | `GlobResolution` inverted `None` polarity (`kinds:None`=all vs `member_kinds:None`=off) | type-design-analyzer | Design | Yes — `resolve.rs:185` gate | **Reject (defer)** | Absorbed into **tethys-mpth** (item 5). Real latent footgun for a future 3rd language; both current impls are correct + asserted. |
| S4-A | Multiple `using static` in one file untested | test-analyzer | Test | Yes — all four fences used ≤1 static using | **Accept** | Added `multiple_static_usings_single_match_resolves` + `..._cross_type_collision_declines`. |
| S4-B | Intra-arm dedup untested | test-analyzer | Test | Partial — trigger is contrived (imports PK blocks repeated usings; needs a multi-namespace-block file) | **Reject (won't fence)** | Documented as defensive in the `resolve.rs` doc instead. No clean, non-contrived behavioral fence exists; a test would encode an artificial DB state. |
| S4-C | Trailing-dot `using static My.Models.;` e2e gap | test-analyzer | Test | Partial — guard is unit-tested; e2e depends on unverified parser output | **Reject (defer)** | **tethys-nvcy** — verify `csharp.rs` emits the trailing-dot suffix before writing the e2e fence (or downgrade the guard comment to "defensive-only"). |
| S5 | `name`/`type_name` adjacent `&str` transposition; use a `*Params` struct | type-design-analyzer | Design | Yes — real hazard | **Reject (won't do)** | Single call site, correctly ordered; a Params struct for one caller isn't proportional. Revisit if a 2nd caller appears. |
| S6 | `symbol.clone()` avoidable via `candidates.pop()` | rust-reviewer | Polish | Yes — `resolve.rs` success arm | **Accept** | Folded into C1's match rewrite (`1 => Ok(candidates.pop())`). |

## Notes

- Two findings (I2, S5) were factually correct observations whose proposed
  fixes did not survive cost/benefit — rejected with rationale, not silently.
- S1's first applied rewrite was itself inaccurate (blamed "repeated usings");
  caught by checking the `imports` table PK, which de-duplicates identical
  directives. The dedup is genuinely defensive — corrected before commit.
- Verification: full suite green (0 failures), `cargo clippy --all-targets`
  clean. New fences: 2 integration + 1 unit assertion.

## Deferral tracker references

- **tethys-mpth** (P4, ModuleResolver seam type-hardening) — extended with
  items (4) `StaticMemberImport` non-empty invariant and (5) `GlobResolution`
  `MemberArm` enum.
- **tethys-nvcy** (P4, new) — verify parser trailing-dot output; add e2e fence
  for the empty-`type_name` guard, or downgrade its comment to defensive-only.
