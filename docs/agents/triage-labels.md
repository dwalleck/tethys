# Triage Labels

The skills speak in terms of five canonical triage roles. This file maps those roles
to the actual label strings used in this repo's issue tracker (rivets).

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
| -------------------------- | -------------------- | ---------------------------------------- |
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `wontfix`            | Will not be actioned                     |

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the
corresponding label string from this table.

## rivets-specific notes

- Applying `wontfix` also closes the issue so it leaves the open backlog:
  `rivets label add <id> wontfix && rivets close <id> -r "wontfix: <why>"`.
- Triage roles are **labels**; rivets' native `status` field
  (open/in_progress/blocked/closed) is orthogonal and not a substitute for them.

## Wayfinder labels use the hyphen form

The `wayfinder` skill labels its map `wayfinder:map` and each ticket
`wayfinder:<type>`, one of `research`, `prototype`, `grilling`, `task`. That colon
form is valid on GitHub and GitLab, which the skill's trackers target — but
**not in rivets**. The Label grammar is owned by **rivets**, not this repo: see
`**Label**` in `../rivets/CONTEXT.md` (tethys' own `CONTEXT.md` has no Label
entry) and `crates/rivets/src/domain/label.rs`, which rivets-g3t7 ratified as
`[a-z0-9]+(?:[-_][a-z0-9]+)*` at 1–50 bytes. A colon-bearing label is rejected at
load, so that record is skipped and the partial-load write guard then refuses
every write to the tracker.

Use the hyphen form here:

| Skill label           | Label in this tracker  |
| --------------------- | ---------------------- |
| `wayfinder:map`       | `wayfinder-map`        |
| `wayfinder:research`  | `wayfinder-research`   |
| `wayfinder:prototype` | `wayfinder-prototype`  |
| `wayfinder:grilling`  | `wayfinder-grilling`   |
| `wayfinder:task`      | `wayfinder-task`       |

All five appear because all five are reachable: the map plus each of the skill's
four ticket types.

Do not edit the installed skill to "fix" this — it is correct for the trackers it
targets. The translation belongs here, next to the triage-role mapping above.
Recorded 2026-09-11 after 17 records were migrated out of the colon form; the
same migration precedent (`*.rs` → `*-rs`) is in rivets' `rivets-g3t7`.
