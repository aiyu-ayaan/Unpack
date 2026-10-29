## v0.0.1-alpha.4 (2026-10-29)

- !alpha : Store pages and categories, update holds and rollback, background checks, dependency preview, signature checks, Void and Gentoo, single window
- docs(readme): acknowledgements; Settings lists new crates and the services Unpack uses
- docs(readme): store pages and sources, update holds and rollback, checks before install
- feat(library): show the disk space each app takes
- feat(install): warn when the app is already installed in another format
- feat(install): preview the packages an install pulls in or removes
- feat(install): check a file against a published SHA-256, signature or checksum list
- docs(architecture): replaced_by column in the apps schema
- feat(library): undo an update Unpack made
- feat(updates): daily background check, notification, tray badge, opt-in auto-update
- feat(updates): skip one version or never update an app
- feat(store): manage Flatpak sources besides Flathub
- feat(store): app pages, categories, and snaps and native apps on the front page
- feat(gui): keep one window and hand a second Open with Unpack to it
- feat(system): drive xbps on Void and emerge on Gentoo
- feat(package): stage source builds into .rpm and Arch packages too
- feat(updates): read plain zsync|<url> AppImage update info
- feat(analyze): pair .orig.tar and .debian.tar files with their .dsc
- feat(cli): remove packages the system reports, not only tracked apps
- fix(ui): Install page header says Update or Fix for those plans
- docs(readme): store, updates, open-with, fix, sandbox, containers, transfer, helper
- feat(helper): polkit action and an allowlisted root helper for package-manager steps
- feat(package): build source into a .deb from a DESTDIR stage so apt can remove it
- feat(container): install packages for other distros inside a distrobox container
- feat(distro): layer packages on read-only bases, NixOS AppImage launcher, glibc/musl loader warnings
- feat(sandbox): try installer scripts in bubblewrap before running them
- feat(transfer): export installed apps and install them again on another computer
- feat(open-with): open package files in Unpack from the file manager or browser
- feat(fix): repair tracked apps: missing runtime, exec bit, FUSE 2 and missing libraries
- feat(updates): list upgradable packages and newer GitHub releases, update through a reviewed plan
- feat(store): browse Flathub and search every package source, install through a reviewed plan

## v0.0.1-alpha.3 (2026-10-27)

- !alpha : Command line build, cancel and download progress, leftover cleanup, distro tests, faster CI
- ci: cache apt downloads, skip Recommends, retry stalled mirrors, set job timeouts
- test(distro): install util-linux for su on Fedora, show the user test's output when it fails
- docs(todo): distro matrix passes in Docker; CLI, CI, cancel, progress and leftovers done
- test(distro): run the no-admin-prompt user test where sudo exists too
- docs: command line, CLI download, distro tests in Docker, cancel and leftovers
- feat(release): publish the static CLI and install it with install-linux.sh --cli
- style(util): keep the test lock above the test module (clippy)
- ci: core tests without WebKit, desktop build, static CLI and the seven-distro matrix
- test(distro): build the static CLI locally and test it in Docker on seven distros
- feat(ui): cancel and download progress on Check, offer to remove leftovers after uninstall
- feat(cli): unpack command line on the same core as the window
- feat(library): hide removed apps from the installed list
- docs: dev flavor, and log the Library remove failure on records with a file name
- feat(flavor): run debug builds as a dev flavor with separate data, cache and identifier
- fix(uninstall): find the real apt package name for records that stored the file name

## v0.0.1-alpha.2 (2026-10-26)

- !alpha : Fix installs that do not open: AppImage library paths, Electron sandbox, missing libraries
- docs: AppImage library path, Electron sandbox, missing-library check; mark the three bugs fixed
- feat(install): report missing shared libraries after a native install
- fix(appimage): start Electron AppImages that the system sandbox blocks; refuse a .deb it cannot read
- fix(runner): run host tools without the AppImage's bundled library paths
- docs(TODO): log the AppImage library-path bug and the missing-library check found with Ghost Downloader
- docs(README): drop the status badge and state the MIT license
- docs: add the MIT license

## v0.0.1-alpha.1 (2026-10-26)

- !alpha : First public build
- docs(README): add the launch film and a walkthrough GIF
- docs: Alpine install and what the local pipeline run found
- ci(release): build an Alpine bundle; rpm only for stable releases because rpm versions cannot contain a dash
- feat(install): musl (Alpine) support, a prebuilt tarball or a source build via scripts/build-musl.sh
- feat(install): pick the AppImage for this CPU (x86_64 or aarch64)
- ci(release): build Linux bundles for aarch64 as well as x86_64
- feat(install): run without libfuse2, refuse musl clearly, wrapper command when needed
- feat(install): curl installer for the AppImage with menu entry, icon and channel fallback
- fix(fetch): name downloads from Content-Disposition or the requested URL, not the redirect target
- feat(ui): paint the splash from index.html so it shows before React loads
- fix(window): dark native window background until the page paints
- fix(ui): apply saved theme before first paint to avoid white flash
- docs: release markers, workflow and setup
- feat(release): marker commits open a release PR; merging builds and publishes bundles
- docs: updates, settings and the new commands
- feat(ui): Settings with updates, about and developer; offer updates at start
- feat(update): check GitHub releases on a channel and self-install (AppImage) or open the package
- refactor(ui): drop the single-tab strip, the titlebar already names the view
- docs: add brand doc, animated logo in README, TODO and index
- feat(brand): looping animated lockup for the README
- feat(ui): animated logo, start-up splash, logo in titlebar and favicon
- feat(brand): replace app icons with the Unpack tray icon
- feat(brand): add Unpack logo: tray symbol, wordmark, lockups, app icon source
- docs(readme): describe the VS Code style UI
- feat(ui): plain-language views in the new style
- feat(ui): VS Code style shell and theme
- feat(ui): own draggable titlebar with window controls
- build: pnpm dev runs the desktop app; pnpm dev:web for the frontend alone
- build: make repo root a pnpm workspace so commands run without cd app
- docs: add README
- chore: vendor UI design skills for Claude and agents
- chore: add codegraph MCP config and Claude project settings
- feat(ui): editor-style shell with black/green terminal theme
- fix(runner): explain pkexec cancel/denied exits and keep DEBIAN_FRONTEND for root steps
- docs: as-built architecture, TODO status; fix bundle category
- feat(ui): install, library, system and history views with dark glass theme
- feat(backend): plan builder, runner, SQLite store, library scan, Tauri commands
- feat: scaffold Tauri app and core detection modules (system, sniff, input, fetch, archive, trace)
- docs: research, format and distro reference, architecture, TODO, CLAUDE.md

