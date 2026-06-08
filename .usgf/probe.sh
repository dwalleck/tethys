#!/usr/bin/env bash
# probe.sh — usgf (using static) probe (prove-it-prototype)
# Core Q: does a COLLIDING bare member name stay UNRESOLVED today (the gap),
#         while a UNIQUE one already resolves (the monotone baseline)?
# Q1: are `using static T;` and plain `using N;` indistinguishable in storage?
# Q2: how are static members stored (name | kind | qualified_name | parent)?
# Q4: what is a `using static My.Models.Helper;` row's source_module?
# Q5: are const / static-field / enum-member symbols extracted at all?
set -euo pipefail
T=/home/dwalleck/repos/tethys/target/debug/tethys
WS=$(mktemp -d /tmp/usgf-probe-XXXXXX); mkdir -p "$WS/src"

cat > "$WS/src/App.cs" <<'EOF'
using static My.Models.Helper;
using static My.Util.Tools;

namespace My.App
{
    public class Runner
    {
        public void Go()
        {
            Assist();          // COLLIDES: Helper.Assist + Other.Assist
            Configure();       // UNIQUE: only Tools.Configure
            var n = MaxRetries;   // const member, collides
            var p = Pi;           // static field, unique
        }
    }
}
EOF
# Helper has Assist (collides with Other.Assist) + MaxRetries (collides) + Pi (unique)
cat > "$WS/src/Helper.cs" <<'EOF'
namespace My.Models
{
    public static class Helper
    {
        public static void Assist() { }
        public const int MaxRetries = 3;
        public static double Pi = 3.14;
    }
    public enum Color { Red, Green }
}
EOF
# Tools has Configure (unique)
cat > "$WS/src/Tools.cs" <<'EOF'
namespace My.Util
{
    public static class Tools
    {
        public static void Configure() { }
    }
}
EOF
# Collision sources elsewhere in the workspace
cat > "$WS/src/Other.cs" <<'EOF'
namespace Some.Where
{
    public static class Other
    {
        public static void Assist() { }
        public const int MaxRetries = 9;
    }
}
EOF
# A plain namespace using, for the is_static indistinguishability check
cat > "$WS/src/Plain.cs" <<'EOF'
using My.Models;

namespace My.App2
{
    public class R2 { public void G() { var c = Color.Red; } }
}
EOF

"$T" -w "$WS" index > /dev/null
DB=$(find "$WS" -name '*.db' | head -1)

echo "== CORE: App.cs member refs (name | kind | state) =="
sqlite3 "$DB" "SELECT COALESCE(r.reference_name,'<nulled>'), r.kind,
  CASE WHEN r.symbol_id IS NULL THEN 'UNRESOLVED'
       ELSE 'resolved->'||(SELECT s.qualified_name||'@'||f2.path FROM symbols s JOIN files f2 ON s.file_id=f2.id WHERE s.id=r.symbol_id) END
  FROM refs r JOIN files f ON r.file_id=f.id WHERE f.path='src/App.cs' ORDER BY 1;"

echo "== Q1/Q4: imports (file | symbol_name | source_module | alias) — static vs plain =="
sqlite3 "$DB" "SELECT f.path, i.symbol_name, i.source_module, COALESCE(i.alias,'') FROM imports i JOIN files f ON i.file_id=f.id ORDER BY 1,3;"

echo "== Q2/Q5: member symbols (name | kind | qualified_name | parent_name) =="
sqlite3 "$DB" "SELECT s.name, s.kind, s.qualified_name, COALESCE(p.name,'<none>')
  FROM symbols s LEFT JOIN symbols p ON s.parent_symbol_id=p.id
  WHERE s.kind NOT IN ('module') ORDER BY s.kind, s.name;"

echo "WS=$WS"
