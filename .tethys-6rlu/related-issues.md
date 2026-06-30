# Prior art — tethys-6rlu (refs.reference_name nulled on resolution)

Tracker checked 2026-06-30 (keyword: reference_name, provenance, resolve_reference, symbol_id, name-queryable).

Directly related:
- **tethys-6rlu** (this) — resolution NULLs refs.reference_name; name-queries over refs vacuous for resolved symbols.
- **tethys-zp2j** (closed, INVALID) — false bug ("bare calls omitted"); its repro queried reference_name and hit exactly this nulling. 6rlu is `discovered-from` zp2j.
- **tethys-4tev** (closed) — Pass 2 resolution: the code that does `UPDATE refs SET symbol_id=?, reference_name=NULL` (src/db/references.rs:157).
- **tethys-1p0f** (closed) — store unresolved refs with symbol_id NULL (the branch where reference_name IS kept).
- **tethys-rndz** (closed) — get_callers/impact use resolved refs (symbol_id join) — they already avoid the footgun by joining.
- **tethys-ygjx** (open) — different gap: fn-as-value + macro-token refs never extracted (not a nulling issue).

No existing ticket covers the reference_name-nulling footgun itself; 6rlu is the first. (Memory roadmap line 15 noted it as "resolution provenance not persisted" but it was unfiled until now.)

Conclusion: no re-discovery. Proceed.
