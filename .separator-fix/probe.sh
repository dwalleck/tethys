#!/usr/bin/env bash
# probe.sh — separator-fix probe (prove-it-prototype)
# Q1: does the Rust extractor ever store '.' in refs.reference_name?
# Q2: what separators land in C# symbols.qualified_name / module_path / imports.source_module?
# Q3: would the post-fix qualified fallback (get_symbol_by_qualified_name) match C# refs? (SQL simulation)
# Q4: are nested C# types stored as Outer::Inner today?
# Q5: does a cross-file C# qualified call resolve in Pass 2 today?
set -euo pipefail
TETHYS=/home/dwalleck/repos/tethys/target/debug/tethys
WS=$(mktemp -d /tmp/sep-probe-XXXXXX)
mkdir -p "$WS/src"

cat > "$WS/src/Auth.cs" <<'EOF'
using System;
using MyApp.Models;

namespace MyApp.Services
{
    public class AuthService
    {
        public void Login(string user)
        {
            var hashed = Hasher.Hash(user);
            var session = new Outer.Inner();
            Console.WriteLine(hashed);
        }
    }
}
EOF

cat > "$WS/src/Hasher.cs" <<'EOF'
namespace MyApp.Models
{
    public static class Hasher
    {
        public static string Hash(string input) { return input; }
    }
    public class Outer
    {
        public class Inner { }
    }
}
EOF

"$TETHYS" -w "$WS" index > /dev/null
DB=$(find "$WS" -name '*.db' | head -1)
echo "DB=$DB"

echo "== Q2/Q4: C# symbols (name | qualified_name | module_path) =="
sqlite3 "$DB" "SELECT name, qualified_name, COALESCE(module_path,'<null>') FROM symbols ORDER BY qualified_name;"

echo "== Q5: C# refs (reference_name | resolution state) =="
sqlite3 "$DB" "SELECT COALESCE(reference_name,'<null>'), CASE WHEN symbol_id IS NULL THEN 'UNRESOLVED' ELSE 'resolved' END FROM refs ORDER BY 1;"

echo "== C# imports (symbol_name | source_module) =="
sqlite3 "$DB" "SELECT symbol_name, source_module FROM imports;"

echo "== Q3: simulate post-fix qualified-fallback lookups =="
for q in 'Hasher.Hash' 'Hasher::Hash' 'Outer.Inner' 'Outer::Inner'; do
  sqlite3 "$DB" "SELECT '$q -> ' || COALESCE((SELECT 'MATCH (' || kind || ')' FROM symbols WHERE qualified_name = '$q'), 'no match');"
done

echo "== Q1: Rust dotted reference_names (tethys self-index) =="
(cd /home/dwalleck/repos/tethys && "$TETHYS" index > /dev/null)
RDB=/home/dwalleck/repos/tethys/.rivets/index/tethys.db
sqlite3 "$RDB" "SELECT COUNT(*) || ' dotted of ' || (SELECT COUNT(*) FROM refs) || ' total refs' FROM refs WHERE reference_name LIKE '%.%';"
sqlite3 "$RDB" "SELECT DISTINCT reference_name FROM refs WHERE reference_name LIKE '%.%' LIMIT 15;"

echo "WS=$WS (left in place for inspection)"
