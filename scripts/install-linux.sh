#!/bin/sh
# Install, update or remove the Unpack desktop app on Linux (x86_64), for this user, no root.
#
#   curl -fsSL https://raw.githubusercontent.com/aiyu-ayaan/Unpack/master/scripts/install-linux.sh | sh
#   ... | sh -s -- --channel beta     beta builds (and stable ones)
#   ... | sh -s -- --version 0.0.1    a specific release
#   ... | sh -s -- --uninstall        remove it (history and settings stay in ~/.local/share/unpack)
#
# The AppImage goes to ~/.local/share/unpack, a place this user owns, because the app updates
# itself by replacing that file (app/src-tauri/src/updates.rs). It also gets a menu entry with
# the app's own icon and an `unpack` command. Re-running it updates.
#
# Works on any x86_64 glibc distro: it checks what the system has, not which distro it is. Without
# libfuse2 (not installed by default on recent Ubuntu, Debian and Fedora) the AppImage is run in its
# extract-and-run mode, so nothing has to be installed first. musl systems (Alpine) cannot run it.
#
# POSIX sh (also busybox ash): it is piped into whatever `sh` is, which on Debian and Ubuntu is dash.
set -eu

API="${UNPACK_RELEASES_URL:-https://api.github.com/repos/aiyu-ayaan/Unpack/releases?per_page=100}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APP="$DATA_HOME/unpack/Unpack.AppImage"
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

die() { printf 'error: %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --channel) CHANNEL="${2:?--channel needs a value}"; shift 2 ;;
    --version) VERSION="${2:?--version needs a value}"; VERSION="${VERSION#v}"; shift 2 ;;
    --from-file) FROM_FILE="${2:?--from-file needs a path}"; shift 2 ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h | --help) sed -n '2,7p' "$0" 2>/dev/null || true; echo "  --from-file PATH   install a local AppImage"; exit 0 ;;
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
  rm -f "$APP" "$APP.part" "$DESKTOP_FILE" "$ICON_FILE"
  # Only our own link or wrapper.
  if { [ -L "$BIN" ] && [ "$(readlink "$BIN")" = "$APP" ]; } || grep -qs "$WRAPPER_MARK" "$BIN" 2>/dev/null; then rm -f "$BIN"; fi
  refresh_menus
  echo "Unpack is removed. Its history and settings are kept in $DATA_HOME/unpack."
  exit 0
fi

case "$(uname -m)" in x86_64 | amd64) ;; *) die "Unpack for Linux is built for x86_64 only; this machine is $(uname -m)." ;; esac

# The AppImage is built against glibc. Alpine and other musl systems fail with a cryptic loader error.
if ldd --version 2>&1 | grep -qi musl; then
  die "this system uses musl libc (Alpine, Void musl); the Unpack AppImage needs glibc. Build from source instead: see the README."
fi

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
  # Every AppImage URL as "<sort key> <version> <url>"; the newest is last after a sort. The key
  # orders by number, then alpha < beta < stable, then the pre-release number (plain `sort -V`
  # puts 0.0.1 before 0.0.1-alpha.1). A channel takes its own builds and every steadier one.
  urls="$(printf '%s\n' "$releases" | grep -oE 'https://[^"]*/Unpack_[0-9][^"/]*_amd64\.AppImage' | sort -u)"
  newest_on() {
    printf '%s\n' "$urls" | awk -v channel="$1" -v want="$VERSION" '{
      v = $0; sub(/.*\/Unpack_/, "", v); sub(/_amd64\.AppImage$/, "", v)
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
  echo "Downloading Unpack $label..."
  fetch_to "$(printf '%s' "$newest" | cut -d' ' -f3)" "$APP.part" || die "the download failed. Nothing was changed."
fi

# An error page saved under the right name must not become the app.
[ "$(head -c 4 "$APP.part" | od -An -c | tr -d ' \n')" = '177ELF' ] || die "that is not an AppImage. Nothing was changed."
chmod 755 "$APP.part"
# A rename within one directory: a running copy keeps running from the old file until restarted.
mv -f "$APP.part" "$APP"
trap - EXIT INT TERM

# The icon comes out of the AppImage itself, so it is always this build's.
tmp="$(mktemp -d)"
if (cd "$tmp" && "$APP" --appimage-extract "$ICON_PATH" >/dev/null 2>&1) && [ -f "$tmp/squashfs-root/$ICON_PATH" ]; then
  mkdir -p "$(dirname "$ICON_FILE")"
  cp "$tmp/squashfs-root/$ICON_PATH" "$ICON_FILE"
else
  echo "warning: could not read the icon out of the AppImage; the menu entry will use a generic one." >&2
fi
rm -rf "$tmp"

# Without libfuse2 an AppImage cannot mount itself; extract-and-run needs nothing installed.
EXTRACT=""
has_fuse2 || EXTRACT=1

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
