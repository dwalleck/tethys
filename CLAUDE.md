# tethys — session quick-reference

Read AGENTS.md first (navigation, invariants, gotchas); CONTEXT.md is the
vocabulary; docs/agents/issue-tracker.md is the rivets tracker workflow.

- Commitlint (CI regex): ONE scope, lowercase-letter-first, no commas —
  `feat(db)` ok; `feat(db,lib)` and `chore(9z7i)` FAIL. Check at commit time;
  fixing after a push means a rebase and force-push.
- CI runs workflows on push AND pull_request with identical job names: a
  green PR run can read BLOCKED until the push twin finishes. Auto-merge is
  enabled repo-wide — `gh pr merge --auto --merge` queues a PR to merge once
  required checks pass. Zero checks within ~2 min of pushing = merge conflict,
  not a queue delay.
- Gates need real exit codes: `cmd > /dev/null 2>&1 && echo OK` — never
  `cmd | tail` (the pipe swallows the failure).
- clippy pedantic runs doc_markdown on tests too: backtick identifiers
  (`refs_named`, `same_file`) in every doc comment.
- `RUST_LOG=tethys=trace` overrides the -v flags (EnvFilter); per-arm
  resolver events are trace-level.
- Every push to origin/main flips open PRs to BEHIND (full CI re-cycle):
  batch tracker/doc commits locally while a merge queue is draining.
