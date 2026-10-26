//! Execute a plan: stream output, merge root steps into one pkexec prompt, record file changes.
use crate::plan::{removal, Action, Input, Meta, Plan, Step, Uninstall};
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
    let mut nested: Option<Uninstall> = None;

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
            Action::IntegrateAppImage { src, name } => created.extend(integrate_appimage(src, name, emit)?),
            Action::InstallPortable { from, name } => created.extend(install_portable(from, name, emit)?),
            Action::InstallBinary { src, name } => created.extend(install_binary(src, name, emit)?),
            Action::InstallBuilt { dirs, ext } => {
                let pkg = find_built(dirs, ext).ok_or_else(|| format!("The build finished but no .{ext} package was found"))?;
                emit("info", &format!("Built package: {}", pkg.display()));
                let kind = sniff(&pkg).map_err(|e| e.to_string())?;
                let sub = crate::plan::build(&Input::Package { path: pkg, kind }, sys, &Meta { source: plan.source.clone(), sha256: None, pipe_to_shell: false });
                if !sub.supported {
                    return Err(sub.warnings.join(" "));
                }
                nested = Some(run(&sub, false, sys, emit)?.uninstall);
            }
        }
        i += 1;
    }

    let uninstall = if let Some(n) = nested {
        n
    } else if let Some(b) = flat_before {
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
    Ok(Outcome { uninstall })
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

fn exec_quote(p: &Path) -> String {
    format!("\"{}\"", p.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$").replace('`', "\\`"))
}

/// Rewrite the app's own .desktop so Exec/Icon point at our copies.
pub fn rewrite_desktop(text: &str, exec: &Path, icon: Option<&Path>) -> String {
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
            out.push(format!("Exec={}{rest}", exec_quote(exec)));
        } else if in_entry && l.starts_with("Icon=") && icon.is_some() {
            out.push(format!("Icon={}", icon.expect("checked").display()));
        } else {
            out.push(l.to_string());
        }
    }
    out.push(String::new());
    out.join("\n")
}

fn minimal_desktop(name: &str, exec: &Path) -> String {
    format!("[Desktop Entry]\nType=Application\nName={name}\nExec={}\nTerminal=false\nCategories=Utility;\n", exec_quote(exec))
}

fn integrate_appimage(src: &Path, name: &str, emit: Emit) -> Result<Vec<String>, String> {
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
    let body = text.map(|t| rewrite_desktop(&t, &dest, icon.as_deref())).unwrap_or_else(|| minimal_desktop(name, &dest));
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
    std::fs::write(&dpath, minimal_desktop(name, &exe)).map_err(|e| e.to_string())?;
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

pub fn uninstall(u: &Uninstall, sys: &System, emit: Emit) -> Result<(), String> {
    match u {
        Uninstall::Manager { manager, ident, user } => {
            let (p, a, root) = removal(manager, ident, *user)?;
            emit("info", &format!("$ {}", display(&p, &a)));
            if root { run_root(&[(p, a, None)], sys, emit) } else { run_command(user_cmd(&p, &a, &None), emit) }
        }
        Uninstall::Files { paths } => remove_files(paths, sys, emit),
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
        let out = rewrite_desktop(t, Path::new("/home/u/Applications/App.AppImage"), Some(Path::new("/i/app.png")));
        assert!(out.contains("Exec=\"/home/u/Applications/App.AppImage\" %U"));
        assert!(out.contains("Icon=/i/app.png"));
        assert!(!out.contains("TryExec"));
        assert!(out.contains("Exec=other"));
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
        let plan = crate::plan::build(&Input::Package { path: script, kind: Kind::Script }, &sys, &Meta { source: "t".into(), sha256: None, pipe_to_shell: false });
        let out = run(&plan, false, &sys, &|_, _| {}).unwrap();
        match &out.uninstall {
            Uninstall::Files { paths } => assert!(paths.iter().any(|p| p.ends_with(".local/bin/hello")), "{paths:?}"),
            other => panic!("{other:?}"),
        }
        uninstall(&out.uninstall, &sys, &|_, _| {}).unwrap();
        assert!(!h.path().join(".local/bin/hello").exists());
    }
}
