#!/usr/bin/env bash
# Probe for tethys-jdly (deprecated-callers).
# Q1: which symbols carry #[deprecated], per the tethys index DB?
# Q2: what refs point at those symbols (the feature's actual output)?
# Target: real codebase — vendored zbus crate in amazon-q-developer-cli.
set -euo pipefail
DB="${1:-/home/dwalleck/repos/amazon-q-developer-cli/crates/zbus/.rivets/index/tethys.db}"

echo "== Q1: symbols with #[deprecated] =="
sqlite3 -header -column "$DB" "
SELECT s.id, s.name, s.kind, f.path, s.line, substr(COALESCE(a.args,''),1,40) AS args
FROM attributes a
JOIN symbols s ON a.symbol_id = s.id
JOIN files   f ON s.file_id  = f.id
WHERE a.name = 'deprecated'
ORDER BY f.path, s.line;"

echo
echo "== Q2: inbound refs to deprecated symbols (call sites) =="
sqlite3 -header -column "$DB" "
SELECT dep.name AS deprecated, rf.path AS ref_file, r.line AS ref_line,
       COALESCE(caller.name,'<top-level>') AS in_symbol, r.kind
FROM attributes a
JOIN symbols dep ON a.symbol_id = dep.id
JOIN refs r      ON r.symbol_id = dep.id
JOIN files rf    ON r.file_id   = rf.id
LEFT JOIN symbols caller ON r.in_symbol_id = caller.id
WHERE a.name = 'deprecated'
ORDER BY dep.name, rf.path, r.line;"
