#!/bin/sh
# Build the command-line-only Unpack: one static musl binary that runs on any x86_64/aarch64 distro.
#
#   scripts/build-cli.sh [OUT_DIR]    writes OUT_DIR/unpack-cli_<version>_<arch> (default dist-cli)
#
# On Alpine (musl) it builds directly. Elsewhere it runs itself in an alpine container with this checkout
# mounted. The build folder (app/src-tauri/target/cli-musl) and the cargo download cache stay on the host
# and are handed back to you, so the next run is incremental and Docker keeps no volume.
# The binary is the same core as the desktop app, built without the `gui` feature (no WebKitGTK).
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$(mkdir -p "${1:-$ROOT/dist-cli}" && cd "${1:-$ROOT/dist-cli}" && pwd)"
IMAGE="${UNPACK_BUILD_IMAGE:-alpine:3.24}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/app/src-tauri/Cargo.toml" | head -n 1)"
TARGET_DIR="$ROOT/app/src-tauri/target/cli-musl"

if ! ldd --version 2>&1 | grep -qi musl; then
  command -v docker >/dev/null 2>&1 || { echo "error: not a musl system and no docker to build in" >&2; exit 1; }
  CARGO_CACHE="${CARGO_HOME:-$HOME/.cargo}/registry"
  mkdir -p "$TARGET_DIR" "$CARGO_CACHE"
  # Root in the container (apk needs it); everything it wrote is given back to this user afterwards.
  docker run --rm -v "$ROOT:/src" -v "$CARGO_CACHE:/root/.cargo/registry" -e HOST_IDS="$(id -u):$(id -g)" -w /src "$IMAGE" sh -c '
    apk add --no-cache -q build-base rust cargo >/dev/null &&
    sh scripts/build-cli.sh /src/'"${OUT#"$ROOT"/}"'; s=$?
    chown -R "$HOST_IDS" /src/app/src-tauri/target/cli-musl /root/.cargo/registry /src/'"${OUT#"$ROOT"/}"'
    exit $s'
  echo "$OUT/unpack-cli_${VERSION}_$(uname -m)"
  exit 0
fi

# In musl: static, and with an explicit --target so build scripts and proc macros still link dynamically.
HOST="$(rustc -vV | sed -n 's/^host: //p')"
cd "$ROOT/app/src-tauri"
RUSTFLAGS="-C target-feature=+crt-static" cargo build --release --locked --no-default-features --target "$HOST" --target-dir "$TARGET_DIR"
BIN="$OUT/unpack-cli_${VERSION}_$(uname -m)"
cp "$TARGET_DIR/$HOST/release/unpack" "$BIN"
strip "$BIN" 2>/dev/null || true
echo "$BIN"
