# tethys-8ym0 — tracker prior art (prove-it-prototype step 0)

Searched rivets for macro/token-tree/refs issues before probing.

## Direct lineage
- **tethys-ygjx** (closed, PR #19) — parent issue. Shipped category 1
  (fn-as-value `value` refs + local-binding scope guard). Category 2 (macro
  token trees) was measured by its probe (`.tethys-ygjx/findings.md`: 7078 raw
  token-tree identifiers / 893 matching in-crate fn names / 591 call-shape)
  and split out as THIS issue. Fence pinning today's behavior:
  `macro_token_identifier_not_emitted_as_value` (src/languages/rust.rs).
- **tethys-y3bx** (open, P2) — untested-code analysis, parked specifically on
  this gap (assert-macro calls produce no ref → best-tested code reads
  untested). Its `.tethys-y3bx/probe.py` + findings are the downstream
  impact oracle: 251 "untested" symbols measured on self-index with this gap
  present.

## Siblings (same substrate, different scope — NOT fixed here)
- **tethys-9l27** (open, P3) — *method-call* shapes inside macro args
  (`assert_eq!(x.unwrap(), …)`) invisible → panic-points under-reports
  (68-site delta measured in `.tethys-53iv/probe3`). 8ym0 targets
  *call-shape identifiers* (free-fn calls `foo(...)`); method shapes stay
  out unless the design says otherwise. NOTE: 9l27's description claims ygjx
  "shipped VALUE refs for macro-token-tree identifiers" — that is wrong per
  the code (MACRO_INVOCATION returns early, rust.rs:226-236) and per the
  ygjx fence; correct the wording when 9l27 is touched.
- **tethys-0nar** (open, P3) — symbol *definitions* inside macro invocations
  (`proptest! { fn … }`) not indexed. Distinct: definitions, not refs.
- **tethys-i09d** (open, P3) — scoped-identifier value uses
  (`crate::foo` / `Type::assoc` as a value). Qualified call shapes inside
  token trees (`crate::foo(...)` in a macro) intersect both issues; design
  must draw the line.

## Machinery this fix consumes
- **tethys-53iv** (closed 2026-07-14) — receiver-typed method resolution;
  `ReceiverCtx` now threads through `extract_references_recursive`. Macro-arm
  changes must not disturb it.
- **tethys-9z7i** slices 1-3 (shipped, ADR-0003) — resolution-strategy
  provenance + `refs_banded` speculative band; the issue's suggested landing
  zone for macro-token refs ("band as speculative so precision consumers can
  exclude").
- **tethys-ml05 / batch-vs-streaming parity (tethys-qycb)** — any new ref
  kind must behave identically in batch and streaming index paths (fence
  equivalence lesson from CBM comparison).
