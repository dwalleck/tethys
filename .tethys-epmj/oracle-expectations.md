# Pre-registered oracle expectations — tethys-epmj

Written BEFORE running probe.sh. Oracle mechanism: static derivation from
source (thiserror attributes + main.rs rendering), independent of the
runtime probe.

## Slice A: `tethys coupling --package zzz-definitely-not-a-package` (text mode)

Derivation chain:
- `src/cli/coupling.rs:83` → `get_package_coupling` returns `Ok(None)`
- `src/cli/coupling.rs:91` → `(None, false)` writes nothing to stdout
- `src/cli/coupling.rs:100` → suggestions printed to stderr only when a
  package name contains the needle; `zzz-definitely-not-a-package` matches
  nothing → no suggestion block
- `src/cli/coupling.rs:101` → `Error::NotFound("package 'zzz-definitely-not-a-package'")`
- `src/error.rs:50` → `#[error("not found: {0}")]`
- `src/main.rs:326` → `eprintln!("error: {e}")` (color suppressed off-tty)
- `Error::NotFound(String)` has no `#[source]` → no "caused by" lines
- `src/main.rs:333` → `ExitCode::FAILURE`

**Expected:** exit code 1; stdout empty; stderr exactly:
`error: not found: package 'zzz-definitely-not-a-package'`

## Slice A2: same, JSON mode

`(None, true)` writes `null` to stdout (coupling.rs:90), same stderr, exit 1.

**Expected:** exit 1; stdout `null`; stderr `error: not found: package '…'`

## Slice A3: `--package eth` (substring of a real package)

`collect_suggestions` is a case-insensitive `contains` filter;
"tethys".contains("eth") → suggestion fires.

**Expected:** exit 1; stderr contains `Did you mean: tethys?` AND the
`error: not found: package 'eth'` line.

## Slice B: NotFound construction-site inventory (blast radius)

Probe mechanism: tethys's own AST index (references/callers query) —
dogfooding. Oracle mechanism: grep text pipeline over src/.

Grep-derived expectation (payload prefix → count, non-test code only):

| prefix        | sites | files |
|---------------|-------|-------|
| `file: `      | 8     | lib.rs |
| `symbol: `    | 6     | lib.rs (5), resolve.rs (1) |
| `file id: `   | 6     | lib.rs (1), resolve.rs (2), db/graph.rs (3) |
| `symbol id: ` | 3     | db/graph.rs |
| `package '`   | 1     | cli/coupling.rs |
| `type: `      | 1     | db/hierarchy.rs |
| (other)       | ?     | db/graph.rs:637 multiline format! — prefix TBD |

(Counts above are my reading of the grep output during step 0; the probe
run recounts them mechanically and the comparison happens in findings.md.)
