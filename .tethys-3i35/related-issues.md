# tethys-3i35 — related issues (tracker prior art)

Searched `.rivets/issues.jsonl` for: `crate::`, `qualified_module_fallback`,
`resolve_module_path`, `crate root`, `crate-root`, plus title matches on
`qualified`/`resolv`.

## Directly load-bearing

- **tethys-xzdr (open, P3)** — same resolver hole, import side.
  `use crate::Foo;` downgraded to MaybeTrait because
  `resolve_module_path(["crate"])` → `resolve_crate_path(&[], crate_root)`
  returns the src/ **directory**, which has no `files` row. Its design notes
  already name the preferred fix (bare `crate` → entry-point file, mirroring
  the workspace-crate arm at src/resolver.rs:57-58) and carry the blast-radius
  caution: the `"crate"` arm feeds self/super sibling resolution,
  `qualified_splits` prefix enumeration, and `compute_dependencies`. A
  resolver-level fix here likely fixes xzdr too (or reduces it to its
  acceptance tests).
- **tethys-6rlu (closed, discovered-from)** — refs_named view keys unresolved
  `crate::helper` refs by the full path string, so bare-name queries miss
  them; documented as a limitation that this fix removes.
- **tethys-9z7i (closed)** — resolution provenance: every resolved ref
  carries a strategy + confidence band. A fix must land under an existing or
  new strategy (`qualified_module_fallback` is the natural fit — the split
  driver stays the same, only prefix→file mapping changes).

## Adjacent hazards to respect (not fix)

- **tethys-bvgb (open, P4)** — `qualified_exact` binds first match on
  duplicate qualified names across crates. Reminder that any new `crate`
  mapping must anchor to the *ref's own* crate (per-file anchor via
  `rust_src_root_for` / `get_crate_for_file`), never workspace-wide.
- **tethys-nkjd (open, P4)** — `resolve_super_path` filesystem-walk
  divergence. Shares `qualified_module_fallback`; out of scope here, but the
  fix must not touch the `super` arm's semantics.
- **tethys-i09d (open, P3)** — scoped-identifier *value* uses
  (`crate::Foo` as a value) never reach the refs table at all. Means this fix
  only benefits call-shaped refs until i09d lands; do not claim otherwise.
- **tethys-7035 / tethys-z9mr / tethys-pdea** — sibling parser/resolver gaps
  on use-statement shapes; pdea (closed) notes nested-group flattening feeds
  a collapsed `["crate"]` path into the same hole xzdr describes.

## Conclusion

The bug is well-triangulated prior art: xzdr dissected the same root cause
from the import side and pre-wrote the design caution list. No duplicate of
the *call-path* symptom exists besides tethys-3i35 itself.
