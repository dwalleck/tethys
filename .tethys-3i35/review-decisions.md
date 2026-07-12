# Review-feedback decisions — PR #24 (/code-review xhigh, 2026-07-12)

Two-axis review (Standards + Spec, parallel sub-agents) of `main...HEAD`.
Each finding verified before any fix was applied
(gilfoyle/assessing-review-feedback).

| # | Finding (one line) | Axis | Category | Verified? | Decision | Note |
|---|---|---|---|---|---|---|
| S1 | AGENTS.md gotcha list (`docs/`, `.separator-fix/`, `.csharp-ns/`) omits the new `.tethys-3i35/` artifact dir — doc-maintenance rule breach | Standards | Convention | Yes (AGENTS.md:180 predates the dir; no commit in the PR touched it) | Accept | Added `.tethys-3i35/` to the list |
| S2 | Canonicalize-or-fall-back idiom repeated 3× inside `bare_crate_root_file` (resolver.rs:120, :134, :140) | Standards | Polish (Duplicated Code, judgement call) | Yes (three textual occurrences, identical semantics) | Accept | Extracted private `canonicalize_or_raw(PathBuf) -> PathBuf`; behavior-preserving, fenced by the existing `src/./main.rs` regression test |
| S3 | Feature Envy: `bare_crate_root_file` reads only `CrateInfo` data — move policy onto `CrateInfo` | Standards | Design (judgement call) | Yes (envy is real) | Reject | Repo standard overrides the baseline smell: AGENTS.md's module map assigns Rust module-path semantics to `src/resolver.rs`, and the approved design (design.md) placed the helper there |
| S4 | Test SQL count shape duplicated 4× beyond `count_resolved` in pass2_crate_root_paths.rs; refs_named.rs keeps twins | Standards | Polish (judgement call) | Refuted on inspection | Reject | Each "repeat" differs deliberately: :98 omits `symbol_id IS NULL` (stronger leftover assert), :110/:227 are broader decoy fences (any ref = failure), :301 is strategy-agnostic per its own comment. Parameterizing would weaken asserts |
| P1 | C11 self-index oracle shipped as one-shot audit, not a permanent CI fence (design falsifier table said "add if absent") | Spec | Design (deferred work) | Yes | Reject (defer) | Tracked at tethys-oojq (filed by the PR itself; verified open in rivets) |
| P2 | Observed strategy transitions (`qualified_exact`×12) exceed the approved C3 amendment's predicted class (`same_crate`) | Spec | Informational | Yes (disclosed in audit.md:30-37; targets unchanged) | No action | Honest deviation recorded for maintainer adjudication; surfaced in the review report |
| P3 | PR body claims "9 unit tests" in src/resolver.rs; the diff adds 10 | Spec | Polish (factual) | Yes (`git diff main...HEAD -- src/resolver.rs \| grep -c '^+.*#\[test\]'` = 10) | Accept | PR body corrected via `gh pr edit`; also refreshed the stale suite count (907 → 908 — commit 1299268 added a regression test after the body was drafted) |
