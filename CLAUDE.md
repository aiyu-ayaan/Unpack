# CLAUDE.md — Unpack

Unpack is a Linux desktop app: drop in a package, archive, script, git URL, or pasted `curl`/`wget` command; it works out what it is, installs it for the user's distro, tracks it in SQLite, and can uninstall it. Target user: someone new to Linux.

## Stack
- Backend: Rust (Tauri 2 commands), SQLite via `rusqlite`
- UI: Tauri 2 + React + TypeScript + Tailwind CSS v4
- Linux integration: shell out to apt, dnf, zypper, pacman, apk, flatpak, snap CLIs

## Layout
- `app/` — the application (`src/` React, `src-tauri/` Rust)
- `development/docs/` — research and design docs. Start at `development/docs/README.md`
- `development/TODO.md` — master task list. Update it in the same commit as the work.

## Commands
Run from `app/`:
- `pnpm install` — frontend deps
- `pnpm tauri dev` — run the app
- `pnpm build` — type-check + build frontend
- `pnpm tauri build --bundles deb` — release `.deb` (bundle category must be a valid freedesktop one, e.g. `Utility`)
- `cargo test --manifest-path src-tauri/Cargo.toml` — backend tests (must pass before committing Rust changes)
- `cargo clippy --manifest-path src-tauri/Cargo.toml` — lints
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

## Gotchas
- Don't `pkill -f vite` from a Claude shell; the pattern matches the shell's own command line. Kill by PID.
- Tests that change `HOME`/`XDG_*` must hold `util::TEST_ENV`.
- Debug binary loads `http://localhost:1420`; start `pnpm dev` first.
