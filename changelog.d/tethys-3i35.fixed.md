- Qualified calls through the crate root (`crate::helper()`,
  `crate::Thing::make()`) now resolve to the crate-root item, so
  `tethys callers`, `tethys impact`, and `tethys reachable` see those call
  sites instead of dropping them.
- `tethys unused-imports` now reports an unused `use crate::X;` of a
  non-trait crate-root item as Definite (shown by default) instead of
  MaybeTrait (hidden behind `--all`).
- `use crate::X;` imports no longer appear in `tethys index`'s unresolved
  dependency count, and `use crate::*;` globs now resolve bare calls to
  crate-root items.
- In crates with both a library and binaries, `crate::` written inside a
  binary root resolves against that binary (matching rustc); shapes whose
  owning target is ambiguous (modules under `src/bin/`) conservatively stay
  unresolved rather than guessing.
