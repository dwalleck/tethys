# tethys-3i35 — slice 7 one-shot audits (2026-07-12)

## C3 — monotonicity on the tethys self-index

Method: identical corpus, different binary. The pre-fix baseline
(`prefix-resolved-refs.tsv`, 4,295 resolved refs) was dumped at plan time
from the plan commit's tree indexed by the pre-fix binary. For the post-fix
side, the SAME tree (git worktree at the plan commit, `6ee6675`) was indexed
by the post-fix binary and dumped identically
(`postfix-resolved-refs.tsv`). Corpus equality confirmed: 96 files,
2,442 symbols, 19,293 references on both runs.

Result (`monotonicity.diff`, classified programmatically):

| class | count | verdict |
|---|---|---|
| refs removed from the resolved set | **0** | required 0 ✓ |
| refs added to the resolved set | **0** | matches probe prediction (no crate-root-tail shapes exist in tethys) ✓ |
| symbol_id / target changes | **0** | required 0 ✓ |
| `unique_workspace` → `explicit_import` | 7 | band speculative→high ✓ (predicted class) |
| `qualified_exact` → `explicit_import` | 12 | band medium→high ✓ (see note) |

All 19 strategy transitions occur in files importing the target via a
bare-crate `use crate::X;` (e.g. `use crate::Tethys;` in indexing.rs,
reindex.rs, resolve.rs), where the explicit-import resolution path
previously FAILED on source module `crate` and now succeeds earlier than
the fallbacks. Every transition preserves the target symbol and upgrades
the confidence band.

**Note (honest deviation):** the design's C3 amendment enumerated
`same_crate`/`unique_workspace` as the expected pre-fix strategies; the
observed set is `qualified_exact`(12)/`unique_workspace`(7). The
`qualified_exact` cases are QUALIFIED refs (`Tethys::relative_path(…)`)
whose first segment is bare-crate-imported — same mechanism the amendment
described, incomplete strategy enumeration on my part. No target changed;
classified as the amendment's intended class, recorded here rather than
papered over.

The 13 pre-fix unresolved `crate::` refs (findings.md) are byte-identical
post-fix — still unresolved, per design (their tails live in submodules
behind re-exports; tracked as tethys-qtq5).

Index duration on the self-corpus: 1.30s post-fix vs 1.47s pre-fix probe
run — the new per-bare-crate-resolution syscalls (canonicalize + exists,
~40 imports) are noise. Loop budget holds.

## C11 — self-index oracle (unused-imports)

`tethys unused-imports` on the current tree (post-fix binary, 98 files):

```
Definite:           0
Possible traits:    25 (use --all to show)
```

tethys builds warning-free (CI-enforced), so any Definite finding would be
a false positive by construction. Zero found — the xzdr confidence upgrade
introduced no false accusations at production scale. (The 25 MaybeTrait
findings are the pre-existing hidden-by-default posture, unchanged.)

## Probe oracle (final agreement)

`probe.py` against the repro workspace after the final slice: both
`crate::helper()` and `crate::Thing::make()` resolve to the rustc-pinned
targets under `qualified_module_fallback`; unresolved `crate::` count 0;
decoy crate untouched (recorded in slice 1; fixture re-exercised every
slice via `tests/pass2_crate_root_paths.rs`).
