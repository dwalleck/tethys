# tethys-jdly — one-shot zbus audit (C8, manual fence approved 2026-07-03)

Run: `tethys -w ~/repos/amazon-q-developer-cli/crates/zbus deprecated-callers
--json` on a fresh `--rebuild` index, built from branch
`feat/tethys-jdly-deprecated-callers` (post-S7). Oracle: rustc
`cargo rustc --profile check -p zbus@4.4.0 -- --force-warn deprecated`
warning list captured in `findings.md` (6 real uses).

## Result

| Measure | Value | Verdict |
|---|---|---|
| Deprecated symbols | 12 | matches Q1 oracle (12 symbol-attached; 32 `pub use` = tethys-tthy) |
| Definite sites | 2 — `method_call` mod.rs:105, `method_return` mod.rs:125 | **both rustc-confirmed → precision 100%, zero false Definite** |
| Maybe sites | 17 (all `via=resolved`) | contain the other 4 rustc-real sites + 13 phantoms — honest tier |
| Path B rows | 0 | matches design measurement (qualified-only; all 36 bare matches were noise) |
| Clean | `new_bare` ×2 | rustc-confirmed: no `new_bare` use warnings |
| JSON determinism | byte-identical across two runs | C9 at CLI level |
| `--force-warn` re-run | unchanged from findings.md (6 warnings) | oracle stable |

## C8 re-baseline (surfaced during the S6 gate, resolved here)

Design C8 predicted "Definite = exactly the 5 rustc-confirmed sites". Measured:
Definite = 2, both rustc-confirmed. The three demotions to Maybe:

- `auth_mechanisms` ×2 — a NON-deprecated `struct_field` named
  `auth_mechanisms` exists (connection/builder.rs:91); the tier rule counts
  all symbol kinds as ambiguity candidates.
- `error` mod.rs:135 — `error` collides with several non-deprecated symbols.

This is the tier rule working as specified, not a bug: tethys name-only
resolution does NOT kind-gate (only Macro is gated, per tethys-v1w8), so a
phantom binding to a field-named symbol is genuinely possible; excluding
fields from candidacy would trade real safety for 3 upgraded rows. The
design-time sentence was a prediction made without running the tier SQL on
zbus (only the raw refs-join was run there). **Amended C8: every Definite
row is rustc-confirmed (precision 100%), and Path B contributes 0 rows on
zbus.** Both halves measured true. The load-bearing property — no false
Definite, no false clean — holds.

Permanent fence: `tests/deprecated_callers.rs::same_file_phantoms_never_definite`
(embeds the zbus same-file phantom shape; carries a TRIPWIRE assert
documenting the current tethys-53iv binding, to flip when 53iv lands).
