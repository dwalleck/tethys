- Qualified calls through the crate root (`crate::helper()`) now resolve,
  so `tethys callers`, `impact`, and `reachable` see those call sites.
- `tethys unused-imports` reports an unused `use crate::X;` of a non-trait
  crate-root item as Definite (shown by default), no longer hidden behind
  `--all` as MaybeTrait.
- `use crate::X;` imports no longer count as unresolved dependencies in
  `tethys index`; `use crate::*;` globs resolve bare calls to root items.
- In bin+lib crates `crate::` inside a binary root resolves against that
  binary (matching rustc); ambiguous `src/bin/` modules stay unresolved.
