# tethys-3i35 — probe findings

## Question probed

Smallest question: "How does the current binary index a `crate::helper()`
call whose target lives at the crate root — and what would the proposed rule
(bare `crate` prefix → crate entry-point file) change, on both a mechanism
repro and production-shape data?"

## Probe

`probe.py` (raw SQL over `.rivets/index/tethys.db`, no tethys code):
lists all refs with resolution status, the unresolved `crate::%` population,
a simulation of the proposed rule (entry-point file + tail qualified-name
lookup, reimplemented independently), and a cross-crate decoy check.

Run 1 — two-crate repro workspace (crate_a has `helper` at root + `Thing::make`;
crate_b has a same-named `helper` decoy):

```
crate_a/src/b.rs:4  (bare helper)      -> helper @ crate_a/src/lib.rs (same_crate)
crate_a/src/b.rs:4  crate::helper       UNRESOLVED
crate_a/src/b.rs:4  crate::Thing::make  UNRESOLVED
simulation: crate::helper      WOULD RESOLVE -> helper (function) @ crate_a/src/lib.rs
simulation: crate::Thing::make WOULD RESOLVE -> Thing::make (method) @ crate_a/src/lib.rs
```

Run 2 — tethys self-index (96 files, 19,293 refs): **13 unresolved `crate::`
refs, all multi-segment tails living in submodules** (`crate::db::build_qualified_name`,
`crate::types::SymbolId::from`, …). The proposed rule newly resolves **zero**
of them — every tail misses the entry-point file, correctly.

## Oracles

1. **rustc** (`cargo check` on the repro): compiles clean — both `crate::`
   refs are valid in-crate references, and `crate` cannot denote the sibling
   crate, so ground-truth targets are exactly what the simulation binds
   (crate_a's `helper` / `Thing::make`). Independent mechanism: the compiler,
   not the resolver.
2. **Textual scan** (`/tmp/oracle3i35.py`, regex over the same indexed file
   set, compared at (file, line)): every `crate::`-named unresolved ref is
   text-visible (zero unexplained rows). 11 text-only sites decompose fully:
   6 fixture-string false positives of the scan (test files writing
   `crate::x()` into fixture source), 4 macro-interior refs
   (`proptest!` bodies at src/types.rs:2188-2202, `matches!` pattern at
   src/db/architecture.rs:951 — the already-filed tethys-0nar / tethys-9l27 /
   tethys-i09d gaps), 1 string literal at src/languages/rust.rs:2586.

Probe and oracle agree on every reachable slice.

## What I learned (that the ticket doesn't say)

1. **The fix's observable surface on real corpora is zero.** Neither tethys
   nor rivets contains a single crate-root-tail qualified call
   (`crate::x()` / `crate::Type::method()`); all 13 self-index `crate::`
   unresolved refs have submodule tails the fix correctly leaves alone. The
   value is mechanism correctness for fixture-shaped/future code and removing
   two documented limitations (refs_named view keying, tethys-xzdr's
   MaybeTrait downgrade) — not a delta on today's self-index.
2. **The surface is wider than the ticket's repro**: `crate::Thing::make()`
   (multi-segment tail at crate root) is equally unresolved and equally fixed
   — methods store `parent::name` qualified names, so the entry-point lookup
   finds them.
3. **Blast radius is structurally bounded**: `resolve_crate_path(&[])` is
   reachable only when the written path is exactly `["crate"]`; the
   `["crate"]` split is the shortest split, enumerated last, and today never
   claims a file — so activating it can only convert unresolved→resolved,
   never rebind an existing resolution.
4. **The import side shares the hole but not the symptom**: the repro's
   `use crate::helper;` fails module resolution (the "Unresolved
   dependencies: 1" stat) yet the b.rs→lib.rs `file_deps` edge already exists
   via resolved-ref corroboration. A resolver-level fix also resolves the
   import (fixes tethys-xzdr's root cause; watch `ref_count`/golden fixtures
   and the unresolved-dependencies stat for expected drift).
5. **Two existing fences anticipate the fix**: tests/refs_named.rs:43 has an
   explicit TRIPWIRE comment with post-fix counts (`helper` 3→4,
   `crate::helper` 1→0); tests/deprecated_callers.rs's `crate::old_q()` "Path
   B recovery" expectations likely tier-shift when the ref resolves.
6. **Bin+lib wrinkle**: `CrateInfo::entry_point_file()` prefers `lib_path`
   over bins, but `crate::` written inside `main.rs` denotes the *bin* crate.
   A naive entry-point rule would map main.rs's `crate::` to lib.rs. Needs an
   explicit design decision (flagged for the design pause).

## Hard-gate checklist

- [x] Probe written, runs against the real codebase (self-index + current binary)
- [x] Oracle defined and produces output (rustc; independent textual scan)
- [x] Probe and oracle agree on a non-trivial slice (repro resolution targets;
      self-index (file,line) reconciliation)
- [x] Learned something new: items 1–6 above; headline — the fix has zero
      observable delta on today's corpora, and its real payoff is the widened
      `crate::Type::method()` surface plus retiring two documented limitations.
