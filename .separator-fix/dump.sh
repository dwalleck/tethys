#!/usr/bin/env bash
# dump.sh <tethys.db> — natural-key projection of the index, stable across
# from-scratch re-indexes (no autoincrement ids, no mtimes). The strict-
# neutrality oracle for the separator-fix loop: byte-identical output ⇔
# behaviorally identical index.
set -euo pipefail
DB="$1"

q() { sqlite3 -batch "$DB" "$1"; }

echo "## files"
q "SELECT path, language, size_bytes FROM files ORDER BY path;"

echo "## symbols"
q "SELECT f.path, s.name, s.module_path, s.qualified_name, s.kind, s.line, s.\"column\",
          COALESCE(s.end_line,-1), COALESCE(s.end_column,-1),
          COALESCE(replace(s.signature, char(10), '\\n'),''), s.visibility,
          COALESCE(p.qualified_name,''), s.is_test
   FROM symbols s
   JOIN files f ON s.file_id = f.id
   LEFT JOIN symbols p ON s.parent_symbol_id = p.id
   ORDER BY 1,6,7,4,5,2,3,8,9,10,11,12,13;"

echo "## refs"
q "SELECT f.path, r.kind, r.line, r.\"column\",
          COALESCE(r.end_line,-1), COALESCE(r.end_column,-1),
          COALESCE(r.reference_name,''),
          COALESCE(tf.path || ':' || ts.line || ':' || ts.qualified_name, '<unresolved>'),
          COALESCE(cs.qualified_name,'')
   FROM refs r
   JOIN files f ON r.file_id = f.id
   LEFT JOIN symbols ts ON r.symbol_id = ts.id
   LEFT JOIN files tf ON ts.file_id = tf.id
   LEFT JOIN symbols cs ON r.in_symbol_id = cs.id
   ORDER BY 1,3,4,2,5,6,7,8,9;"

echo "## imports"
q "SELECT f.path, i.symbol_name, i.source_module, COALESCE(i.alias,'')
   FROM imports i JOIN files f ON i.file_id = f.id
   ORDER BY 1,2,3,4;"

echo "## file_deps"
q "SELECT ff.path, tf.path, d.ref_count
   FROM file_deps d
   JOIN files ff ON d.from_file_id = ff.id
   JOIN files tf ON d.to_file_id = tf.id
   ORDER BY 1,2;"

echo "## call_edges"
q "SELECT cf.path || ':' || cs.line || ':' || cs.qualified_name,
          ef.path || ':' || es.line || ':' || es.qualified_name, e.call_count
   FROM call_edges e
   JOIN symbols cs ON e.caller_symbol_id = cs.id JOIN files cf ON cs.file_id = cf.id
   JOIN symbols es ON e.callee_symbol_id = es.id JOIN files ef ON es.file_id = ef.id
   ORDER BY 1,2;"
