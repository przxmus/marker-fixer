#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: scripts/release/bump_version.sh <x.y.z>" >&2
  exit 1
fi

NEW_VERSION="$1"

if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$ ]]; then
  echo "Invalid version: $NEW_VERSION" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CARGO_TOML="$ROOT_DIR/Cargo.toml"

CURRENT_VERSION="$(awk -F'"' '/^version = / {print $2; exit}' "$CARGO_TOML")"

if [[ "$CURRENT_VERSION" == "$NEW_VERSION" ]]; then
  echo "Version is already $NEW_VERSION"
  exit 0
fi

# Update only the package version line.
awk -v new_version="$NEW_VERSION" '
  BEGIN { updated = 0; in_package = 0 }
  /^\[package\]/ { in_package = 1; print; next }
  /^\[/ && $0 != "[package]" { in_package = 0 }
  in_package && /^version = / && updated == 0 {
    print "version = \"" new_version "\""
    updated = 1
    next
  }
  { print }
' "$CARGO_TOML" > "$CARGO_TOML.tmp"
mv "$CARGO_TOML.tmp" "$CARGO_TOML"

(
  cd "$ROOT_DIR"
  cargo generate-lockfile >/dev/null
)

echo "Bumped version: $CURRENT_VERSION -> $NEW_VERSION"
