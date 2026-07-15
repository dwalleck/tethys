- New `tethys untested-code [--json]` command: lists product functions and
  methods that no test can reach, walking the reference graph from every
  indexed test (Rust `#[test]` and friends, C# `[Fact]`/`[Test]`/etc.).
- Functions exercised only through assert-style macros count as reached.
- With no tests indexed the result is reported as indeterminate (with a
  warning) instead of listing the entire workspace.
- Reachability, not verification: `--help` and the report footer name the
  known false-positive sources.
