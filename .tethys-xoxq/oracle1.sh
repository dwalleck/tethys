#!/usr/bin/env bash
# tethys-xoxq oracle 1 — independent of the index: raw grep over sources.
# Ground truth for "cross-package uses of <symbol>": every grep hit for the
# name outside the declaring crate's directory, excluding definitions,
# comments, and string literals (hand-reviewed below).
set -euo pipefail
WS=~/repos/amazon-q-developer-cli
SYM="${1:?symbol}"
DECL_CRATE="${2:?declaring crate dir, e.g. crates/fig_auth}"

echo "=== all grep hits outside $DECL_CRATE ==="
grep -rn --include='*.rs' "\b$SYM\b" "$WS" \
  | sed "s|$WS/||" \
  | grep -v "^$DECL_CRATE/" \
  | grep -v "^target/" || true
