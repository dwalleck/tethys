#!/usr/bin/env bash
# probe2 (tethys-haw5): resolution substrate — which library symbols would
# deprecated-callers actually list callers for, if marked [Obsolete]?
# Run AFTER probe1.sh (reuses its index). Oracle: grep the same workspace for
# call text (e.g. "Result.Combine(") and compare file:line item by item.
set -euo pipefail
WS="$1"
DB="$WS/.rivets/index/tethys.db"

echo "== library symbols with resolved cross-file incoming refs =="
sqlite3 -header "$DB" "SELECT s.name, s.kind, COUNT(*) sites FROM refs r
  JOIN symbols s ON r.symbol_id = s.id
  JOIN files sf ON s.file_id = sf.id
  JOIN files rf ON r.file_id = rf.id
  WHERE sf.path LIKE 'src/%' AND rf.path NOT LIKE 'src/%'
  GROUP BY s.id ORDER BY sites DESC LIMIT 12;"

echo "== resolved sites for Combine (compare vs: grep -rn 'Combine(' test*/) =="
sqlite3 "$DB" "SELECT rf.path || ':' || r.line FROM refs r
  JOIN symbols s ON r.symbol_id = s.id
  JOIN files sf ON s.file_id = sf.id
  JOIN files rf ON r.file_id = rf.id
  WHERE s.name = 'Combine' AND sf.path LIKE 'src/%' AND rf.path NOT LIKE 'src/%'
  ORDER BY rf.path, r.line;"

echo "== instance-method calls stay unresolved (variable receivers) =="
sqlite3 -header "$DB" "SELECT r.kind, COUNT(*) n, SUM(r.symbol_id IS NOT NULL) resolved
  FROM refs r JOIN files f ON r.file_id = f.id
  WHERE f.language = 'csharp' GROUP BY r.kind;"
