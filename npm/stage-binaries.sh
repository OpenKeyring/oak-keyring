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

stage_binary() {
  local target="$1"
  local package_dir="$2"
  local archive="dist/ok-v$VERSION-$target.tar.gz"
  local binary_path

  if [[ -f "$archive" ]]; then
    binary_path="$(tar -tzf "$archive" | awk '/\/ok$/ { print; exit }')"
    if [[ -z "$binary_path" ]]; then
      echo "No ok binary found in $archive" >&2
      exit 1
    fi

    mkdir -p "$package_dir/bin"
    tar -xOzf "$archive" "$binary_path" > "$package_dir/bin/ok"
  elif [[ -x "target/$target/release/ok" ]]; then
    mkdir -p "$package_dir/bin"
    cp "target/$target/release/ok" "$package_dir/bin/ok"
  else
    echo "Missing $archive and target/$target/release/ok" >&2
    echo "Run scripts/package-preview.sh first." >&2
    exit 1
  fi

  chmod 755 "$package_dir/bin/ok"
}

stage_binary "aarch64-apple-darwin" "npm/ok-darwin-arm64"
stage_binary "x86_64-apple-darwin" "npm/ok-darwin-x64"

echo "Staged npm binaries for ok v$VERSION"
