#!/bin/sh
# Install, update or remove the Unpack desktop app on Linux (x86_64 or aarch64), for this user, no root.
#
#   curl -fsSL https://raw.githubusercontent.com/aiyu-ayaan/Unpack/master/scripts/install-linux.sh | sh
#   ... | sh -s -- --channel beta     beta builds (and stable ones)
#   ... | sh -s -- --version 0.0.1    a specific release
#   ... | sh -s -- --uninstall        remove it (history and settings stay in ~/.local/share/unpack)
#   ... | sh -s -- --from-source      musl only: build it here instead of downloading the build
#   ... | sh -s -- --cli              the command line only (one static binary, no window, any distro)
#
# The AppImage goes to ~/.local/share/unpack, a place this user owns, because the app updates
# itself by replacing that file (app/src-tauri/src/updates.rs). It also gets a menu entry with
# the app's own icon and an `unpack` command. Re-running it updates.
#
# Works on any x86_64 or aarch64 distro: it checks what the system has, not which distro it is. Without
# libfuse2 (not installed by default on recent Ubuntu, Debian and Fedora) the AppImage is run in its
# extract-and-run mode, so nothing has to be installed first. musl systems (Alpine) cannot run an
# AppImage: they get the `-musl.tar.gz` the release workflow builds on Alpine, or build it from source
# with scripts/build-musl.sh when there is none. Either way WebKitGTK comes from `apk` (asks for root).
#
# POSIX sh (also busybox ash): it is piped into whatever `sh` is, which on Debian and Ubuntu is dash.
set -eu

API="${UNPACK_RELEASES_URL:-https://api.github.com/repos/aiyu-ayaan/Unpack/releases?per_page=100}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APP="$DATA_HOME/unpack/Unpack.AppImage"
# musl: a plain binary instead of the AppImage.
MUSL_APP="$DATA_HOME/unpack/unpack"
# --cli: the static command-line build, no desktop parts.
CLI_APP="$DATA_HOME/unpack/unpack-cli"
REPO="https://github.com/aiyu-ayaan/Unpack"
BIN="$HOME/.local/bin/unpack"
# StartupWMClass matches the window class Tauri gives the app, which ties the window to this entry.
DESKTOP_FILE="$DATA_HOME/applications/unpack.desktop"
ICON_PATH="usr/share/icons/hicolor/512x512/apps/unpack.png"
ICON_FILE="$DATA_HOME/icons/hicolor/512x512/apps/unpack.png"
WRAPPER_MARK="installed by the Unpack install script"

CHANNEL=stable
VERSION=""
FROM_FILE=""
UNINSTALL=0
FROM_SOURCE=0
CLI=0

die() { printf 'error: %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --channel) CHANNEL="${2:?--channel needs a value}"; shift 2 ;;
    --version) VERSION="${2:?--version needs a value}"; VERSION="${VERSION#v}"; shift 2 ;;
    --from-file) FROM_FILE="${2:?--from-file needs a path}"; shift 2 ;;
    --uninstall) UNINSTALL=1; shift ;;
    --from-source) FROM_SOURCE=1; shift ;;
    --cli) CLI=1; shift ;;
    -h | --help) sed -n '2,9p' "$0" 2>/dev/null || true; echo "  --from-file PATH   install a local AppImage"; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
done
case "$CHANNEL" in stable | beta | alpha) ;; *) die "--channel must be stable, beta or alpha" ;; esac
[ "$(id -u)" -ne 0 ] || die "run this as the user who will use Unpack, not as root."

refresh_menus() {
  command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$DATA_HOME/applications" 2>/dev/null || true
  command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$DATA_HOME/icons/hicolor" 2>/dev/null || true
}

if [ "$UNINSTALL" -eq 1 ]; then
  rm -f "$APP" "$APP.part" "$MUSL_APP" "$MUSL_APP.part" "$CLI_APP" "$CLI_APP.part" "$DESKTOP_FILE" "$ICON_FILE"
  # Only our own link or wrapper.
  if { [ -L "$BIN" ] && { [ "$(readlink "$BIN")" = "$APP" ] || [ "$(readlink "$BIN")" = "$MUSL_APP" ] || [ "$(readlink "$BIN")" = "$CLI_APP" ]; }; } || grep -qs "$WRAPPER_MARK" "$BIN" 2>/dev/null; then rm -f "$BIN"; fi
  refresh_menus
  echo "Unpack is removed. Its history and settings are kept in $DATA_HOME/unpack."
  exit 0
fi

# Release file suffix per CPU, as the release workflow names them.
case "$(uname -m)" in
  x86_64 | amd64) ARCH=amd64 ;;
  aarch64 | arm64) ARCH=aarch64 ;;
  *) die "Unpack for Linux is built for x86_64 and aarch64; this machine is $(uname -m)." ;;
esac

# Release files are Unpack_<version>_<suffix>; the command line is unpack-cli_<version>_<uname -m>.
PREFIX=Unpack
# The AppImage is built against glibc; musl systems get a native build instead. The CLI is static: any libc.
MUSL=""
if [ "$CLI" -eq 1 ]; then
  [ "$FROM_SOURCE" -eq 0 ] || die "--from-source is not needed with --cli; that build runs everywhere."
  PREFIX=unpack-cli
  APP="$CLI_APP"
  ASSET="_$(uname -m)"
elif ldd --version 2>&1 | grep -qi musl; then
  MUSL=1
  APP="$MUSL_APP"
  ASSET="_$(uname -m)-musl\\.tar\\.gz"
else
  [ "$FROM_SOURCE" -eq 0 ] || die "--from-source is for musl systems (Alpine); this one runs the AppImage."
  ASSET="_${ARCH}\\.AppImage"
fi

# Root only for system packages, through whichever of sudo or doas exists.
as_root() {
  if command -v sudo >/dev/null 2>&1; then sudo "$@"
  elif command -v doas >/dev/null 2>&1; then doas "$@"
  else die "need root to run: $* (install sudo or doas, or run that as root first)"
  fi
}
# Install the apk packages that are missing, if this system has apk; elsewhere say what is needed.
need_apk() {
  if ! command -v apk >/dev/null 2>&1; then
    echo "warning: no apk here; make sure these (or your system's equivalents) are installed: $*" >&2
    return 0
  fi
  missing=""
  for p in "$@"; do apk info -e "$p" >/dev/null 2>&1 || missing="$missing $p"; done
  [ -z "$missing" ] && return 0
  echo "Installing system packages:$missing"
  # shellcheck disable=SC2086
  as_root apk add $missing || die "could not install:$missing"
}

has_fuse2() {
  { PATH="$PATH:/sbin:/usr/sbin" ldconfig -p 2>/dev/null | grep -q 'libfuse\.so\.2'; } && return 0
  for d in /lib /lib64 /usr/lib /usr/lib64 /lib/x86_64-linux-gnu /usr/lib/x86_64-linux-gnu; do
    ls "$d"/libfuse.so.2* >/dev/null 2>&1 && return 0
  done
  return 1
}

mkdir -p "$DATA_HOME/unpack"
trap 'rm -f "$APP.part"' EXIT INT TERM

if [ -n "$FROM_FILE" ]; then
  cp "$FROM_FILE" "$APP.part" || die "cannot read $FROM_FILE"
  label="$(basename "$FROM_FILE")"
else
  if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL --retry 3 "$1"; }
    fetch_to() { curl -fL --retry 3 --progress-bar -o "$2" "$1"; }
  elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO- "$1"; }
    fetch_to() { wget -q --show-progress -O "$2" "$1"; }
  else
    die "curl or wget is needed to download Unpack."
  fi

  echo "Looking up Unpack releases..."
  releases="$(fetch "$API")" || die "could not reach GitHub. Try again in a while."
  # Every build URL as "<sort key> <version> <url>"; the newest is last after a sort. The key
  # orders by number, then alpha < beta < stable, then the pre-release number (plain `sort -V`
  # puts 0.0.1 before 0.0.1-alpha.1). A channel takes its own builds and every steadier one.
  find_urls() { printf '%s\n' "$releases" | grep -oE "https://[^\"]*/${PREFIX}_[0-9][^\"/]*$1" | sort -u || true; }
  urls="$(find_urls "$ASSET")"
  SOURCE=""
  if [ -n "$MUSL" ] && { [ "$FROM_SOURCE" -eq 1 ] || [ -z "$urls" ]; }; then
    # Build that release from source: its version comes from the release's other files.
    [ "$FROM_SOURCE" -eq 1 ] || echo "No Alpine (musl) build in the releases; building from source instead."
    SOURCE=1
    urls="$(find_urls '_[a-z0-9]+\.AppImage')"
  fi
  newest_on() {
    printf '%s\n' "$urls" | awk -v channel="$1" -v want="$VERSION" -v prefix="$PREFIX" '{
      v = $0; sub(".*/" prefix "_", "", v); sub(/_.*/, "", v)
      if (v !~ /^[0-9]+\.[0-9]+\.[0-9]+(-(alpha|beta)\.[0-9]+)?$/) next
      split(v, p, /[.-]/)
      stage = 2; n = 99999
      if (p[4] == "alpha") { stage = 0; n = p[5] } else if (p[4] == "beta") { stage = 1; n = p[5] }
      if (want != "" ? v != want : (stage == 0 && channel != "alpha") || (stage == 1 && channel == "stable")) next
      printf "%08d.%08d.%08d.%d.%08d %s %s\n", p[1], p[2], p[3], stage, n, v, $0
    }' | sort | tail -n 1
  }
  # No stable release yet: fall back to beta, then alpha. A named --version never falls back.
  newest=""
  for c in stable beta alpha; do
    case "$CHANNEL:$c" in stable:* | beta:beta | beta:alpha | alpha:alpha) ;; *) continue ;; esac
    newest="$(newest_on "$c")"
    if [ -n "$newest" ]; then
      [ "$c" = "$CHANNEL" ] || [ -n "$VERSION" ] || echo "No $CHANNEL release yet; using the newest $c build."
      break
    fi
    [ -z "$VERSION" ] || break
  done
  [ -n "$newest" ] || die "no Linux build found for ${VERSION:+version $VERSION}$([ -n "$VERSION" ] || echo "any channel"). See the releases page."
  label="$(printf '%s' "$newest" | cut -d' ' -f2)"
  if [ -n "$MUSL" ]; then
    work="$(mktemp -d)"
    trap 'rm -rf "$APP.part" "$work"' EXIT INT TERM
    if [ -n "$SOURCE" ]; then
      echo "Downloading the Unpack $label source..."
      fetch_to "$REPO/archive/refs/tags/v$label.tar.gz" "$work/src.tar.gz" || die "could not download the source of v$label."
      mkdir "$work/src" && tar -xzf "$work/src.tar.gz" -C "$work/src" --strip-components 1
      # shellcheck disable=SC2046
      need_apk $(sh "$work/src/scripts/build-musl.sh" --deps)
      echo "Building Unpack $label (this takes a while)..."
      sh "$work/src/scripts/build-musl.sh" "$work/out" >"$work/build.log" 2>&1 || { tail -n 30 "$work/build.log" >&2; die "the build failed. Nothing was changed."; }
      tarball="$(ls "$work"/out/*-musl.tar.gz)"
    else
      echo "Downloading Unpack $label..."
      tarball="$work/unpack.tar.gz"
      fetch_to "$(printf '%s' "$newest" | cut -d' ' -f3)" "$tarball" || die "the download failed. Nothing was changed."
    fi
    tar -xzf "$tarball" -C "$work" unpack unpack.png || die "that is not an Unpack build. Nothing was changed."
    cp "$work/unpack" "$APP.part"
    mkdir -p "$(dirname "$ICON_FILE")"
    cp "$work/unpack.png" "$ICON_FILE"
  else
    echo "Downloading Unpack $label..."
    fetch_to "$(printf '%s' "$newest" | cut -d' ' -f3)" "$APP.part" || die "the download failed. Nothing was changed."
  fi
fi

# An error page saved under the right name must not become the app.
[ "$(head -c 4 "$APP.part" | od -An -c | tr -d ' \n')" = '177ELF' ] || die "that is not a Linux program. Nothing was changed."
chmod 755 "$APP.part"
# A rename within one directory: a running copy keeps running from the old file until restarted.
mv -f "$APP.part" "$APP"
trap - EXIT INT TERM
[ -z "${work:-}" ] || rm -rf "$work"

if [ "$CLI" -eq 1 ]; then
  mkdir -p "$(dirname "$BIN")"
  if [ ! -e "$BIN" ] || { [ -L "$BIN" ] && [ "$(readlink "$BIN")" = "$CLI_APP" ]; }; then
    rm -f "$BIN" && ln -s "$CLI_APP" "$BIN"
    echo "Unpack $label (command line) is installed. Run 'unpack --help'."
  else
    # The desktop app's own 'unpack' command already takes the same commands.
    echo "Unpack $label (command line) is installed at $CLI_APP. $BIN already exists (the desktop app takes the same commands), so it was left as it is."
  fi
  case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) echo "note: add $HOME/.local/bin to your PATH to use the 'unpack' command." >&2 ;; esac
  exit 0
fi

# The musl build links against the system's WebKitGTK and GTK.
[ -z "$MUSL" ] || need_apk webkit2gtk-4.1 gtk+3.0

# The icon comes out of the AppImage itself, so it is always this build's (musl: from its tarball).
tmp="$(mktemp -d)"
if [ -n "$MUSL" ]; then
  [ -f "$ICON_FILE" ] || echo "warning: no icon; the menu entry will use a generic one." >&2
elif (cd "$tmp" && "$APP" --appimage-extract "$ICON_PATH" >/dev/null 2>&1) && [ -f "$tmp/squashfs-root/$ICON_PATH" ]; then
  mkdir -p "$(dirname "$ICON_FILE")"
  cp "$tmp/squashfs-root/$ICON_PATH" "$ICON_FILE"
else
  echo "warning: could not read the icon out of the AppImage; the menu entry will use a generic one." >&2
fi
rm -rf "$tmp"

# Without libfuse2 an AppImage cannot mount itself; extract-and-run needs nothing installed.
EXTRACT=""
[ -n "$MUSL" ] || has_fuse2 || EXTRACT=1

# Same fields as the entry Tauri writes into the .deb. The icon is an absolute path so it shows
# without waiting for the icon theme cache.
mkdir -p "$(dirname "$DESKTOP_FILE")"
cat >"$DESKTOP_FILE" <<ENTRY
[Desktop Entry]
Type=Application
Name=Unpack
Comment=Install anything on Linux
Exec=${EXTRACT:+env APPIMAGE_EXTRACT_AND_RUN=1 }"$APP"
TryExec=$APP
Icon=$ICON_FILE
Terminal=false
Categories=Utility;
StartupWMClass=unpack
ENTRY
refresh_menus

# `unpack` on the command line: a link, or a small wrapper when the AppImage needs extract-and-run.
mkdir -p "$(dirname "$BIN")"
if [ ! -e "$BIN" ] || [ -L "$BIN" ] || grep -qs "$WRAPPER_MARK" "$BIN" 2>/dev/null; then
  rm -f "$BIN"
  if [ -n "$EXTRACT" ]; then
    printf '#!/bin/sh\n# %s\nAPPIMAGE_EXTRACT_AND_RUN=1 exec "%s" "$@"\n' "$WRAPPER_MARK" "$APP" >"$BIN"
    chmod 755 "$BIN"
  else
    ln -s "$APP" "$BIN"
  fi
fi

echo "Unpack $label is installed: find it in your applications menu, or run 'unpack'."
if [ -n "$EXTRACT" ]; then
  echo "note: libfuse2 is not installed, so Unpack unpacks itself on every start (a little slower). Installing it ('libfuse2' or 'fuse-libs' with your package manager) makes it start faster." >&2
fi
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) echo "note: add $HOME/.local/bin to your PATH to use the 'unpack' command." >&2 ;; esac
