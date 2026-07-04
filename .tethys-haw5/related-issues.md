# tethys-haw5 — prior art (tracker sweep, 2026-07-03)

- **tethys-jdly** (closed) — Rust deprecated-callers analysis this slice extends. PR #9.
- **tethys-l6nt** (open) — parent PRD; C# parity user story.
- **tethys-53iv** (open) — method calls resolve by name only; misattribution risk.
  Relevant: C# caller lists ride on resolution; ticket says conservative/narrow is acceptable.
- **tethys-zwaz** (open) — converge analysis-command CLI output (envelope fences, shared
  display helpers). Relevant to AC "JSON output shape identical across languages".
- **tethys-xov3** (closed) — C# nested type extraction (substrate this relies on).
- **tethys-itz7** (closed) — imports stored for Rust + C# (feeds using-corroboration).

Code prior art: tests/deprecated_callers.rs:382 (test C11) already fences the gap —
C# [Obsolete] yields no findings because csharp.rs hardcodes `attributes: Vec::new()`.

Filed from this probe:
- **tethys-xebx** — C# parser doesn't extract properties; [Obsolete] on a property is
  invisible (symbol + attribute + member-access refs all missing). discovered-from haw5.
