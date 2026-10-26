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
- [ ] Tauri 2 + Vite + React + TS + Tailwind v4 project in `app/`
- [ ] App icon, CSP, minimal capabilities
- [ ] `cargo check` and `pnpm build` green

## M2 — Backend core (Rust)
- [ ] `system.rs`: os-release parse, package managers, runtimes, helpers, arch, immutable flag
- [ ] `sniff.rs`: magic-byte classification for every format in `docs/02`
- [ ] `input.rs`: file path / URL / git URL / curl / wget parsing (incl. `| sh`, `sudo`, `-o`)
- [ ] `fetch.rs`: HTTPS download to private temp, size cap, SHA-256
- [ ] `archive.rs`: list members, path-traversal + size checks, extract (tar family, zip, 7z, bare compressed)
- [ ] `plan.rs`: kind × system → steps (native, bootstrap, unsupported with advice)
- [ ] `runner.rs`: run steps, merge root steps into one `pkexec`, stream logs
- [ ] `trace.rs`: snapshot diff of `~/.local`, `/opt`, `/usr/local` for scripts and builds
- [ ] `store.rs`: SQLite schema, migrations via `user_version`
- [ ] `library.rs`: scan apt/dnf/zypper/pacman/apk/snap/flatpak installed lists
- [ ] Unit tests for all of the above

## M3 — UI
- [ ] App shell, sidebar, dark glass theme
- [ ] Install view: drop zone, text input, plan review, live log, success/failure
- [ ] Library view: tracked + system packages, search, uninstall with confirm
- [ ] System view: detected distro, managers, missing helpers
- [ ] History view
- [ ] Empty/error states in plain language (target: first-time Linux user)

## M4 — Format completeness (per-format install + uninstall working end to end)
- [ ] .deb  - [ ] .rpm  - [ ] .pkg.tar.zst  - [ ] .apk (Alpine)  - [ ] Android .apk rejection
- [ ] .snap  - [ ] .flatpak / .flatpakref / .flatpakrepo
- [ ] .AppImage (copy, chmod, .desktop, icon, FUSE2 check, uninstall)
- [ ] .run  - [ ] .bin  - [ ] .sh
- [ ] Archives: tar.gz/tgz, tar.xz/txz, tar.bz2/tbz2, tar.zst, tar, zip, 7z, gz, xz, bz2, zst
- [ ] Portable-app archive install (`~/.local/opt`, symlink, .desktop)
- [ ] Source: source archive build recipes (autotools, CMake, Meson, Cargo, Go, Python, Node)
- [ ] Source: `.dsc` (+ checksum verify, build-dep, dpkg-buildpackage)
- [ ] Source: `.orig.tar.*` / `.debian.tar.*` pairing warning
- [ ] Source: git URL (clone, detect, build; PKGBUILD / debian/ / .spec preferred)
- [ ] curl / wget command input end to end

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
- [ ] `tauri build` → .deb, .rpm, AppImage
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
