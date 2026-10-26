# Updates

Unpack updates itself from the GitHub releases of `aiyu-ayaan/Unpack` (`app/src-tauri/src/updates.rs`). Modelled on Domus.

## Channels
Tags: `0.2.0-alpha.1`, `0.2.0-beta.1`, `0.2.0` (a leading `v` is fine). A stable release sorts above all pre-releases of the same version.

| Channel | Sees |
|---|---|
| stable | stable |
| beta | beta + stable |
| alpha | alpha + beta + stable |

The default channel is the one the installed build belongs to (an alpha build starts on alpha). Settings can change it; it is stored in `~/.local/share/unpack/settings.json` with the version the user skipped with *Later*.

## Release assets the updater looks for
| Install type | Asset name ends with |
|---|---|
| AppImage (`$APPIMAGE` set) | `_amd64.AppImage` / `_aarch64.AppImage` |
| dpkg system | `_amd64.deb` / `_arm64.deb` |
| rpm system (no dpkg) | `.x86_64.rpm` / `.aarch64.rpm` |

Releases without a matching asset are skipped. These are the default names `tauri build` writes. How releases are made: [08-release.md](08-release.md).

## Flow
1. At start the app asks `update_check`; the answer is kept in the backend. A found update opens a dialog (notes, *Later*, *Update*). *Later* is remembered for that version. Settings shows the same state.
2. `update_install` takes the kept offer (the page never sends a URL). Only `https://github.com/aiyu-ayaan/Unpack/releases/download/` URLs are accepted.
3. AppImage: download to `<file>.part`, check the `\x7fELF` header, rename over the running file, start it, quit. Package: save to the Downloads folder and `xdg-open` it (the desktop's software installer asks for the password).
4. Progress is emitted as `update-progress`.

Debug builds never offer updates.

## Limits
- No signature check: trust is GitHub's HTTPS and the repository. Add checksums or signatures to releases before relying on this widely.
- Unauthenticated GitHub API: 60 requests per hour per IP; one check per start.
- Not yet run against a real release **(unverified)**.
