- `tethys index` now removes deleted files from the index: rows for source
  files deleted from disk since the last index are purged instead of
  lingering forever.
- Streaming-mode indexing no longer resurrects dependency edges from deleted
  files, so `coupling`, `callers`, `cycles`, and `impact` results no longer
  include phantom contributions from files that no longer exist.
