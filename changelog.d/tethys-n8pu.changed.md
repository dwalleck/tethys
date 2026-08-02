- Library API: `Tethys::get_dependencies` and `Tethys::get_dependents` now
  resolve result paths in one pass instead of one indexed-file lookup per
  returned file — faster on large result sets; returned paths, ordering,
  and `NotFound` behavior are unchanged. No CLI command is affected.
