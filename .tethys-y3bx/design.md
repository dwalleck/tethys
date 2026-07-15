# tethys-y3bx — falsifiable design: untested-code analysis

## Purpose

`tethys untested-code`: report product functions/methods that no test can
reach — multi-root forward closure from `is_test` symbols over the reference
graph, complement against product fn/method symbols. PRD Act 1 stage 3;
deliberately precedes dead code (a wrong "untested" claim wastes a test run,
a wrong "dead" claim deletes live code).

## Probe evidence this design stands on (`findings.md`, resumed section)

- The traversal substrate is load-bearing NOW: untested(refs)=235 vs
  untested(call_edges)=266 on self-index (gap=30 — every assert-only-tested
  fn), because `macro_call` refs are excluded from call_edges (8ym0 posture).
- 235 cross-validates the independent `.tethys-8ym0/probe2.py` prediction.
- Grep-trace oracle 4/4 item agreement (crate_glob_covers/scalar/
  is_excluded_dir TESTED; print_reachability_result UNTESTED).
- Composition: 77/235 are scoping clusters (benches 20, CLI 43, LSP 14),
  ~14 are the 9l27 method-shape class, 5 proptest (0nar).

## Core design

1. **Facade** `Tethys::get_untested_code()` → `Vec<UntestedFinding>`
   (name, kind, file, line, `module_path`-qualified display name), backed by
   `src/db/untested.rs`:
   - roots = `SELECT id FROM symbols WHERE is_test = 1`;
   - edges = one query `SELECT in_symbol_id, symbol_id FROM refs WHERE both
     NOT NULL` into an adjacency map; single multi-root BFS (O(V+E), no
     depth cap, no path tracking — `bfs_reachable` is single-source +
     path-tracking + call_edges, the wrong tool three ways);
   - report = product symbols (`is_test=0`, kind ∈ {function, method}) not
     in the closure, sorted (file, line).
2. **CLI** `tethys untested-code [--json]` following the house analysis
   pattern (`cli/visibility_tightening.rs` template): human table grouped by
   file; JSON `{summary: {test_roots, product_fns, untested_count},
   findings: [...]}` via `to_json_pretty`; data→stdout, diagnostics→stderr,
   `write_report` for BrokenPipe safety (no new divergence for tethys-zwaz).
3. **Zero-roots posture** (`test_roots == 0`): the analysis is
   *indeterminate*, not "everything is untested" — empty findings, summary
   carries `test_roots: 0` + `indeterminate: true`, stderr note ("no test
   roots indexed — result indeterminate"). Suppression over a 697-row
   accusation dump; vocabulary shared with tethys-09wx (confirmed-empty vs
   indeterminate).
4. **No default exclusions**: benches/, CLI-layer, and LSP files are
   reported like everything else — path-sorted output makes the clusters
   visible; downstream filters (jq on `--json`) handle scoping. Revisit only
   on consumer demand.
5. **Semantics honesty**: "reached by a test" ≠ "asserted on" — this is
   reachability, not coverage quality; module docs + `--help` say so.

Error posture: the report is a *suggestion to investigate* (cheapest failure
mode in the whole roadmap); known FP classes are documented, not silently
filtered.

## AC #2 rewording (flag D-D)

The issue's AC #2 says "traversal consumes refs including top-level
references — a fixture pins a case that call_edges alone would miss". Two
corrections, both probe-verified:
- Top-level refs (`in_symbol_id NULL`) **cannot participate in any
  reachability traversal** — an edge needs a source symbol. Documented
  limitation, not a satisfiable fixture.
- The honest, now-real pin is the **kind-exclusion divergence**: a
  `macro_call`-only-tested fn is reachable via refs and invisible via
  call_edges. The fixture exists (the 8ym0 F1 workspace) and the fence
  asserts the divergence directly.

## Input shapes

| # | shape | handling |
|---|-------|----------|
| S1 | fn called directly from `#[test]` | not reported (C1) |
| S2 | fn reached transitively, incl. through a cycle | not reported (C3) |
| S3 | fn tested ONLY via assert-macro (`macro_call` edge) | not reported; call_edges lacks the edge — divergence fenced (C2) |
| S4 | fn no test reaches | reported with name/kind/file/line (C1) |
| S5 | `is_test` symbols themselves | never reported (C6) |
| S6 | non-fn/method symbols (structs, consts, variants) | out of kind scope (C6; documented) |
| S7 | zero test roots | indeterminate posture (C4) |
| S8 | zero product fns | empty report, exit 0 (C4 fixture arm) |
| S9 | C# `[Fact]` root + tested/untested C# methods | parity (C5) |
| S10 | untested fn calling another fn (a→b, no test) | BOTH reported (closure starts at roots only; C1 fixture arm) |
| S11 | self-recursive untested fn | reported once (C3 cycle arm) |
| S12 | benches// CLI-layer fns | reported, path-sorted (design §4) |
| S13 | dyn-dispatch-only reached method | false "untested" — documented (tethys-j2r1 type hierarchy is dead-code-stage infra) |
| S14 | proptest-defined test fns' callees | false "untested" — documented (tethys-0nar) |
| S15 | method-shape-in-macro tested (`x.as_str()` in assert) | false "untested" — documented (tethys-9l27, ~14 self-index sites) |
| S16 | top-level refs | not traversable (no source); documented (D-D) |
| S17 | `--json` | stable envelope (C7) |
| S18 | duplicate same-named fns | independent by symbol id (C1 collision arm) |

Subtractive sweep: purely additive (new read-only analysis; no constraint
removed, no existing query touched). One sentence, per the design rules.

## Falsification

Fences in a new `tests/untested_code.rs` unless noted; every fixture builds
its own index.

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| C1 | Report = product fn/method ∖ refs-closure from is_test roots | probe BFS (independent Python impl of the same rule) vs real data + grep-trace items | probe 235 + 4/4 grep items | done | **passed** (resumed probe = cheapest falsifier) | F-U1 tested/untested pair + S10 pair + S18 collision |
| C2 | Assert-only-tested fn reads TESTED; call_edges provably lacks the edge | 8ym0-F1-shaped fixture; assert fn absent from report AND `call_edges` empty for it | SQL on fixture index (two independent asserts) | 15m | pending | F-U2 — fails if traversal ever switches to call_edges |
| C3 | Transitive closure incl. cycles | chain test→a→b + cycle b↔c fixture; none reported | hand-enumerated expectation | 10m | pending | F-U3 |
| C4 | Zero-roots → indeterminate (empty findings, flagged summary, stderr note); zero-prod-fns → empty, exit 0 | no-test fixture + all-test fixture | JSON fields + exit code | 10m | pending | F-U4 |
| C5 | C# parity: `[Fact]` root covers callees; untested C# method reported | C# fixture (tested + untested methods) | hand-enumerated | 15m | pending | F-U5 |
| C6 | is_test symbols and non-fn/method kinds never reported | fixture with test fns + structs/consts | report scan | in F-U1 | pending | F-U1 asserts |
| C7 | CLI: `--json` stable envelope {summary{test_roots, product_fns, untested_count}, findings[]}; (file,line) sort; data→stdout | run CLI on fixture, parse JSON, check order | serde parse + jq-style field asserts | 15m | pending | F-U7 (drives through the binary seam per PRD test posture) |
| C8 | Self-index items: crate_glob_covers TESTED, print_reachability_result UNTESTED, count == probe | run binary on tethys itself vs probe | probe + grep-trace | 10m | pending | **audit-only** (self-index counts drift with every commit; CI fences are the fixtures) — needs approval |
| C9 | O(V+E) single pass; self-index wall < 1s | time the command | `time` | 5m | pending | **manual** (audit number; same posture as 8ym0 C12) — needs approval |

Cheapest falsifier (C1) ran before this document: the resumed probe computes
the exact proposed rule against the real index with independent oracle
agreement (see findings.md).

## Negative space (deliberately not doing)

1. **No coverage-quality claim** — reached ≠ asserted-on; docs say
   "reachability, not verification" (settled semantics, not deferred work).
2. **No default scope exclusions** (benches/CLI/LSP) — report-all,
   path-sorted; downstream filters cut scope (settled unless D-A overturns).
3. **Known FP classes documented, not fixed here**: method-shape macro calls
   (tethys-9l27), proptest-defined fns (tethys-0nar), dyn dispatch
   (tethys-j2r1, promoted to Act-1 infra for the DEAD-CODE stage, not this
   one).
4. **No MCP tool** — Act 2 (tethys-o4re); the facade method is the seam it
   will wrap.
5. **No --depth flag** — the closure is total by definition; depth semantics
   belong to callers/impact (tethys-w0qw / tethys-3yxn).
6. **No is_test detection changes** — roots are whatever s8hv-era detection
   marks; proptest gap stays with tethys-0nar.

## Open decisions flagged for approval

- **D-A (report posture)**: report ALL untested product fns path-sorted
  (recommended) vs default-excluding benches//CLI clusters behind an
  `--all` flag. Probe: 77 of 235 self-index rows are those clusters.
- **D-B (zero-roots posture)**: indeterminate (empty findings + flagged
  summary + stderr note, recommended) vs truthful-but-useless report-all.
- **D-C (naming/scope)**: command `untested-code`, kind scope
  function+method only (recommended; matches probe and prior art).
- **D-D (AC #2 rewording)**: accept the corrected AC — divergence fenced via
  the macro_call case; top-level refs documented as non-traversable.
- **D-E (fence posture)**: C8 audit-only and C9 manual (recommended; same
  approval shape as 8ym0's D-C).
