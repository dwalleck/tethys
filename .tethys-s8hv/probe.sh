#!/usr/bin/env bash
# probe.sh — tethys-s8hv oracle: does the index capture unit tests?
#
# PROBE (index): count is_test symbols; check known unit tests are present.
# ORACLE (source, independent): count #[test]/#[tokio::test]/#[rstest] in src+tests.
#
# Before the fix: is_test≈330 (integration only), known unit tests ABSENT.
# After recursing MOD_ITEM into declaration_list: is_test should approach the
# source count and the known unit tests should be indexed with is_test=1.
#
# This is the cheapest falsifier for the fix. Run: bash .tethys-s8hv/probe.sh
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

DB=.rivets/index/tethys.db
[ -f "$DB" ] || { echo "no index at $DB — run: tethys index -w ."; exit 2; }

probe_is_test=$(sqlite3 "$DB" "SELECT COUNT(*) FROM symbols WHERE is_test=1;")
probe_in_src=$(sqlite3 "$DB" "SELECT COUNT(*) FROM symbols s JOIN files f ON s.file_id=f.id WHERE s.is_test=1 AND f.path NOT LIKE '%/tests/%' AND f.path NOT LIKE 'tests/%';")

# Oracle: independent source count.
o_test=$(grep -rn -E '^\s*#\[test\]' src tests 2>/dev/null | wc -l | tr -d ' ')
o_tokio=$(grep -rn -E '#\[tokio::test\]' src tests 2>/dev/null | wc -l | tr -d ' ')
o_rstest=$(grep -rn -E '^\s*#\[rstest\]' src tests 2>/dev/null | wc -l | tr -d ' ')
oracle=$(( o_test + o_tokio + o_rstest ))

echo "PROBE  (index):  is_test=$probe_is_test   (of which in src/: $probe_in_src)"
echo "ORACLE (source): ~$oracle test fns (#[test] $o_test + tokio $o_tokio + rstest $o_rstest)"

echo "--- known unit tests (must be is_test=1 symbols after the fix) ---"
fail=0
for n in is_excluded_dir_allows_lib normalize_path_is_idempotent extracts_simple_function; do
  got=$(sqlite3 "$DB" "SELECT is_test FROM symbols WHERE name='$n' LIMIT 1;")
  if [ "$got" = "1" ]; then echo "  OK   $n (is_test=1)"; else echo "  MISS $n (indexed=${got:-no})"; fail=1; fi
done

echo "----------------------------------------"
if [ "$probe_in_src" -gt 0 ] && [ "$fail" -eq 0 ]; then
  echo "✅ unit tests ARE indexed (fix working)"
else
  echo "❌ unit tests NOT indexed (bug present): src is_test=$probe_in_src, known-test miss=$fail"
fi
