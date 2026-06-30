#!/usr/bin/env bash
# Independent oracle for tethys-6rlu probe.
# Counts FREE-FUNCTION call sites textually from source — no DB, no resolver.
#   free_calls(X) = occurrences of `X(`  minus  `.X(` (method)  minus  `fn X(` (definition)
# Occurrence-counting (rg -o ... | wc -l), so multiple calls on one line each count.
set -euo pipefail
cd "$(dirname "$0")/.."
SRC=src
count() { { rg -o "$1" "$SRC" || true; } | wc -l | tr -d ' '; }  # 0 on no-match, no abort
for X in node_text parse_scoped_identifier extract_call_reference node_span extract_struct_constructor; do
  total=$(count "\b${X}\s*\(")
  method=$(count "\.${X}\s*\(")
  defs=$(count "fn\s+${X}\s*\(")
  printf "%s\t%d\n" "$X" "$((total - method - defs))"
done
