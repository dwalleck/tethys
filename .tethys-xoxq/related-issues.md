# tethys-xoxq prior art (tracker search, 2026-07-04)

Issues that bear on visibility tightening, with the direction they cut:

- **tethys-v1w8** (closed) — `pub use` re-export targets now carry inbound
  `kind='reexport'` refs. This is why xoxq is unblocked: re-exported items
  have ref evidence. AC2 depends on it.
- **tethys-pv7w** (open) — GLOB and MODULE re-exports do NOT yet mark their
  targets referenced. A `pub use m::*`-exported item has no reexport ref →
  looks crate-local → **false tightening candidate**. Must scope around or
  tier down.
- **tethys-ygjx** (open) — fn-as-value refs missing (callbacks). A cross-crate
  callback-only use is invisible → **false candidate**. Issue text says:
  document as known limitation, conservative tiering absorbs it.
- **tethys-z9mr** (open) — resolver declines single-segment relative import
  paths. Unresolved refs = missing use evidence → potential false candidates.
- **tethys-3i35** (open) — `crate::X()` root-level qualified calls unresolved.
  Same-crate evidence missing — harmless for xoxq (same-crate refs don't
  rescue a candidate; they ARE the candidate condition).
- **tethys-53iv** (open) — phantom name-only method bindings. A phantom
  cross-crate ref would SUPPRESS a true candidate — the SAFE direction here.
- **tethys-9z7i** (open) — provenance bands; would let xoxq weight explicit-
  import resolutions over unique-name fallback. Not a blocker.
- **tethys-6rlu** (closed) — `refs_named` view for name queries; useful for
  probe SQL.

## Key inversion (new insight from this search)

For deprecated-callers, a MISSED ref = under-reporting (safe). For visibility
tightening, a MISSED cross-crate ref = a pub item wrongly flagged tightenable
(accusation — forbidden by the PRD posture). Resolver gaps that were benign
for jdly are the PRIMARY false-positive source for xoxq. The probe must
measure: what fraction of real cross-crate uses appear as resolved
cross-package refs in the index?
