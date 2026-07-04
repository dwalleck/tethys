# Issue tracker: rivets

Issues and PRDs for this repo live in rivets, a local JSONL issue tracker stored at
`.rivets/issues.jsonl` and driven by the `rivets` CLI. Issue IDs look like
`tethys-xxxx`. GitHub (`dwalleck/tethys`) is used for pull requests only — reference
rivets IDs in commit and PR messages.

## Core commands

- `rivets create --title "..." -t <bug|feature|task|epic|chore> -p <0-4> -l "a,b" -D "..."`
  — create an issue. Priority: 0=critical … 4=backlog. Also accepts
  `--acceptance`, `--design`, `--deps "type:id,..."` (types: blocks, related,
  parent-child, discovered-from), and `--external-ref`.
- `rivets list` / `rivets ready` / `rivets blocked` — query the backlog.
  `ready` truncates to 10 results by default (hybrid sort) — use
  `rivets ready -n 100` or `rivets list` for a full survey. Only `blocks`
  edges to unclosed issues (and blocked parents, transitively) gate
  readiness; `related`/`parent-child` links to open issues do not.
- `rivets show <id>` — full detail for one issue (closed issues stay readable).
- `rivets update <id> [-D ...] [-s <status>] [-p N]` — modify fields; status values:
  open, in_progress, blocked, closed.
- `rivets close <id>... -r "reason"` — close one or more issues. The reason is the
  permanent audit trail (used for duplicate closures, consolidations, wontfix).
- `rivets dep add`, `rivets label add/remove/list-all` — manage relations and labels.
- Add `--json` to any command for machine-readable output.

## Conventions

- **Search before filing.** Every historical duplicate cluster came from scan-and-file
  sessions that skipped this. Check `rivets list` / grep the JSONL for the symptom
  first; if a duplicate exists, update it instead.
- Close duplicates with `-r "Duplicate of tethys-XXXX (…)"` pointing at the survivor.
- When consolidating several issues into one, the new issue's description lists the
  absorbed IDs; the absorbed issues close with a reason pointing at the bundle.
- File paths cited in issues created before June 2026 use the old `crates/tethys/...`
  layout; the workspace is now flat (`src/...`). Treat old file:line refs as hints,
  not addresses.

## When a skill says "publish to the issue tracker"

`rivets create`. PRDs are epics with labels `prd,ready-for-agent` (see
`rivets show tethys-l6nt` for the pattern).

## When a skill says "fetch the relevant ticket"

`rivets show <id>` (add `--json` when parsing).

## CLI gotcha

Option values that start with `-` (e.g. markdown checklists) must use the equals
form: `--acceptance="- [ ] ..."` — clap rejects the space-separated form.
