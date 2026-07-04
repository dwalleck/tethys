#!/usr/bin/env bash
# tethys-xoxq probe 3 — the tier rule the design will propose:
# Definite-tightenable = pub symbol whose name is workspace-unique AND with
# zero cross-package evidence in (a) resolved refs, (b) imports rows,
# (c) unresolved refs' qualified text. Survivors get the grep oracle.
set -euo pipefail
DB=~/repos/amazon-q-developer-cli/.rivets/index/tethys.db
PKG="${1:-fig_auth}"

sqlite3 "$DB" "
WITH pkg_syms AS (
  SELECT s.id, s.name, s.kind, fp.package_id
  FROM symbols s
  JOIN arch_file_packages fp ON fp.file_id = s.file_id
  JOIN arch_packages p       ON p.id = fp.package_id
  WHERE p.name = '$PKG' AND s.visibility = 'public'
    AND s.kind IN ('function','struct','enum','trait','type_alias','constant')
),
unique_names AS (
  SELECT name FROM symbols GROUP BY name HAVING COUNT(*) = 1
),
xref_ids AS (           -- (a) cross-package resolved refs
  SELECT DISTINCT r.symbol_id FROM refs r
  JOIN arch_file_packages rfp ON rfp.file_id = r.file_id
  JOIN symbols s ON s.id = r.symbol_id
  JOIN arch_file_packages sfp ON sfp.file_id = s.file_id
  WHERE rfp.package_id != sfp.package_id
),
ximport_names AS (      -- (b) imports of this package's items from other packages
  SELECT DISTINCT i.symbol_name AS name FROM imports i
  JOIN arch_file_packages ifp ON ifp.file_id = i.file_id
  JOIN arch_packages ip       ON ip.id = ifp.package_id
  WHERE ip.name != '$PKG'
    AND (i.source_module = '$PKG' OR i.source_module LIKE '$PKG::%')
),
xunresolved_names AS (  -- (c) unresolved qualified text mentioning the package
  SELECT DISTINCT
    CASE WHEN INSTR(r.reference_name, '::') > 0
         THEN REPLACE(r.reference_name, RTRIM(r.reference_name,
              REPLACE(r.reference_name, '::', '')), '')
         ELSE r.reference_name END AS name
  FROM refs r
  JOIN arch_file_packages rfp ON rfp.file_id = r.file_id
  JOIN arch_packages rp       ON rp.id = rfp.package_id
  WHERE r.symbol_id IS NULL AND rp.name != '$PKG'
    AND (r.reference_name LIKE '$PKG::%')
)
SELECT ps.name, ps.kind FROM pkg_syms ps
WHERE ps.name IN (SELECT name FROM unique_names)
  AND ps.id  NOT IN (SELECT symbol_id FROM xref_ids)
  AND ps.name NOT IN (SELECT name FROM ximport_names)
  AND ps.name NOT IN (SELECT name FROM xunresolved_names)
ORDER BY ps.name"
