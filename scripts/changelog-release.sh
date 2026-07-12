#!/usr/bin/env bash
# changelog-release.sh — assemble changelog.d/ fragments into CHANGELOG.md
# and bump the crate version, ready for a release commit + tag.
#
# Usage:
#   scripts/changelog-release.sh <version>   # e.g. scripts/changelog-release.sh 0.2.0
#
# What it edits (no commits, no tags — those stay in your hands):
#   1. Cargo.toml `version` (Cargo.lock synced via cargo metadata)
#   2. CHANGELOG.md — new `## [<version>] - <date>` section assembled from
#      changelog.d/*.md fragments, grouped in Keep-a-Changelog category order
#   3. changelog.d/ — consumed fragments are deleted
#
# Refuses to run on a dirty tree (the release commit should contain exactly
# this script's output) or with zero fragments. Fragment shape is fenced by
# tests/changelog_lint.rs — run scripts/gate.sh first.
#
# Portability: must run under macOS system bash 3.2 + BSD tools — no GNU
# `sed -i`/`0,/re/`, no bash-4 `${var^}` (PR #22 review finding).
set -euo pipefail

cd "$(dirname "$0")/.." || exit 2

VERSION="${1:-}"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: scripts/changelog-release.sh <version>   (e.g. 0.2.0)" >&2
  exit 2
fi

if [[ -n "$(git status --porcelain)" ]]; then
  echo "❌ working tree is dirty — commit or stash first so the release" >&2
  echo "   commit contains only the version bump + changelog assembly" >&2
  exit 1
fi

if [[ -f CHANGELOG.md ]] && grep -q "^## \[$VERSION\]" CHANGELOG.md; then
  echo "❌ CHANGELOG.md already has a section for $VERSION" >&2
  exit 1
fi

shopt -s nullglob

# ── assemble the section from fragments, Keep-a-Changelog category order ──
consumed=()
section=""
for cat in added changed deprecated removed fixed security; do
  files=(changelog.d/*."$cat".md)
  [[ ${#files[@]} -eq 0 ]] && continue
  heading="$(tr '[:lower:]' '[:upper:]' <<< "${cat:0:1}")${cat:1}"
  section+="### ${heading}"$'\n\n'
  for f in "${files[@]}"; do
    section+="$(cat "$f")"$'\n'
    consumed+=("$f")
  done
  section+=$'\n'
done

if [[ ${#consumed[@]} -eq 0 ]]; then
  echo "❌ no fragments in changelog.d/ — nothing to release" >&2
  exit 1
fi

section="## [$VERSION] - $(date +%Y-%m-%d)"$'\n\n'"$section"
while [[ "$section" == *$'\n' ]]; do section="${section%$'\n'}"; done

# ── bump Cargo.toml (first `version =` line is [package].version) ──
awk -v ver="$VERSION" '
  !done && /^version = ".*"/ { $0 = "version = \"" ver "\""; done = 1 }
  { print }
' Cargo.toml > Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml
grep -q "^version = \"$VERSION\"$" Cargo.toml || {
  echo "❌ Cargo.toml version bump failed" >&2
  exit 1
}
cargo metadata --format-version 1 > /dev/null # sync Cargo.lock

# ── insert the section into CHANGELOG.md (newest first, after the header) ──
if [[ ! -f CHANGELOG.md ]]; then
  cat > CHANGELOG.md << 'EOF'
# Changelog

All notable changes to tethys are documented here, newest release first.
Entries are written for users of the CLI; the commit log and PR bodies
carry the internal story.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
EOF
fi

# ENVIRON (not awk -v) so backslashes in fragment text pass through verbatim.
SECTION="$section" awk '
  !ins && /^## \[/ { print ENVIRON["SECTION"]; print ""; ins = 1 }
  { print }
  END { if (!ins) { print ""; print ENVIRON["SECTION"] } }
' CHANGELOG.md > CHANGELOG.md.tmp
mv CHANGELOG.md.tmp CHANGELOG.md

rm "${consumed[@]}"

echo "✅ assembled ${#consumed[@]} fragment(s) into CHANGELOG.md [$VERSION]"
echo
echo "Review the diff, then:"
echo "  git add Cargo.toml Cargo.lock CHANGELOG.md changelog.d"
echo "  git commit -m \"chore(release): v$VERSION\""
echo "  git tag v$VERSION"
echo "  git push origin main v$VERSION   # tag triggers .github/workflows/release.yml"
echo
echo "NOTE: the main push flips open PRs to BEHIND (full CI re-cycle) —"
echo "release when the merge queue is empty (see CLAUDE.md)."
