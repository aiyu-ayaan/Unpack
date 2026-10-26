# 02 — Format reference

For each format: how to recognise it (magic bytes beat file extensions), how to read metadata without installing, how to install, how to remove, and traps. Commands marked **(unverified)** came from memory, not a fetched source.

## Detection order

1. Read first 512 bytes. Match magic. Extension is only a tie-breaker.
2. If it is an archive, peek at the member list (never extract blindly).
3. If it is text, look at the first line (`#!`) and known headers.
4. Otherwise report "unknown" and offer to open the file manager.

| Kind | Magic / marker |
|---|---|
| deb | `!<arch>\n` then member `debian-binary` |
| rpm | `ED AB EE DB` |
| gzip | `1F 8B` |
| xz | `FD 37 7A 58 5A 00` |
| bzip2 | `42 5A 68` (`BZh`) |
| zstd | `28 B5 2F FD` (also pacman `.pkg.tar.zst`) |
| zip | `50 4B 03 04` (also Android APK, Java JAR, some `.flatpak`-like bundles are NOT zip) |
| 7z | `37 7A BC AF 27 1C` |
| squashfs | `hsqs` (snap; AppImage type 2 appends squashfs after the ELF runtime) |
| ELF | `7F 45 4C 46` |
| AppImage | ELF with bytes `41 49 02` (`AI\x02`) at offset 8 (type 2) or `AI\x01` (type 1, ISO9660) |
| Shell script | `#!` |
| Flatpak ref | text starting `[Flatpak Ref]` |
| Flatpak repo | text starting `[Flatpak Repo]` |
| tar | `ustar` at offset 257 |

## Installable package formats

### .deb
- Structure: `ar` archive with `debian-binary`, `control.tar.{gz,xz,zst}`, `data.tar.*`.
- Metadata: `dpkg-deb -f file.deb Package Version Architecture Depends`. Architecture check against `dpkg --print-architecture`.
- Install (Debian family): `apt install ./file.deb` — preferred over `dpkg -i` because apt resolves dependencies.
- Remove: `apt remove <Package>` (or `purge`).
- Non-Debian distro: not installable natively. Options in order: find native equivalent, Flatpak/AppImage of same app, `alien`/`debtap` conversion, container (distrobox). `bsdtar -xf file.deb` lists the members; the data tarball holds the files.
- Trap: arm64 `.deb` on amd64 box; `Depends` on packages not in this distro's release (e.g. Ubuntu `.deb` on Debian stable).

### .rpm
- Magic `ED AB EE DB`. Metadata: `rpm -qp --queryformat '%{NAME} %{VERSION}-%{RELEASE} %{ARCH}\n' file.rpm`.
- Install: Fedora/RHEL `dnf install ./file.rpm`; openSUSE `zypper install ./file.rpm` (will prompt about unsigned packages — surface that prompt, do not silence it).
- Remove: `dnf remove <name>` / `zypper remove <name>` / `rpm -e <name>`.
- Immutable Fedora variants: `rpm-ostree install ./file.rpm` layers the package and needs a reboot (detect `/run/ostree-booted`).
- Trap: signature/GPG key missing; `.rpm` built for another distro's library versions.

### .pkg.tar.zst (also `.pkg.tar.xz`)
- A zstd-compressed tar containing `.PKGINFO`, `.MTREE`, optional `.INSTALL`. Metadata: `bsdtar -xOf file .PKGINFO` (fields `pkgname`, `pkgver`, `arch`, `depend`).
- Install: `pacman -U file.pkg.tar.zst`. Remove: `pacman -R <pkgname>`.
- Trap: local `-U` installs do not resolve dependencies from repos unless they are already satisfiable by sync; pacman will refuse and say which are missing.

### .apk (Alpine) — and the Android clash
- Alpine `.apk` (v2 format): concatenated gzip tar segments (signature, control, data). Metadata in `.PKGINFO` inside. Install: `apk add --allow-untrusted ./file.apk` (the flag is needed for unsigned/local packages — show a warning). Remove: `apk del <name>`.
- Newer apk-tools use a different (ADB) container **(unverified)**; plan to detect and error cleanly.
- **Android `.apk` is a zip.** If an `.apk` begins `PK`, it is an Android app: report "this is an Android app, not a Linux package" instead of failing obscurely. (Waydroid is the only route; out of scope.)

### .snap
- squashfs image with `meta/snap.yaml` (name, version, confinement). Read: `unsquashfs -cat file.snap meta/snap.yaml` **(unverified)**.
- Install local: `snap install --dangerous file.snap` (unsigned local snap; add `--classic` if confinement is `classic` and the user consents).
- Remove: `snap remove <name>`. Needs `snapd` running — not default on Fedora/Arch; bootstrap step: install `snapd` then enable `snapd.socket`.

### .flatpak / .flatpakref / .flatpakrepo
- `.flatpak`: single-file bundle. `flatpak install --bundle file.flatpak` (assumed when the name ends `.flatpak`).
- `.flatpakref`: small INI file pointing to a remote. `flatpak install --from file.flatpakref` (assumed when the name ends `.flatpakref`).
- `.flatpakrepo`: remote definition; `flatpak remote-add --from` **(unverified exact syntax)**.
- Scope: system-wide by default; `--user` avoids root. **Prefer `--user`** to avoid privilege prompts.
- Remove: `flatpak uninstall <app-id>`. Needs `flatpak` package; bootstrap if missing.

### .AppImage
- ELF runtime + squashfs payload. Needs the `libfuse.so.2` library to mount; on Ubuntu 24.04 the package is `libfuse2t64` (not `libfuse2`). Without FUSE, `--appimage-extract-and-run` (or env `APPIMAGE_EXTRACT_AND_RUN=1`) works without mounting **(unverified for every runtime)**.
- Metadata: `./app.AppImage --appimage-extract 'usr/share/applications/*.desktop'` and icon; `--appimage-extract` unpacks to `squashfs-root/`.
- "Install" = copy to `~/Applications` (or `~/.local/share/unpack/appimages`), `chmod +x`, write `.desktop` with absolute `Exec=`, copy icon into `~/.local/share/icons/hicolor/…`, run `update-desktop-database ~/.local/share/applications`. Validate with `desktop-file-validate`.
- Updates: embedded update information (zsync transport or `gh-releases-zsync|owner|repo|latest|file.zsync`), applied by `appimageupdatetool`.
- Remove: delete the AppImage, the `.desktop`, icons. No root.

### .run (vendor installer)
- Usually a **makeself** self-extracting shell script with an embedded archive (NVIDIA driver, VirtualBox, many vendors). Safe introspection flags **(unverified, makeself-specific)**: `--info`, `--list`, `--check`. Others (Qt Installer Framework) are ELF binaries with their own flags.
- Always unmanaged: we run it and trace file changes. Often needs root. Warn that drivers (NVIDIA) should come from the distro instead.

### .bin
- Meaning varies: ELF executable, shell stub (Oracle/Java style installers), or raw firmware/data. Sniff magic; if ELF/`#!` treat as an installer/executable, else refuse.

### .sh
- Not a format; code. Show the full text, scan for risky patterns (`sudo`, `rm -rf`, writes to `/etc`, nested `curl | sh`), then run only after consent. Trace file changes.

## Archive formats

`.tar.gz/.tgz`, `.tar.xz/.txz`, `.tar.bz2/.tbz2`, `.tar.zst`, `.tar`, `.zip`, `.7z`, and bare `.gz/.xz/.bz2/.zst`.

Extraction does not install. Classify contents:

| Contents contain | Classification | Plan |
|---|---|---|
| `configure`, `CMakeLists.txt`, `meson.build`, `Makefile`, `Cargo.toml`, `go.mod`, `setup.py`/`pyproject.toml`, `package.json` | Source tree | Build plan (see below) |
| `debian/` dir or `PKGBUILD` or `*.spec` | Source with native recipe | Prefer building a native package |
| `install.sh` / `setup.sh` / `*.run` at top | Vendor installer in archive | Treat as script |
| ELF executables + `.desktop` or `bin/` | Portable app | Extract to `~/.local/opt/<name>`, symlink into `~/.local/bin`, write `.desktop` |
| `.deb`/`.rpm`/`.AppImage` inside | Wrapper | Recurse on inner file |
| Anything else | Plain files | Offer "extract to…" only |

Safety rules: reject absolute paths and `..` members (zip-slip), refuse symlinks that escape the target, cap total uncompressed size and file count (decompression bombs), extract into a fresh private temp dir.

Bare compressed single files (`.gz/.xz/.bz2/.zst` that are not tarballs): decompress to temp, then classify the result.

## Source formats

- **Source archives** (`.tar.gz`, `.tar.xz`): see table above.
- **`.dsc`**: Debian source control. Lists sibling files (`.orig.tar.*`, `.debian.tar.*`) with checksums. Verify checksums, then `dpkg-source -x file.dsc`, `apt build-dep` (or `mk-build-deps`) for build dependencies, `dpkg-buildpackage -us -uc -b`, install the produced `.deb`. Debian family only.
- **`.orig.tar.gz` + `.debian.tar.xz`**: only meaningful together with the `.dsc`. Warn if a bare `.orig.tar.gz` is dropped.
- **git repositories**: `git clone --depth 1`. Support `https://…`, `git@…`, `.git` suffix. Then classify the tree like a source archive.

Build recipes (default prefix `~/.local`, no root):

| Detect | Build and install | Uninstall |
|---|---|---|
| autotools (`configure` or `configure.ac`) | `./configure --prefix=$HOME/.local && make -j && make install` | traced file list (`make uninstall` is unreliable) |
| CMake | `cmake -B build -DCMAKE_INSTALL_PREFIX=$HOME/.local && cmake --build build && cmake --install build` | traced file list (`install_manifest.txt` helps) |
| Meson | `meson setup build --prefix=$HOME/.local && ninja -C build install` | traced file list |
| Cargo | `cargo install --path . --root $HOME/.local` | `cargo uninstall` |
| Go | `go install ./...` with `GOBIN=$HOME/.local/bin` | delete traced files |
| Python | `pipx install .` or `pip install --user .` | `pipx uninstall` / `pip uninstall` |
| Node | `npm install -g --prefix $HOME/.local` | `npm uninstall -g` |
| `PKGBUILD` (Arch) | `makepkg -si` | `pacman -R` |
| `debian/` (Debian family) | `dpkg-buildpackage -us -uc -b` then `apt install ./../*.deb` | `apt remove` |
| `*.spec` (RPM family) | `rpmbuild -ba` **(unverified details)** | `dnf remove` |

Build dependencies are the user's biggest problem. v1 prints the missing tool (`gcc`, `make`, `cmake`) and the distro-specific install command (e.g. `apt install build-essential`).

## curl / wget input

Accept a pasted command such as `curl -fsSL https://x.sh | sh`, `wget -O app.AppImage https://…`, `sudo bash -c "$(curl …)"`. Parse with shell-word splitting, pull out the URL, flag `pipe_to_shell`. Never execute the pasted text. Download ourselves, hash, classify like any local file.
