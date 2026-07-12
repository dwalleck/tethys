---
name: ship
description: Run the full issue-to-merge pipeline for one tethys rivets issue — verify/pick the issue, branch, gilfoyle loop (probe → falsifiable design → HARD PAUSE for approval → plan → checkpointed build), pre-PR code review with fixes, open PR, watch CI, merge on green after confirmation, close the issue. Use whenever the user says "ship <issue-id>", "work tethys-XXXX", "pick the next issue and build it", "run the pipeline", or asks to take any rivets issue end-to-end — even if they don't say "ship".
---

# ship — one rivets issue, end to end

The pipeline has exactly **two hard pauses**: design approval and merge
confirmation. Everything between them runs autonomously. The pauses are not
ceremony — in past runs the design pause caught decisions only the user
could make (manual fences, naming, output posture), and the checkpointed
build's own stop-on-drift rules caught bugs every fixture missed. Do not
soften either.

## 0. Select and verify the issue

- If the user named an issue: `rivets show <id>`. Confirm it's open and its
  `blocks` dependencies are closed.
- If not: propose one. `rivets ready` truncates to 10 results by default
  (hybrid sort) — with 30+ ready issues, P3/P4 candidates fall below the
  fold and look "hidden". Use `rivets ready -n 100` or `rivets list` for
  a full survey. (Only `blocks` edges to unclosed issues — and blocked
  parents, transitively — actually gate readiness; `related` and
  `parent-child` links to open issues do NOT. A previous version of this
  skill claimed otherwise; that was a truncation artifact misdiagnosed as
  dependency filtering.) Check blockers via `rivets show`, rank by
  priority and the PRD roadmap (tethys-l6nt), and present the pick with a
  one-line rationale before starting.
- Features get the full loop below. If the issue is a small bug
  (single-subsystem, reproducible by a test, no design decisions), say so
  and suggest `/sweep-bugs` instead — the full loop is wasteful there.
- **Epics**: the default is one issue → one PR after the full loop, but an
  epic whose description prescribes its own delivery structure (e.g.
  tethys-9z7i: "each slice its own PR; slice 2 through the gilfoyle loop")
  overrides the default — honor the issue's verbiage. Surface the
  structure at this step so the user knows the run will produce multiple
  PRs, run each prescribed slice through the pipeline stages appropriate
  to it (a docs slice needs the design pause, not a probe harness), and
  close the epic only when every non-deferred slice has merged.

## 1. Setup

```
git checkout main && git pull --ff-only
rivets update <id> -s in_progress
git checkout -b feat/<id>-<short-slug>       # e.g. feat/tethys-xoxq-visibility-tightening
```

If `.rivets/issues.jsonl` has uncommitted changes at this point, commit
them on main FIRST (`chore(rivets): ...`) before branching — tracker
state from a previous cycle should not ride into this branch's history
unannounced.

## 2. The gilfoyle loop

Run the four skills in order. Each one's own gates apply in full; this
skill only sequences them.

1. `/gilfoyle:prove-it-prototype <id>` — probes + independent oracle +
   `related-issues.md` + `findings.md`, committed as
   `chore(<slug>): prove-it-prototype probes + findings (<id>)`.
   Artifacts live in `.<id-short>/` (e.g. `.tethys-xoxq/`) and are
   committed to the branch — they are the audit trail.
2. `/gilfoyle:falsifiable-design` — claims table, cheapest falsifier RUN
   before presenting, negative space, tracker-clean deferrals. Commit it.
3. **HARD PAUSE.** Present the design summary and every flagged open
   decision. Wait for explicit approval. Do not start the plan on a hedge
   ("looks fine I guess") — ask for a real yes, and apply any requested
   changes to the design first.
4. `/gilfoyle:budgeted-plan` — slices with claims/oracles/fixtures/budgets.
   Commit it.
5. `/gilfoyle:checkpointed-build` — one commit per slice, all gates per
   slice, STOP on drift per that skill's rules (drift stops surface to the
   user; they are not this skill's to adjudicate).

## 3. Pre-PR review (before opening the PR, not after)

Run `/code-review` with `--fix` against `main` on the finished branch.
Verify each finding before applying (the reviewer can be wrong in both
directions); commit fixes as their own conventional commits. Doing this
pre-PR means the PR opens already-reviewed instead of collecting bot
churn.

Also before the PR: write the changelog fragment
`changelog.d/<rivets-id>.<category>.md` — category one of
added/changed/deprecated/removed/fixed/security; 1-5 bullets written for
CLI users (name commands, flags, observable behavior; no rivets IDs or
slice numbers — the PR body carries that story). Commit it
(`docs(changelog): fragment for <id>`, or ride it with a review-fix
commit). The `changelog` CI job blocks fragment-less PRs;
`tests/changelog_lint.rs` fences the format; docs/CI-only PRs take the
`skip-changelog` label instead. Never edit CHANGELOG.md on a branch.

## 4. Open the PR

Push (`git push -u origin <branch>`), then `gh pr create` with the house
body shape (see PR #10/#11 for worked examples):

- `Closes <rivets-id> (<one-line what>)`
- **What this does** — the design's core rule in prose, with the numbers
  that justify it (probe measurements).
- **Acceptance criteria** — checklist, each AC mapped to the named test
  fence that proves it.
- **Method + evidence** — pointer to the `.<id-short>/` artifacts (probes,
  design, plan, audits) and the headline audit results.
- **Discovered and filed** — issues filed during the loop, with IDs.
- **Notable behavior changes** beyond the ticket.

## 5. Watch CI

Use a Monitor (poll `gh pr checks` / `mergeStateStatus`, 30s interval,
emit on pass OR fail — silence must not look like success). Known quirks,
all hit in practice:

- **Commitlint**: CI validates every subject against
  `^(feat|fix|docs|style|refactor|perf|test|build|ci|chore)(\([a-z][a-z0-9-]*\))?!?: .{3,}`
  — ONE lowercase scope, hyphens ok, **no commas** (`feat(db,lib)` fails;
  pick the primary scope and note the second file in the body). Check
  subjects against this at commit time, not at PR time — rewording a
  merged-in commit means a rebase and force-push.
- **Zero checks reported within ~2 min of pushing** = merge conflict
  fingerprint, not a queue delay. Run `git diff origin/main --stat`.
- The workflow runs on both `push` and `pull_request`, with identical job
  names. Branch protection waits on the **latest run per context**, so a
  green PR-event run can still read BLOCKED while the push-event twin
  finishes. Wait; don't re-push.
- Auto-merge is **enabled** in repo settings — `gh pr merge --auto --merge`
  queues the PR to merge automatically once `CI Success` passes, so you need
  not babysit to the finish. Manual `gh pr merge --merge` on a `CLEAN`
  `mergeStateStatus` still works if you'd rather merge on green yourself.

## 6. Merge and close out

- **Before merging: fetch and assess bot review comments** —
  `gh api repos/<owner>/<repo>/pulls/<n>/comments` (gemini posts within
  minutes of PR-open, so they're in by the time CI is green). Treat each
  finding as a hypothesis: VERIFY against the code before accepting
  (grep the identifier, read the schema, EXPLAIN the query plan) and
  record accept/reject per finding. Calibration from experience: factual
  checks (wrong identifier names, inconsistent tracker IDs) are usually
  real — fix pre-merge; speculative perf/hardening suggestions usually
  fail verification (SQLite already eliminates the join; the "bottleneck"
  is 95 rows) — reject with evidence; a finding that converges with an
  already-known limitation gets FILED, not hotfixed. Fixes push a new
  commit → CI re-runs → the merge waits.
- **PAUSE**: confirm with the user before merging (skippable only if they
  already said "merge when green" this session).
- `gh pr merge <n> --merge` (merge-commit convention, matching history).
- `git checkout main && git pull --ff-only`.
- `rivets close <id> -r "Shipped: PR #<n> merged to main (<sha>). <AC/fence
  summary>. Fixed in-branch: <ids>. Filed: <ids>."`
- Note the jsonl change now sitting uncommitted on main; it rides with the
  next branch or the next `chore(rivets)` commit.
- Offer the next pick; do not start it unprompted.

## Conventions that bind throughout

- **Gates use real exit codes.** `cargo clippy --all-targets -- -D warnings
  > /dev/null 2>&1 && echo OK` — never `cmd | tail -1` (the pipe swallows
  the exit code; a gate leaked a clippy failure into a commit exactly this
  way once).
- Full gate per slice: `cargo nextest run`, clippy pedantic `-D warnings`,
  `cargo fmt --check`, doctests.
- **Impact analysis dogfoods tethys.** For a slice that changes a function's
  signature/name/semantics, list callers with
  `tethys callers <sym> --exclude-speculative` (run `tethys index` first) as
  the precision tier, `grep` as the recall net — EXCEPT when the slice edits
  tethys's own resolver/call-edge logic, where the tool can't oracle a change
  to itself and `grep` is the source of truth. See AGENTS.md → "Dogfood tethys
  for impact analysis."
- Tracker discipline everywhere: every deferral names a verified rivets ID;
  discovered bugs are filed before (or with) their fix; duplicates searched
  before filing.
- If parallel work is in flight (other open PRs from ship/sweep sessions),
  keep `.rivets/issues.jsonl` OUT of this branch: file issues from a
  separate main checkout, or queue them in `.<id-short>/to-file.md` and
  file at close-out. The jsonl is one-line-per-issue and conflicts at
  merge almost every time two branches touch it.
