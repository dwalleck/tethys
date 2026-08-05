# Probe findings — tethys-09wx (query standing for affected-tests)

## Smallest question

"For one changed file, can the index stand behind an affected-tests answer —
is the file indexed, and is its indexed mtime/size current?"

## Probe

`probe.py` classifies a changed file per the issue's two v1 triggers using raw
SQL + `os.stat` (no tethys code): UNINDEXED / STALE / CURRENT.
`probe.sh` constructs ground truth by manipulation on the real repo: fresh
index (everything CURRENT by construction), marker file + `touch` one source
file (STALE by construction), a never-existed path (UNINDEXED by construction).

## Oracles

1. **Staleness**: `find src -name '*.rs' -newer .tethys-09wx/marker` —
   filesystem timestamp ordering, no DB. Agreed exactly: the one touched file.
2. **Membership**: `git ls-files 'src/**/*.rs'` vs `SELECT path FROM files`,
   both through `LC_ALL=C sort`. SETS IDENTICAL (first run showed a phantom
   one-line diff — locale collation vs SQLite ORDER BY, an oracle-comparison
   bug, fixed in probe.sh).

## Agreement

Probe and both oracles agree on every slice: CURRENT/STALE/UNINDEXED
classification matches constructed ground truth; DB membership matches git.

## CLI behavior slices (the bug evidence, confirmed + extended)

| invocation (`--names-only`) | exit | stdout |
|---|---|---|
| `src/lib.rs` (indexed, current) | 0 | WARN line + 980 test names |
| `src/does_not_exist.rs` | 0 | WARN line only |
| `src/reindex.rs` (touched → stale) | 0 | WARN + 981 names, staleness invisible |
| `./src/lib.rs` (same file, `./` prefix) | 0 | 0 test names — **false clean** |
| absolute path | 0 | 980 test names |

## What I learned (didn't know before)

1. **Logs go to stdout** (`fmt().init()` default, main.rs:248): any WARN
   breaks "empty stdout = confirmed" — moving logs to stderr is a hard
   prerequisite for the contract, not a nicety. → filed **tethys-sspl** (P2).
2. **Every relative changed-file path fires a spurious WARN** ("outside
   workspace root") because the canonicalize fallback is absolute-only.
   → filed **tethys-vk3z** (P4).
3. **`./src/lib.rs` silently misses the index** (exact-string lookup;
   `normalize_path` only handles backslashes) — false confirmed-clean today,
   would be a false INDETERMINATE under trigger (a). Zero-false-alarm requires
   lexical input normalization. → filed **tethys-xetb** (P2, fix in-branch).
4. **The staleness substrate is solid**: `classify_indexed_file`'s
   mtime_ns+size comparison matched the independent oracle 1:1, and per-file
   classification needs only a stat + one row read — no full-workspace walk
   (`get_stale_files`) required at query time.

## Design-pause questions surfaced for later

- Deleted-on-disk changed file (row exists, stat fails): indeterminate?
  (Probe classifies STALE-deleted; index edges for it are by definition
  outdated, so arguably yes — needs a decision.)
- Path outside workspace (`../elsewhere/x.rs`): unindexable — same
  "unindexed" reason kind, or its own?
- Machine-readable reasons on stderr will share the channel with tracing
  logs (post-sspl): reason lines need a format logs can't collide with.
