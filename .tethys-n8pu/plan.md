# tethys-n8pu plan — set-oriented direct file-deps hydration

Source: `.tethys-n8pu/design.md` (approved 2026-08-01). Claims C1-C5
already fenced by the probe test; this plan implements C6, C7, and the
C1-C5 count assertions that make the fence complete.

## Slice 1: Set-oriented hydration + count fence

**Claim:** C6 (exactly 2 statements, 0 per-ID lookups, any N) plus C1-C5
assertions completed in the probe fence; C9 (API unchanged: Tethys
signatures untouched; crate-internal ID getters deleted — approved drift,
see design.md Architecture).

**Oracle:** the probe test's rusqlite `trace` hook counts statements on the
live connection (independent runtime measurement); the direct-SQL JOIN
oracle for the result sets; code inspection for the removed loop.

**Stress fixture:** probe fixture (lib.rs → a.rs, b.rs, c.rs; a.rs → c.rs;
b.rs leaf). N=3 deps and N=2 dependents force the count assertion to be
non-vacuous (a single-result fixture would pass with 1 leftover per-ID
lookup); the wrong-JOIN-direction bug flips the returned sets; the
empty-root and missing-root slices pin C3/C4; dedup assertion pins C5.

**Loop budget:** the per-ID lookup loop (`file_ids_to_paths`, O(N) SQL
statements, N = result count) is REMOVED. Replacement: one set-oriented
LEFT JOIN (SQLite executes it as index scan + row source lookups —
O(N) rows returned, N ≤ files ≈ 50k, inherent to output size). The
per-row dangling-warn loop iterates only rows with NULL path — zero
iterations at production scale (FK=ON makes dangling rows impossible);
O(N) worst case in corrupt DBs, bounded by result count. No wall-clock
phase introduced.

**Files:** `src/db/file_deps.rs` (add `get_file_dependency_paths` /
`get_file_dependent_paths`, return `(Vec<PathBuf>, usize missing)`;
DELETE dead `get_file_dependencies`/`get_file_dependents`),
`src/lib.rs` (rewire `get_dependencies`/`get_dependents`, delete
`file_ids_to_paths`), probe test in `src/db/file_deps.rs` gains count
assertions.

**Code (advisory):**

```rust
// db/file_deps.rs
/// Get workspace-relative paths of files `file_id` directly depends on.
///
/// Single set-oriented LEFT JOIN; `(paths, missing)` where `missing` is
/// the count of file_deps rows whose target file is absent (warned per
/// row — possible only in a hand-edited DB; FK cascades prevent it in
/// real indexes).
pub fn get_file_dependency_paths(&self, file_id: FileId) -> Result<(Vec<PathBuf>, usize)> {
    let conn = self.connection()?;
    let mut stmt = conn.prepare(
        "SELECT fd.to_file_id, f.path
         FROM file_deps fd LEFT JOIN files f ON f.id = fd.to_file_id
         WHERE fd.from_file_id = ?1",
    )?;
    let rows = stmt.query_map([file_id.as_i64()], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    let mut paths = Vec::new();
    let mut missing = 0;
    for row in rows {
        let (dep_id, path) = row?;
        match path {
            Some(p) => paths.push(p.into()),
            None => {
                warn!(source_file_id = %file_id.as_i64(), missing_file_id = dep_id,
                    "file_deps references non-existent file, possible database corruption");
                missing += 1;
            }
        }
    }
    Ok((paths, missing))
}
// (mirror for dependents: SELECT fd.from_file_id, f.path ... WHERE fd.to_file_id = ?1)
```

```rust
// lib.rs — both methods take this shape
pub fn get_dependencies(&self, path: &Path) -> Result<Vec<PathBuf>> {
    let file_id = self
        .db
        .get_file_id(&self.relative_path(path))?
        .ok_or_else(|| Error::NotFound(format!("file: {}", path.display())))?;
    let (paths, missing_count) = self.db.get_file_dependency_paths(file_id)?;
    if missing_count > 0 {
        debug!(file = %path.display(), missing_count,
            "Some dependency file IDs could not be resolved");
    }
    Ok(paths)
}
```

**Verification:**
- [ ] `cargo test --lib db::file_deps::n8pu_probe` — count assertions
      (total == 2, per-id == 0 per direction) pass
- [ ] Stress fixture expected outcome: deps/dependents sets unchanged vs
      pre-fix probe output
- [ ] Oracle still agrees: probe == direct SQL on same db
- [ ] Budget: 0 per-ID lookups at N=3; loop removed from source
- [ ] `cargo check` — no dead code, no public signature drift (C9)

## Slice 2: Dangling-row defense fence

**Claim:** C7 — dangling `file_deps` rows are skipped with the established
per-row `warn!`; valid rows still returned; call does not error.

**Oracle:** `tracing_test::traced_test` capture of the `warn!` event;
direct SQL row count for the valid set.

**Stress fixture:** after indexing the probe fixture, open the db file with
a second connection, `PRAGMA foreign_keys = OFF`, INSERT a `file_deps` row
with `to_file_id = 99999` (dangling — FK bypassed by the second
connection), then call `get_dependencies("src/lib.rs")` through Tethys.
Expected: the 3 valid paths returned, exactly one `warn!` event
mentioning the dangling id, no error. Bug classes this fails under: inner
JOIN (silently drops the dangling row, warn never fires); hydration that
errors on corrupt rows; warn with wrong payload.

**Loop budget:** dangling-warn loop fires exactly once for the one injected
row (production scale: zero — FK=ON). O(N) worst case bound stated in
slice 1.

**Files:** `src/db/file_deps.rs` (test only).

**Code (advisory):**

```rust
#[tracing_test::traced_test]
#[test]
fn dangling_dep_row_is_warned_and_skipped() {
    // build probe fixture, index
    // second connection, FK OFF, INSERT INTO file_deps (from_file_id, to_file_id, ref_count)
    //   VALUES (<lib_id>, 99999, 1)
    // deps = tethys.get_dependencies("src/lib.rs").unwrap();
    // assert deps == [a, b, c] (3 valid, dangling skipped)
    // assert logs_contain("99999") && logs_contain("corruption")
}
```

**Verification:**
- [ ] Unit test passes (warn captured, valid set intact, no error)
- [ ] Oracle: direct SQL shows exactly 4 `file_deps` rows for lib.rs, 3
      with resolvable targets
- [ ] Budget: single dangling row exercised; no loop added

## Gate: full verification (C8, C9)

**Claim:** C8 — existing Rust/C# file-deps suites pass unchanged; C9 —
public API unchanged.

**Files:** none (read-only gate).

**Verification:**
- [ ] `cargo nextest run` full suite
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo fmt --check`
- [ ] doctests
- [ ] `git diff origin/main --stat` — only the two source files + probe

## Plan Self-Review

1. **Loops:** slice 1 removes the O(N)-statements loop; new per-row loops
   are O(N) output-size (paths) and O(dangling) (warn), both bounded and
   stated. Slice 2 adds no loop.
2. **Fixtures:** slice 1 fixture fails wrong-JOIN-direction, count
   regression, missing-root order, dupes; slice 2 fixture fails
   inner-JOIN-silent-drop, corrupt-row error, wrong warn payload. No
   happy-path-only fixtures.
3. **Doc-comment preconditions:** none introduced — the new Index methods
   take a valid `FileId` (caller-owned), return `Result`; no
   load-bearing caller preconditions to enforce.
4. **Write targets:** no stdout/stderr writes in shipped code; probe
   `println!`s are `#[cfg(test)]` diagnostics only. No CLI output
   changes.
5. **Tracker references:** tethys-71if (ID-getter contraction), tethys-zoi3
   (future file_deps coverage), tethys-8ya3 (write-path batching) — all
   verified existing, all cited in design.md; plan introduces no new
   deferrals.
