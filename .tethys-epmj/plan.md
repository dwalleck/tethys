# Budgeted plan — tethys-epmj: `PackageNotFound` variant

Design: `.tethys-epmj/design.md` (approved; Display option (a) byte-identical,
payload = bare name, package variant only). Claims 1-7 map to two slices;
claims 3/5/7 are verification-time properties enforced in slice verification
(the slice-done definition already requires the prove-it oracle re-run).

## Slice 1: Add the variant — `Error::PackageNotFound` + Display + categorization

**Claim:** Design claims 1 (Display renders exactly `not found: package '{name}'`),
4 (`IndexErrorKind::from` → `IoError`), 6 (the categorization match is the
only forced consumer — stays wildcard-free).

**Oracle:** Probe slice A's captured pre-change stderr
(`error: not found: package 'zzz-definitely-not-a-package'`) — the unit
test asserts `to_string()` equality against that literal, so the runtime
capture (not the thiserror attr) is the ground truth. For claim 4: the
error.rs module-doc 4xx/5xx rule (user-fixable input → `IoError` bucket,
same as `NotFound`).

**Stress fixture:** Display unit test over adversarial payloads, expected
outputs written here, before implementation:

| payload | expected `to_string()` |
|---|---|
| `zzz-definitely-not-a-package` | `not found: package 'zzz-definitely-not-a-package'` |
| `` (empty) | `not found: package ''` |
| `päckage-ü` | `not found: package 'päckage-ü'` |
| `it's` (embedded quote) | `not found: package 'it's'` (verbatim, no escaping) |
| `package 'x'` (payload that looks pre-formatted) | `not found: package 'package 'x''` (no de-duplication — proves construction never double-formats) |

Bug classes targeted: escaping/quoting surprises; accidental
`format!`-style pre-decoration; ASCII assumption.

**Loop budget:** No new loops. Enum variant + match arm + `#[error]` attr.

**Wall budget:** n/a (not an always-on phase).

**Files:** `src/error.rs` only.

**Code (advisory):**
```rust
/// Requested package was not found in the index.
///
/// The payload is the bare package name as requested (no formatting),
/// so programmatic consumers can use it without parsing. Rendering is
/// deliberately byte-identical to the legacy `NotFound("package '…'")`
/// payload shape.
#[error("not found: package '{0}'")]
PackageNotFound(String),
```
and in `From<&Error> for IndexErrorKind`: add `| Error::PackageNotFound(_)`
to the `IoError` arm. Extend `error_to_index_error_kind_mapping` test.
New test `package_not_found_display_exact` with the fixture table above.

**Verification:**
- [ ] Unit tests pass (`cargo nextest run error`)
- [ ] Stress fixture (table above) produces expected outcomes
- [ ] prove-it-prototype oracle still agrees with binary (probe.sh diff —
      trivially unchanged in this slice: no construction site converted yet;
      run anyway to prove the no-op)
- [ ] No new loops (budget trivially holds); full gates: nextest, clippy
      pedantic `-D warnings`, fmt --check, doctests
- [ ] Claim-6 arm check: `From<&Error>` match contains no `_` wildcard

## Slice 2: Convert the CLI construction site + retype the fences

**Claim:** Design claims 2 (`run_detail_to` returns
`Err(PackageNotFound(n))`, `n` == bare requested name, text AND json modes),
5 (other 27 `NotFound(format!` sites untouched), 3/7 (end-to-end CLI
behavior byte-identical, miss and success paths).

**Oracle:** probe.sh committed pre-change captures (slices A/A2/A3 +
control) — re-run post-change, diff must be empty. For claim 5: `grep -c
"Error::NotFound(format!" src/` == 27 (grep, NOT `tethys callers` — the
tool cannot see this site, tethys-staf). For claim 2's payload: string
equality against the literal argument passed in the test (independent of
Display machinery).

**Stress fixture:** Unit tests over `run_detail_to` with a `Vec<u8>` writer
(existing test harness pattern), expected outcomes written before
implementation:

| input | mode | expected |
|---|---|---|
| missing `no-such-pkg` | text | `Err(PackageNotFound(n))`, `n == "no-such-pkg"`; nothing on writer |
| missing `no-such-pkg` | json | `Err(PackageNotFound(n))`, `n == "no-such-pkg"`; writer == `null\n` |
| missing `it's-nöt-here` (quote+unicode) | text | payload verbatim `it's-nöt-here` — catches decoration/escaping at the construction site |
| BrokenPipe on `null` write (existing test) | json | still `Err(PackageNotFound(_))` — pipe error must not mask the variant (retype existing fence) |
| existing package | both | unchanged success (existing tests keep passing) |

Bug classes targeted: json arm forgotten (only text converted); payload
decorated with `package '…'`; BrokenPipe masking regression; over-reach
into other flavors (grep count).

**Loop budget:** No new loops. One expression changes; tests change asserts.

**Wall budget:** n/a.

**Files:** `src/cli/coupling.rs` only.

**Code (advisory):**
```rust
Err(tethys::Error::PackageNotFound(name.to_string()))
```
Retype asserts in `run_detail_text_mode_returns_not_found_err`,
`run_detail_json_mode_writes_null_then_returns_not_found_err`,
`run_detail_json_notfound_swallows_broken_pipe_and_still_returns_not_found`
to `matches!(err, tethys::Error::PackageNotFound(ref n) if n == …)`; add
the quote+unicode payload case. Keep test names' `not_found` stems accurate
(rename to `package_not_found` where the assert changes meaning).
Doc comments: backtick all identifiers (clippy pedantic `doc_markdown`
runs on tests too).

**Verification:**
- [ ] Unit tests pass (`cargo nextest run coupling`)
- [ ] Stress fixture table produces expected outcomes
- [ ] prove-it-prototype oracle agrees: probe.sh re-run, slices A/A2/A3 +
      control diff empty against pre-change capture
- [ ] Claim-5 gate: `grep -c "Error::NotFound(format!" src/` == 27
- [ ] No new loops; full gates: nextest, clippy pedantic `-D warnings`,
      fmt --check, doctests

## Impact analysis discipline

Per AGENTS.md two-tier rule: `tethys callers` is the precision tier, grep
the recall net — and for THIS feature grep is the source of truth, because
the converted site is exactly the shape `tethys callers` cannot see
(tethys-staf, filed from the probe). Slice 2's claim-5 gate encodes this.

## Doc-comment preconditions

The new variant's doc comment states the payload semantic ("bare package
name, no formatting"). Classification: **not a caller precondition** — it
documents what the single constructor site puts in; enforced by slice 2's
payload-equality tests, not by runtime checks. No `debug_assert!` needed.

## Output streams

No new writes. Existing classification unchanged and correct: coupling
detail/`null` → stdout (data); suggestion block + error line → stderr
(diagnostic).

## Tracker references

- tethys-staf (verified, filed this session): resolver blind spot → grep
  is claim-5's oracle.
- tethys-2uv6 (verified, filed this session): `#[non_exhaustive]` posture,
  out of this plan's scope.
- tethys-o4re (verified, pre-existing): the MCP consumer that will
  construct `PackageNotFound` from `get_package_coupling()`'s `None` —
  not built here.

## Plan Self-Review

1. **Loops:** none added in either slice — no complexity statements needed
   beyond "no new loops"; budgets trivially hold.
2. **Fixtures:** slice 1 targets escaping/pre-decoration/ASCII bugs (incl.
   the pre-formatted-payload trap); slice 2 targets forgotten-json-arm,
   decorated-payload, BrokenPipe-masking, over-reach. No happy-path-only
   fixtures.
3. **Doc-comment preconditions:** one payload-semantic note, classified
   non-precondition, enforced by tests (above). No unenforced contracts.
4. **Write targets:** no new writes; existing classification audited (above).
5. **Tracker references:** all three cited IDs verified to exist and cover
   the deferred work (staf, 2uv6 filed this session; o4re shown at probe
   step 0). No uncited deferrals.
