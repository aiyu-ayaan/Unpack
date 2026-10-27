use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use std::process::Command;

/// Dirs a GUI-launched process may be missing from PATH.
const EXTRA_PATH: [&str; 3] = ["/usr/sbin", "/sbin", "/usr/local/sbin"];

pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    let found = path
        .split(':')
        .chain(EXTRA_PATH)
        .map(|d| Path::new(d).join(bin))
        .find(|p| p.is_file());
    found
}

pub fn has(bin: &str) -> bool {
    which(bin).is_some()
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

/// Debug builds are the dev flavor: own data and cache dirs, so a build run from source never touches the
/// database or settings of the installed app.
const APP_DIR: &str = if cfg!(debug_assertions) { "unpack-dev" } else { "unpack" };

/// `~/.local/share/unpack` (`unpack-dev` in debug builds)
pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join(APP_DIR)
}

/// `~/.cache/unpack` (`unpack-dev` in debug builds)
pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join(APP_DIR)
}

/// Variables the AppImage points at its bundled libraries and data. Handed on, they make host tools
/// (`dpkg-deb`, `apt-get`, `xdg-open`, an installed AppImage) load the wrong libraries and fail.
const BUNDLE_VARS: [&str; 9] = [
    "LD_LIBRARY_PATH", "PATH", "XDG_DATA_DIRS", "GTK_PATH", "GIO_EXTRA_MODULES", "GDK_PIXBUF_MODULE_FILE", "GDK_PIXBUF_MODULEDIR",
    "GI_TYPELIB_PATH", "GSETTINGS_SCHEMA_DIR",
];

/// `value` without the entries inside `appdir`; `None` when nothing is left.
fn without_appdir(value: &str, appdir: &str) -> Option<String> {
    let kept: Vec<&str> = value.split(':').filter(|p| !p.is_empty() && !p.starts_with(appdir)).collect();
    (!kept.is_empty()).then(|| kept.join(":"))
}

/// A `Command` for a program on the host. Use this, not `Command::new`, for everything the app runs.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut c = Command::new(program);
    if let Some(appdir) = std::env::var_os("APPDIR") {
        let appdir = appdir.to_string_lossy();
        for var in BUNDLE_VARS {
            if let Ok(v) = std::env::var(var) {
                match without_appdir(&v, &appdir) {
                    Some(kept) => c.env(var, kept),
                    None => c.env_remove(var),
                };
            }
        }
    }
    c
}

/// Run a command to completion and return stdout when it exits 0.
pub fn capture(program: &str, args: &[&str]) -> Result<String, String> {
    let out = command(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("{program} failed: {}", err.trim()))
    }
}

/// `capture` for a long-running program (`git clone`): it is killed when `keep_going` returns false.
pub fn capture_until(program: &str, args: &[&str], keep_going: &dyn Fn() -> bool) -> Result<(), String> {
    use std::io::Read;
    let mut child = command(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let mut pipe = child.stderr.take().expect("piped");
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = pipe.read_to_string(&mut s);
        s
    });
    loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            let err = err.join().unwrap_or_default();
            return if st.success() { Ok(()) } else { Err(format!("{program} failed: {}", err.trim())) };
        }
        if !keep_going() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(crate::fetch::CANCELLED.into());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Shell-quoted display form of a command.
pub fn display(program: &str, args: &[String]) -> String {
    let mut all = vec![program.to_string()];
    all.extend(args.iter().cloned());
    shlex::try_join(all.iter().map(String::as_str)).unwrap_or_else(|_| all.join(" "))
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Tests that change HOME must hold this so others reading it don't race.
#[cfg(test)]
pub static TEST_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::without_appdir;

    #[test]
    fn bundle_entries_are_dropped_and_host_entries_kept() {
        let v = "/tmp/.mount_X/usr/lib/:/tmp/.mount_X/lib64/:/usr/local/lib";
        assert_eq!(without_appdir(v, "/tmp/.mount_X").as_deref(), Some("/usr/local/lib"));
        assert_eq!(without_appdir("/tmp/.mount_X/usr/lib/:", "/tmp/.mount_X"), None);
        assert_eq!(without_appdir("/usr/bin:/bin", "/tmp/.mount_X").as_deref(), Some("/usr/bin:/bin"));
    }
}
