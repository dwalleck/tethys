# C# parity research artifacts

Evidence behind the C#-parity decisions, captured into `main` from six research
and prototype branches so the material does not live only on unmerged branches.

Tracker records `tethys-4015` (Wayfinder: C# parity with Rust), `tethys-f1rt`,
`tethys-jmj0` and `tethys-zqw3` cite these branches as their resolution
artifacts, so the content is preserved here rather than remaining reachable only
through a branch ref.

| Artifact | Source branch | Commit |
|---|---|---|
| `research/binding-semantic-host.md` | `prototype/tethys-07eh-binding-model` | `138c6a6` |
| `prototypes/binding-model/index.html`, `prototypes/binding-model/observations.json` | `prototype/tethys-07eh-binding-model` | `138c6a6` |
| `prototypes/project-model/` (contract, probes, timings, framework observations, model) | `prototype/tethys-chlt-project-model` | `c98c726` |
| `research/csharp-ls-cli.md` | `research/csharp-parity-lsp` | `e5987f5` |
| `research/msbuild-discovery.md` | `research/csharp-parity-msbuild` | `7477c36` |
| `research/omnisharp-legacy.md` | `research/csharp-parity-omnisharp` | `f274be0` |
| `research/workflow-parity.md` | `research/csharp-parity-workflow` | `dafece0` |

The documents are captured verbatim; only their location changed. Formatting and
content are as authored on those branches.

## Note on a pinned revision

`tethys-4015` cites `research/csharp-parity-lsp@07def9b`. That commit is reachable
from no branch — it is an earlier revision of `research/csharp-ls-cli.md`. What is
captured here is the branch head `e5987f5`, a later revision of the same document
(+26/-10 lines), so the artifact is preserved in its most complete form rather
than at the pinned commit.

## Note on probe tooling

`prototypes/project-model/tethys-chlt-evaluation-probe.py` and
`tethys-chlt-framework-probe.py` are the probes that produced the adjacent
`*.json` observations, and `prototypes/binding-model/index.html` renders the
binding-model observations. They are included so the recorded numbers can be
re-derived rather than only read.

## Provenance

Recovered during a branch-clearance audit of the tethys and rivets repositories.
The six source branches were retained specifically because they held the only
copy of this material; with it captured here, they are no longer the sole
location.
