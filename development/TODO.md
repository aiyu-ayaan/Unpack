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
- [x] **Bug (fixed):** records saved before the .deb name fix still hold the file name, so Library > Remove failed with `apt` exit 100 on prod. Remove now reads the real name from the saved .deb, and says plainly when apt has no such package **(unverified on a fresh alpha.1 record)**
- [x] Library > Installed with Unpack lists installed apps only; removed ones stay in History
- [x] App flavor: debug builds use `~/.local/share/unpack-dev` and `~/.cache/unpack-dev`; `pnpm dev` uses `tauri.dev.conf.json` (own identifier, "Unpack (dev)" title), so a dev run never touches the installed app
- [x] **Bug (fixed):** Electron AppImages (AFFiNE) did nothing from the menu: "The SUID sandbox helper binary was found, but is not configured correctly" because AppArmor restricts user namespaces; the menu entry now adds `--no-sandbox` with a plan warning. Not yet seen working on a clean install **(unverified)**
- [x] Allow Actions to create PRs in the repo settings, push, and run the first `!alpha` release (v0.0.1-alpha.1 and alpha.2 published)
- [ ] Verify the AppImage self-replace and the `.deb` hand-off on a real system **(unverified)**
- [ ] Check icon in a real GNOME/KDE launcher and the built `.deb` **(unverified)**

## Command line
- [x] `unpack <command>` in the same binary (`cli.rs`): check, install, list, remove (`--purge`), scan, history, system, version; `--yes`, `--json`, `--root`, `--allow-http`; exit codes 0/1/2/3
- [x] `gui` cargo feature: `--no-default-features` builds the CLI without Tauri/WebKit; `ops.rs` shares install/uninstall/record between window and CLI
- [x] Root steps use `sudo` in a terminal, `pkexec` otherwise
- [x] `scripts/build-cli.sh`: static musl binary (`unpack-cli_<version>_<arch>`), published by the release workflow, installed with `install-linux.sh --cli`
- [ ] Remove packages the system reports (Library's "Everything on this computer") from the CLI

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
Containers: `scripts/test-distros.sh` (CLI build, as root plus a normal user without an admin prompt; desktop parts such as menus and the window are not covered). Native package install/remove with leftovers purge, traced script, portable archive, https download, refusals: all pass.
- [x] Debian 12 and Ubuntu 24.04 tested (containers)
- [x] Fedora 42 and Rocky Linux 9 tested (containers)
- [x] openSUSE Tumbleweed tested (container)
- [x] Arch tested (container)
- [x] Alpine 3.24 tested (container)
- [ ] Same flows on a real desktop (polkit window, menu entries, AppImage, Flatpak, Snap) **(unverified)**
- [x] Immutable: Silverblue (rpm-ostree layering with `--apply-live`), MicroOS (transactional-update), SteamOS (refuse system writes, advise Flatpak/AppImage/distrobox); native upgrades not listed there
- [ ] Verify rpm-ostree and transactional-update flows on Silverblue and Aeon **(unverified)**
- [x] NixOS: AppImage menu entries through `appimage-run`, `nix-ld` note for foreign programs (detected by the missing glibc loader, not the distro name)
- [ ] Void / Gentoo: universal formats + source plan only
- [x] glibc vs musl warning from the program's `PT_INTERP`

## M6 — Packaging Unpack itself
- [~] `tauri build` → .deb (built OK, 5 MiB); .rpm and AppImage not tried
- [ ] Flatpak manifest (host-spawn caveat) — evaluate feasibility
- [ ] AUR PKGBUILD, COPR/PPA notes
- [x] CI (GitHub Actions, `.github/workflows/ci.yml`): core tests and clippy without WebKit, desktop build, static CLI, seven-distro matrix **(unverified: not yet run on GitHub)**

## Store and updates
- [x] Store view: Flathub popular apps with icons; search across Flathub, the Snap Store and the native repositories; Install opens the reviewed plan (`catalog.rs`, `plan::plan_repo`); `unpack search` / `unpack get`

## M7 — Extra features (ideas)
- [x] Update checks: every manager's upgradable list, Flatpak, snap, GitHub releases of tracked apps, AppImage `gh-releases-zsync` info (Updates view, `unpack updates` / `unpack upgrade`)
- [ ] Plain `zsync|<url>` AppImage update info
- [x] "Fix my install": missing libfuse2, no flatpak/snapd, missing exec bit, missing shared libraries with the providing package per manager (Library > Fix, `unpack fix`)
- [ ] Verify library providers: `dnf repoquery --whatprovides`, `zypper search --provides`, `pacman -F` (needs `pacman -Fy`), `apk search so:` **(unverified)**
- [x] Staged install → build native package so PM owns uninstall: `.deb` from `DESTDIR` staging on apt systems (Install > Build a system package instead, `--package`)
- [ ] Staged builds to `.rpm` (rpmbuild with a generated spec) and Arch (generated PKGBUILD over the stage)
- [x] Sandbox untrusted scripts: bubblewrap preview for single-script plans (Install > Try it in a sandbox first, `unpack check --sandbox`)
- [ ] Verify the bwrap preview on Ubuntu 24.04 (AppArmor userns restriction), Fedora and Arch desktops **(unverified: the dev shell blocks namespaces)**
- [x] Container fallback for foreign formats: distrobox container per family, install inside, `distrobox-export` apps or commands, `Uninstall::Container`
- [ ] Verify the distrobox flow end to end with podman on a desktop **(unverified)**
- [x] Polkit action + narrow privileged helper instead of generic `pkexec`: `dev.unpack.app.helper` shipped in the .deb/.rpm, `unpack helper` with a per-manager allowlist; other root steps keep `pkexec sh -c`
- [ ] Verify the helper prompt and `auth_admin_keep` on GNOME and KDE with the installed .deb **(unverified)**
- [x] Export / import tracked list (move to new machine): Settings > Move to another computer, `unpack export` / `unpack import`
- [x] Drop-from-browser integration: `.desktop` MIME handler for `.deb/.rpm/.AppImage…` ("Open with Unpack", Settings > Files; `unpack open <file>`)
- [ ] Verify "Open with Unpack" in GNOME Files, Dolphin and a browser's download list **(unverified)**; a second open starts a second window (single-instance plugin if needed)
- [x] Leftover cleaner after uninstall (`~/.config/<app>`, caches) with preview (Library offer, `unpack remove --purge`)
- [ ] i18n
- [ ] Disk usage per app

## Verification (items marked unverified in docs)
- [ ] snap `--dangerous` / `--classic` flow
- [ ] `.flatpakrepo` add syntax
- [ ] makeself flags on a real `.run`
- [x] Alpine apk v3 container detection (apk-tools 3.0.8 on Alpine 3.24)
- [ ] FUSE2 package names on Fedora/openSUSE/Alpine
- [ ] `unsquashfs -cat` for snap.yaml
- [ ] Tauri capability + CSP config on current Tauri 2.x

## Known gaps found while building (add here)
- [x] No end-to-end test on a real package install (now `scripts/test-distros.sh`, seven distros in Docker)
- [~] `analyze` cancel: the window gets its answer at once; the worker stops at the next download chunk or git poll, but a huge local archive listing still runs to its end in the background
- [x] Download has no progress events (`analyze-progress`, MB shown under Check)
- [~] Work folders: removed after a failed/cancelled analyze and a successful install; a failed install keeps its folder until the next startup cleanup
- [ ] Trace snapshot walks all of `~/.local` (slow if it holds a lot of data)
- [x] Uninstall of leftover config/cache dirs (matched by app name and flatpak id only)
- [ ] Bare `.orig.tar.gz` / `.debian.tar.xz` handling
- [ ] System-wide uninstall list shows libraries on dnf/zypper (no "user-installed" filter for zypper)
- [x] **Bug (fixed):** the AppImage passes its own `LD_LIBRARY_PATH` (bundled `liblzma`, `libzstd`, `libbz2`, …) to every command it runs. `dpkg-deb` then fails (`version 'XZ_5.4' not found`), `capture()` errors are swallowed by `unwrap_or_default()`, and a `.deb` is tracked under its file name instead of its package name with no version. Found installing `Ghost-Downloader-v4.4.5-Linux-x86_64.deb`: stored ident `Ghost-Downloader-v4.4.5-Linux-x86_64` instead of `ghost-downloader`, so Library > Remove fails (`apt` exit 100). Fix: drop `LD_LIBRARY_PATH` (and `APPDIR`-derived variables) in `util::capture` and the runner; do not default silently when `dpkg-deb`/`rpm` fail, refuse or warn instead
- [x] **Bug (fixed, checks apt/rpm/pacman/apk installs):** an install can report "Installed" for an app that cannot start. Ghost Downloader's `.deb` declares no `Depends`, so `apt` did not pull `libxcb-cursor0`; the app dies with `Could not load the Qt platform plugin "xcb"` and the menu entry does nothing. Fix: after a native install, run `ldd` over the package's ELF files (`dpkg -L` / `rpm -ql`) and show missing libraries (`libxcb-cursor.so.0 => not found`) with the package that provides them and a one-click install ("Fix my install", M7)
