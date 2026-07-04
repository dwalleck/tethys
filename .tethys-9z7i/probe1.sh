#!/usr/bin/env bash
# tethys-9z7i slice 2, probe 1 — schema evolution facts:
# (a) what CREATE TABLE/INDEX IF NOT EXISTS does to an EXISTING db when the
#     new schema adds refs.strategy (+ an index on it) — the xvlw failure;
# (b) whether idempotent ALTER TABLE ADD COLUMN migration works;
# (c) whether `tethys index --rebuild` TODAY gives a fresh schema (canary
#     column must vanish) — adjudicates stale-vs-live for tethys-xvlw.
# Oracle: sqlite3 PRAGMA table_info / error text — independent of tethys code.
set -euo pipefail
S=${SCRATCH:-/tmp/9z7i-probe1}; rm -rf "$S"; mkdir -p "$S/src"
printf '[package]\nname = "scratch"\nversion = "0.0.0"\nedition = "2021"\n' > "$S/Cargo.toml"
printf 'pub fn a() {}\npub fn b() {\n    a();\n}\n' > "$S/src/lib.rs"
TETHYS=${TETHYS:-./target/release/tethys}
DB="$S/.rivets/index/tethys.db"

$TETHYS index -w "$S" > /dev/null 2>&1
echo "=== (a) new schema against existing db ==="
sqlite3 "$DB" "CREATE TABLE IF NOT EXISTS refs (id INTEGER PRIMARY KEY, symbol_id INTEGER, file_id INTEGER, kind TEXT, line INTEGER, column INTEGER, in_symbol_id INTEGER, reference_name TEXT, strategy TEXT);" \
  && echo "CREATE TABLE IF NOT EXISTS: no error, table untouched (columns: $(sqlite3 "$DB" "SELECT COUNT(*) FROM pragma_table_info('refs')"))"
sqlite3 "$DB" "CREATE INDEX IF NOT EXISTS idx_refs_strategy ON refs(strategy);" 2>&1 \
  || echo "CREATE INDEX on missing column: FAILS (the xvlw mode)"
sqlite3 "$DB" "SELECT strategy FROM refs LIMIT 1;" 2>&1 || echo "SELECT strategy: FAILS on old db"

echo "=== (b) idempotent ALTER migration ==="
HAS=$(sqlite3 "$DB" "SELECT COUNT(*) FROM pragma_table_info('refs') WHERE name='strategy'")
[ "$HAS" = "0" ] && sqlite3 "$DB" "ALTER TABLE refs ADD COLUMN strategy TEXT;"
sqlite3 "$DB" "CREATE INDEX IF NOT EXISTS idx_refs_strategy ON refs(strategy);" && echo "post-ALTER: index creates fine"
sqlite3 "$DB" "SELECT COUNT(*) FROM refs WHERE strategy IS NULL;" | xargs -I{} echo "post-ALTER: {} existing rows read as strategy NULL"

echo "=== (c) --rebuild freshness canary ==="
sqlite3 "$DB" "ALTER TABLE refs ADD COLUMN xvlw_canary TEXT;"
$TETHYS index --rebuild -w "$S" > /dev/null 2>&1
CANARY=$(sqlite3 "$DB" "SELECT COUNT(*) FROM pragma_table_info('refs') WHERE name='xvlw_canary'")
if [ "$CANARY" = "0" ]; then echo "--rebuild: canary GONE — fresh schema (xvlw fixed by reset())"; else echo "--rebuild: canary SURVIVED — xvlw still live"; fi
