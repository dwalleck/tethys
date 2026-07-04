#!/usr/bin/env bash
# tethys-xoxq probe 2 — the inversion question:
# "Which fig_auth pub symbols look tightenable (zero cross-package resolved
#  refs) — and how many of those does a grep oracle refute?"
# A refuted candidate = a false accusation the PRD posture forbids.
set -euo pipefail
DB=~/repos/amazon-q-developer-cli/.rivets/index/tethys.db

# Naive candidates: pub symbols in fig_auth, top-level kinds only (methods
# inherit reachability from their parent; keep the probe simple), with ZERO
# cross-package resolved refs.
sqlite3 "$DB" "
WITH fig_auth_syms AS (
  SELECT s.id, s.name, s.kind
  FROM symbols s
  JOIN arch_file_packages fp ON fp.file_id = s.file_id
  JOIN arch_packages p       ON p.id = fp.package_id
  WHERE p.name = 'fig_auth' AND s.visibility = 'public'
    AND s.kind IN ('function','struct','enum','trait','type_alias','constant')
),
cross_ref_ids AS (
  SELECT DISTINCT r.symbol_id
  FROM refs r
  JOIN arch_file_packages rfp ON rfp.file_id = r.file_id
  JOIN symbols s              ON s.id = r.symbol_id
  JOIN arch_file_packages sfp ON sfp.file_id = s.file_id
  WHERE rfp.package_id != sfp.package_id
)
SELECT name, kind FROM fig_auth_syms
WHERE id NOT IN (SELECT symbol_id FROM cross_ref_ids)
ORDER BY name"
