#!/bin/sh
# Replay the release workflow's "Linux bundles" job in Docker: the same Ubuntu, the same apt line (read from
# .github/workflows/release.yml), then `pnpm tauri build --bundles deb,appimage`. Use it before changing
# that job. The checkout is copied in, so nothing on the host is written; the image it pulled is removed after.
#
#   scripts/test-release-build.sh [--keep]
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE=ubuntu:22.04
KEEP=0
[ "${1:-}" != --keep ] || KEEP=1

PKGS="$(sed -n 's/.*apt-get install -y --no-install-recommends //p' "$ROOT/.github/workflows/release.yml" | head -n 1)"
[ -n "$PKGS" ] || { echo "error: no apt-get install line found in release.yml" >&2; exit 1; }

had="$(docker images -q "$IMAGE")"
cleanup() { [ "$KEEP" -eq 1 ] || [ -n "$had" ] || docker rmi -f "$IMAGE" >/dev/null 2>&1 || true; }
trap cleanup EXIT

# The tree without build output and dependencies, streamed in.
git -C "$ROOT" ls-files -z --cached --others --exclude-standard | tar -C "$ROOT" --null -T - -cf - |
  docker run --rm -i -e PKGS="$PKGS" -e APPIMAGE_EXTRACT_AND_RUN=1 -e NO_STRIP=true "$IMAGE" bash -c '
    set -eu
    mkdir /src && tar -C /src -xf - && cd /src
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    # The runner image has these already; a bare container does not.
    apt-get install -y -qq --no-install-recommends ca-certificates curl git >/dev/null
    apt-get install -y --no-install-recommends $PKGS
    curl -fsSL https://deb.nodesource.com/setup_22.x | bash - >/dev/null && apt-get install -y -qq nodejs >/dev/null
    curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal -q && . "$HOME/.cargo/env"
    npm install -g -q pnpm@12 >/dev/null
    pnpm install --frozen-lockfile
    pnpm test
    pnpm tauri build --bundles deb,appimage
    ls -l app/src-tauri/target/release/bundle/deb app/src-tauri/target/release/bundle/appimage
  '
