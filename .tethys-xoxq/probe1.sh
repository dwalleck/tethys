#!/usr/bin/env bash
# tethys-xoxq probe 1 — smallest question:
# "Can the index classify a pub symbol's resolved refs as same- vs
#  cross-package, and does the cross-package list match ground truth?"
# Probe mechanism: SQL over refs/symbols/arch_file_packages.
# Oracle mechanism (independent): grep -rn over workspace sources, crate
# attribution by path prefix (crates/<name>/). Run oracle1.sh after this.
set -euo pipefail
DB=~/repos/amazon-q-developer-cli/.rivets/index/tethys.db

echo "=== pub symbols (fn/struct/enum kinds) with cross-package resolved refs, distinctive names ==="
sqlite3 "$DB" "
SELECT s.name, s.kind, p.name AS pkg, COUNT(*) AS xrefs
FROM refs r
JOIN arch_file_packages rfp ON rfp.file_id = r.file_id
JOIN symbols s              ON s.id = r.symbol_id
JOIN arch_file_packages sfp ON sfp.file_id = s.file_id
JOIN arch_packages p        ON p.id = sfp.package_id
WHERE rfp.package_id != sfp.package_id
  AND s.visibility = 'public'
  AND s.kind IN ('function','struct','enum')
  AND LENGTH(s.name) > 12          -- distinctive => clean grep oracle
GROUP BY s.id
HAVING xrefs BETWEEN 2 AND 8
ORDER BY xrefs LIMIT 10"

SYM="${1:-}"
[ -z "$SYM" ] && exit 0

echo
echo "=== cross-package ref sites for '$SYM' (probe answer) ==="
sqlite3 "$DB" "
SELECT rf.path || ':' || r.line
FROM refs r
JOIN files rf               ON rf.id = r.file_id
JOIN arch_file_packages rfp ON rfp.file_id = rf.id
JOIN symbols s              ON s.id = r.symbol_id
JOIN arch_file_packages sfp ON sfp.file_id = s.file_id
WHERE s.name = '$SYM' AND s.visibility = 'public'
  AND rfp.package_id != sfp.package_id
ORDER BY 1"
echo "=== declaring file(s) ==="
sqlite3 "$DB" "
SELECT DISTINCT sf.path FROM symbols s JOIN files sf ON sf.id = s.file_id
WHERE s.name = '$SYM' AND s.visibility = 'public'"
