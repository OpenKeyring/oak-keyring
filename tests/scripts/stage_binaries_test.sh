#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

cp -R "$ROOT_DIR/npm" "$TMP_DIR/npm"

cat > "$TMP_DIR/Cargo.toml" <<'EOF'
[package]
name = "oak-keyring"
version = "0.8.0-preview.1"
edition = "2021"
EOF

make_archive() {
  local target="$1"
  local payload="$2"
  local package="ok-v0.8.0-preview.1-$target"
  local package_dir="$TMP_DIR/build/$package"

  mkdir -p "$package_dir" "$TMP_DIR/dist"
  printf '%s\n' "$payload" > "$package_dir/ok"
  chmod 755 "$package_dir/ok"
  printf 'readme\n' > "$package_dir/README.md"
  printf 'install\n' > "$package_dir/INSTALL.md"
  printf 'security\n' > "$package_dir/SECURITY.md"

  tar -C "$TMP_DIR/build" -czf "$TMP_DIR/dist/$package.tar.gz" \
    "$package/ok" \
    "$package/README.md" \
    "$package/INSTALL.md" \
    "$package/SECURITY.md"
}

make_archive "aarch64-apple-darwin" "arm64 binary"
make_archive "x86_64-apple-darwin" "x64 binary"

mkdir -p "$TMP_DIR/fakebin"
cat > "$TMP_DIR/fakebin/tar" <<'EOF'
#!/usr/bin/env python3
import os
import sys

def package_name(archive):
    return os.path.basename(archive).removesuffix(".tar.gz")

if len(sys.argv) >= 3 and sys.argv[1] == "-tzf":
    package = package_name(sys.argv[2])
    try:
        print(f"{package}/ok", flush=True)
        for index in range(10000):
            print(f"{package}/doc-{index}.md", flush=True)
    except BrokenPipeError:
        sys.stderr.write("tar: stdout: write error\n")
        sys.exit(2)
    sys.exit(0)

if len(sys.argv) >= 4 and sys.argv[1] == "-xOzf":
    archive = sys.argv[2]
    if "aarch64-apple-darwin" in archive:
        print("arm64 binary")
    elif "x86_64-apple-darwin" in archive:
        print("x64 binary")
    else:
        sys.stderr.write(f"unexpected archive: {archive}\n")
        sys.exit(2)
    sys.exit(0)

sys.stderr.write(f"unexpected tar invocation: {' '.join(sys.argv[1:])}\n")
sys.exit(2)
EOF
chmod 755 "$TMP_DIR/fakebin/tar"

export PATH="$TMP_DIR/fakebin:$PATH"

bash "$TMP_DIR/npm/stage-binaries.sh"

test -x "$TMP_DIR/npm/ok-darwin-arm64/bin/ok"
test -x "$TMP_DIR/npm/ok-darwin-x64/bin/ok"
grep -q "arm64 binary" "$TMP_DIR/npm/ok-darwin-arm64/bin/ok"
grep -q "x64 binary" "$TMP_DIR/npm/ok-darwin-x64/bin/ok"
