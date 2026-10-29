# 03 — Distro and package-manager compatibility

Goal: work on any distro. Strategy: never hard-code distro names for behaviour. Detect **capabilities** (which package manager binary exists, is root filesystem writable, is the OS immutable) and use `/etc/os-release` only for display and fallbacks.

## Detection

1. Parse `/etc/os-release` (`ID`, `ID_LIKE`, `VERSION_ID`, `PRETTY_NAME`, `VARIANT_ID`). Fall back to `/usr/lib/os-release`.
2. Probe `PATH` for package managers: `apt-get`, `dnf`, `yum`, `zypper`, `pacman`, `apk`, `xbps-install`, `emerge`, `nix`, `rpm-ostree`, `transactional-update`, `eopkg`.
3. Probe universal runtimes: `flatpak`, `snap` (and `snapd` socket `/run/snapd.socket`), `distrobox`, `toolbox`.
4. Probe helpers: `pkexec`, `sudo`, `git`, `curl`/`wget`, `tar`, `unzip`, `7z`/`7zz`, `bsdtar`, `unsquashfs`, `dpkg-deb`, `rpm`, `desktop-file-validate`, `update-desktop-database`, `gcc`, `make`, `cmake`, `meson`, `cargo`.
5. Immutable check: `/run/ostree-booted` exists, or `transactional-update` present, or root mounted read-only.
6. CPU arch: `uname -m` mapped to `x86_64`, `aarch64`, `armv7l`, `riscv64`.

## Family map

| Family | `ID` / `ID_LIKE` examples | Native pkg | Local-file install | Remove |
|---|---|---|---|---|
| Debian | debian, ubuntu, linuxmint, pop, elementary, zorin, kali, raspbian | `.deb` | `apt install ./f.deb` | `apt remove` |
| Red Hat | fedora, rhel, centos, rocky, almalinux, nobara | `.rpm` | `dnf install ./f.rpm` (`yum` on old) | `dnf remove` |
| SUSE | opensuse-leap, opensuse-tumbleweed, sles | `.rpm` | `zypper install ./f.rpm` | `zypper remove` |
| Arch | arch, manjaro, endeavouros, garuda, cachyos | `.pkg.tar.zst` | `pacman -U f` | `pacman -R` |
| Alpine | alpine, postmarketos | `.apk` | `apk add --allow-untrusted ./f.apk` | `apk del` |
| Void | void | `.xbps` (not in our scope) | n/a — use universal formats | `xbps-remove` |
| Gentoo | gentoo | source ebuilds | n/a — use universal formats or source plan | `emerge -C` |
| NixOS | nixos | Nix store | FHS-dependent binaries fail; AppImage works via `appimage-run`; others need `nix-ld` | declarative |

## Immutable and special systems

| System | How to detect | Strategy |
|---|---|---|
| Fedora Silverblue/Kinoite, Bazzite, Universal Blue | `/run/ostree-booted` and `rpm-ostree` on PATH (`System.layer = "rpm-ostree"`) | `.rpm` files, Store packages and bootstrap tools → `rpm-ostree install --idempotent --apply-live` (layered; recorded with manager `rpm-ostree`, removed with `rpm-ostree uninstall`). Plan warns a reboot may be needed. Native upgrades are not listed (the base updates as a whole). `--apply-live` on local RPMs **(unverified)** |
| openSUSE MicroOS / Aeon | `transactional-update` on PATH (`layer = "transactional-update"`) | `transactional-update --non-interactive pkg install [--allow-unsigned-rpm] <pkg or file>`, effective after reboot; removal `transactional-update --non-interactive pkg remove`. Whether the new snapshot sees a file under `~/.cache` **(unverified)** |
| SteamOS (Steam Deck) | `steamos-readonly` on PATH (read-only, no layering tool) | Anything that writes the system (native packages, bootstrap) is refused with advice: Flatpak, AppImage, portable apps, or a distrobox container. Do not disable read-only. |
| NixOS | No glibc loader at `/lib64/ld-linux-x86-64.so.2` (`System.no_fhs_loader`) | AppImage menu entries run through `appimage-run` when installed; otherwise the plan says to install it or enable `programs.appimage.binfmt`. A downloaded ELF whose loader is missing gets a `programs.nix-ld.enable` note. Native formats are refused (no native manager). |
| ChromeOS Crostini | Debian in container | Works as Debian; no snapd. |
| Containers / WSL | `/.dockerenv`, `WSL_DISTRO_NAME` | Warn that GUI apps may not show; systemd may be absent (breaks snapd). |

## Decision table: artifact × system → plan

Priority: **(1) native format for this family** → **(2) universal runtime already installed** → **(3) bootstrap universal runtime** → **(4) user-prefix portable install** → **(5) conversion** → **(6) refuse with advice**.

| Artifact | Native family | Other families |
|---|---|---|
| `.deb` | `apt install ./f` | Offer `alien`/`debtap` only after warning; otherwise advise finding Flatpak/native equivalent; container route later. |
| `.rpm` | `dnf`/`zypper` | Same as above. |
| `.pkg.tar.zst` | `pacman -U` | Same. |
| `.apk` (Alpine) | `apk add` | Same. |
| `.snap` | needs `snapd` (bootstrap if absent) | Refuse on immutable w/o snapd. |
| `.flatpak/.flatpakref` | needs `flatpak` (bootstrap if absent), `--user` | Works everywhere flatpak exists. |
| `.AppImage` | user-prefix, needs FUSE2 (bootstrap `libfuse2`/`fuse-libs`/`fuse2` by family) | Works everywhere. |
| `.run/.sh/.bin` | Run with trace; root only if the script asks | Same. |
| Archives | classify contents | Same. |
| Source | native recipe if present; else user-prefix build | Same. |

## FUSE2 package names (for AppImage bootstrap)

| Family | Package |
|---|---|
| Ubuntu 24.04+ / Debian 13+ | `libfuse2t64` (24.04 per sources); older: `libfuse2` |
| Fedora / RHEL | `fuse-libs` **(unverified)** |
| openSUSE | `libfuse2` **(unverified)** |
| Arch | `fuse2` |
| Alpine | `fuse` (musl; many glibc AppImages will not run on Alpine at all) |

## Privilege model

- Never run the whole app as root.
- Steps are tagged `needs_root`. Consecutive root steps are merged into one `pkexec sh -c '…'` call so the user sees one password prompt.
- `pkexec` resets the environment (no `HOME`, `DISPLAY`); do not rely on them in root steps. The merged script starts with `export DEBIAN_FRONTEND=noninteractive;`.
- `pkexec` exit 126 = password dialog dismissed; exit 127 with a "Not authorized" / "authentication agent" message on stderr = not granted or no polkit agent running (common on bare window managers). Both become plain-language errors; 127 also shows the `sudo sh -c …` fallback. Plain 127 without that message is the script's own "command not found".
- If `pkexec` is missing, show the exact `sudo` command for the user to run in a terminal (copy button). A custom polkit action + narrow helper binary is a later milestone.
- Prefer user-scope (`flatpak --user`, `~/.local`) whenever the format allows it.

## glibc and musl

`plan::elf_interp` reads `PT_INTERP` from a 64-bit ELF (a downloaded program or `.bin` installer). A musl program (`ld-musl-*`) on a glibc system, or a glibc program (`ld-linux-*`) on musl, gets a warning (Alpine: `gcompat`). AppImages on musl keep their own warning.

## Known hard cases

- **glibc vs musl:** an AppImage or tarball built for glibc will not run on Alpine/Void-musl. Detect `ldd --version` output; warn before install.
- **Wayland vs X11:** irrelevant to install but affects whether a tray/GUI works; no action.
- **Old distro releases:** `.deb` built on a newer Ubuntu may need newer libc than Debian stable has. `apt` surfaces this; relay the message in plain words.
- **Third-party repo setup scripts** (add apt/dnf repo + key). Treat as scripts; extra warning about repo trust.
