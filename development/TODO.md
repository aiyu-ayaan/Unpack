# Unpack — master TODO

Legend: `[x]` done, `[ ]` open, `[~]` partly done. Update this file in the same commit as the work.

## M0 — Research and docs
- [x] Survey existing tools (bauh, Gear Lever, AppImageLauncher, AM, PackageKit, alien, distrobox…)
- [x] Format reference for all requested formats (`docs/02-formats.md`)
- [x] Distro / package-manager matrix, immutable distros (`docs/03-distro-compat.md`)
- [x] Architecture, schema, commands (`docs/04-architecture.md`)
- [x] Security model (`docs/05-security.md`)
- [x] Root `CLAUDE.md`

## M1 — Scaffold
- [x] Tauri 2 + Vite + React + TS + Tailwind v4 project in `app/`
- [x] App icon, CSP, minimal capabilities
- [x] `cargo check` and `pnpm build` green

## M2 — Backend core (Rust)
- [x] `system.rs`: os-release parse, package managers, runtimes, helpers, arch, immutable flag
- [x] `sniff.rs`: magic-byte classification for every format in `docs/02`
- [x] `input.rs`: file path / URL / git URL / curl / wget parsing (incl. `| sh`, `sudo`, `-o`)
- [x] `fetch.rs`: HTTPS download to private temp, size cap, SHA-256
- [x] `archive.rs`: list members, path-traversal + size checks, extract (tar family, zip, 7z, bare compressed)
- [x] `plan.rs`: kind × system → steps (native, bootstrap, unsupported with advice)
- [x] `runner.rs`: run steps, merge root steps into one `pkexec`, stream logs
- [x] pkexec failure handling: cancelled (126), not authorized / no polkit agent (127) with `sudo` fallback, `DEBIAN_FRONTEND` inside root script
- [ ] Verify pkexec flows on a desktop without a polkit agent **(unverified)**
- [x] `trace.rs`: snapshot diff of `~/.local`, `/opt`, `/usr/local` for scripts and builds
- [x] `store.rs`: SQLite schema, migrations via `user_version`
- [x] `library.rs`: scan apt/dnf/zypper/pacman/apk/snap/flatpak installed lists
- [x] Unit tests for all of the above

## M3 — UI
- [x] App shell: own draggable titlebar, activity bar, tab strip, status bar (distro, manager, running task)
- [x] VS Code style theme (Dark/Light Modern greys, green accent), OS default and toggle, reduced-motion support
- [ ] Verify undecorated window resizing from edges on GNOME/KDE/X11/Wayland **(unverified)**
- [x] Progress bars: analyze/scan/remove (indeterminate), install (per-step from runner `== label ==` markers)
- [x] Install view: drop zone, text input, plan review, live log, success/failure
- [x] Library view: tracked + system packages, search, uninstall with confirm
- [x] System view: detected distro, managers, missing helpers
- [x] History view
- [x] Empty/error states in plain language (target: first-time Linux user)
- [x] Brand: tray logo, wordmark, lockups, app icons, animated README logo (`docs/06-brand.md`)
- [x] Animated start-up splash, logo in titlebar and favicon
- [x] Removed the single-tab strip; layout fills the window
- [x] Settings view: updates (channel, check, download), about, developer, credits
- [x] Self-update (`updates.rs`): GitHub releases, channels, AppImage self-replace, package download + `xdg-open`, offer at start
- [x] Release workflow: marker commits (`!alpha` ... `!patch`) open a release PR, merging builds AppImage + deb + rpm, tags and publishes (`docs/08-release.md`)
- [x] `scripts/install-linux.sh`: curl installer (AppImage in `~/.local/share/unpack`, menu entry, icon, `unpack` command, channel fallback stable > beta > alpha, `--uninstall`); tested with a local AppImage in a throwaway HOME
- [x] Alpine / musl: `scripts/build-musl.sh` (also `--from-source` in the installer), `build-musl` job in the release workflow; built and run on Alpine 3.24 in Docker
- [x] Replayed the release pipeline locally (plan, version bump, changelog, bundles, name checks, notes); found that rpm rejects pre-release versions, so rpm is stable-only
- [x] Launch film `Unpack.mp4` (vertical, 2K) and README walkthrough GIF `res/unpack.gif`
- [x] **Bug (fixed):** Electron AppImages (AFFiNE) did nothing from the menu: "The SUID sandbox helper binary was found, but is not configured correctly" because AppArmor restricts user namespaces; the menu entry now adds `--no-sandbox` with a plan warning. Not yet seen working on a clean install **(unverified)**
- [ ] Allow Actions to create PRs in the repo settings, push, and run the first `!alpha` release; fix what the first real run shows **(unverified)**
- [ ] Verify the AppImage self-replace and the `.deb` hand-off on a real system **(unverified)**
- [ ] Check icon in a real GNOME/KDE launcher and the built `.deb` **(unverified)**

## M4 — Format completeness
Status: all code paths below exist and are unit-tested for planning/parsing. `[~]` = implemented, not yet run end to end on a real distro (see M5).
- [~] .deb  - [~] .rpm  - [~] .pkg.tar.zst  - [~] .apk (Alpine)  - [~] Android .apk rejection
- [~] .snap  - [~] .flatpak / .flatpakref / .flatpakrepo
- [~] .AppImage (copy, chmod, .desktop, icon, FUSE2 check, uninstall)
- [~] .run  - [~] .bin  - [~] .sh
- [~] Archives: tar.gz/tgz, tar.xz/txz, tar.bz2/tbz2, tar.zst, tar, zip, 7z, gz, xz, bz2, zst
- [~] Portable-app archive install (`~/.local/opt`, symlink, .desktop)
- [~] Source: source archive build recipes (autotools, CMake, Meson, Cargo, Go, Python, Node)
- [~] Source: `.dsc` (+ checksum verify, build-dep, dpkg-buildpackage)
- [ ] Source: `.orig.tar.*` / `.debian.tar.*` pairing warning (bare file currently shows 'unrecognised')
- [~] Source: git URL (clone, detect, build; PKGBUILD / debian/ / .spec preferred)
- [~] curl / wget command input end to end

## M5 — Distro coverage
- [ ] Debian/Ubuntu family tested (VM/container)
- [ ] Fedora/RHEL family tested
- [ ] openSUSE tested
- [ ] Arch/Manjaro tested
- [ ] Alpine tested
- [ ] Immutable: Silverblue (rpm-ostree), SteamOS, MicroOS plans
- [ ] NixOS notes (`appimage-run`, `nix-ld`)
- [ ] Void / Gentoo: universal formats + source plan only
- [ ] glibc vs musl warning

## M6 — Packaging Unpack itself
- [~] `tauri build` → .deb (built OK, 5 MiB); .rpm and AppImage not tried
- [ ] Flatpak manifest (host-spawn caveat) — evaluate feasibility
- [ ] AUR PKGBUILD, COPR/PPA notes
- [ ] CI (GitHub Actions) building on Ubuntu LTS

## M7 — Extra features (ideas)
- [ ] Update checks: AppImage zsync info, GitHub releases feed, `apt/dnf list --upgradable`
- [ ] "Fix my install": missing libfuse2, no flatpak/snapd, missing exec bit
- [ ] Staged install → build native package (fpm-like) so PM owns uninstall
- [ ] Sandbox untrusted scripts (bubblewrap / firejail preview)
- [ ] Container fallback for foreign formats (distrobox export)
- [ ] Polkit action + narrow privileged helper instead of generic `pkexec`
- [ ] Export / import tracked list (move to new machine)
- [ ] Drop-from-browser integration: `.desktop` MIME handler for `.deb/.rpm/.AppImage…` ("Open with Unpack")
- [ ] Leftover cleaner after uninstall (`~/.config/<app>`, caches) with preview
- [ ] i18n
- [ ] Disk usage per app

## Verification (items marked unverified in docs)
- [ ] snap `--dangerous` / `--classic` flow
- [ ] `.flatpakrepo` add syntax
- [ ] makeself flags on a real `.run`
- [ ] Alpine apk v3 container detection
- [ ] FUSE2 package names on Fedora/openSUSE/Alpine
- [ ] `unsquashfs -cat` for snap.yaml
- [ ] Tauri capability + CSP config on current Tauri 2.x

## Known gaps found while building (add here)
- [ ] No end-to-end test on a real package install (needs VM/container matrix; unit tests cover plan building only)
- [ ] `analyze` on a huge directory/archive blocks a worker thread with no cancel button
- [ ] Download has no progress events (install bar is per step only)
- [ ] Work folders of abandoned plans stay until next startup cleanup
- [ ] Trace snapshot walks all of `~/.local` (slow if it holds a lot of data)
- [ ] Uninstall of leftover config/cache dirs
- [ ] Bare `.orig.tar.gz` / `.debian.tar.xz` handling
- [ ] System-wide uninstall list shows libraries on dnf/zypper (no "user-installed" filter for zypper)
- [x] **Bug (fixed):** the AppImage passes its own `LD_LIBRARY_PATH` (bundled `liblzma`, `libzstd`, `libbz2`, …) to every command it runs. `dpkg-deb` then fails (`version 'XZ_5.4' not found`), `capture()` errors are swallowed by `unwrap_or_default()`, and a `.deb` is tracked under its file name instead of its package name with no version. Found installing `Ghost-Downloader-v4.4.5-Linux-x86_64.deb`: stored ident `Ghost-Downloader-v4.4.5-Linux-x86_64` instead of `ghost-downloader`, so Library > Remove fails (`apt` exit 100). Fix: drop `LD_LIBRARY_PATH` (and `APPDIR`-derived variables) in `util::capture` and the runner; do not default silently when `dpkg-deb`/`rpm` fail, refuse or warn instead
- [x] **Bug (fixed, checks apt/rpm/pacman/apk installs):** an install can report "Installed" for an app that cannot start. Ghost Downloader's `.deb` declares no `Depends`, so `apt` did not pull `libxcb-cursor0`; the app dies with `Could not load the Qt platform plugin "xcb"` and the menu entry does nothing. Fix: after a native install, run `ldd` over the package's ELF files (`dpkg -L` / `rpm -ql`) and show missing libraries (`libxcb-cursor.so.0 => not found`) with the package that provides them and a one-click install ("Fix my install", M7)
