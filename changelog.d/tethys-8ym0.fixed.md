- Function calls inside macro arguments (`assert_eq!(helper(), 1)`,
  `vec![build()]`) are now indexed as references; previously macro
  arguments were skipped, so assert-tested code carried no references.
- `deprecated-callers` now lists deprecated calls made inside macros.
- `callers` and `impact` are deliberately unaffected: the new `macro_call`
  reference kind stays out of the call graph, keeping blast-radius precise.
- Method calls (`x.method()`) and path-qualified calls (`m::f()`) inside
  macro arguments remain unindexed for now.
