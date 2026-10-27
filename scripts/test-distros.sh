#!/bin/sh
# Test this checkout's Unpack on real distros in Docker: builds the static CLI from the local source, runs
# tests/distro/run.sh in each distro image, then removes every image it pulled.
#
#   scripts/test-distros.sh                     all distros
#   scripts/test-distros.sh debian arch         some of them
#   scripts/test-distros.sh --keep ...          keep the pulled images (faster reruns)
#   scripts/test-distros.sh --no-build ...      reuse the CLI already in dist-cli/
#
# Distros: debian ubuntu fedora rocky opensuse arch alpine
# Logs: test-results/<distro>.log, summary in test-results/summary.txt. Exit status: number of failed distros.
# Cleanup only touches what this run pulled: images that were already on the machine are kept, and no
# volumes are made (scripts/build-cli.sh keeps its build cache on the host).
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RESULTS="$ROOT/test-results"
KEEP=0
BUILD=1
ALL="debian ubuntu fedora rocky opensuse arch alpine"
WANT=""

image_of() {
  case "$1" in
    debian) echo debian:12 ;;
    ubuntu) echo ubuntu:24.04 ;;
    fedora) echo fedora:42 ;;
    rocky) echo rockylinux/rockylinux:9 ;;
    opensuse) echo opensuse/tumbleweed:latest ;;
    arch) echo archlinux:latest ;;
    alpine) echo alpine:3.24 ;;
    *) return 1 ;;
  esac
}

for a in "$@"; do
  case "$a" in
    --keep) KEEP=1 ;;
    --no-build) BUILD=0 ;;
    -h | --help) sed -n '2,13p' "$0"; exit 0 ;;
    *) image_of "$a" >/dev/null || { echo "unknown distro: $a (choose from: $ALL)" >&2; exit 64; }; WANT="$WANT $a" ;;
  esac
done
[ -n "$WANT" ] || WANT="$ALL"
command -v docker >/dev/null 2>&1 || { echo "error: docker is needed" >&2; exit 1; }

# Images present before this run are never removed.
BEFORE="$(docker images --format '{{.Repository}}:{{.Tag}}')"
PULLED=""
cleanup() {
  [ "$KEEP" -eq 1 ] && return 0
  for img in $PULLED; do docker rmi -f "$img" >/dev/null 2>&1 && echo "removed image $img"; done
}
trap cleanup EXIT
trap 'exit 130' INT TERM
track() { printf '%s\n' "$BEFORE" | grep -qx "$1" || case " $PULLED " in *" $1 "*) ;; *) PULLED="$PULLED $1" ;; esac; }

# The build image counts as pulled too when it was not here before.
track alpine:3.24
if [ "$BUILD" -eq 1 ]; then
  echo "== building the CLI from this checkout"
  "$ROOT/scripts/build-cli.sh" "$ROOT/dist-cli" || { echo "error: build failed" >&2; exit 1; }
fi
BIN="$(ls -t "$ROOT"/dist-cli/unpack-cli_* 2>/dev/null | head -n 1)"
[ -n "$BIN" ] || { echo "error: no CLI in dist-cli/; run without --no-build" >&2; exit 1; }

rm -rf "$RESULTS" && mkdir -p "$RESULTS"
failed=0
for d in $WANT; do
  img="$(image_of "$d")"
  track "$img"
  echo "== $d ($img)"
  docker run --rm -e DISTRO="$d" -v "$BIN:/usr/local/bin/unpack:ro" -v "$ROOT/tests/distro:/t:ro" "$img" sh /t/run.sh >"$RESULTS/$d.log" 2>&1
  code=$?
  grep -E '^(PASS|FAIL|==)' "$RESULTS/$d.log" | grep -E '^(FAIL|==)' || true
  if [ "$code" -eq 0 ]; then
    echo "$d: ok" >>"$RESULTS/summary.txt"
  else
    failed=$((failed + 1))
    echo "$d: FAILED (exit $code, see test-results/$d.log)" >>"$RESULTS/summary.txt"
  fi
done

echo "== summary"
cat "$RESULTS/summary.txt"
exit "$failed"
