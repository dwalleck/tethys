# Raw request (verbatim)

Date: 2026-06-09
Requester: dwalleck

> Yes, but lets do this as a gilfoyle loop. I want to prove these performance
> improvements and detect unintended side effects before we call this work done

Context: "Yes" refers to the assistant's proposed performance PR scope:
1. Wrap each file's full write (file + symbols + refs + imports) in one
   transaction with cached statements (#1 in the review recap).
2. Pass 2 resolution: memoize (file, ref_name) -> outcome per file, and batch
   resolution UPDATEs in a transaction (layers a+b; the in-memory symbol
   multimap layer c explicitly deferred).
3. Share the fast ancestor-walk crate map between run_architecture_phase and
   build_file_crate_map (#3 in the recap).
Incremental update() (#4) explicitly deferred to a follow-up PR.

## SCAN: vague terms requiring resolution
- "prove these performance improvements" — no number, unit, fixture, or method
- "detect unintended side effects" — no equivalence definition (DB files are
  never byte-identical: indexed_at timestamps, rowid assignment order)
- failure-semantics change unpinned: one-transaction-per-file makes a crash
  drop the whole file instead of leaving symbols-without-refs
- measurement fixture unpinned: criterion synthetic workspaces vs self-index
  of the tethys repo vs both
