#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="$(
  awk '
    $0 == "[package]" { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' Cargo.toml
)"

if [[ -z "$VERSION" ]]; then
  echo "Could not read package version from Cargo.toml" >&2
  exit 1
fi

TARGETS=(
  "aarch64-apple-darwin"
  "x86_64-apple-darwin"
)

DIST_DIR="$ROOT_DIR/dist"
STAGE_ROOT="$DIST_DIR/package-stage"

rm -rf "$STAGE_ROOT"
mkdir -p "$DIST_DIR" "$STAGE_ROOT"
rm -f "$DIST_DIR"/ok-v"$VERSION"-*.tar.gz "$DIST_DIR/checksums.txt"

for target in "${TARGETS[@]}"; do
  echo "Building ok for $target"
  cargo build --release --target "$target" --bin ok

  binary="$ROOT_DIR/target/$target/release/ok"
  if [[ ! -x "$binary" ]]; then
    echo "Expected binary not found or not executable: $binary" >&2
    exit 1
  fi

  package_name="ok-v$VERSION-$target"
  package_dir="$STAGE_ROOT/$package_name"
  archive="$DIST_DIR/$package_name.tar.gz"

  rm -rf "$package_dir"
  mkdir -p "$package_dir"
  cp "$binary" "$package_dir/ok"

  for doc in README.md README-ZH.md INSTALL.md INSTALL-ZH.md SECURITY.md LICENSE; do
    if [[ -f "$doc" ]]; then
      cp "$doc" "$package_dir/"
    fi
  done

  tar -C "$STAGE_ROOT" -czf "$archive" "$package_name"
done

(
  cd "$DIST_DIR"
  for archive in ok-v"$VERSION"-*.tar.gz; do
    shasum -a 256 "$archive"
  done > checksums.txt
)

rm -rf "$STAGE_ROOT"

echo "Packaged preview artifacts in $DIST_DIR"
cat "$DIST_DIR/checksums.txt"
