#!/usr/bin/env bash
# Probe driver for tethys-09wx. Constructs ground truth by manipulation:
#   1. fresh index -> every indexed file CURRENT by construction
#   2. drop a marker file, then touch one real file -> that file STALE by construction
#   3. ask about a file that never existed -> UNINDEXED by construction
# Then shows the CLI's current behavior (exit codes + stdout) for the
# confirmed-vs-indeterminate cases the feature must distinguish.
set -u
cd "$(git rev-parse --show-toplevel)"
BIN=target/debug/tethys

echo "== fresh index =="
$BIN index --rebuild >/dev/null 2>&1 && echo "index OK" || echo "index FAILED"
touch .tethys-09wx/marker
sleep 0.05
touch src/reindex.rs   # mtime bump AFTER indexing: STALE by construction

echo
echo "== probe: classify three files (expect CURRENT / STALE / UNINDEXED) =="
python3 .tethys-09wx/probe.py src/lib.rs src/reindex.rs src/does_not_exist.rs

echo
echo "== oracle 1 (staleness): find -newer marker, no DB involved =="
find src -name '*.rs' -newer .tethys-09wx/marker

echo
echo "== oracle 2 (membership): git ls-files vs DB file set, src/*.rs =="
# LC_ALL=C on both sides: locale sort vs SQLite ORDER BY collate differently
# on resolver.rs/resolve.rs, which shows as a phantom diff (order, not set).
diff <(git ls-files 'src/*.rs' 'src/**/*.rs' | LC_ALL=C sort) \
     <(sqlite3 .rivets/index/tethys.db \
        "SELECT path FROM files WHERE path LIKE 'src/%' AND path LIKE '%.rs'" | LC_ALL=C sort) \
  && echo "SETS IDENTICAL"

echo
echo "== CLI slice: current behavior (the bug evidence) =="
out=$($BIN affected-tests --names-only src/lib.rs 2>/dev/null); rc=$?
echo "indexed file with dependents : exit=$rc stdout_lines=$(printf '%s' "$out" | grep -c .)"
out=$($BIN affected-tests --names-only src/does_not_exist.rs 2>&1 >/dev/null); rc=$?
outl=$($BIN affected-tests --names-only src/does_not_exist.rs 2>/dev/null | wc -l)
echo "unindexed file               : exit=$rc stdout_lines=$outl stderr='$out'"
out=$($BIN affected-tests --names-only src/reindex.rs 2>/dev/null); rc=$?
echo "STALE file (touched)         : exit=$rc stdout_lines=$(printf '%s' "$out" | grep -c .)"
