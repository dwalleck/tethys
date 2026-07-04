# tethys-jdly probe findings (prove-it-prototype, 2026-07-02)

Target: vendored `zbus` 4.4.0 crate in `~/repos/amazon-q-developer-cli` (61 files,
1511 symbols, 10327 refs — real production code, 44 `#[deprecated]` attributes).
Probe: `probe.sh` (SQL over the tethys index). Index built with
`tethys -w .../crates/zbus index` (no LSP).

## Q1 — which symbols carry #[deprecated]?

**Probe**: `attributes.name = 'deprecated'` joined to symbols → 12 items, all methods.
**Oracle**: `oracle-q1.sh` — grep/awk over the source, attributing each attribute to
the item it precedes (handles multi-line attributes). → 44 items: 12 fn/method + 32
`pub use` re-exports.

**Verdict: exact agreement on the symbol-attached slice — 12/12 match on file, line,
name.** `attributes.args` preserves the raw `since = "...", note = "..."` text
(multi-line formatting intact), sufficient for the note/since acceptance criterion.

**Disagreement (structural, expected but load-bearing): deprecated `pub use`
re-exports are invisible.** Attributes hang off `symbol_id`; re-exports produce refs
(tethys-v1w8), not symbols, so there is nowhere to attach `#[deprecated]`. In zbus,
32 of 44 deprecations (73%) are the alias-rename idiom
(`#[deprecated] pub use message::Builder as MessageBuilder;`). Follow-up ticket filed
(see below); jdly scopes to symbol-attached deprecations and documents the limitation.

## Q2 — what refs point at deprecated symbols (the feature's actual output)?

**Probe**: join `refs` (not `call_edges` — top-level refs are skipped there) on
deprecated symbol ids → **19 call sites**.
**Oracle**: the Rust compiler's own name resolution:
`cargo rustc --profile check -p zbus@4.4.0 -- --force-warn deprecated`
(force-warn overrides the crate's `#[allow(deprecated)]`) → **6 real uses**.

**Verdict: DISAGREEMENT.**
- True positives (5): `Builder::method_call` mod.rs:105, `method_return` :125,
  `error` :135, `auth_mechanisms` blocking/connection/builder.rs:112 and
  connection/builder.rs:206. All five resolve correctly because the callee name is
  workspace-unique (or all candidates are deprecated anyway).
- False positives (14): every bare method call named `path`/`interface`/`member`/
  `reply_serial` on `QuickFields` or `Header` receivers (in `header()`, in the
  deprecated methods' own bodies, in Debug/Display impls) misattributed to the
  same-named deprecated `Message::` methods; plus `Message::signal` (test,
  builder.rs:392) bound to same-file deprecated `Builder::signal`.
- False negative (1): the actual deprecated use `Builder::signal` at mod.rs:119
  bound instead to same-file non-deprecated `Message::signal`.

Precision 5/19 (26%), recall 5/6 (83%) on real data. **Every error traces to
name-only method resolution — the already-filed tethys-53iv** (found in step-0
prior-art check). Skill taxonomy cause #1: substrate gap, already ticketed; jdly
must scope around it.

## What I learned (the one-sentence gate)

The jdly issue's claim "findings here are factual — no confidence tiers needed" is
**false**: on the first real crate probed, 74% of reported deprecated-callers were
phantom edges from name-only method resolution, so deprecated-callers needs the same
Definite/Maybe tiering posture as every other analysis (or must consume tethys-9z7i
provenance bands).

Secondary: the dominant real-world deprecation idiom (`#[deprecated] pub use`
alias renames — 73% of zbus's deprecations) is structurally unrepresentable in the
current schema.

## Hard-gate checklist

- [x] Probe written, runs against a real codebase (`probe.sh`, zbus 4.4.0)
- [x] Oracle defined and produces output (grep/awk for Q1; rustc `--force-warn
      deprecated` for Q2 — different mechanisms from the tethys index)
- [x] Probe and oracle agree on a non-trivial slice (Q1: 12/12 exact; Q2: the
      5 unique-name sites)
- [x] Learned something new (tiering claim falsified; re-export idiom invisible)

## Design constraints going into falsifiable-design

1. Query surface confirmed: `attributes(name='deprecated')` ⋈ `symbols` ⋈ `refs`
   (must include `in_symbol_id NULL` top-level refs; builder.rs:392 was one).
2. Findings MUST be tiered. Cheapest heuristic available today: callee-name
   uniqueness among indexed symbols (unique → Definite; multiple same-name
   candidates → Maybe/speculative). This reproduces rustc's verdict exactly on
   zbus: all 5 unique-name resolutions were true, all ambiguous ones wrong.
   Alternative: block on tethys-9z7i and consume its confidence band.
3. `#[deprecated] pub use` re-exports: out of scope, documented limitation,
   follow-up ticket.
4. Oracle for future fences: `--force-warn deprecated` compile of a fixture crate
   is a perfect independent oracle and should become the e2e test's ground truth.
