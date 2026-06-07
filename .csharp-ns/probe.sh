#!/usr/bin/env bash
# probe.sh — csharp-ns probe (prove-it-prototype)
# Q-core: is an unqualified type usage (new Widget() under `using My.Models;`)
#         extracted, with what name/kind, and does it resolve today?
# Q1: what lives at csharp.rs:109-111 (jwf9's possibly-stale reference)?
# Q2: namespace symbol storage across declaration styles vs stored source_module strings
# Q3: ref kinds available for the types-only filter
# Q4: today's L1 file_deps baseline (incl. unused-using edges)
# Q5: extraction of using static / alias / global using forms
set -euo pipefail
T=/home/dwalleck/repos/tethys/target/debug/tethys
WS=$(mktemp -d /tmp/cns-probe-XXXXXX); mkdir -p "$WS/src"

cat > "$WS/src/App.cs" <<'EOF'
using System;
using My.Models;
using Other.Stuff;
using static My.Models.Helper;
using W = My.Models.Widget;

namespace My.App
{
    public class Runner
    {
        public void Go()
        {
            var w = new Widget();
            Helper.Assist();
            Assist();
            Console.WriteLine("x");
        }
    }
}
EOF
cat > "$WS/src/Models.cs" <<'EOF'
namespace My.Models
{
    public class Widget { }
    public static class Helper { public static void Assist() { } }
}
EOF
cat > "$WS/src/Other.cs" <<'EOF'
namespace Other.Stuff
{
    public class UnusedThing { }
}
EOF
cat > "$WS/src/Scoped.cs" <<'EOF'
namespace My.Scoped;

public class FileScopedThing { }
EOF
cat > "$WS/src/Nested.cs" <<'EOF'
namespace Outer1
{
    namespace Inner1
    {
        public class NestedThing { }
    }
}
EOF
cat > "$WS/src/UseScoped.cs" <<'EOF'
using My.Scoped;
using Outer1.Inner1;

namespace My.App2
{
    public class Runner2
    {
        public void Go2()
        {
            var a = new FileScopedThing();
            var b = new NestedThing();
        }
    }
}
EOF
cat > "$WS/src/GlobalUsings.cs" <<'EOF'
global using My.Globals;
EOF
cat > "$WS/src/Globals.cs" <<'EOF'
namespace My.Globals
{
    public class GlobalThing { }
}
EOF

"$T" -w "$WS" index > /dev/null
DB=$(find "$WS" -name '*.db' | head -1)

echo "== Q-core/Q3: refs (file | name | kind | state) =="
sqlite3 "$DB" "SELECT f.path, COALESCE(r.reference_name,'<nulled-resolved>'), r.kind,
  CASE WHEN r.symbol_id IS NULL THEN 'UNRESOLVED'
       ELSE 'resolved->'||(SELECT s.qualified_name||'@'||f2.path FROM symbols s JOIN files f2 ON s.file_id=f2.id WHERE s.id=r.symbol_id) END
  FROM refs r JOIN files f ON r.file_id=f.id ORDER BY 1,2;"

echo "== Q2: namespace + type symbols (name | qualified | kind | file) =="
sqlite3 "$DB" "SELECT s.name, s.qualified_name, s.kind, f.path FROM symbols s JOIN files f ON s.file_id=f.id ORDER BY f.path, s.line;"

echo "== Q2/Q5: imports (file | symbol_name | source_module | alias) =="
sqlite3 "$DB" "SELECT f.path, i.symbol_name, i.source_module, COALESCE(i.alias,'') FROM imports i JOIN files f ON i.file_id=f.id ORDER BY 1,3;"

echo "== Q4: L1 file_deps baseline =="
sqlite3 "$DB" "SELECT ff.path||' -> '||tf.path||' ('||d.ref_count||')' FROM file_deps d JOIN files ff ON d.from_file_id=ff.id JOIN files tf ON d.to_file_id=tf.id ORDER BY 1;"

echo "WS=$WS"
