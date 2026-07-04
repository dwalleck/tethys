# tethys-haw5 probe findings (prove-it-prototype, 2026-07-03)

Probes ran against **real production data**, not fixtures:
- C# side: `Tethys.Results` (dwalleck's published C# library; copied to scratchpad, 31 .cs files)
- Rust side: `rand-0.8.5` straight out of the cargo registry (4 genuine `#[deprecated]` items)

Probe scripts: `probe1.sh` (C# gap, end-to-end), `probe2.sh` (C# resolution substrate),
`probe3.sh` (Rust storage shape + JSON target). Each prints DB/CLI state; oracles are
raw-text `grep` scans compared item by item.

## Oracle

The oracle for every slice is `grep -rn` over the workspace source — raw text scanning,
sharing no mechanism with tree-sitter parsing or the SQLite index. Agreement achieved on
three slices:

1. **C# gap (probe1)**: grep finds exactly 1 `[Obsolete]` (GenericResult.cs:37, on property
   `Data`); index has 0 attribute rows, no `Data` symbol, 0 findings. Disagreement fully
   diagnosed as two substrate gaps (one = this ticket, one newly filed as tethys-xebx).
2. **Resolution substrate (probe2)**: index lists 12 resolved cross-file sites for
   `Combine`; grep finds exactly those 12 as `Result.Combine(...)` — item-by-item equal.
   Grep's extra sites are all explained: 6 × `Result<int>.Combine` (generic receiver,
   unresolved), 3 × `HashCode.Combine` (BCL, correctly not misattributed).
3. **Rust shape (probe3)**: grep finds exactly 4 `#[deprecated]` in rand-0.8.5; the
   attributes table has exactly those 4 rows, args text verbatim (incl. one multi-line).

## What I learned (that the ticket doesn't say)

**The ticket's blocker statement understates the gap: C# extraction has no property
symbols and no member-access refs at all, so the most common real-world `[Obsolete]`
carrier (a property — the only one in the probe repo) stays invisible even after
attribute extraction ships; method-scoped AC1 is achievable, but only for
static-receiver invocations, because instance calls through variables never resolve
(~10% overall call resolution in the probe repo).**

## Facts the design must stand on

- **Storage shape to match (AC2)**, from real Rust rows:
  `attributes(symbol_id, name, args, line)` — `name` = leading identifier (`deprecated` →
  C# `Obsolete`), `args` = raw text inside outermost parens, whitespace preserved,
  NULL for bare markers; `line` = the attribute's own line.
  C# mapping: `[Obsolete("msg", true)]` → name=`Obsolete`, args=`"msg", true`.
- **JSON target (AC4)**, from real Rust run: `summary{symbol_count, with_callers, clean,
  site_count}` + `deprecated[]{symbol{name, kind, file, line, since, note}, sites[{file,
  line, column, caller, tier, via}]}`.
  Open design question: `since` is Rust-only; C#'s error flag (AC3) needs a home —
  strict shape identity vs. an `error` field must be decided in falsifiable-design.
- **AC1 fixture constraint**: the `[Obsolete]` method must be invoked with a static/type
  receiver (`Legacy.Run()` style) or be a constructed class — variable-receiver instance
  calls (`svc.Run()`) do not resolve and would make AC1 unfalsifiable-in-the-good-way
  (always empty). Do not loosen resolution to dodge this (ticket forbids it).
- **C# `function` kind = static method** in the existing index vocabulary; instance
  methods are `method`.
- Related: tethys-xebx (property gap, filed from this probe), tethys-53iv (Rust
  misattribution — C#'s conservatism is the intended contrast), tethys-zwaz (JSON
  envelope convergence).
