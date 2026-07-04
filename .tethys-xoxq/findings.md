# tethys-xoxq probe findings (prove-it-prototype, 2026-07-04)

Probed against the real amazon-q-developer-cli workspace (43 packages,
56,500 fully package-attributed resolved refs, 6,482 cross-package),
indexed with release tethys at HEAD. Probes: `probe1.sh` (endpoint
attribution for one symbol), `probe2.sh` (naive candidate list),
`probe3.sh` (evidence-based tier rule).

## Oracle

Independent mechanism: `grep -rn` over workspace sources with crate
attribution by path prefix (`crates/<name>/`), qualified-use classification
by `fig_auth::…` / `use fig_auth::…` patterns (`oracle1.sh`). The index
never touches this pipeline. Agreement achieved on two slices:

- **Slice 1** (`start_device_authorization`): probe's cross-package sites =
  oracle's genuine use sites, 2/2 item-for-item (oracle's third hit is a
  `use` line — imports are not refs by convention).
- **Slice 3** (Definite tier rule on all of fig_auth): 5/5 survivors have
  zero external grep hits. Naive refs-only rule: ≥5/15 candidates false.

## What I learned (the one-sentence version)

A pub item's missing cross-crate evidence is usually not missing use — it
is use that the resolver either stole (same-file name-only binding NULLs
the qualified text, destroying the evidence), never recorded (macro token
trees), or declined (non-unique names) — so Definite tightening must be
gated on evidence *recoverability*, not on ref absence.

## Substrate facts (all verified, receipts in probe output)

1. **Package attribution exists and is correct**: `arch_packages` /
   `arch_file_packages` (manifest-sourced). Both ref endpoints join cleanly.
2. **The self-index is ONE package**: lib and bin crates of one Cargo
   package merge, so `src/main.rs` consuming the lib's public API is
   invisible as cross-crate use. The self-index review (AC6) must account
   for this or every lib-pub item used by the CLI looks tightenable.
3. **Naive rule is 33% false on fig_auth** (5+/15): `logout`,
   `is_logged_in`, `is_amzn_user`, `BearerResolver`, `refresh_token` all
   have genuine cross-crate uses with zero cross-package resolved refs.
4. **Why, by filed bug** (step-0 predictions confirmed):
   - **tethys-53iv**: `fig_auth::logout()` at fig_desktop commands.rs:238
     bound to the same file's own `logout` (line 237). The phantom binding
     also NULLs `reference_name` — the `fig_auth::` prefix is
     UNRECOVERABLE from the index afterward.
   - **tethys-ygjx**: `matches!(fig_auth::is_amzn_user()…)` (midway.rs:24)
     produced no ref row at all — macro token-tree use.
   - **tethys-z9mr**: fig_auth's own `pub use builder_id::{…}` reexport
     refs stay unresolved for non-unique names.
   - cfg-gated twin symbols (`is_amzn_user` ×2 in builder_id.rs) defeat the
     unique-name fallback and uniqueness checks — a Maybe-inflation source.
5. **Cross-package resolution is mostly the unique-name fallback**: 5,384
   of 6,482 (83%) cross-package resolved refs bind workspace-unique names.
6. **Rescue evidence exists in-index**:
   - `imports` rows carry cross-crate `source_module`
     (`fig_auth::builder_id|BearerResolver` from fig_api_client, etc.).
   - Unresolved refs keep `reference_name` text (`fig_auth::refresh_token`)
     — recoverable by qualified-prefix matching, jdly-Path-B style —
     UNLESS 53iv stole the ref first.
   - Qualified-call-without-import on a name-collided symbol (fig_desktop's
     `fig_auth::logout()`) leaves NO recoverable evidence today. Only
     tiering absorbs it.
7. **The tier rule that survives the oracle**: Definite-tightenable =
   workspace-unique name AND zero cross-package evidence in (a) resolved
   refs, (b) imports rows targeting the symbol's package path, (c)
   unresolved-ref qualified text. Non-unique names are at most Maybe.
   (Probe3's last-segment SQL hack should be Rust-side in the real thing.)
8. **tethys-pv7w residual** (glob/module re-exports carry no refs): fig_auth
   has no glob re-exports, so this probe couldn't exercise it; the design
   must handle it by scope-out or tier-down.

## Hard-gate checklist

- [x] Probe runs against the real codebase (probe1–3.sh)
- [x] Oracle defined, independent, produces output (grep/oracle1.sh)
- [x] Agreement on non-trivial slices (2/2 sites; 5/5 Definite survivors)
- [x] Learned something new (evidence destruction by phantom binding; the
      lib/bin merge; the 33% naive false rate)
