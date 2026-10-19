#!/usr/bin/env bash
# Bump the workspace version (Cargo.toml [workspace.package]).
# Usage: scripts/bump-version.sh --patch | --minor | --major
# Author: Passwd Word
set -euo pipefail

cd "$(dirname "$0")/.."

MODE="${1:---patch}"
TOML="Cargo.toml"

CUR=$(grep -m1 '^version' "$TOML" | sed -E 's/.*"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/')
IFS=. read -r MAJOR MINOR PATCH <<< "$CUR"

case "$MODE" in
  --major) MAJOR=$((MAJOR+1)); MINOR=0; PATCH=0 ;;
  --minor) MINOR=$((MINOR+1)); PATCH=0 ;;
  --patch) PATCH=$((PATCH+1)) ;;
  *) echo "usage: $0 --patch|--minor|--major" >&2; exit 1 ;;
esac

NEW="$MAJOR.$MINOR.$PATCH"
# Only touch the [workspace.package] version (first occurrence in file).
perl -0pi -e "s/^version = \"$CUR\"/version = \"$NEW\"/m" "$TOML"
echo "$CUR -> $NEW"
echo "NOTE: run 'cargo check' once to refresh Cargo.lock, then commit both."
