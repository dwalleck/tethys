# tethys-dvsw — prior art (tracker sweep, 2026-07-15)

Issue's own description is STALE (May 2026): prescribes a call_edges+refs
LEFT JOIN and "trivial" posture. Superseded by the transferred AC
(speculative-band suppression, ADR-0003) and everything below.

## Binding constraints from shipped work

- **tethys-y3bx** (untested-code, PR #28): traversal must read REFS, not
  call_edges — `value`/`macro_call`/`inherit` kinds are excluded from
  call_edges, and top-level refs have `in_symbol_id NULL`. Dead-code note
  recorded on y3bx applies verbatim.
- **tethys-j2r1** (type hierarchy, PR #31): method-level `inherit` markers
  (kind='inherit', in_symbol = trait-impl method) exist SPECIFICALLY as
  dvsw's suppression channel for Rust trait-impl methods. C# has
  type-level edges only → method suppression deferred to tethys-3b06.
  Method-override resolution was deferred INTO dvsw's design at the j2r1
  pause.
- **ADR-0003 / tethys-9z7i**: speculative-band edges are SUPPRESSIONS for
  dead code (transferred AC on dvsw). Post-53iv, unknown-receiver method
  calls bind unique-or-DECLINE — ambiguous method calls leave UNRESOLVED
  refs (reference_name preserved), so an unresolved-textual channel
  (deprecated-callers Path B / visibility-tightening channel c pattern)
  is likely load-bearing.
- **tethys-aay4** (parent_symbol_id): container linkage exists — needed to
  decide container liveness (a struct whose only use is `Type::method()`
  gets no ref on the type symbol; the method does).
- **tethys-s8hv**: unit tests ARE indexed (is_test=1); test fns have zero
  inbound refs by nature → must be excluded as candidates.

## Known false-positive sources (open, to document not fix)

- **tethys-9l27** (P3): refs inside macro invocations invisible for
  method-shape (`.unwrap()` in assert! args). Bare call-shape fixed by
  8ym0 (macro_call kind); method-shape remains.
- **tethys-0nar** (P3): fns defined inside proptest!/macro invocations not
  indexed as symbols (~5 self-index).
- **tethys-7dqj / tethys-ewa7** (P4): nested macro-name refs and
  path-shaped calls in macro token trees emit no ref.
- **tethys-wbrh** (P3): fn-as-value gaps (struct field init, assignment
  RHS, tuple/array) — a fn passed only in those positions looks dead.
- **tethys-i09d** (P3): scoped-identifier VALUE uses (`crate::Foo`,
  `Type::assoc_fn` as a value) omitted from refs.
- **tethys-pv7w** (P3, related-linked to dvsw): glob/module re-exports
  don't mark target symbols referenced — public-only per its scope, and
  dvsw filters public symbols, so impact is limited to pub(crate) globs.
- **tethys-msn0** (P3): phantom file_dep from import-usage detection —
  file_deps only, refs unaffected; no dvsw impact expected.
- **tethys-0aqj** (P4): kind-blind binding — same-named symbols of
  different kinds collide; can fabricate a ref onto a dead symbol
  (false NEGATIVE — acceptable under suppressions-not-accusations).
- **tethys-9181** (P3): construct refs bind the TYPE symbol — relevant to
  method liveness (constructors with zero direct refs).
- **tethys-3b06** (P4): C# method-level suppression requires override
  resolution — deferred; C# gets type-granular suppression only.
- **tethys-m7zm** (P3): policy for analyses on newly-indexed test code —
  a symbol referenced ONLY by tests is live per refs; report-all posture
  (y3bx precedent) leaves it alive. Note in module docs.

## Verdict

No existing ticket describes the finder itself; dvsw is the ticket. All
adjacent bugs above are filed — none re-discovered here get new tickets
unless the probe surfaces a NEW class.
