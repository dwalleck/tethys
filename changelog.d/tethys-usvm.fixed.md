- `tethys cycles` no longer slows down with workspace size when the
  workspace has few circular dependencies. Cycle detection now restricts
  each search to the group of files that can actually reach each other, so
  files on no cycle cost nothing instead of being walked anyway. A
  10,000-file workspace containing a single cycle went from 9.2 s to
  0.04 s; workspaces with no cycles at all skip the search entirely.
  Reported cycles are unchanged, down to their order.
