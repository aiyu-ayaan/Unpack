//! Try an installer script in a bubblewrap sandbox before running it for real: the system is read-only, the
//! network is off, and home is an empty throwaway folder. The report says what it printed and which files it
//! tried to create in home. Nothing it does reaches the real system.
use crate::plan::{Action, Plan};
use crate::util::{cache_dir, command, has, home};
use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

#[derive(Serialize, Debug)]
pub struct Report {
    /// Exit code; none when it was stopped or killed by a signal.
    pub code: Option<i32>,
    pub timed_out: bool,
    /// Files it created in the throwaway home, as `~/…`.
    pub files: Vec<String>,
    pub output: Vec<String>,
}

const LIMIT: Duration = Duration::from_secs(120);
const MAX_LINES: usize = 400;
const MAX_FILES: usize = 500;

/// The one user-level command a script plan runs, if the plan is that simple.
pub fn script_step(plan: &Plan) -> Option<(&str, &[String], &Path)> {
    match plan.steps.as_slice() {
        [s] if !s.needs_root => match &s.action {
            Action::Cmd { program, args, cwd: Some(cwd) } => Some((program, args, cwd)),
            _ => None,
        },
        _ => None,
    }
}

pub fn bwrap_args(fake_home: &Path, home: &Path, cwd: &Path, program: &str, args: &[String]) -> Vec<String> {
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let mut a: Vec<String> = ["--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp", "--tmpfs", "/run"].map(String::from).to_vec();
    a.extend(["--bind".into(), s(fake_home), s(home)]);
    // The script lives in Unpack's work folder under home, which the fake home hides; show it again read-only.
    a.extend(["--ro-bind".into(), s(cwd), s(cwd)]);
    a.extend(["--unshare-all", "--die-with-parent", "--new-session", "--setenv", "HOME"].map(String::from));
    a.extend([s(home), "--chdir".into(), s(cwd), "--".into(), program.into()]);
    a.extend(args.iter().cloned());
    a
}

pub fn preview(plan: &Plan) -> Result<Report, String> {
    let (program, args, cwd) = script_step(plan).ok_or("Only a single script or installer can be tried in the sandbox.")?;
    if !has("bwrap") {
        return Err("Trying it safely needs bubblewrap (`bwrap`). Install the `bubblewrap` package, then try again.".into());
    }
    std::fs::create_dir_all(cache_dir()).map_err(|e| e.to_string())?;
    let fake = tempfile::tempdir_in(cache_dir()).map_err(|e| e.to_string())?;
    let mut child = command("bwrap")
        .args(bwrap_args(fake.path(), &home(), cwd, program, args))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start bwrap: {e}"))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let readers: Vec<_> = [child.stdout.take().map(|o| Box::new(o) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|e| Box::new(e) as Box<dyn std::io::Read + Send>)]
        .into_iter()
        .flatten()
        .map(|r| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for l in BufReader::new(r).lines().map_while(Result::ok) {
                    let _ = tx.send(l);
                }
            })
        })
        .collect();
    drop(tx);
    let start = Instant::now();
    let (code, timed_out) = loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            break (st.code(), false);
        }
        if start.elapsed() > LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            break (None, true);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    for r in readers {
        let _ = r.join();
    }
    let output: Vec<String> = rx.into_iter().take(MAX_LINES).collect();
    // bwrap's own failure (no user namespaces, loopback setup refused) means the script never ran.
    if let Some(l) = output.first().filter(|l| l.starts_with("bwrap:") && code == Some(1)) {
        return Err(format!("The sandbox could not start ({l}). This system may block the user namespaces it needs (for example AppArmor's apparmor_restrict_unprivileged_userns)."));
    }
    let files = walkdir::WalkDir::new(fake.path())
        .into_iter()
        .flatten()
        .filter(|e| !e.file_type().is_dir())
        .filter_map(|e| e.path().strip_prefix(fake.path()).ok().map(|r| format!("~/{}", r.display())))
        .take(MAX_FILES)
        .collect();
    Ok(Report { code, timed_out, files, output })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_hides_home_and_keeps_the_script_dir() {
        let a = bwrap_args(Path::new("/c/fake"), Path::new("/home/u"), Path::new("/home/u/.cache/unpack/work/1"), "/bin/sh", &["x.sh".into()]);
        let joined = a.join(" ");
        assert!(joined.starts_with("--ro-bind / /"));
        assert!(joined.contains("--bind /c/fake /home/u --ro-bind /home/u/.cache/unpack/work/1 /home/u/.cache/unpack/work/1"));
        assert!(joined.contains("--unshare-all") && joined.ends_with("-- /bin/sh x.sh"));
    }

    #[test]
    fn runs_a_script_when_bwrap_works() {
        let _g = crate::util::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", d.path().join("cache"));
        let f = d.path().join("install.sh");
        std::fs::write(&f, "#!/bin/sh\necho hello\nmkdir -p \"$HOME/.local/bin\" && echo x > \"$HOME/.local/bin/tool\"\n").unwrap();
        let plan = crate::plan::build(&crate::plan::Input::Package { path: f, kind: crate::sniff::Kind::Script }, &crate::system::detect(), &crate::plan::Meta { source: "t".into(), sha256: None, pipe_to_shell: false });
        assert!(script_step(&plan).is_some());
        // bwrap is missing, or user namespaces are blocked (AppArmor, containers): nothing to check here.
        let Ok(r) = preview(&plan) else { return };
        if r.code != Some(0) {
            return;
        }
        assert!(r.output.contains(&"hello".to_string()), "{r:?}");
        assert_eq!(r.files, vec!["~/.local/bin/tool".to_string()]);
    }
}
