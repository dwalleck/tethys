#!/usr/bin/env bash
# probe3 (tethys-53iv): what does the index ACTUALLY bind today on a real
# codebase (tethys itself, copied to scratchpad)? Measures the at-stake
# bind population and samples the phantom class (std-collision names like
# is_empty/as_str). Raw SQL + CLI only.
set -euo pipefail
WS="$1"
TETHYS=(cargo run --quiet --manifest-path /home/dwalleck/repos/tethys/Cargo.toml --)
"${TETHYS[@]}" index -w "$WS" --rebuild >/dev/null
DB="$WS/.rivets/index/tethys.db"

echo "== A. call refs bound to method-kind symbols, by strategy =="
sqlite3 -header "$DB" "SELECT r.strategy, COUNT(*) AS n FROM refs r
  JOIN symbols ts ON ts.id = r.symbol_id
  WHERE r.kind = 'call' AND ts.kind = 'method'
  GROUP BY r.strategy ORDER BY n DESC;"

echo "== B. binds for std-collision names (phantom candidates) =="
sqlite3 -header "$DB" "SELECT ts.name, ts.kind, f2.path AS decl_file, COUNT(*) AS bind_count
  FROM refs r JOIN symbols ts ON ts.id = r.symbol_id
  JOIN files f2 ON f2.id = ts.file_id
  WHERE r.kind = 'call' AND ts.name IN ('is_empty','as_str','as_i64','len','get','insert','contains','parse')
  GROUP BY ts.id ORDER BY bind_count DESC;"

echo "== C. sample is_empty bind sites (src only, for hand-verification) =="
sqlite3 -header "$DB" "SELECT f.path, r.line, r.strategy FROM refs r
  JOIN symbols ts ON ts.id = r.symbol_id
  JOIN files f ON f.id = r.file_id
  WHERE r.kind = 'call' AND ts.name = 'is_empty' AND f.path LIKE 'src/%'
  ORDER BY f.path, r.line LIMIT 10;"

echo "== D. panic-points on tethys (expect accurate: no in-crate unwrap/expect methods) =="
"${TETHYS[@]}" panic-points -w "$WS" 2>/dev/null | sed -n 1,8p

echo "== E. total unwrap/expect refs stored (vs grep oracle) =="
sqlite3 "$DB" "SELECT COUNT(*) FROM refs r JOIN files f ON f.id=r.file_id
  WHERE r.reference_name IN ('unwrap','expect') AND f.path LIKE 'src/%';"
