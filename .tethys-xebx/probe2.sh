#!/usr/bin/env bash
# probe2 (tethys-xebx): index a REAL C# repo (Tethys.Results) and report what the
# system records TODAY for member declarations and member reads: symbol kinds
# present, refs kinds present, the [Obsolete] Data property's visibility, and
# deprecated-callers output. No feature code; raw SQL + CLI only.
set -euo pipefail
WS="$1"
TETHYS=(cargo run --quiet --manifest-path /home/dwalleck/repos/tethys/Cargo.toml --)
"${TETHYS[@]}" index -w "$WS" --rebuild >/dev/null
DB="$WS/.rivets/index/tethys.db"

echo "== A. C# symbol kind distribution (what extraction produces today) =="
sqlite3 -header "$DB" "SELECT s.kind, COUNT(*) AS n FROM symbols s
  JOIN files f ON s.file_id = f.id WHERE f.language = 'csharp'
  GROUP BY s.kind ORDER BY n DESC;"

echo "== B. symbol rows named Data / Value (member declarations) =="
sqlite3 -header "$DB" "SELECT s.name, s.kind, s.line, f.path FROM symbols s
  JOIN files f ON s.file_id = f.id WHERE s.name IN ('Data','Value');"

echo "== C. refs kind distribution for C# files =="
sqlite3 -header "$DB" "SELECT r.kind, COUNT(*) AS n FROM refs r
  JOIN files f ON r.file_id = f.id WHERE f.language = 'csharp'
  GROUP BY r.kind ORDER BY n DESC;"

echo "== D. any refs whose name is Data (member reads, if any existed) =="
sqlite3 -header "$DB" "SELECT f.path, r.line, r.kind, r.strategy FROM refs_named rn
  JOIN refs r ON r.id = rn.id JOIN files f ON r.file_id = f.id
  WHERE rn.name = 'Data' ORDER BY f.path, r.line;"

echo "== E. attribute rows mentioning Obsolete (haw5 shipped this; expect >=1) =="
sqlite3 -header "$DB" "SELECT a.name, a.args, a.line, a.symbol_id FROM attributes a
  WHERE a.name LIKE '%Obsolete%';"

echo "== F. module_path of symbols in GenericResult.cs (namespace attribution) =="
sqlite3 -header "$DB" "SELECT s.name, s.kind, s.module_path FROM symbols s
  JOIN files f ON s.file_id = f.id WHERE f.path LIKE '%GenericResult.cs'
  ORDER BY s.line LIMIT 12;"

echo "== G. imports rows for BasicTests.cs (union-arm corroboration input) =="
sqlite3 -header "$DB" "SELECT i.* FROM imports i JOIN files f ON i.file_id = f.id
  WHERE f.path LIKE '%BasicTests.cs';"

echo "== H. deprecated-callers --json =="
"${TETHYS[@]}" deprecated-callers -w "$WS" --json

echo "== I. strategy distribution for C# refs (resolution reality today) =="
sqlite3 -header "$DB" "SELECT r.strategy, COUNT(*) AS n FROM refs r
  JOIN files f ON r.file_id = f.id WHERE f.language = 'csharp'
  GROUP BY r.strategy ORDER BY n DESC;"

echo "== J. variable-receiver instance calls: GetValueOrDefault refs =="
sqlite3 -header "$DB" "SELECT rn.name, r.strategy, COUNT(*) AS n FROM refs r
  JOIN refs_named rn ON rn.id = r.id JOIN files f ON r.file_id = f.id
  WHERE rn.name LIKE '%GetValueOrDefault%' GROUP BY rn.name, r.strategy;"

echo "== K. sample unresolved variable-receiver reference_names =="
sqlite3 "$DB" "SELECT DISTINCT reference_name FROM refs
  WHERE symbol_id IS NULL AND reference_name LIKE '%::%' LIMIT 12;"
