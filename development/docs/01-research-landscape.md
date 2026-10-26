# 01 — Research: existing tools and the gap

Goal of Unpack: a person who just moved to Linux drops a file, a link, or a pasted `curl`/`wget` command into one window, and the app works out what it is, how to install it on *their* distro, installs it, remembers it, and can remove it later.

Research method: web searches (sources listed in [sources.md](sources.md)) plus prior knowledge of the tools. Claims marked **(unverified)** came from memory and were not confirmed by a fetched source; verify before relying on them in code.

## 1. What already exists

| Tool | Scope | What it does well | Gap for our users |
|---|---|---|---|
| **GNOME Software / KDE Discover** | Repos, Flatpak, Snap (via plugins), PackageKit | Polished store UI | Cannot take an arbitrary `.deb`/`.rpm`/tarball/script from a website; weak local-file story; no "what did this script do" tracking |
| **GDebi / Eddy / `apt install ./x.deb`** | One format (`.deb`) | Resolves dependencies for a local `.deb` | One format; no uninstall tracking beyond the package manager |
| **bauh** | AppImage, Arch repos/AUR, Flatpak, Snap, web apps | Closest to a "universal" GUI front end (per its listing) | Manages *known sources*; not a "drop anything" installer; Python/Qt UI |
| **Gear Lever** | AppImage only | Drag-and-drop, desktop integration, update management, opens AppImages from the file manager; GPL-3.0, on Flathub | AppImage only |
| **AppImageLauncher** | AppImage integration | Auto-creates `.desktop` entry, icons, MIME associations | Reported as dated by some users; AppImage only |
| **AM (appman)** | AppImage and other portable formats | AUR-style database of portable apps, CLI | CLI only; curated database, not arbitrary input |
| **Warehouse** (Flatpak manager) **(unverified)** | Flatpak data/remotes | Manage Flatpak installs | Flatpak only |
| **PackageKit / `pkcon`** | Cross-distro D-Bus API; `install-local` command exists on openSUSE builds | Abstracts apt/dnf/zypper/pacman backends | Backend quality varies; older versions had an authorization bug in `InstallFiles`/`GetDetailsLocal` (fixed in 1.2.3+). Not installed everywhere (Alpine, minimal Arch) |
| **alien** **(unverified)** | Converts deb/rpm/tgz | Cross-format fallback | Conversions are lossy; scripts often break |
| **debtap** (AUR) **(unverified)** | deb → pacman package | Lets Arch users install `.deb` content | Arch only, AUR dependency |
| **distrobox / toolbox** **(unverified)** | Run another distro in a container, export apps | Right answer for "this only ships a `.deb` and I'm on Fedora" | Needs container runtime; heavy for novices |
| **eget, bin, Stew, soar** **(unverified)** | Install release binaries from GitHub URLs | URL → binary in `~/.local/bin` | CLI; GitHub-release centric |
| **topgrade** **(unverified)** | Updates everything | Unified update | Not an installer |
| **Nix / Homebrew on Linux** | Own package universe | Huge catalogs | Different mental model; not a file-opener |

## 2. The gap Unpack fills

1. **Input-first, not catalog-first.** Every other tool starts from a list of known apps. Unpack starts from "I have *this thing*".
2. **One plan, any distro.** Detect the distro and package manager, then choose native install, universal runtime (Flatpak/Snap/AppImage), conversion, or a clearly-labelled fallback.
3. **Explain before acting.** Show what the artifact is, what will run, and whether it needs root. Show script contents for `.sh`/`.run`/`curl | sh`.
4. **Track everything, including scripts.** Package managers already track their own installs. Scripts, tarballs and source builds are untracked by anyone. Unpack records file-level traces so they can be removed.
5. **Fix the first-run papercuts** that hit new users: missing `libfuse2` for AppImages, no `flatpak`/`snapd` installed, wrong architecture, Android `.apk` mistaken for Alpine `.apk`, no executable bit.

## 3. Feature ideas worth taking from the research

- From Gear Lever / AppImageLauncher: drag-and-drop, `.desktop` + icon integration, update info (see [02](02-formats.md#appimage)).
- From bauh: one list view across package sources.
- From GDebi: show dependency resolution *before* install.
- From PackageKit: a possible optional backend; we avoid hard dependency on it.
- From distrobox: fallback for foreign formats (later milestone).
- From the `curl | bash` discussion: download first, hash, show, then run — never pipe straight (see [05-security.md](05-security.md)).
- From Android-style stores **(unverified)**: "watch a URL for new releases" (GitHub release feed) for update notices.

## 4. Reverse-engineering note

The request asked for research instead of reverse engineering this time. No binaries were decompiled. Behaviour of other tools is taken from their public documentation and descriptions only.
