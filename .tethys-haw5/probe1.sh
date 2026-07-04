#!/usr/bin/env bash
# probe1 (tethys-haw5): index a REAL C# repo (Tethys.Results) and report what the
# system records today for its [Obsolete] member: symbol row, attribute rows,
# refs, and deprecated-callers output. No feature code; raw SQL + CLI only.
set -euo pipefail
WS="$1"
TETHYS=(cargo run --quiet --manifest-path /home/dwalleck/repos/tethys/Cargo.toml --)
"${TETHYS[@]}" index -w "$WS" --rebuild >/dev/null
DB="$WS/.rivets/index/tethys.db"

echo "== A. symbol rows named 'Data' (the [Obsolete] property) =="
sqlite3 -header "$DB" "SELECT s.id, s.name, s.kind, s.line, f.path
  FROM symbols s JOIN files f ON s.file_id = f.id
  WHERE s.name = 'Data' AND f.path LIKE '%GenericResult.cs';"

echo "== B. ALL attribute rows in the index (name, count) =="
sqlite3 -header "$DB" "SELECT a.name, COUNT(*) AS n FROM attributes a
  GROUP BY a.name ORDER BY n DESC;"

echo "== C. attribute rows mentioning Obsolete =="
sqlite3 -header "$DB" "SELECT a.name, a.args, a.line FROM attributes a
  WHERE a.name LIKE '%Obsolete%';"

echo "== D. refs resolved to the Data symbol (call sites the feature would list) =="
sqlite3 -header "$DB" "SELECT f.path, r.line, r.kind FROM refs r
  JOIN files f ON r.file_id = f.id
  WHERE r.symbol_id IN (
    SELECT s.id FROM symbols s JOIN files f2 ON s.file_id = f2.id
    WHERE s.name = 'Data' AND f2.path LIKE '%GenericResult.cs')
  ORDER BY f.path, r.line;"

echo "== E. indexed C# file count =="
sqlite3 "$DB" "SELECT COUNT(*) FROM files WHERE language = 'csharp';"

echo "== F. deprecated-callers --json =="
"${TETHYS[@]}" deprecated-callers -w "$WS" --json
