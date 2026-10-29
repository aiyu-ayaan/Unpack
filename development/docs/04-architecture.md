# 04 — Architecture

| Layer | Technology |
|---|---|
| Backend | Rust (Tauri 2 commands) |
| UI | Tauri 2 webview + React + TypeScript + Tailwind CSS v4 |
| Storage | SQLite via `rusqlite` (bundled) |
| Linux integration | Shelling out to APT, DNF, Zypper, Pacman, apk, Flatpak, Snap CLIs. No PackageKit dependency. |

Repo layout:

```
Unpack/
  CLAUDE.md                 project guide for Claude
  development/
    TODO.md                 master task list
    docs/                   these docs
  app/                      the Tauri application
    package.json, vite.config.ts, index.html
    src/                    React UI
    src-tauri/              Rust backend
      src/
        main.rs             no arguments: the window (gui feature); `open <file>`: the window with that file; any other argument: cli::main
        lib.rs              module list; re-exports gui::run when the `gui` feature is on
        gui.rs              Tauri setup, shared state, command registration (feature `gui`)
        cli.rs              `unpack <command>`: check, install, list, remove, scan, history, system
        ops.rs              install/uninstall with their SQLite record, leftover folders; shared by gui and cli
        util.rs             which(), capture(), XDG dirs, display quoting
        analyze.rs          input string → Plan (download/clone/list, recursion into wrappers)
        system.rs           distro / package manager / helper detection
        sniff.rs            magic-byte file classification
        input.rs            user input → Source (file, url, git, curl command)
        fetch.rs            download + SHA-256
        archive.rs          list / safety-check / extract archives
        plan.rs             Source + System → Plan (steps, warnings, root flags)
        runner.rs           execute a Plan, stream logs, pkexec merge
        trace.rs            filesystem snapshot diff for untracked installers
        store.rs            SQLite schema + queries
        library.rs          scan system package managers for installed software
        upgrades.rs         what can be updated (each manager's upgradable list, newer GitHub releases of tracked apps, AppImage `.upd_info`) and the update plan
        fix.rs              "Fix my install": missing runtime (Flatpak, snapd, FUSE 2), lost exec bit, missing shared libraries
        libcheck.rs         `ldd` over a native package's files; which package provides a missing library, per manager
        mime.rs             "Open with Unpack": hidden `unpack-open.desktop` for package MIME types, set as default with `xdg-mime`
        catalog.rs          Store: Flathub popular/search (HTTPS API) plus native and snap search (local CLIs)
```

## Data flow

```
user input (drop / path / URL / curl cmd / git URL)
   │
   ▼
analyze(input)  ──► input::parse ─► fetch (if remote) ─► sniff ─► archive peek
   │                                                          │
   │                          system::detect ─────────────────┤
   ▼                                                          ▼
Plan { id, title, kind, warnings[], steps[], uninstall: UninstallSpec }   (kept in backend memory by id)
   │
   ▼  UI shows plan, user approves
execute(plan_id)  ──► runner: for each step group → spawn → stream "plan-log" events
   │                         └─ trace::snapshot before/after for unmanaged installers
   ▼
store: apps row (name, version, kind, manager, ident, files[], uninstall spec) + events
```

The UI never sends commands. See [05-security.md](05-security.md).

## Core types (Rust)

- `System { distro, family, arch, managers[], runtimes[], helpers[], immutable }`
- `Kind` — `Deb, Rpm, PacmanPkg, AlpineApk, AndroidApk, Snap, FlatpakBundle, FlatpakRef, FlatpakRepo, AppImage, RunInstaller, BinInstaller, Script, Archive(ArchiveFmt), Dsc, GitRepo, Unknown`
- `Step { label, program, args, needs_root, cwd?, env? }`
- `Plan { id, title, kind, name, version, source, sha256, warnings[], steps[], uninstall }`
- `UninstallSpec` — one of:
  - `Manager { manager, ident }` (apt/dnf/zypper/pacman/apk/snap/flatpak)
  - `Files { paths[] , dirs[] }` (AppImage, portable archives, traced scripts and builds)
  - `Manual { note }` (nothing safe can be derived)
- `Supported` result inside plan: `Native`, `NeedsBootstrap(steps)`, `Unsupported(reason, alternatives[])`.

## SQLite schema (v1)

```sql
CREATE TABLE apps (
  id            INTEGER PRIMARY KEY,
  name          TEXT NOT NULL,
  version       TEXT,
  kind          TEXT NOT NULL,       -- Kind as string
  manager       TEXT,                -- apt|dnf|zypper|pacman|apk|snap|flatpak|files|manual
  ident         TEXT,                -- package name / app-id
  source        TEXT,                -- original input (path, URL, git URL)
  sha256        TEXT,
  installed_at  INTEGER NOT NULL,    -- unix seconds
  status        TEXT NOT NULL,       -- installed|removed|failed
  uninstall     TEXT NOT NULL        -- JSON UninstallSpec
);
CREATE TABLE events (
  id      INTEGER PRIMARY KEY,
  app_id  INTEGER REFERENCES apps(id) ON DELETE CASCADE,
  ts      INTEGER NOT NULL,
  level   TEXT NOT NULL,             -- info|warn|error
  message TEXT NOT NULL
);
```

DB path: `$XDG_DATA_HOME/unpack/unpack.db` (default `~/.local/share/unpack/`). Debug builds use `unpack-dev` in place of `unpack` (data and cache), so running from source never touches the installed app. Schema version tracked with `PRAGMA user_version`.

## Tauri commands

| Command | Purpose |
|---|---|
| `detect_system()` | Return `System` |
| `analyze(input, allow_http)` | Parse, fetch/clone, sniff, build `Plan`, store it by id; emits `analyze-progress` while downloading |
| `store_popular()` / `store_search(query)` | Store entries `{source, id, name, summary, icon}`: Flathub's API, plus `apt-cache search`, `dnf repoquery`, `zypper search`, `pacman -Ss`, `apk search`, `snap find` |
| `store_plan(source, id)` | Build and keep a `Plan` for one entry (`plan::plan_repo`; the id is checked with `safe_ident`); run it with `execute` |
| `list_upgrades()` | Upgradable packages from apt/dnf/zypper/pacman/apk, Flatpak (user and system) and snap, plus tracked apps whose GitHub release has a newer file; Unpack's own records are marked with `app_id` |
| `upgrade_plan(picks)` | One `Plan` (kind `update`, records nothing) for picked `{manager, name, user}`; names checked with `safe_ident`, apt refreshes its lists first, Arch always does a full `-Syu` |
| `update_app_plan(app_id)` | Analyze the newer release file of a tracked app; on success the new record replaces the old one (old files removed, old row `updated`) |
| `fix_plan(app_id)` | Repair plan for a tracked app (kind `fix`, records nothing); unsupported with advice when nothing is wrong |
| `take_opened()` | The file `unpack open <file>` started the window with, once; the page checks it like a dropped file |
| `file_handler()` / `set_file_handler(on)` | Whether "Open with Unpack" is on; write or remove `~/.local/share/applications/unpack-open.desktop` (`Exec=<AppImage or binary> open %f`) and set or clear it as the default for `.deb`, `.rpm`, AppImage, Flatpak and snap types |
| `cancel_analyze()` | Stop the running analyze: the call returns `Cancelled` at once, the worker stops at its next check and removes its work folder |
| `execute(plan_id, as_root)` | Run the stored plan, emit `plan-log`, record app + event in SQLite |
| `list_apps()` | Tracked apps from SQLite |
| `history(app_id?)` | Event log |
| `scan_system()` | Packages the system managers report (read-only) |
| `uninstall_app(app_id)` | Run the stored `Uninstall` spec, mark row `removed` |
| `leftovers(app_id)` / `remove_leftovers(app_id)` | Settings/cache/data folders named after the app (`~/.config`, `~/.cache`, `~/.local/share`, `~/.local/state`, `~/.var/app`); removal recomputes the list, the page never sends paths |
| `uninstall_system(manager, ident)` | Remove a package Unpack did not install (name is validated) |
| `update_info()` / `update_check()` | Installed version, channel; look for a newer release (see [07-updates.md](07-updates.md)) |
| `update_install()` / `update_set_channel(c)` / `update_skip()` | Download the offer from the last check; remember channel; remember "Later" |
| `open_link(which)` | Open one of Unpack's own links (`repo`, `issues`, `developer`, `site`); the page never sends an address |

Events: `plan-log {plan_id, stream, line}` (`plan_id` is `"uninstall"` during removals), `plan-done <plan_id>`, `analyze-progress {done, total}` (bytes), `update-progress <0..1 | null>`.

## Command line

The same binary takes commands: `unpack install file.deb`, `unpack list`, `unpack remove 3 --purge` (`unpack --help` for all). It uses the same `analyze → Plan → ops::install` path and the same database as the window. Built with `--no-default-features` it has no Tauri or WebKitGTK at all; `scripts/build-cli.sh` builds that as one static musl binary (`unpack-cli_<version>_<arch>`), published with each release. In a terminal, root steps use `sudo` (it asks on the terminal) instead of a `pkexec` window; without a terminal nothing is asked and `--yes` is required.

## Implementation notes (as built)

- AppImages are copied to `~/Applications`; their own `.desktop` is rewritten (`Exec=` → our copy, `Icon=` → absolute path under `~/.local/share/unpack/icons`) and saved as `~/.local/share/applications/unpack-<name>.desktop`. Metadata is read with the AppImage's own `--appimage-extract <glob>`, which does not need FUSE **(unverified on every runtime)**. This executes the runtime from the file, as launching it would.
- Portable archives go to `~/.local/opt/<name>`, with a symlink in `~/.local/bin` and a `.desktop`.
- Scripts, `.run`, `.bin` and source builds are *traced*: snapshot of `~/.local`, `/opt`, `/usr/local` before and after; new files become the `Files` uninstall spec. Limits: files written elsewhere (`/usr/bin`, `/etc`) are not seen; modified existing files are not seen. The runner then reports `Manual` when nothing new was found.
- `Files` removal only touches `~/.local`, `~/Applications`, `~/Unpacked`, `/opt`, `/usr/local` (`runner::safe_to_remove`). Paths needing root go through one `pkexec rm -rf --`.
- Native package-recipe builds (`PKGBUILD`, `debian/`, `.dsc`) end in an `InstallBuilt` step: the produced package is planned and installed like a downloaded one, so the package manager owns removal.
- A downloaded app's record keeps the link as given in `source` (updates read the GitHub project from it); when redirects led elsewhere the plan shows and the install event records the final address.
- Updates of tracked apps: a `source` of `https://github.com/<owner>/<repo>/releases/download/<tag>/<file>` is compared with the project's latest release, and the replacement file is the one whose name is the old name with the new version, else the one with the same shape (digits ignored). AppImages that carry `gh-releases-zsync|owner|repo|tag|pattern` in their `.upd_info` ELF section are checked the same way. Plain `zsync|<url>` update info is not used yet.
- Plans keep a SHA-256 of the analysed file; `execute` re-hashes and aborts on mismatch.
- Work folders live in `~/.cache/unpack/work/<plan-id>`. A failed or cancelled analyze and a successful install remove theirs; anything left (a failed install, kept for a retry) is deleted at the next start once a day old.

## UI

Views: **Install** (drop zone + input; plan review; live log; also reviews plans other views hand it), **Updates** (count badge in the activity bar; apps installed with Unpack first, then the rest; Update all builds one plan), **Store** (Flathub's popular apps with icons, search across Flathub, the Snap Store and the distro's repositories, Install opens the plan in Install), **Library** (tracked and system-wide, search, Fix, uninstall), **System** (detected distro, managers, missing helpers with one-click bootstrap), **History** (events). VS Code style shell: our own draggable titlebar (native decorations off; minimize / maximize / close through the window API, allowed in `capabilities/default.json`), icon activity bar, tab strip with a 2px progress line, and a status bar with the distro and the running task. Tokens follow VS Code's Dark Modern / Light Modern with a green accent; they are CSS variables in `app/src/index.css` switched by `html[data-theme]` (OS preference by default, toggle saved in `localStorage`). Plain-language sans-serif text; monospace only for commands and the Output panel. Install progress is per step, derived from the runner's `== label ==` log markers. `lucide-react` icons.

## Testing

- Rust unit tests: `sniff` (byte fixtures), `input` (curl/wget/git parsing), `system` (os-release parsing), `plan` (kind × family matrix → expected commands), `archive` (path-traversal rejection), `trace` (diff), `ops` (leftovers), `cli` (flags, app lookup).
- `pnpm test` (`cargo test --no-default-features`, no WebKit needed) must pass before each commit that touches Rust.
- Frontend: `pnpm build` type-checks.
- Real distros: `scripts/test-distros.sh` builds the static CLI from the checkout and runs `tests/distro/run.sh` in Docker on Debian 12, Ubuntu 24.04, Fedora 42, Rocky 9, openSUSE Tumbleweed, Arch and Alpine 3.24: a native package (`tree`) installed, tracked, removed with leftovers purged; a traced script; a portable archive; an https download; refusals; a non-root user with no admin tool. Images it pulled are removed afterwards (`--keep` to keep them). CI runs the same matrix on every push.
