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

