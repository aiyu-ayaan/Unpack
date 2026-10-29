//! Execute a plan: stream output, merge root steps into one pkexec prompt, record file changes.
use crate::plan::{removal, safe_ident, Action, Input, Meta, Plan, Step, Uninstall};
use crate::sniff::{sniff, Kind};
use crate::system::System;
use crate::util::{cache_dir, capture, display, has, home, which};
use crate::{fetch, trace};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;

pub type Emit<'a> = &'a dyn Fn(&str, &str);

/// Set by the command line when it runs in a terminal: root steps ask for the password with `sudo` there
/// instead of a `pkexec` window.
pub static TERMINAL_SUDO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn pump<R: Read + Send + 'static>(r: R, stream: &'static str, tx: mpsc::Sender<(&'static str, String)>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut rd = BufReader::new(r);
        let mut buf = vec![];
        while rd.read_until(b'\n', &mut buf).unwrap_or(0) > 0 {
            let _ = tx.send((stream, String::from_utf8_lossy(&buf).trim_end().to_string()));
            buf.clear();
        }
    })
}

fn run_status(mut cmd: Command, emit: Emit) -> Result<std::process::ExitStatus, String> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Could not start: {e}"))?;
    let (tx, rx) = mpsc::channel();
    let a = pump(child.stdout.take().expect("piped"), "out", tx.clone());
    let b = pump(child.stderr.take().expect("piped"), "err", tx);
    for (s, l) in rx {
        emit(s, &l);
    }
    let _ = (a.join(), b.join());
    child.wait().map_err(|e| e.to_string())
}

fn run_command(cmd: Command, emit: Emit) -> Result<(), String> {
    let st = run_status(cmd, emit)?;
    if st.success() { Ok(()) } else { Err(format!("Command failed ({st})")) }
}

/// Turn a pkexec exit into a message a new user can act on. pkexec exits 126 when the
/// password dialog is dismissed and 127 when authorization fails or no polkit agent runs.
fn pkexec_error(code: Option<i32>, auth_err: bool, sudo_hint: &str) -> String {
    match code {
        Some(126) => "The admin password prompt was cancelled. Nothing that needs admin rights was run.".into(),
        Some(127) if auth_err => format!(
            "Admin rights were not granted. If no password window appeared, no polkit authentication agent is running in this session.\nRun this in a terminal instead:\n  {sudo_hint}"
        ),
        _ => format!("Command failed (exit {})", code.map_or("signal".into(), |c| c.to_string())),
    }
}

fn user_cmd(program: &str, args: &[String], cwd: &Option<PathBuf>) -> Command {
    let mut c = crate::util::command(program);
    c.args(args).env("DEBIAN_FRONTEND", "noninteractive");
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    c
}

type RootCmd = (String, Vec<String>, Option<PathBuf>);

fn run_root(group: &[RootCmd], sys: &System, emit: Emit) -> Result<(), String> {
    let parts: Vec<String> = group
        .iter()
        .map(|(p, a, cwd)| {
            let line = display(p, a);
            match cwd {
                Some(d) => format!("cd {} && {line}", shlex::try_quote(&d.to_string_lossy()).map(|c| c.into_owned()).unwrap_or_default()),
                None => line,
            }
        })
        .collect();
    let script = parts.join(" && ");
    emit("info", &format!("$ {script}"));
    // pkexec clears the environment, so set what package managers need inside the script.
    let full = format!("export DEBIAN_FRONTEND=noninteractive; {script}");
    let sudo_hint = format!("sudo sh -c {}", shlex::try_quote(&full).unwrap_or_default());
    if sys.is_root {
        let mut cmd = crate::util::command("sh");
        cmd.args(["-c", &full]);
        return run_command(cmd, emit);
    }
    if TERMINAL_SUDO.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(sudo) = which("sudo") {
            // sudo reads the password from the terminal itself, so the piped output stays as it is.
            let mut cmd = crate::util::command(sudo);
            cmd.args(["sh", "-c", &full]);
            return run_command(cmd, emit);
        }
    }
    let Some(pk) = which("pkexec") else {
        return Err(format!("Admin rights are needed but `pkexec` is missing. Run this in a terminal:\n  {sudo_hint}"));
    };
    emit("info", "Waiting for the admin password…");
    let mut cmd = crate::util::command(pk);
    cmd.args(["sh", "-c", &full]);
    let auth_err = std::cell::Cell::new(false);
    let watch = |s: &str, l: &str| {
        if s == "err" && (l.contains("Not authorized") || l.contains("authentication agent") || l.starts_with("Error executing command as another user")) {
            auth_err.set(true);
        }
        emit(s, l);
    };
    let st = run_status(cmd, &watch)?;
    if st.success() { Ok(()) } else { Err(pkexec_error(st.code(), auth_err.get(), &sudo_hint)) }
}

pub struct Outcome {
    pub uninstall: Uninstall,
    /// Problems found after a successful install.
    pub notes: Vec<String>,
}

pub fn run(plan: &Plan, as_root: bool, sys: &System, emit: Emit) -> Result<Outcome, String> {
    if let Some((path, want)) = &plan.verify {
        if &fetch::sha256_file(path).map_err(|e| e.to_string())? != want {
            return Err("The file changed after it was checked. Nothing was run; analyze it again.".into());
        }
    }
    let roots = trace::roots();
    let before = plan.trace.then(|| trace::snapshot(&roots));
    let flat_before = plan.discover_flatpak.then(flatpak_apps);
    let mut created: Vec<String> = vec![];
    let mut nested: Option<Outcome> = None;

    let last_cmd = plan.steps.iter().rposition(|s| matches!(s.action, Action::Cmd { .. }));
    let mut i = 0;
    while i < plan.steps.len() {
        let s = &plan.steps[i];
        let root = s.needs_root || (as_root && plan.root_optional && Some(i) == last_cmd);
        emit("info", &format!("== {} ==", s.label));
        match &s.action {
            Action::Cmd { .. } if root => {
                let mut group: Vec<RootCmd> = vec![];
                while i < plan.steps.len() {
                    let n = &plan.steps[i];
                    let nroot = n.needs_root || (as_root && plan.root_optional && Some(i) == last_cmd);
                    match &n.action {
                        Action::Cmd { program, args, cwd } if nroot => group.push((program.clone(), args.clone(), cwd.clone())),
                        _ => break,
                    }
                    i += 1;
                }
                run_root(&group, sys, emit)?;
                continue;
            }
            Action::Cmd { program, args, cwd } => {
                emit("info", &format!("$ {}", display(program, args)));
                run_command(user_cmd(program, args, cwd), emit)?;
            }
            Action::Extract { archive, fmt, dest } => {
                std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
                crate::archive::check_members(&crate::archive::list(archive, *fmt)?)?;
                crate::archive::extract(archive, *fmt, dest)?;
                if dest.starts_with(home().join("Unpacked")) {
                    created.push(dest.to_string_lossy().into());
                }
            }
            Action::IntegrateAppImage { src, name, no_sandbox, launcher } => created.extend(integrate_appimage(src, name, *no_sandbox, launcher.as_deref(), emit)?),
            Action::InstallPortable { from, name } => created.extend(install_portable(from, name, emit)?),
            Action::InstallBinary { src, name } => created.extend(install_binary(src, name, emit)?),
            Action::PackageDeb { stage, out, name, version } => {
                package_deb(stage, out, name, version.as_deref(), emit)?;
            }
            Action::ContainerExport { container, manager, file } => {
                let u = container_export(container, manager, file, emit)?;
                nested = Some(Outcome { uninstall: u, notes: vec![] });
            }
            Action::InstallBuilt { dirs, ext } => {
                let pkg = find_built(dirs, ext).ok_or_else(|| format!("The build finished but no .{ext} package was found"))?;
                emit("info", &format!("Built package: {}", pkg.display()));
                let kind = sniff(&pkg).map_err(|e| e.to_string())?;
                let sub = crate::plan::build(&Input::Package { path: pkg, kind }, sys, &Meta { source: plan.source.clone(), ..Meta::default() });
                if !sub.supported {
                    return Err(sub.warnings.join(" "));
                }
                nested = Some(run(&sub, false, sys, emit)?);
            }
        }
        i += 1;
    }

    if let Some(n) = nested {
        return Ok(n);
    }
    let uninstall = if let Some(b) = flat_before {
        let new: Vec<String> = flatpak_apps().into_iter().filter(|a| !b.contains(a)).collect();
        match (&plan.uninstall, new.as_slice()) {
            (Uninstall::Manager { manager, user, .. }, [one]) => Uninstall::Manager { manager: manager.clone(), ident: one.clone(), user: *user },
            (u, _) => u.clone(),
        }
    } else if plan.trace || !created.is_empty() {
        if let Some(b) = before {
            created.extend(trace::diff(&b, &trace::snapshot(&roots)));
        }
        created.sort();
        created.dedup();
        if created.is_empty() {
            Uninstall::Manual { note: "The installer finished but no new files were found in ~/.local, /opt or /usr/local. It may have installed elsewhere; check its own uninstaller.".into() }
        } else {
            Uninstall::Files { paths: created }
        }
    } else {
        plan.uninstall.clone()
    };
    let notes = match &uninstall {
        Uninstall::Manager { manager, ident, user: false } => crate::libcheck::report(manager, ident, emit).into_iter().collect(),
        _ => vec![],
    };
    Ok(Outcome { uninstall, notes })
}

fn flatpak_apps() -> Vec<String> {
    capture("flatpak", &["list", "--user", "--app", "--columns=application"]).map(|o| o.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()).unwrap_or_default()
}

fn find_built(dirs: &[PathBuf], ext: &str) -> Option<PathBuf> {
    let suffix = format!(".{ext}");
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = vec![];
    for d in dirs {
        for e in std::fs::read_dir(d).ok()?.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.ends_with(&suffix) && !n.contains("-dbgsym") && !n.contains("-debug") {
                if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                    found.push((t, e.path()));
                }
            }
        }
    }
    found.sort();
    found.pop().map(|(_, p)| p)
}

// ------------------------------------------------------------ builtin installers

fn safe_name(s: &str) -> String {
    let n: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect();
    n.trim_start_matches('.').to_string()
}

fn desktop_path(name: &str) -> PathBuf {
    home().join(".local/share/applications").join(format!("unpack-{}.desktop", safe_name(name)))
}

fn refresh_menu() {
    if has("update-desktop-database") {
        let _ = crate::util::command("update-desktop-database").arg(home().join(".local/share/applications")).output();
    }
}

pub(crate) fn exec_quote(p: &Path) -> String {
    format!("\"{}\"", p.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$").replace('`', "\\`"))
}

/// Rewrite the app's own .desktop so Exec/Icon point at our copies. `exec` is the quoted command.
pub fn rewrite_desktop(text: &str, exec: &str, icon: Option<&Path>, flags: &str) -> String {
    let mut out = vec![];
    let mut in_entry = false;
    for l in text.lines() {
        if l.starts_with('[') {
            in_entry = l.trim() == "[Desktop Entry]";
        }
        if in_entry && l.starts_with("TryExec=") {
            continue;
        }
        if in_entry && l.starts_with("Exec=") {
            let rest = l["Exec=".len()..].split_once(' ').map(|(_, r)| format!(" {r}")).unwrap_or_default();
            let flags = if flags.is_empty() || rest.contains(flags) { String::new() } else { format!(" {flags}") };
            out.push(format!("Exec={exec}{flags}{rest}"));
        } else if in_entry && l.starts_with("Icon=") && icon.is_some() {
            out.push(format!("Icon={}", icon.expect("checked").display()));
        } else {
            out.push(l.to_string());
        }
    }
    out.push(String::new());
    out.join("\n")
}

fn minimal_desktop(name: &str, exec: &str, flags: &str) -> String {
    let flags = if flags.is_empty() { String::new() } else { format!(" {flags}") };
    format!("[Desktop Entry]\nType=Application\nName={name}\nExec={exec}{flags}\nTerminal=false\nCategories=Utility;\n")
}

fn integrate_appimage(src: &Path, name: &str, no_sandbox: bool, launcher: Option<&Path>, emit: Emit) -> Result<Vec<String>, String> {
    let file = src.file_name().ok_or("bad file name")?;
    let dest_dir = home().join("Applications");
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(file);
    if dest.exists() {
        return Err(format!("{} already exists. Remove it first or rename the download.", dest.display()));
    }
    std::fs::copy(src, &dest).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    emit("info", &format!("Copied to {}", dest.display()));
    let mut created = vec![dest.to_string_lossy().to_string()];

    let tmp = tempfile::tempdir_in(ensure_cache()?).map_err(|e| e.to_string())?;
    let extract = |pat: &str| {
        let _ = crate::util::command(&dest).args(["--appimage-extract", pat]).current_dir(tmp.path()).stdin(Stdio::null()).output();
    };
    extract("*.desktop");
    extract(".DirIcon");
    let root = tmp.path().join("squashfs-root");
    let desktop_src = std::fs::read_dir(&root).ok().and_then(|d| d.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "desktop")));
    let text = desktop_src.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
    let icon_name = text.as_deref().and_then(|t| t.lines().find_map(|l| l.strip_prefix("Icon=")).map(str::to_string));
    if let Some(i) = &icon_name {
        extract(&format!("{i}.png"));
        extract(&format!("{i}.svg"));
    }
    let icon = ["png", "svg"]
        .iter()
        .filter_map(|ext| icon_name.as_ref().map(|i| root.join(format!("{i}.{ext}"))))
        .chain([root.join(".DirIcon")])
        .find(|p| p.exists() && p.canonicalize().is_ok_and(|c| c.starts_with(tmp.path())))
        .and_then(|p| {
            let ext = if std::fs::read(&p).map(|b| b.starts_with(b"\x89PNG")).unwrap_or(false) { "png" } else { "svg" };
            let out = home().join(".local/share/unpack/icons");
            std::fs::create_dir_all(&out).ok()?;
            let target = out.join(format!("{}.{ext}", safe_name(name)));
            std::fs::copy(&p, &target).ok()?;
            Some(target)
        });
    if let Some(i) = &icon {
        created.push(i.to_string_lossy().into());
    }
    let flags = if no_sandbox { "--no-sandbox" } else { "" };
    let exec = match launcher {
        Some(l) => format!("{} {}", exec_quote(l), exec_quote(&dest)),
        None => exec_quote(&dest),
    };
    let body = text.map(|t| rewrite_desktop(&t, &exec, icon.as_deref(), flags)).unwrap_or_else(|| minimal_desktop(name, &exec, flags));
    let dpath = desktop_path(name);
    std::fs::create_dir_all(dpath.parent().expect("has parent")).map_err(|e| e.to_string())?;
    std::fs::write(&dpath, body).map_err(|e| e.to_string())?;
    created.push(dpath.to_string_lossy().into());
    refresh_menu();
    emit("info", "Added to the app menu");
    Ok(created)
}

fn ensure_cache() -> Result<PathBuf, String> {
    let d = cache_dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

fn is_program(p: &Path) -> bool {
    let Ok(m) = std::fs::metadata(p) else { return false };
    if !m.is_file() || m.permissions().mode() & 0o111 == 0 {
        return false;
    }
    let mut head = [0u8; 4];
    std::fs::File::open(p).and_then(|mut f| f.read(&mut head)).map(|n| n >= 2 && (head.starts_with(b"\x7fELF") || head.starts_with(b"#!"))).unwrap_or(false)
}

fn install_portable(from: &Path, name: &str, emit: Emit) -> Result<Vec<String>, String> {
    let safe = safe_name(name);
    let dest = home().join(".local/opt").join(&safe);
    if dest.exists() {
        return Err(format!("{} already exists", dest.display()));
    }
    std::fs::create_dir_all(dest.parent().expect("has parent")).map_err(|e| e.to_string())?;
    run_command({ let mut c = crate::util::command("mv"); c.arg(from).arg(&dest); c }, emit)?;
    let mut exes: Vec<PathBuf> = walkdir::WalkDir::new(&dest)
        .max_depth(2)
        .into_iter()
        .flatten()
        .map(|e| e.into_path())
        .filter(|p| is_program(p) && !p.extension().is_some_and(|x| x == "so"))
        .collect();
    let lname = name.to_lowercase();
    exes.sort_by_key(|p| {
        let n = p.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        (!(n == lname), !n.contains(&lname), p.components().count())
    });
    let exe = exes.first().ok_or("No program found inside the archive")?.clone();
    let mut created = vec![dest.to_string_lossy().to_string()];
    let bin = home().join(".local/bin");
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let link = bin.join(exe.file_name().expect("file"));
    if link.symlink_metadata().is_ok() {
        emit("warn", &format!("{} already exists; command not linked", link.display()));
    } else {
        std::os::unix::fs::symlink(&exe, &link).map_err(|e| e.to_string())?;
        created.push(link.to_string_lossy().into());
        emit("info", &format!("Command available as `{}`", link.file_name().expect("file").to_string_lossy()));
    }
    let dpath = desktop_path(name);
    std::fs::create_dir_all(dpath.parent().expect("has parent")).map_err(|e| e.to_string())?;
    std::fs::write(&dpath, minimal_desktop(name, &exec_quote(&exe), "")).map_err(|e| e.to_string())?;
    created.push(dpath.to_string_lossy().into());
    refresh_menu();
    Ok(created)
}

fn install_binary(src: &Path, name: &str, emit: Emit) -> Result<Vec<String>, String> {
    let bin = home().join(".local/bin");
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let dest = bin.join(safe_name(name));
    if dest.exists() {
        return Err(format!("{} already exists", dest.display()));
    }
    std::fs::copy(src, &dest).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    emit("info", &format!("Installed {}. Make sure ~/.local/bin is in your PATH.", dest.display()));
    Ok(vec![dest.to_string_lossy().into()])
}

/// `dpkg-deb --build` over a `DESTDIR` staging folder, with a generated `DEBIAN/control`.
fn package_deb(stage: &Path, out: &Path, name: &str, version: Option<&str>, emit: Emit) -> Result<(), String> {
    let staged = walkdir::WalkDir::new(stage).into_iter().flatten().any(|e| !e.file_type().is_dir());
    if !staged {
        return Err("The build installed no files into the staging folder, so there is nothing to package.".into());
    }
    let control = stage.join("DEBIAN");
    std::fs::create_dir_all(&control).map_err(|e| e.to_string())?;
    std::fs::write(control.join("control"), crate::plan::deb_control(name, version)).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let deb = out.join(format!("{}.deb", crate::plan::deb_name(name)));
    let mut c = crate::util::command("dpkg-deb");
    c.args(["--build", "--root-owner-group"]).arg(stage).arg(&deb);
    emit("info", &format!("$ dpkg-deb --build --root-owner-group {} {}", stage.display(), deb.display()));
    run_command(c, emit)
}

// ------------------------------------------------------------ distrobox containers

fn inside(container: &str, args: &[&str]) -> Result<String, String> {
    let mut a = vec!["enter", container, "--"];
    a.extend(args);
    capture("distrobox", &a)
}

/// `key` value from `Key : value` or `key = value` lines.
fn value_of(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once(':').or_else(|| l.split_once('='))?;
        (k.trim() == key).then(|| v.trim().to_string())
    })
}

/// The package's name, read inside the container (the host may not have the tools for its format).
fn container_ident(container: &str, manager: &str, file: &str) -> Option<String> {
    let name = match manager {
        "apt" => inside(container, &["dpkg-deb", "-f", file, "Package"]).ok().map(|o| o.trim().to_string()),
        "dnf" => inside(container, &["rpm", "-qp", "--qf", "%{NAME}", file]).ok().map(|o| o.trim().to_string()),
        "pacman" => inside(container, &["pacman", "-Qip", file]).ok().and_then(|o| value_of(&o, "Name")),
        _ => inside(container, &["tar", "-xzOf", file, ".PKGINFO"]).ok().and_then(|o| value_of(&o, "pkgname")),
    };
    name.filter(|n| safe_ident(n))
}

/// Install-time half of a container install: name the package, export its menu entries (or, when it has
/// none, the first few commands it puts in /usr/bin) to the host.
fn container_export(container: &str, manager: &str, file: &Path, emit: Emit) -> Result<Uninstall, String> {
    let ident = container_ident(container, manager, &file.to_string_lossy()).ok_or("Installed in the container, but its package name could not be read")?;
    let list = match manager {
        "apt" => inside(container, &["dpkg", "-L", &ident]),
        "dnf" => inside(container, &["rpm", "-ql", &ident]),
        "pacman" => inside(container, &["pacman", "-Qlq", &ident]),
        _ => inside(container, &["apk", "info", "-L", &ident]),
    }
    .unwrap_or_default();
    let paths: Vec<String> = list.lines().map(|l| if l.starts_with('/') { l.trim().to_string() } else { format!("/{}", l.trim()) }).collect();
    let mut apps = vec![];
    for d in paths.iter().filter_map(|p| p.strip_prefix("/usr/share/applications/")?.strip_suffix(".desktop")).filter(|n| safe_ident(n)) {
        match inside(container, &["distrobox-export", "--app", d]) {
            Ok(_) => apps.push(d.to_string()),
            Err(e) => emit("warn", &format!("Could not add {d} to the menu: {e}")),
        }
    }
    let mut bins = vec![];
    if apps.is_empty() {
        let dest = home().join(".local/bin").to_string_lossy().to_string();
        for b in paths.iter().filter(|p| p.starts_with("/usr/bin/") && !p.contains("..")).take(5) {
            if inside(container, &["distrobox-export", "--bin", b, "--export-path", &dest]).is_ok() {
                bins.push(b.clone());
            }
        }
    }
    emit("info", &format!("{ident} is installed in {container}; added {} menu entries and {} commands", apps.len(), bins.len()));
    let _ = std::fs::remove_file(file);
    Ok(Uninstall::Container { container: container.into(), manager: manager.into(), ident, apps, bins })
}

fn container_uninstall(container: &str, manager: &str, ident: &str, apps: &[String], bins: &[String], emit: Emit) -> Result<(), String> {
    if !safe_ident(container) || !container.starts_with("unpack-") {
        return Err(format!("Refusing unknown container {container:?}"));
    }
    let dest = home().join(".local/bin").to_string_lossy().to_string();
    for a in apps.iter().filter(|a| safe_ident(a)) {
        if let Err(e) = inside(container, &["distrobox-export", "--app", a, "--delete"]) {
            emit("warn", &format!("Could not remove {a} from the menu: {e}"));
        }
    }
    for b in bins.iter().filter(|b| b.starts_with("/usr/bin/") && !b.contains("..")) {
        let _ = inside(container, &["distrobox-export", "--bin", b, "--export-path", &dest, "--delete"]);
    }
    let (p, a, _) = removal(manager, ident, false)?;
    let mut args: Vec<String> = ["enter", container, "--", "sudo", &p].map(String::from).to_vec();
    args.extend(a);
    emit("info", &format!("$ {}", display("distrobox", &args)));
    run_command(user_cmd("distrobox", &args, &None), emit)
}

// ------------------------------------------------------------ uninstall

fn removable_roots() -> Vec<PathBuf> {
    let h = home();
    vec![h.join(".local"), h.join("Applications"), h.join("Unpacked"), PathBuf::from("/opt"), PathBuf::from("/usr/local")]
}

/// Only delete inside known install areas, never a root itself, never via `..`.
pub fn safe_to_remove(p: &Path) -> bool {
    p.is_absolute()
        && !p.components().any(|c| matches!(c, std::path::Component::ParentDir))
        && removable_roots().iter().any(|r| p.starts_with(r) && p != r)
}

/// Whether dpkg knows `name` as an installed package; true when it can't be asked.
fn apt_installed(name: &str) -> bool {
    !has("dpkg-query") || capture("dpkg-query", &["-W", "-f", "${Status}", name]).is_ok_and(|s| s.ends_with("installed"))
}

/// Records made before the .deb name check stored the file name instead of the package name, so apt can't find
/// them. Read the real name from the saved .deb when it is still there.
pub fn heal(u: &Uninstall, source: Option<&str>) -> Uninstall {
    if let (Uninstall::Manager { manager, ident, user }, Some(src)) = (u, source) {
        if manager == "apt" && !apt_installed(ident) && src.ends_with(".deb") && Path::new(src).is_file() {
            if let Some(name) = capture("dpkg-deb", &["-f", src, "Package"]).ok().map(|o| o.trim().to_string()).filter(|n| !n.is_empty()) {
                return Uninstall::Manager { manager: manager.clone(), ident: name, user: *user };
            }
        }
    }
    u.clone()
}

pub fn uninstall(u: &Uninstall, sys: &System, emit: Emit) -> Result<(), String> {
    match u {
        Uninstall::Manager { manager, ident, .. } if manager == "apt" && !apt_installed(ident) => {
            Err(format!("apt has no installed package called \"{ident}\", so there is nothing for it to remove. Look the app up in Everything on this computer."))
        }
        Uninstall::Manager { manager, ident, user } => {
            let (p, a, root) = removal(manager, ident, *user)?;
            emit("info", &format!("$ {}", display(&p, &a)));
            if root { run_root(&[(p, a, None)], sys, emit) } else { run_command(user_cmd(&p, &a, &None), emit) }
        }
        Uninstall::Files { paths } => remove_files(paths, sys, emit),
        Uninstall::Container { container, manager, ident, apps, bins } => container_uninstall(container, manager, ident, apps, bins, emit),
        Uninstall::Traced => Err("No record of what this installed".into()),
        Uninstall::Manual { note } => Err(format!("Can't remove automatically. {note}")),
    }
}

fn remove_files(paths: &[String], sys: &System, emit: Emit) -> Result<(), String> {
    let roots = removable_roots();
    let mut need_root: Vec<String> = vec![];
    for s in paths {
        let p = Path::new(s);
        if !safe_to_remove(p) {
            emit("warn", &format!("Skipped (outside install areas): {s}"));
            continue;
        }
        let r = match p.symlink_metadata() {
            Ok(m) if m.is_dir() => std::fs::remove_dir_all(p),
            Ok(_) => std::fs::remove_file(p),
            Err(_) => continue, // already gone
        };
        match r {
            Ok(()) => {
                emit("info", &format!("Removed {s}"));
                trace::prune_empty_parents(p, &roots);
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => need_root.push(s.clone()),
            Err(e) => return Err(format!("{s}: {e}")),
        }
    }
    if !need_root.is_empty() {
        let mut args = vec!["-rf".to_string(), "--".to_string()];
        args.extend(need_root);
        run_root(&[("rm".into(), args, None)], sys, emit)?;
    }
    refresh_menu();
    Ok(())
}

#[allow(dead_code)]
fn _unused(_: &Step, _: Kind) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_is_fenced() {
        let _g = crate::util::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let h = home();
        assert!(safe_to_remove(&h.join(".local/bin/app")));
        assert!(safe_to_remove(Path::new("/opt/app")));
        assert!(!safe_to_remove(Path::new("/opt")));
        assert!(!safe_to_remove(&h.join(".local")));
        assert!(!safe_to_remove(Path::new("/usr/bin/ls")));
        assert!(!safe_to_remove(Path::new("/etc/passwd")));
        assert!(!safe_to_remove(&h.join(".local/../.ssh/id_rsa")));
        assert!(!safe_to_remove(Path::new("relative/path")));
    }

    #[test]
    fn rewrites_desktop() {
        let t = "[Desktop Entry]\nName=App\nExec=AppRun %U\nIcon=app\nTryExec=AppRun\n[Desktop Action x]\nExec=other\n";
        let out = rewrite_desktop(t, &exec_quote(Path::new("/home/u/Applications/App.AppImage")), Some(Path::new("/i/app.png")), "");
        assert!(out.contains("Exec=\"/home/u/Applications/App.AppImage\" %U"));
        assert!(out.contains("Icon=/i/app.png"));
        assert!(!out.contains("TryExec"));
        assert!(out.contains("Exec=other"));
    }

    #[test]
    fn desktop_gets_no_sandbox_once() {
        let t = "[Desktop Entry]\nExec=AppRun %U\n";
        let out = rewrite_desktop(t, &exec_quote(Path::new("/a/B.AppImage")), None, "--no-sandbox");
        assert!(out.contains("Exec=\"/a/B.AppImage\" --no-sandbox %U"), "{out}");
        let again = rewrite_desktop("[Desktop Entry]\nExec=AppRun --no-sandbox %U\n", "\"/a/B.AppImage\"", None, "--no-sandbox");
        assert_eq!(again.matches("--no-sandbox").count(), 1);
        assert!(minimal_desktop("B", "\"/a/B.AppImage\"", "--no-sandbox").contains("Exec=\"/a/B.AppImage\" --no-sandbox\n"));
    }

    #[test]
    fn packages_a_staged_build() {
        if !has("dpkg-deb") {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let stage = d.path().join("stage");
        assert!(package_deb(&stage, &d.path().join("out"), "Tool", Some("1.0"), &|_, _| {}).is_err(), "empty stage");
        std::fs::create_dir_all(stage.join("usr/local/bin")).unwrap();
        std::fs::write(stage.join("usr/local/bin/tool"), "#!/bin/sh\n").unwrap();
        package_deb(&stage, &d.path().join("out"), "Tool", Some("1.0"), &|_, _| {}).unwrap();
        let deb = d.path().join("out/unpack-tool.deb");
        assert_eq!(capture("dpkg-deb", &["-f", &deb.to_string_lossy(), "Package", "Version"]).unwrap(), "Package: unpack-tool\nVersion: 1.0~unpack1\n");
        assert!(capture("dpkg-deb", &["-c", &deb.to_string_lossy()]).unwrap().contains("./usr/local/bin/tool"));
    }

    #[test]
    fn runs_and_streams() {
        let lines = std::sync::Mutex::new(vec![]);
        let emit = |s: &str, l: &str| lines.lock().unwrap().push(format!("{s}:{l}"));
        let mut c = crate::util::command("sh");
        c.args(["-c", "echo a; echo b >&2"]);
        run_command(c, &emit).unwrap();
        let got = lines.lock().unwrap().clone();
        assert!(got.contains(&"out:a".to_string()) && got.contains(&"err:b".to_string()));
        let mut bad = crate::util::command("sh");
        bad.args(["-c", "exit 3"]);
        assert!(run_command(bad, &emit).is_err());
    }

    #[test]
    fn explains_pkexec_exits() {
        assert!(pkexec_error(Some(126), false, "sudo x").contains("cancelled"));
        let denied = pkexec_error(Some(127), true, "sudo x");
        assert!(denied.contains("not granted") && denied.contains("sudo x"));
        // 127 without an auth message is the script's own "command not found".
        assert_eq!(pkexec_error(Some(127), false, "sudo x"), "Command failed (exit 127)");
    }

    #[test]
    fn script_plan_runs_and_traces() {
        // End to end for the unmanaged-script path, with a temp HOME so nothing real is touched.
        let _g = crate::util::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let h = tempfile::tempdir().unwrap();
        let script = h.path().join("install.sh");
        std::fs::write(&script, format!("#!/bin/sh\nmkdir -p {0}/.local/bin && echo x > {0}/.local/bin/hello\n", h.path().display())).unwrap();
        std::env::set_var("HOME", h.path());
        std::env::set_var("XDG_DATA_HOME", h.path().join("share"));
        std::env::set_var("XDG_CACHE_HOME", h.path().join("cache"));
        let sys = crate::system::detect();
        let plan = crate::plan::build(&Input::Package { path: script, kind: Kind::Script }, &sys, &Meta { source: "t".into(), ..Meta::default() });
        let out = run(&plan, false, &sys, &|_, _| {}).unwrap();
        match &out.uninstall {
            Uninstall::Files { paths } => assert!(paths.iter().any(|p| p.ends_with(".local/bin/hello")), "{paths:?}"),
            other => panic!("{other:?}"),
        }
        uninstall(&out.uninstall, &sys, &|_, _| {}).unwrap();
        assert!(!h.path().join(".local/bin/hello").exists());
    }
}
