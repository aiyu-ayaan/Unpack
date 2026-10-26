#!/bin/sh
# Build Unpack from source on a musl system (Alpine) into the tarball the install script installs.
#
#   scripts/build-musl.sh --deps      the Alpine packages the build needs (apk add $(scripts/build-musl.sh --deps))
#   scripts/build-musl.sh [OUT_DIR]   build; writes OUT_DIR/Unpack_<version>_<arch>-musl.tar.gz (default dist-musl)
#
# Used by the release workflow (alpine container) and by install-linux.sh --from-source.
# The AppImage needs glibc, so musl systems get a plain binary linked against their own WebKitGTK.
set -eu

BUILD_DEPS="build-base pkgconf rust cargo nodejs npm webkit2gtk-4.1-dev gtk+3.0-dev librsvg-dev openssl-dev"
if [ "${1:-}" = --deps ]; then echo "$BUILD_DEPS"; exit 0; fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$(mkdir -p "${1:-$ROOT/dist-musl}" && cd "${1:-$ROOT/dist-musl}" && pwd)"
cd "$ROOT"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' app/src-tauri/Cargo.toml | head -n 1)"
ARCH="$(uname -m)"

# pnpm through npx, so nothing is installed globally (no root needed).
npx --yes pnpm@12 install --frozen-lockfile
npx --yes pnpm@12 tauri build --no-bundle

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
cp app/src-tauri/target/release/unpack "$stage/unpack"
cp app/src-tauri/icons/icon.png "$stage/unpack.png"
tar -czf "$OUT/Unpack_${VERSION}_${ARCH}-musl.tar.gz" -C "$stage" unpack unpack.png
echo "$OUT/Unpack_${VERSION}_${ARCH}-musl.tar.gz"
