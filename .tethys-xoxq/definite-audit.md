# tethys-xoxq C14 audit: Definite tier vs grep oracle (2026-07-04)

Workspace: amazon-q-developer-cli (43 packages), indexed at build S10 with
the shipped binary (incl. the tethys-lwsc glob-path fix). Run:
`tethys visibility-tightening --json --workspace-closed` (the widest
Definite set — it subsumes the default-flags set, whose Definite tier is
further restricted to non-root-reachable items).

Oracle: probe-phase grep pipeline — `grep -rn '\b<name>\b' crates/`
excluding the declaring crate; independent of the index.

## Verdicts (per symbol)

| Package | Definite finding | External grep hits | Verdict |
|---|---|---|---|
| fig_auth | `scopes_match` (function) | 0 | CONFIRMED |
| fig_auth | `SqliteSecretStore` (struct) | 0 | CONFIRMED |
| fig_ipc | `input_method_command` (function) | 0 | CONFIRMED |
| fig_ipc | `restart_command` (function) | 0 | CONFIRMED |
| fig_ipc | `LocalIpc` (trait) | 0 | CONFIRMED |
| fig_ipc | `send_recv_command_to_socket_with_timeout` (function) | 0 | CONFIRMED |

**Zero refuted Definite findings (6/6).**

## probe3 agreement (fig_auth)

probe3 re-run returns its original 5 candidates. The shipped Definite set
{scopes_match, SqliteSecretStore} is a strict subset; the deltas are
design-added demotions the probe predates, each verified:

- `BuilderIdToken`, `DeviceRegistration`, `OAuthFlow` → Maybe
  (`glob-reexport-risk`): their module is glob-imported inside fig_auth
  (design C6 / tethys-pv7w guard).
- `SecretStoreImpl` ×2 → Maybe (`shared-name`): cfg twins (design C4);
  probe3 excluded twins from its Definite set entirely — listing them as
  Maybe is the intended honest form.

## S8 drift note (for the record)

The first S8 oracle run returned fig_auth Definite = 0: the S2 glob
widening was head-keyed, so `use fig_auth::pkce::*` (fig_desktop_api)
suppressed the whole crate. Fixed in the S8 commit (`crate_glob_covers`,
module-exact); the fences `cross_package_glob_import_excludes_globbed_module_only`
and `crate_glob_coverage_is_module_exact` encode the bug class. The
deterministic floor for this audit lives in the S1–S7 fixture fences per
the design's Regression-fence column.

Not run: `cargo check` compile oracle (best-effort per plan) — the q-cli
workspace does not build in this environment (missing toolchain deps
unrelated to tethys).
