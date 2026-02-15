#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="$ROOT_DIR/build"
STAGING_DIR="$BUILD_DIR/staging"

DEFAULT_TARGETS=(
  "aarch64-apple-darwin"
  "x86_64-apple-darwin"
  "x86_64-unknown-linux-musl"
  "x86_64-pc-windows-gnu"
)

TARGETS=()
KEEP_STAGING=0
CLEAN=0
VERSION_OVERRIDE=""

usage() {
  cat <<'USAGE'
Usage: ./build.sh [options]

Build marker-fixer for multiple platforms and package clean release artifacts.

Options:
  --targets <csv>       Comma-separated target list.
  --version <x.y.z>     Override package version used in artifact names.
  --clean               Remove the build/ directory before building.
  --keep-staging        Keep intermediate staging directories.
  -h, --help            Show this help message.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --targets)
      IFS=',' read -r -a TARGETS <<< "$2"
      shift 2
      ;;
    --version)
      VERSION_OVERRIDE="$2"
      shift 2
      ;;
    --clean)
      CLEAN=1
      shift
      ;;
    --keep-staging)
      KEEP_STAGING=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ ${#TARGETS[@]} -eq 0 ]]; then
  TARGETS=("${DEFAULT_TARGETS[@]}")
fi

if [[ -n "$VERSION_OVERRIDE" ]]; then
  VERSION="$VERSION_OVERRIDE"
else
  VERSION="$(awk -F'"' '/^version = / {print $2; exit}' "$ROOT_DIR/Cargo.toml")"
fi

if [[ -z "$VERSION" ]]; then
  echo "Failed to determine package version from Cargo.toml" >&2
  exit 1
fi

if [[ $CLEAN -eq 1 ]]; then
  rm -rf "$BUILD_DIR"
fi

mkdir -p "$BUILD_DIR" "$STAGING_DIR"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required but was not found in PATH" >&2
  exit 1
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup is required but was not found in PATH" >&2
  exit 1
fi

needs_zigbuild=0
for target in "${TARGETS[@]}"; do
  case "$target" in
    aarch64-apple-darwin|x86_64-apple-darwin)
      ;;
    *)
      needs_zigbuild=1
      ;;
  esac
done

if [[ $needs_zigbuild -eq 1 ]]; then
  if ! cargo zigbuild --help >/dev/null 2>&1; then
    echo "Installing cargo-zigbuild..."
    cargo install cargo-zigbuild --locked
  fi

  if ! command -v zig >/dev/null 2>&1; then
    echo "zig is required for cross-target builds (linux/windows from non-native hosts)." >&2
    echo "Install zig and rerun. Example: brew install zig" >&2
    exit 1
  fi
fi

normalize_target() {
  local target="$1"
  case "$target" in
    aarch64-apple-darwin)
      echo "macos arm64 marker-fixer"
      ;;
    x86_64-apple-darwin)
      echo "macos x86_64 marker-fixer"
      ;;
    x86_64-unknown-linux-musl)
      echo "linux x86_64 marker-fixer"
      ;;
    x86_64-pc-windows-gnu)
      echo "windows x86_64 marker-fixer.exe"
      ;;
    *)
      echo "Unsupported target: $target" >&2
      exit 1
      ;;
  esac
}

ensure_bundled_tools() {
  local platform="$1"
  local arch="$2"
  local vendor_dir="$ROOT_DIR/vendor/fftools/$platform/$arch"

  if [[ ! -d "$vendor_dir" ]]; then
    echo "Missing bundled tool directory: $vendor_dir" >&2
    exit 1
  fi

  if [[ "$platform" == "windows" ]]; then
    [[ -f "$vendor_dir/ffprobe.exe" ]] || { echo "Missing $vendor_dir/ffprobe.exe" >&2; exit 1; }
    [[ -f "$vendor_dir/ffmpeg.exe" ]] || { echo "Missing $vendor_dir/ffmpeg.exe" >&2; exit 1; }
  else
    [[ -f "$vendor_dir/ffprobe" ]] || { echo "Missing $vendor_dir/ffprobe" >&2; exit 1; }
    [[ -f "$vendor_dir/ffmpeg" ]] || { echo "Missing $vendor_dir/ffmpeg" >&2; exit 1; }
  fi
}

build_target() {
  local target="$1"
  read -r platform arch bin_name < <(normalize_target "$target")

  ensure_bundled_tools "$platform" "$arch"

  echo "Building target: $target"
  rustup target add "$target" >/dev/null

  case "$target" in
    aarch64-apple-darwin|x86_64-apple-darwin)
      cargo build --release --target "$target"
      ;;
    *)
      cargo zigbuild --release --target "$target"
      ;;
  esac

  local bin_path="$ROOT_DIR/target/$target/release/$bin_name"
  if [[ ! -f "$bin_path" ]]; then
    echo "Build output not found: $bin_path" >&2
    exit 1
  fi

  local pkg_name="marker-fixer-v${VERSION}-${target}"
  local pkg_dir="$STAGING_DIR/$pkg_name"
  local pkg_zip="$BUILD_DIR/${pkg_name}.zip"

  rm -rf "$pkg_dir" "$pkg_zip"
  mkdir -p "$pkg_dir" "$pkg_dir/fftools/$platform/$arch"

  cp "$bin_path" "$pkg_dir/$bin_name"
  cp -R "$ROOT_DIR/vendor/fftools/$platform/$arch/." "$pkg_dir/fftools/$platform/$arch/"
  cp "$ROOT_DIR/README.md" "$pkg_dir/README.md"
  cp "$ROOT_DIR/THIRD_PARTY_NOTICES.md" "$pkg_dir/THIRD_PARTY_NOTICES.md"

  (
    cd "$STAGING_DIR"
    zip -qr "$pkg_zip" "$pkg_name"
  )

  echo "Created: $pkg_zip"
}

for target in "${TARGETS[@]}"; do
  build_target "$target"
done

checksum_file() {
  local file="$1"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file"
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file"
  else
    echo "Neither shasum nor sha256sum is available." >&2
    exit 1
  fi
}

(
  cd "$BUILD_DIR"
  : > sha256sums.txt
  for artifact in marker-fixer-v${VERSION}-*.zip; do
    checksum_file "$artifact" >> sha256sums.txt
  done
)

echo "Wrote checksums: $BUILD_DIR/sha256sums.txt"

if [[ $KEEP_STAGING -eq 0 ]]; then
  rm -rf "$STAGING_DIR"
fi

echo "Build complete. Artifacts are in: $BUILD_DIR"
