# tethys-jdly — prior art check (prove-it-prototype step 0)

Searched `.rivets/issues.jsonl` for `deprecat|attribut|obsolete` (2026-07-02).

- **tethys-haw5** — C# `[Obsolete]` parity; blocked ON jdly. Confirms C# is out of scope here.
- **tethys-53iv** — method calls resolve by name only; caveat for call-site *attribution*
  (a ref claiming to point at a deprecated method may be a phantom edge from name-only
  resolution). Affects confidence in reported call sites, not the query shape.
- No existing ticket about attribute extraction gaps or deprecated-detection bugs.

Upstream spec artifact: the rivets issue `tethys-jdly` itself (detailed AC, parent PRD
`tethys-l6nt`, label `ready-for-agent`) — treated as the signed-off spec.
