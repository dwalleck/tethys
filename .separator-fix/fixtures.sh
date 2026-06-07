#!/usr/bin/env bash
# fixtures.sh <kind> <dest-dir> — deterministically (re)create a fixture workspace.
# kinds: csharp | c6trap
set -euo pipefail
KIND="$1"; DEST="$2"
rm -rf "$DEST"; mkdir -p "$DEST"

case "$KIND" in
csharp)
  mkdir -p "$DEST/src"
  cat > "$DEST/src/Auth.cs" <<'EOF'
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
  cat > "$DEST/src/Hasher.cs" <<'EOF'
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
  ;;
c6trap)
  mkdir -p "$DEST/app/src" "$DEST/helper/src"
  printf '[workspace]\nmembers = ["app", "helper"]\nresolver = "2"\n' > "$DEST/Cargo.toml"
  printf '[package]\nname = "app"\nversion = "0.1.0"\nedition = "2021"\n' > "$DEST/app/Cargo.toml"
  printf '[package]\nname = "helper"\nversion = "0.1.0"\nedition = "2021"\n' > "$DEST/helper/Cargo.toml"
  # Interpretation A target: exists, does NOT contain do_thing
  printf 'pub fn unrelated() {}\n' > "$DEST/app/src/helper.rs"
  printf 'mod helper;\npub fn use_it() {\n    helper::do_thing();\n}\n' > "$DEST/app/src/lib.rs"
  # Interpretation B target: workspace crate helper HAS do_thing
  printf 'pub fn do_thing() {}\n' > "$DEST/helper/src/lib.rs"
  ;;
*) echo "unknown fixture kind: $KIND" >&2; exit 1 ;;
esac
echo "$DEST"
