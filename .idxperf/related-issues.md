# Related tracker issues (step 0)

- tethys-q8qw (open, P2): Implement incremental index updates — OUT of scope
  for this loop; this work is a prerequisite-adjacent speedup, not incremental.
- tethys-7uw0 (closed): Performance benchmarking on rivets codebase — prior
  benchmark infrastructure (benches/indexing.rs, benches/queries.rs) came from
  this; we reuse it for measurement.
- No existing ticket covers per-row autocommit writes, statement caching, or
  Pass 2 memoization. New tracker issue to be created when scope is signed.
