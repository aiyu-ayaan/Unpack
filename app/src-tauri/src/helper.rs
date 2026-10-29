//! The narrow privileged helper. When Unpack is installed as a system package, a polkit action
//! (`dev.unpack.app.helper`, see `polkit/`) lets `pkexec /usr/bin/unpack helper <json>` run as root with its
//! own message. The helper takes a list of commands, checks every one against a fixed allowlist of package
//! manager operations, and runs them without a shell. Anything else is refused before the first one runs.
//! Plans with root steps outside that list (a script run as administrator) still use `pkexec sh -c`.
use crate::plan::safe_ident;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

pub const POLICY: &str = "/usr/share/polkit-1/actions/dev.unpack.app.helper.policy";
pub const EXE: &str = "/usr/bin/unpack";

/// The policy is installed and this process is the binary it names (no version skew with another copy).
pub fn usable() -> bool {
    Path::new(POLICY).is_file() && std::env::current_exe().is_ok_and(|e| e == Path::new(EXE))
}

fn plain_abs(s: &str) -> bool {
    let p = Path::new(s);
    p.is_absolute() && !s.chars().any(char::is_control) && !p.components().any(|c| matches!(c, Component::ParentDir))
}

/// A package name or a local file to install.
fn operand(s: &str) -> bool {
    safe_ident(s) || plain_abs(s)
}

/// What each program may be asked: `(subcommands, flags)`. Every other argument must be an operand.
fn rules(program: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    Some(match program {
        "apt-get" => (&["install", "remove", "update", "build-dep"], &["-y", "--only-upgrade"]),
        "dnf" | "yum" => (&["install", "remove", "upgrade"], &["-y"]),
        "zypper" => (&["install", "remove", "update"], &["--non-interactive", "--allow-unsigned-rpm"]),
        "pacman" => (&[], &["-U", "-S", "-R", "-Syu", "--noconfirm", "--needed"]),
        "apk" => (&["add", "del", "upgrade"], &["--allow-untrusted"]),
        "snap" => (&["install", "remove", "refresh"], &["--dangerous", "--classic"]),
        "flatpak" => (&["uninstall", "update"], &["--system", "-y", "--noninteractive"]),
        "rpm-ostree" => (&["install", "uninstall"], &["--idempotent", "--apply-live"]),
        "transactional-update" => (&["pkg", "install", "remove"], &["--non-interactive", "--allow-unsigned-rpm"]),
        _ => return None,
    })
}

/// Root-owned leftovers `Files` removal may delete: inside /opt or /usr/local, never those folders themselves.
fn removable(s: &str) -> bool {
    plain_abs(s) && ["/opt/", "/usr/local/"].iter().any(|r| s.starts_with(r) && s.len() > r.len())
}

pub fn allowed(c: &Cmd) -> Result<(), String> {
    let refuse = || Err(format!("not an allowed admin command: {} {}", c.program, c.args.join(" ")));
    if let Some(d) = &c.cwd {
        // Only `apt-get build-dep ./` runs in a folder (the unpacked source).
        if !(plain_abs(d) && c.args.iter().any(|a| a == "build-dep")) {
            return refuse();
        }
    }
    let (program, args): (&str, &[String]) = match (c.program.as_str(), c.args.as_slice()) {
        ("env", [first, prog, rest @ ..]) if first == "DEBIAN_FRONTEND=noninteractive" && prog == "apt-get" => ("apt-get", rest),
        ("rm", [rf, dashes, paths @ ..]) if rf == "-rf" && dashes == "--" && !paths.is_empty() => {
            return if paths.iter().all(|p| removable(p)) { Ok(()) } else { refuse() };
        }
        (p, a) => (p, a),
    };
    let Some((subs, flags)) = rules(program) else { return refuse() };
    let mut seen_sub = 0;
    for a in args {
        let ok = if subs.contains(&a.as_str()) {
            seen_sub += 1;
            true
        } else {
            flags.contains(&a.as_str()) || operand(a) || (a == "./" && c.cwd.is_some())
        };
        if !ok {
            return refuse();
        }
    }
    // transactional-update takes `pkg install`; everything else at most one subcommand.
    let max = if program == "transactional-update" { 2 } else { 1 };
    if seen_sub > max || (!subs.is_empty() && seen_sub == 0) {
        return refuse();
    }
    Ok(())
}

/// `unpack helper <json>`, run by pkexec as root. Exit 0 when every command succeeded.
pub fn main(json: &str) -> i32 {
    let cmds: Vec<Cmd> = match serde_json::from_str(json) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("unpack helper: bad request: {e}");
            return 2;
        }
    };
    if let Some(e) = cmds.iter().find_map(|c| allowed(c).err()) {
        eprintln!("unpack helper: {e}");
        return 2;
    }
    for c in &cmds {
        let mut cmd = std::process::Command::new(&c.program);
        cmd.args(&c.args).env("DEBIAN_FRONTEND", "noninteractive");
        if let Some(d) = &c.cwd {
            cmd.current_dir(d);
        }
        match cmd.status() {
            Ok(s) if s.success() => {}
            Ok(s) => return s.code().unwrap_or(1),
            Err(e) => {
                eprintln!("unpack helper: {}: {e}", c.program);
                return 1;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(program: &str, args: &[&str]) -> Cmd {
        Cmd { program: program.into(), args: args.iter().map(|s| s.to_string()).collect(), cwd: None }
    }

    #[test]
    fn allows_what_plans_make() {
        for (p, a) in [
            ("env", &["DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y", "/home/u/.cache/unpack/work/1/x.deb"][..]),
            ("env", &["DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "--only-upgrade", "-y", "vlc", "libc6"]),
            ("env", &["DEBIAN_FRONTEND=noninteractive", "apt-get", "update"]),
            ("dnf", &["install", "-y", "fuse-libs"]),
            ("zypper", &["--non-interactive", "install", "--allow-unsigned-rpm", "/tmp/a.rpm"]),
            ("pacman", &["-U", "--noconfirm", "/tmp/a.pkg.tar.zst"]),
            ("pacman", &["-Syu", "--noconfirm"]),
            ("apk", &["add", "--allow-untrusted", "/tmp/a.apk"]),
            ("snap", &["install", "--dangerous", "--classic", "/tmp/a.snap"]),
            ("flatpak", &["uninstall", "--system", "-y", "org.a.A"]),
            ("rpm-ostree", &["install", "--idempotent", "--apply-live", "vlc"]),
            ("transactional-update", &["--non-interactive", "pkg", "install", "vlc"]),
            ("rm", &["-rf", "--", "/opt/app", "/usr/local/bin/tool"]),
        ] {
            assert!(allowed(&cmd(p, a)).is_ok(), "{p} {a:?}");
        }
        let mut bd = cmd("env", &["DEBIAN_FRONTEND=noninteractive", "apt-get", "build-dep", "-y", "./"]);
        bd.cwd = Some("/home/u/.cache/unpack/work/1/src".into());
        assert!(allowed(&bd).is_ok());
    }

    #[test]
    fn refuses_everything_else() {
        for (p, a) in [
            ("sh", &["-c", "id"][..]),
            ("env", &["DEBIAN_FRONTEND=noninteractive", "sh", "-c", "id"]),
            ("env", &["LD_PRELOAD=/x.so", "apt-get", "install", "vlc"]),
            ("apt-get", &["-o", "APT::Update::Pre-Invoke::=id", "update"]),
            ("apt-get", &["install", "-y", "--allow-unauthenticated", "vlc"]),
            ("dnf", &["install", "-y", "--setopt=x=y", "vlc"]),
            ("dnf", &["-y", "vlc"]),
            ("dnf", &["install", "remove", "vlc"]),
            ("pacman", &["-U", "--config", "/tmp/evil.conf", "/tmp/a.pkg.tar.zst"]),
            ("rpm", &["-i", "/tmp/a.rpm"]),
            ("rm", &["-rf", "--", "/opt"]),
            ("rm", &["-rf", "--", "/etc/passwd"]),
            ("rm", &["-rf", "--", "/opt/../etc"]),
            ("apt-get", &["install", "/tmp/../etc/x.deb"]),
        ] {
            assert!(allowed(&cmd(p, a)).is_err(), "{p} {a:?}");
        }
        let mut c = cmd("dnf", &["install", "-y", "vlc"]);
        c.cwd = Some("/tmp".into());
        assert!(allowed(&c).is_err(), "only build-dep runs in a folder");
        assert_eq!(main("not json"), 2);
        assert_eq!(main(r#"[{"program":"sh","args":["-c","id"]}]"#), 2);
    }
}
