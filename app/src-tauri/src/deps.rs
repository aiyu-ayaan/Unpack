//! What a package install will pull in or remove, asked from the package manager in simulation mode before the
//! user confirms. Runs as the user; a manager that needs root even to simulate (zypper) gives no preview.
use crate::plan::{Action, Plan};
use crate::util::command;

#[derive(Debug, Default, PartialEq)]
pub struct Preview {
    pub installs: Vec<String>,
    pub removes: Vec<String>,
}

/// `Inst name (…)` and `Remv name […]` lines (apt-get -s).
pub fn parse_apt(out: &str) -> Preview {
    let mut p = Preview::default();
    for l in out.lines() {
        let mut w = l.split_whitespace();
        match (w.next(), w.next()) {
            (Some("Inst"), Some(n)) => p.installs.push(n.into()),
            (Some("Remv"), Some(n)) => p.removes.push(n.into()),
            _ => {}
        }
    }
    p
}

/// The transaction table of `dnf install --assumeno`: rows under `Installing…:` and `Removing…:` headers.
pub fn parse_dnf(out: &str) -> Preview {
    let mut p = Preview::default();
    let mut into: Option<bool> = None;
    for l in out.lines() {
        if l.starts_with("Transaction Summary") || l.starts_with("Transaction summary") {
            break;
        }
        if !l.starts_with(' ') {
            let h = l.trim_end_matches(':').to_lowercase();
            into = if h.starts_with("installing") || h.starts_with("upgrading") { Some(true) } else if h.starts_with("removing") || h.starts_with("downgrading") { Some(false) } else { None };
            continue;
        }
        let Some(name) = l.split_whitespace().next() else { continue };
        match into {
            Some(true) => p.installs.push(name.into()),
            Some(false) => p.removes.push(name.into()),
            None => {}
        }
    }
    p
}

/// `(1/3) Installing name (1.2-r0)` lines (apk add --simulate).
pub fn parse_apk(out: &str) -> Preview {
    let mut p = Preview::default();
    for l in out.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.as_slice() {
            [_, "Installing", n, ..] => p.installs.push(n.to_string()),
            [_, "Purging", n, ..] => p.removes.push(n.to_string()),
            _ => {}
        }
    }
    p
}

/// `name-1.2_1 install x86_64 <repo> …` lines (xbps-install -n).
pub fn parse_xbps(out: &str) -> Preview {
    let mut p = Preview::default();
    for l in out.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        let Some(name) = w.first().and_then(|nv| nv.rsplit_once('-')).map(|x| x.0.to_string()) else { continue };
        match w.get(1) {
            Some(&"install") => p.installs.push(name),
            Some(&"remove") => p.removes.push(name),
            _ => {}
        }
    }
    p
}

fn stdout(program: &str, args: &[String]) -> String {
    command(program).args(args).env("LC_ALL", "C").stdin(std::process::Stdio::null()).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// The simulation of one install command, if its manager has one that runs as the user.
fn simulate(program: &str, args: &[String]) -> Option<Preview> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let operands = |skip: &[&str]| -> Vec<String> { args.iter().filter(|a| !skip.contains(&a.as_str())).cloned().collect() };
    match (program, words.as_slice()) {
        ("env", [_, "apt-get", "install", ..]) => {
            let mut a = vec!["-s".to_string()];
            a.extend(args[1..].iter().skip(1).cloned());
            Some(parse_apt(&stdout("apt-get", &a)))
        }
        ("dnf" | "yum", ["install", ..]) => {
            let mut a: Vec<String> = ["-C", "install", "--assumeno"].map(String::from).to_vec();
            a.extend(operands(&["install", "-y"]));
            Some(parse_dnf(&stdout(program, &a)))
        }
        ("pacman", ["-S" | "-U", ..]) => {
            let mut a: Vec<String> = vec![words[0].into(), "--print".into(), "--print-format".into(), "%n".into()];
            a.extend(operands(&["-S", "-U", "--noconfirm", "--needed"]));
            Some(Preview { installs: stdout("pacman", &a).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect(), removes: vec![] })
        }
        ("apk", ["add", ..]) => {
            let mut a = vec!["add".to_string(), "--simulate".into()];
            a.extend(operands(&["add"]));
            Some(parse_apk(&stdout("apk", &a)))
        }
        ("xbps-install", ["-Sy", ..]) => {
            let mut a = vec!["-n".to_string()];
            a.extend(operands(&["-Sy"]));
            Some(parse_xbps(&stdout("xbps-install", &a)))
        }
        _ => None,
    }
}

/// Fill `plan.pulls` (other packages the install adds) and warn about packages it would remove.
pub fn preview(plan: &mut Plan) {
    if !plan.supported {
        return;
    }
    let found = plan.steps.iter().find_map(|s| match &s.action {
        Action::Cmd { program, args, .. } if s.needs_root => simulate(program, args),
        _ => None,
    });
    let Some(p) = found else { return };
    let own = |n: &String| *n == plan.name || plan.steps.iter().any(|s| s.display.ends_with(&format!(" {n}")));
    plan.pulls = p.installs.into_iter().filter(|n| !own(n)).collect();
    if !p.removes.is_empty() {
        plan.warnings.push(format!("Installing this also removes {} package{}: {}. Check that you don't need them.", p.removes.len(), if p.removes.len() > 1 { "s" } else { "" }, p.removes.join(", ")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_simulation() {
        let apt = parse_apt("NOTE: simulation\nInst libfoo1 (1.2 Debian:12 [amd64])\nInst vlc (3.0 Debian:12 [amd64])\nRemv old-vlc [2.0]\nConf libfoo1 (1.2)\n");
        assert_eq!(apt, Preview { installs: vec!["libfoo1".into(), "vlc".into()], removes: vec!["old-vlc".into()] });
        let dnf = parse_dnf("Dependencies resolved.\n====\n Package  Arch  Version  Repo  Size\n====\nInstalling:\n vlc  x86_64  3.0  rpmfusion  10 k\nInstalling dependencies:\n a52dec  x86_64  0.7  fedora  1 k\nInstalling weak dependencies:\n vlc-plugins  x86_64  3.0  r  1 k\nRemoving dependent packages:\n oldthing  x86_64  1  @System  1 k\n\nTransaction Summary\n====\nInstall  3 Packages\n");
        assert_eq!(dnf, Preview { installs: vec!["vlc".into(), "a52dec".into(), "vlc-plugins".into()], removes: vec!["oldthing".into()] });
        assert_eq!(parse_apk("(1/2) Installing libx (1.0-r0)\n(2/2) Installing vlc (3.0-r0)\nOK: 10 MiB in 20 packages\n").installs, ["libx", "vlc"]);
        assert_eq!(parse_xbps("libx-1.0_1 install x86_64 r 1 1\nvlc-3.0_1 install x86_64 r 1 1\nold-1_1 remove x86_64 r 1 1\n"), Preview { installs: vec!["libx".into(), "vlc".into()], removes: vec!["old".into()] });
    }

    #[test]
    fn previews_an_apt_install() {
        if !crate::util::has("apt-get") {
            return;
        }
        let mut p = Plan::new("apt", "zzz-no-such-package", "x");
        p.cmd("Install", true, "env", &["DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y", "zzz-no-such-package"], None);
        preview(&mut p);
        assert!(p.pulls.is_empty() && p.supported);
    }
}
