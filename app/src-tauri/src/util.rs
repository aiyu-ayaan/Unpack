use std::path::{Path, PathBuf};
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

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

/// `~/.local/share/unpack`
pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("unpack")
}

/// `~/.cache/unpack`
pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("unpack")
}

/// Run a command to completion and return stdout when it exits 0.
pub fn capture(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
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
