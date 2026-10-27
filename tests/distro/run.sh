#!/bin/sh
# Runs inside one distro container, as root, with the static CLI at /usr/local/bin/unpack.
# Started by scripts/test-distros.sh, which sets DISTRO. Prints PASS/FAIL lines; exits with the failure count.
#
# What it covers on every distro: detection, a native package (`tree`) installed, tracked, removed with its
# leftovers purged, a traced install script, a portable archive, an https download, the refusals (http,
# unknown file), and a non-root user with no sudo/pkexec getting the terminal command to run.
set -u

PASS=0
FAIL=0
pass() { PASS=$((PASS + 1)); echo "PASS: $*"; }
fail() { FAIL=$((FAIL + 1)); echo "FAIL: $*"; }
# check "<name>" <command...>: passes when the command exits 0.
check() { name="$1"; shift; if "$@" >/tmp/out 2>&1; then pass "$name"; else fail "$name"; sed 's/^/    | /' /tmp/out | tail -n 25; fi; }

W=/work
mkdir -p "$W" && cd "$W" || exit 99

# ---- per distro: the expected native manager, how to ask it whether `tree` is installed, `tree` downloaded as a package file, tools the tests use
case "$DISTRO" in
  debian | ubuntu)
    NATIVE=apt
    INSTALLED="dpkg -s tree"
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq && apt-get download -qq tree ;;
  fedora)
    NATIVE=dnf
    INSTALLED="rpm -q tree"
    dnf -y -q install tar gzip >/dev/null && dnf -y -q download tree ;;
  rocky)
    NATIVE=dnf
    INSTALLED="rpm -q tree"
    dnf -y -q install tar gzip dnf-plugins-core >/dev/null && dnf -y -q download tree ;;
  opensuse)
    NATIVE=zypper
    INSTALLED="rpm -q tree"
    zypper -nq install tar gzip >/dev/null && zypper -nq download tree && cp /var/cache/zypp/packages/*/*/tree-*.rpm "$W"/ ;;
  arch)
    NATIVE=pacman
    INSTALLED="pacman -Q tree"
    pacman -Sy --noconfirm --needed -q tar gzip >/dev/null && pacman -Sw --noconfirm -q tree >/dev/null && cp /var/cache/pacman/pkg/tree-*.pkg.tar.zst "$W"/ ;;
  alpine)
    NATIVE=apk
    INSTALLED="apk info -e tree"
    apk update -q && apk add -q tar gzip && apk fetch -q tree -o "$W" ;;
  *) echo "unknown DISTRO=$DISTRO"; exit 99 ;;
esac
PKG="$(ls "$W"/tree[-_]* 2>/dev/null | head -n 1)"
[ -n "$PKG" ] && pass "setup: downloaded $(basename "$PKG")" || fail "setup: could not download the tree package"

echo "== $(unpack version) on $(. /etc/os-release; echo "$PRETTY_NAME")"

# ---- detection
check "system: detects $NATIVE" sh -c "unpack system --json | grep -q '\"native\": \"$NATIVE\"'"
check "system: sees it runs as root" sh -c "unpack system --json | grep -q '\"is_root\": true'"

# ---- native package: check, install, list, remove + purge
if [ -n "$PKG" ]; then
  check "native: check accepts $(basename "$PKG")" unpack check "$PKG"
  check "native: install" unpack install "$PKG" --yes
  check "native: package installed" $INSTALLED
  check "native: tree runs" tree --version
  check "native: listed" sh -c "unpack list --json | grep -q '\"name\": \"tree\"'"
  mkdir -p /root/.config/tree /root/.cache/tree && echo x >/root/.config/tree/settings
  check "native: remove --purge" unpack remove tree --yes --purge
  # Not `command -v tree`: busybox (Alpine) has its own tree applet.
  check "native: package removed" sh -c "! $INSTALLED"
  check "native: leftovers purged" sh -c '[ ! -e /root/.config/tree ] && [ ! -e /root/.cache/tree ]'
  check "native: marked removed" sh -c "unpack list --all --json | grep -q '\"status\": \"removed\"'"
fi

# ---- install script, traced
cat >"$W/hello-install.sh" <<'EOF'
#!/bin/sh
mkdir -p "$HOME/.local/bin" && printf '#!/bin/sh\necho hello\n' >"$HOME/.local/bin/hello-unpack" && chmod +x "$HOME/.local/bin/hello-unpack"
EOF
check "script: install" unpack install "$W/hello-install.sh" --yes
check "script: created file" test -x /root/.local/bin/hello-unpack
check "script: tracked as files" sh -c "unpack list --json | grep -q hello-unpack"
check "script: remove" unpack remove hello-install --yes
check "script: file removed" test ! -e /root/.local/bin/hello-unpack

# ---- portable archive
mkdir -p "$W/a/mytool-1.0" && printf '#!/bin/sh\necho mytool\n' >"$W/a/mytool-1.0/mytool" && chmod +x "$W/a/mytool-1.0/mytool"
tar -czf "$W/mytool-1.0.tar.gz" -C "$W/a" mytool-1.0
check "archive: install" unpack install "$W/mytool-1.0.tar.gz" --yes
check "archive: command linked" sh -c '[ "$(/root/.local/bin/mytool)" = mytool ]'
check "archive: remove" unpack remove mytool-1.0 --yes
check "archive: removed" sh -c '[ ! -e /root/.local/bin/mytool ] && [ ! -e /root/.local/opt/mytool-1.0 ]'

# ---- network and refusals
check "https: download and classify a script" unpack check https://raw.githubusercontent.com/aiyu-ayaan/Unpack/master/scripts/install-linux.sh
check "http: refused" sh -c 'unpack check http://example.com/app.deb 2>&1 | grep -q "Refusing plain http"'
head -c 4096 /dev/zero >"$W/junk.bin"
check "unknown file: exit 2" sh -c 'unpack check '"$W"'/junk.bin; [ $? -eq 2 ]'
check "not a terminal: install asks for --yes" sh -c 'unpack install '"$W"'/hello-install.sh </dev/null 2>&1 | grep -q -- "--yes"'

# ---- a normal user with no admin prompt gets the command to run instead. Without a terminal the CLI never
# uses sudo, so only pkexec (a password window) has to be absent.
if [ -n "$PKG" ] && ! command -v pkexec >/dev/null 2>&1; then
  if command -v useradd >/dev/null 2>&1; then useradd -m tester; else adduser -D tester; fi
  cp "$PKG" /home/tester/ && chown tester /home/tester/"$(basename "$PKG")"
  check "user: no admin tool, says what to run" sh -c "su tester -s /bin/sh -c 'unpack install /home/tester/$(basename "$PKG") --yes' 2>&1 | grep -q 'sudo sh -c'"
fi

echo "== $DISTRO: $PASS passed, $FAIL failed"
exit "$FAIL"
