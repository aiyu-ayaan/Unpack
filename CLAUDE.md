# CLAUDE.md — Unpack

Unpack is a Linux desktop app: drop in a package, archive, script, git URL, or pasted `curl`/`wget` command; it works out what it is, installs it for the user's distro, tracks it in SQLite, and can uninstall it. Target user: someone new to Linux.

## Stack
- Backend: Rust (Tauri 2 commands), SQLite via `rusqlite`
- UI: Tauri 2 + React + TypeScript + Tailwind CSS v4
- Linux integration: shell out to apt, dnf, zypper, pacman, apk, flatpak, snap CLIs

## Layout
- `app/` — the application (`src/` React, `src-tauri/` Rust)
- `assets/brand/` — logo SVGs and `build.py` (generates them and `app/src/components/logoPaths.ts`). See `development/docs/06-brand.md`
- `development/docs/` — research and design docs. Start at `development/docs/README.md`
- `development/TODO.md` — master task list. Update it in the same commit as the work.

## Commands
Run from the repo root (a pnpm workspace; root scripts forward to `app/`):
- `pnpm install` — frontend deps
- `pnpm dev` — run the dev flavor (own `unpack-dev` data/cache dirs and identifier, so the installed app is untouched; starts Vite and the desktop window); `pnpm dev:web` — frontend only
- `pnpm build` — type-check + build frontend
- `pnpm tauri build --bundles deb` — release `.deb` (bundle category must be a valid freedesktop one, e.g. `Utility`)
- `pnpm test` — backend tests, `cargo test --no-default-features` (no WebKit; must pass before committing Rust changes)
- `scripts/build-cli.sh` — static musl CLI into `dist-cli/` (in an alpine container; cache stays on the host)
- `scripts/test-distros.sh [--keep] [distro...]` — that CLI tested in Docker on 7 distros; removes the images it pulled
- `pnpm lint` — `cargo clippy` (desktop build; CI also runs it with `--no-default-features` and `-D warnings`)
- System build deps (Debian/Ubuntu): `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev build-essential`

## Rules
- Never hard-code distro names for behaviour. Detect capabilities (which binaries exist, immutable root). `/etc/os-release` is for display and fallback.
- The frontend never sends command lines. It calls `analyze(input)` → backend builds a `Plan` and keeps it by id → `execute(plan_id)`. See `development/docs/05-security.md`.
- Commands are `program + args` vectors, never interpolated shell strings. Root steps are merged into one `pkexec` call.
- Parse untrusted files as the unprivileged user; hand root only a vetted path.
- Prefer user scope (`flatpak --user`, `~/.local`) when the format allows.
- Do not silently pass `--allow-untrusted`, `--no-gpg-checks`, `--dangerous`; surface them in the plan warnings.
- Every install must record an `UninstallSpec` in SQLite.
- Keep docs and TODO current. Mark unverified facts **(unverified)** in docs until tested on a real system.
- Commit in small stages with Conventional Commit messages. Do not add co-author or attribution trailers.
- Releases are marker commits on `master` (`!alpha`, `!beta`, `!stable`, `!fix`, `!feat`, `!major`, `!patch`, e.g. `!alpha : First build`) that open a release PR; merging it builds and publishes. Only commit a marker when the user asks for a release. Rules in `development/docs/08-release.md`; read it before changing `.github/workflows/release.yml`, `scripts/release-version.mjs`, `scripts/set-version.mjs` or any version field. Never edit a version by hand.

## Gotchas
- Don't `pkill -f vite` from a Claude shell; the pattern matches the shell's own command line. Kill by PID.
- Tests that change `HOME`/`XDG_*` must hold `util::TEST_ENV`.
- Debug binary loads `http://localhost:1420`; start `pnpm dev` first.
- `unpack` with no arguments opens the window; any argument goes to `cli.rs`. Tauri code lives only in `gui.rs` behind the `gui` feature.
- busybox (Alpine) has its own `tree` applet: check removal with the package manager, not `command -v`.
