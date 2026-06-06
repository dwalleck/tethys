# Probe findings (prove-it-prototype, 2026-06-06)

Probe: `.separator-fix/probe.sh` — real binary, production-shape C# mini-workspace
(namespaces, using directives, cross-file qualified call, nested type) + tethys
self-index for the Rust side.

## Results

| Q | Spec's assumption | Reality (probe) | Oracle confirmation |
|---|---|---|---|
| Q1 Rust dotted refs | believed impossible | 0 of 11,657 refs contain `.` | source has dotted method calls (`rg '\w+\.\w+\('`), DB has none → Rust extractor never emits `.` ✓ |
| Q2 C# storage separator | `.` per audit of batch_writer.rs:378 | **`::`** — `AuthService::Login`, `Hasher::Hash`; only imports.source_module and namespace names use `.` | `tethys search Hash` (different query path) shows `Hasher::Hash`; batch_writer.rs:360 joins refs/quals with hardcoded `"::"` for ALL languages — the `.` match at :378 applies ONLY to store_imports |
| Q3 post-fix `.` lookup | would newly match | `Hasher.Hash` → no match; `Hasher::Hash` → MATCH | direct SQL on actual storage |
| Q4 nested types | `Outer::Inner` (csharp.rs:736) | confirmed: `Outer::Inner` | consistent with Q2: ALL C# quals use `::` |
| Q5 cross-file C# qualified call | "silently falls through, unresolved" | **RESOLVES TODAY**: `call@Auth.cs:10 → Hasher::Hash`, `construct@Auth.cs:11 → Outer::Inner`; only external `Console::WriteLine` unresolved | join refs→symbols matches hand-read ground truth of fixture; ref count (3) matches grep of source |

## What I learned (one sentence)

C# references and qualified names are already stored AND resolved with `::` — the
extractor's segment paths are joined with a hardcoded `"::"` for all languages
(batch_writer.rs:360), so C# cross-file qualified refs resolve today via the
qualified-name fallback, the "C# refs silently fall through the :: gate" bug is
FALSE, and decision #4 (unify C# storage to `.`) would BREAK working resolution
unless storage and resolution flip in lockstep.

## Implications for the spec (must return to interrogation)

1. The premise in "What this is" is wrong — this is not a bug fix; it is a pure
   seam refactor plus a SEPARATOR POLICY decision that is now open again.
2. B2/B3 baselines are wrong: both behaviors already pass pre-change (with `::`
   spellings). The C# monotone criterion has a non-zero baseline.
3. Decision #4 needs re-deciding with correct facts. The real choice is:
   internal canonical `::` for all languages (behavior-neutral, strict oracle
   for BOTH languages) vs. flipping C# storage+resolution to `.` together
   (user-visible output changes from `AuthService::Login` to `AuthService.Login`).
4. The genuinely broken thing remains tethys-jwf9 (C# import arms dead-end:
   dotted source_modules can never resolve through Rust-only resolve_module_path)
   — already out of scope by decision #2.
