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
