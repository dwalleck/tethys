# changelog.d — pending release notes

One fragment per PR. At release time `scripts/changelog-release.sh <version>`
assembles every fragment here into a new `CHANGELOG.md` section and deletes
them — between releases, this directory IS the unreleased changelog.

## Format

Filename: `<id>.<category>.md`

- `<id>` — the rivets issue ID (e.g. `tethys-53iv`); lowercase letters,
  digits, hyphens. Use `pr-<n>` or a short slug when there is no rivets
  issue (dependency bumps, process changes).
- `<category>` — one of `added`, `changed`, `deprecated`, `removed`,
  `fixed`, `security` (the Keep a Changelog sections).

Body: 1-5 markdown bullets (`- ...`); continuation lines are indented two
spaces. Written for **users of the tethys CLI** — name commands, flags, and
observable behavior. No rivets IDs, slice numbers, or internal module paths
in the text: the commit log and PR body carry that story.

Illustrative example (`tethys-abcd.added.md`):

    - `tethys callers` accepts `--json` for machine-readable output.

## Enforcement

- The `changelog` CI job fails any PR that adds no fragment. The
  `skip-changelog` PR label is the only exemption (docs/CI/chore-only PRs).
  The label is read when the workflow event fires — after applying it,
  re-run the check or push again.
- `tests/changelog_lint.rs` fences the filename and body format (runs in
  `scripts/gate.sh` and CI like any other test).
- Never edit `CHANGELOG.md` directly in a PR — parallel PRs would conflict
  on it; fragments have distinct filenames precisely so they never do.
