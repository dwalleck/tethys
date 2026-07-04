# tethys-xoxq C15: hand-reviewed self-index run (2026-07-04)

`tethys visibility-tightening` on tethys itself (default flags, fresh
`--rebuild` index). Approved-manual fence per the design sign-off; the
mechanical floor is `root_reachable_ceiling` / the S1–S9 fixture fences.

## Numbers

- 62 candidates: 26 Definite, 36 Maybe
  (12 `root-reachable`, 16 `shared-name`+`root-reachable`, 8 `shared-name`)

## Definite tier (26) — hand classification

Every Definite finding is a `pub` item inside a PRIVATE module
(`batch_writer`, `db`, `graph`, `languages`, `parallel`, `resolver`,
`types`-internal) — not externally nameable, which is exactly why the
root-reachability ceiling let them through. Verification:

- Spot-checked six across modules (`WriteStats`, `SymbolData`,
  `node_text`, `RefId`, `BatchWriter`, `LanguageSupport`): zero uses via
  public `tethys::` paths in tests/benches/bin, zero `pub use` re-exports
  in lib.rs (re-exported items would have been C5-excluded anyway).
- Integration tests and benches consume only the root API; unit tests
  inside the crate are unaffected by `pub(crate)`.

**Observed false-positive rate in the Definite tier: 0/26.** All 26 are
compile-safe `pub` → `pub(crate)` tightenings (several are `pub` purely
for intra-crate sibling-module access; the tightening makes that intent
explicit).

## The lib/bin merge (probe fact 2), observed

`src/main.rs` + `src/cli/*` are the same `arch_packages` row as the lib,
so lib-root items consumed only by the CLI produce no cross-package
evidence and DO appear as candidates — but every one of them is
root-reachable (they are the lib's public API), so the C7 ceiling caps
them at Maybe. **Zero bin-consumed items reached Definite.** The merge
limitation is therefore absorbed exactly as the design predicted: it
inflates Maybe (12+16 root-reachable findings include the CLI-consumed
API), never Definite. Running with `--workspace-closed` on a lib+bin
package WOULD promote those to Definite — the flag's docs say to assert
it only when nothing outside the workspace consumes the code, and the bin
is inside the workspace, so the flag remains honest here; the caveat is
that tightening a CLI-used lib export breaks the bin, which rustc catches
at the first `cargo check`.

## Verdict

Suggestions reviewed; no false Definite observed on the self-index. The
Maybe tier is where human judgment is required, by design.
