#!/usr/bin/env bash
# Cheapest falsifier for design claim C9 (tethys-xebx), RUN 2026-07-05, PASSED.
# Question: does the existing deprecated-callers machinery carry a `property`
# symbol + `field_access` refs end to end with ZERO analysis changes?
# Method: inject synthetic rows (the exact rows the feature would produce)
# into a freshly indexed corpus DB, run the real CLI. No feature code.
# NOTE: leaves the synthetic rows in the DB; probe2.sh --rebuilds to reset.
set -euo pipefail
WS="$1"
TETHYS=(cargo run --quiet --manifest-path /home/dwalleck/repos/tethys/Cargo.toml --)
"${TETHYS[@]}" index -w "$WS" --rebuild >/dev/null
DB="$WS/.rivets/index/tethys.db"

sqlite3 "$DB" <<'EOF'
INSERT INTO symbols (file_id, name, module_path, qualified_name, kind, line, column, end_line, end_column, signature, visibility, parent_symbol_id, is_test)
SELECT id, 'Data', '', 'Result::Data', 'property', 38, 9, 38, 32, 'public T Data => Value', 'public', NULL, 0
FROM files WHERE path LIKE '%GenericResult.cs';
EOF
SYM=$(sqlite3 "$DB" "SELECT id FROM symbols WHERE kind='property' AND name='Data';")
sqlite3 "$DB" "INSERT INTO attributes (symbol_id, name, args, line) VALUES ($SYM, 'Obsolete', '\"Use Value instead. Data will be removed in a future major version.\"', 37);"
sqlite3 "$DB" <<'EOF'
INSERT INTO refs (symbol_id, file_id, kind, line, column, in_symbol_id, reference_name, strategy)
SELECT NULL, id, 'field_access', 77, 55, NULL, 'result::Data', NULL FROM files WHERE path LIKE '%BasicTests.cs';
INSERT INTO refs (symbol_id, file_id, kind, line, column, in_symbol_id, reference_name, strategy)
SELECT NULL, id, 'field_access', 23, 70, NULL, 'dataResult::Data', NULL FROM files WHERE path LIKE '%test-package.cs';
EOF

# PASS = symbol_count 1 (kind property, note parsed, error null) and exactly
# 2 Maybe sites via unresolved-qualified at BasicTests.cs:77 + test-package.cs:23
"${TETHYS[@]}" deprecated-callers -w "$WS" --json
