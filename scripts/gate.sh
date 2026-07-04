#!/usr/bin/env bash
# gate.sh — run the same checks CI runs, locally, with real exit codes.
#
# Mirrors .github/workflows/ci.yml jobs: format, lint, test, doc tests.
# Each check runs to completion (no fail-fast) so you see every failure in
# one pass; the script exits non-zero if ANY check failed.
#
# Deliberately does NOT pipe command output through tail/grep — a pipe would
# swallow the real exit code (see CLAUDE.md). Every command's status is
# captured directly.
#
# Usage:
#   scripts/gate.sh            # run all checks
#   scripts/gate.sh --fast     # skip doc tests (the slow tail of the run)
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2

FAST=0
for arg in "$@"; do
  case "$arg" in
    --fast) FAST=1 ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown arg: $arg" >&2; exit 2 ;;
  esac
done

failed=()

run() {
  local name="$1"; shift
  echo "━━━ ${name} ━━━"
  if "$@"; then
    echo "✅ ${name}"
  else
    echo "❌ ${name} (exit $?)"
    failed+=("$name")
  fi
  echo
}

run "format"    cargo fmt --all -- --check
run "lint"      cargo clippy --all-targets --all-features -- -D warnings
run "test"      cargo nextest run --all-features
[ "$FAST" -eq 0 ] && run "doctest" cargo test --doc --all-features

echo "════════════════════════════════════════"
if [ ${#failed[@]} -eq 0 ]; then
  echo "✅ gate PASSED"
  exit 0
fi
echo "❌ gate FAILED: ${failed[*]}"
exit 1
