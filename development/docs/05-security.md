# 05 — Security model

Unpack installs untrusted software for non-expert users, so the app itself must be hard to turn into a weapon.

## Principles

1. **Explain, then act.** Every install shows a plan: what the file is, which commands will run, whether root is needed.
2. **Never pipe network to a shell.** For `curl … | sh`, download to a private temp file, compute SHA-256, show the script, require consent, then run the local copy. This is not magic safety (a malicious script is still malicious; also a server can serve different content to `curl` than to a browser, so we show what *we* fetched). It guarantees the user saw what runs, and gives a hash to record.
3. **Least privilege.** User-scope first. Root only for steps that need it, merged into one `pkexec` prompt.
4. **Frontend cannot inject commands.** The webview asks the backend to `analyze(input)`, which returns a plan stored in backend memory under an id. `execute(plan_id)` runs only what the backend built. The UI never sends a command line to run.
5. **No shell interpolation.** Commands are `program + args` vectors. When a shell is required (merged root steps), arguments are quoted with `shlex`.
6. **Untrusted archives.** Reject path traversal, absolute members and escaping symlinks; cap size and member count; extract into a fresh `0700` temp dir.
7. **Integrity where available.** Verify `.dsc` checksums, show GPG/signature state from the package manager, never silently pass `--allow-untrusted` / `--no-gpg-checks` / `--dangerous` — the UI must state it and the user must accept.
8. **Download hygiene.** HTTPS only by default (HTTP needs an explicit tick), follow redirects up to a limit, size cap, record the link as given, the final URL when redirects changed it, and the SHA-256.
9. **Record everything.** Each install writes an event log row (what ran, exit code, output tail) to SQLite.

## Threats and mitigations

| Threat | Mitigation |
|---|---|
| Malicious pasted command (`curl … \| sudo bash`) | Parse, do not execute pasted text; strip `sudo`; fetch and review |
| Script that wipes files | Static pattern scan (`rm -rf /`, `dd of=/dev`, `mkfs`, fork bombs) → red warning; never run silently |
| Zip-slip / tar traversal | Member-path validation before extract (done by our code, then system tar) |
| Decompression bomb | Cap uncompressed size via listing before extract |
| Wrong architecture binary | Check ELF machine / `.deb` Architecture / `.rpm` arch vs host |
| Android `.apk` mistaken for Alpine | Zip magic check |
| Privilege escalation via helper | v1 uses `pkexec` with fixed commands built in backend. v2 helper accepts only a closed verb set |
| Compromised update source | Update checks are notification-only until signature verification exists |
| UI XSS (Tauri webview) | Strict CSP, no remote content, no `dangerouslySetInnerHTML`, Tauri capabilities restricted to the commands we define |
| Electron AppImage blocked by the OS sandbox rules | Detected from the image listing (`chrome-sandbox`, never run) plus `apparmor_restrict_unprivileged_userns`; the plan warns, and only then does the menu entry add `--no-sandbox` |
| Host tools loading the AppImage'"'"'s bundled libraries | Every child process is built by `util::command`, which drops `LD_LIBRARY_PATH`, `PATH` and GTK/GIO/XDG entries under `APPDIR` |
| Missing libraries after install | `ldd` over the installed ELF files, as the user, after the package is in place; findings are warnings, never actions |
| TOCTOU between analysis and install | Hash the file at analyze time; re-hash before execute; abort on mismatch |

## PackageKit lesson

An advisory cited in research: PackageKit ≤ 1.1.13 let `InstallFiles`, `GetFilesLocal`, `GetDetailsLocal` touch files before checking authorization (fixed 1.2.3). Takeaway: local-file install is a privileged-parse attack surface. Do **all** parsing of untrusted packages as the unprivileged user, and only hand a vetted path to root.

## Tauri hardening checklist

- `app.security.csp` set; `default-src 'self'`.
- Capabilities: only `core:default` plus our own commands and the event system. No `shell`, `fs`, `http` plugin exposed to the webview.
- No `withGlobalTauri`.
- Drag-and-drop paths come from Tauri's drag event, not from `<input type=file>` blob reads.

## Store

- The Store's lists come from Flathub's HTTPS API and local package tools. The page gets `{source, id}` pairs back and sends only those to `store_plan`; the backend rebuilds the command from a fixed template and refuses ids that fail `plan::safe_ident` (no leading `-`, only `A-Za-z0-9+._:@-`).
- Icons are the only remote content the page loads. The CSP allows images from `https://dl.flathub.org` only, and `catalog.rs` drops any icon URL elsewhere.
- Flathub installs go to the user installation (`flatpak --user`). Snap and native installs need the admin password.

## Updates

- The page picks updates by `{manager, name, user}` from the backend's own list; `upgrade_plan` checks every name with `safe_ident` and only accepts the system's own native manager, Flatpak and snap.
- A new release of a tracked app is found by the backend (GitHub API, `safe_ident` on owner, repo and tag) and analyzed like any download: the user reviews the plan, the file is hashed, nothing runs before approval.

## Sandbox preview

- "Try it in a sandbox first" runs a script plan's single step with `bwrap --ro-bind / / --dev /dev --proc /proc --tmpfs /tmp --tmpfs /run --bind <throwaway> $HOME --ro-bind <work dir> <work dir> --unshare-all --die-with-parent --new-session`. The system is read-only, the network and every other namespace are separate, and home is an empty folder deleted afterwards. It stops after 2 minutes.
- It is a preview, not a guarantee: a script can behave differently with network access or when it detects a sandbox, and files outside home are not reported. Readable files outside home (`/etc`, `/opt`) are visible to it, but it has no network to send them anywhere.
- Ubuntu 24.04 and later restrict unprivileged user namespaces with AppArmor; bubblewrap has its own profile there, but whether `--unshare-net` works is **(unverified)**. When bwrap cannot start, the page says so instead of reporting a result.

## Privileged helper (polkit)

- The `.deb` and `.rpm` install `/usr/share/polkit-1/actions/dev.unpack.app.helper.policy`: action `dev.unpack.app.helper`, `exec.path` `/usr/bin/unpack`, `exec.argv1` `helper`, `auth_admin_keep` for the active session. The password prompt names Unpack and says what it is for.
- When that policy exists and the running binary is `/usr/bin/unpack`, root steps run as `pkexec /usr/bin/unpack helper '<json>'`. `helper.rs` refuses the whole request unless every command matches its allowlist: `apt-get` (via `env DEBIAN_FRONTEND=noninteractive` only), `dnf`/`yum`, `zypper`, `pacman`, `apk`, `snap`, `flatpak`, `rpm-ostree`, `transactional-update`, each with fixed subcommands and flags, operands that are `safe_ident` package names or absolute paths without `..`, and `rm -rf --` only below `/opt/` or `/usr/local/`. No shell runs; options such as `-o`, `--config`, `--setopt` or `--allow-unauthenticated` are refused. The helper checks again as root; the caller's check only decides which path to take.
- Root steps outside the allowlist (a script the user chose to run as administrator) and the AppImage or dev builds (no policy, or a different binary) still use `pkexec sh -c`, as before.
- A package the helper installs can still run its own maintainer scripts as root: the helper narrows what Unpack can be made to run, not what a package does.

## Checking a file against what its publisher printed
On the plan review (or `--verify` on the command line) the user can paste a SHA-256, or give an https link or a path to a detached OpenPGP signature (`.sig`, `.asc`) or a `SHA256SUMS`-style list. A hash is compared with the plan's own SHA-256 (the same file `execute` checks again before running). A list matches when it holds the file's hash, and is a mismatch when it names the file with another hash. A signature is checked with `gpg --batch --status-fd 1 --verify` as the user, against the user's keyring only: Unpack never fetches keys itself; without the key it says which key to import. A good signature from a key that is not trusted says to make sure the key is the project's. A mismatch (hash, list or `BADSIG`) refuses the plan, so it can't be run.
