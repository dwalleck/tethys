#!/usr/bin/env bash
# fixtures.sh <kind> <dest-dir> — deterministically (re)create a fixture workspace.
# kinds:
#   csharp-gt — ground-truth WS (probe shape, NO collision file): 4 namespace
#               declaration styles, 5 using-forms, used + unused usings.
#   xdir      — cross-directory C# WS (K-hybrid bucket split, claim C9 shape).
set -euo pipefail
KIND="$1"; DEST="$2"
rm -rf "$DEST"; mkdir -p "$DEST/src"

case "$KIND" in
csharp-gt)
  cat > "$DEST/src/App.cs" <<'EOF'
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
  cat > "$DEST/src/Models.cs" <<'EOF'
namespace My.Models
{
    public class Widget { }
    public static class Helper { public static void Assist() { } }
}
EOF
  cat > "$DEST/src/Other.cs" <<'EOF'
namespace Other.Stuff
{
    public class UnusedThing { }
}
EOF
  cat > "$DEST/src/Scoped.cs" <<'EOF'
namespace My.Scoped;

public class FileScopedThing { }
EOF
  cat > "$DEST/src/Nested.cs" <<'EOF'
namespace Outer1
{
    namespace Inner1
    {
        public class NestedThing { }
    }
}
EOF
  cat > "$DEST/src/UseScoped.cs" <<'EOF'
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
  cat > "$DEST/src/GlobalUsings.cs" <<'EOF'
global using My.Globals;
EOF
  cat > "$DEST/src/Globals.cs" <<'EOF'
namespace My.Globals
{
    public class GlobalThing { }
}
EOF
  ;;
xdir)
  rm -rf "$DEST/src"; mkdir -p "$DEST/services" "$DEST/models"
  cat > "$DEST/services/Svc.cs" <<'EOF'
using Domain.Models;
namespace App.Services
{
    public class Svc
    {
        public void Run() { var w = new Widget(); }
    }
}
EOF
  cat > "$DEST/models/Widget.cs" <<'EOF'
namespace Domain.Models
{
    public class Widget { }
}
EOF
  ;;
*) echo "unknown fixture kind: $KIND" >&2; exit 1 ;;
esac
echo "$DEST"
