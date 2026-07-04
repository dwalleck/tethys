#!/usr/bin/env bash
# probe3 (tethys-haw5): the Rust storage shape C# must match (AC2) and the
# target JSON envelope (AC4), from real data: rand-0.8.5 out of the cargo
# registry (4 genuine #[deprecated] items, 2 with in-crate callers).
# Oracle: grep -rn "#\[deprecated" <ws>/src — compare rows item by item.
set -euo pipefail
WS="$1" # a copy of rand-0.8.5 (or any real crate with #[deprecated])
TETHYS=(cargo run --quiet --manifest-path /home/dwalleck/repos/tethys/Cargo.toml --)
"${TETHYS[@]}" index -w "$WS" --rebuild >/dev/null
DB="$WS/.rivets/index/tethys.db"

echo "== stored shape of #[deprecated] attribute rows =="
sqlite3 -header "$DB" "SELECT a.name, a.args, a.line, s.name sym, s.kind, f.path
  FROM attributes a JOIN symbols s ON a.symbol_id = s.id
  JOIN files f ON s.file_id = f.id
  WHERE a.name = 'deprecated' ORDER BY f.path, a.line;"

echo "== target JSON shape (non-empty findings) =="
"${TETHYS[@]}" deprecated-callers -w "$WS" --json
