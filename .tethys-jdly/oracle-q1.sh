#!/usr/bin/env bash
# Q1 oracle for tethys-jdly: find every #[deprecated...] attribute in zbus and
# report the item it precedes — using only grep/awk, no tethys machinery.
# Handles multi-line attributes by consuming lines until the closing )].
set -euo pipefail
ZBUS="${1:-/home/dwalleck/repos/amazon-q-developer-cli/crates/zbus/src}"

find "$ZBUS" -name '*.rs' | sort | while read -r f; do
  awk -v file="${f#"$ZBUS"/}" '
    /#\[deprecated\(/ && !/\)\]/ { in_attr = 1; pending = 1; next }
    in_attr { if (/\)\]/) in_attr = 0; next }
    /#\[deprecated/ { pending = 1; next }
    pending && !/^\s*#\[/ && !/^\s*\/\// && !/^\s*$/ {
      gsub(/^[ \t]+/, "");
      printf "%s:%d  %s\n", file, NR, substr($0, 1, 70);
      pending = 0
    }
  ' "$f"
done
