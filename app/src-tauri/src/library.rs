//! Read-only scan of what package managers report as installed.
use crate::util::{capture, has};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SysPkg {
    pub name: String,
    pub version: String,
    pub manager: String,
}

fn pkg(m: &str, name: &str, version: &str) -> SysPkg {
    SysPkg { name: name.into(), version: version.into(), manager: m.into() }
}

/// Lines of "name<TAB>version".
fn tabbed(m: &str, out: &str) -> Vec<SysPkg> {
    out.lines().filter_map(|l| {
        let (n, v) = l.split_once('\t').unwrap_or((l, ""));
        let n = n.trim();
        (!n.is_empty()).then(|| pkg(m, n, v.trim()))
    }).collect()
}

pub fn parse_snap(out: &str) -> Vec<SysPkg> {
    out.lines().skip(1).filter_map(|l| {
        let mut w = l.split_whitespace();
        Some(pkg("snap", w.next()?, w.next().unwrap_or("")))
    }).collect()
}

pub fn parse_pacman(out: &str) -> Vec<SysPkg> {
    out.lines().filter_map(|l| {
        let (n, v) = l.split_once(' ')?;
        Some(pkg("pacman", n, v))
    }).collect()
}

pub fn scan() -> Vec<SysPkg> {
    let mut all = vec![];
    if has("apt-mark") && has("dpkg-query") {
        let manual: std::collections::HashSet<String> = capture("apt-mark", &["showmanual"]).unwrap_or_default().lines().map(String::from).collect();
        let vers: HashMap<String, String> = tabbed("apt", &capture("dpkg-query", &["-W", "-f", "${Package}\t${Version}\n"]).unwrap_or_default()).into_iter().map(|p| (p.name, p.version)).collect();
        all.extend(manual.iter().map(|n| pkg("apt", n, vers.get(n).map(String::as_str).unwrap_or(""))));
    }
    if has("dnf") {
        if let Ok(o) = capture("dnf", &["repoquery", "--userinstalled", "--qf", "%{name}\t%{version}-%{release}\n", "-q"]) {
            all.extend(tabbed("dnf", &o));
        }
    } else if has("zypper") && has("rpm") {
        if let Ok(o) = capture("rpm", &["-qa", "--qf", "%{NAME}\t%{VERSION}-%{RELEASE}\n"]) {
            all.extend(tabbed("zypper", &o));
        }
    }
    if has("pacman") {
        if let Ok(o) = capture("pacman", &["-Qe"]) {
            all.extend(parse_pacman(&o));
        }
    }
    if has("apk") {
        if let Ok(o) = std::fs::read_to_string("/etc/apk/world") {
            all.extend(o.lines().filter(|l| !l.trim().is_empty()).map(|n| pkg("apk", n.trim(), "")));
        }
    }
    if has("snap") {
        if let Ok(o) = capture("snap", &["list"]) {
            all.extend(parse_snap(&o));
        }
    }
    if has("flatpak") {
        if let Ok(o) = capture("flatpak", &["list", "--app", "--columns=application,version"]) {
            all.extend(tabbed("flatpak", &o));
        }
    }
    all.sort_by_key(|a| a.name.to_lowercase());
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers() {
        let s = parse_snap("Name Version Rev Tracking Publisher Notes\nvlc 3.0 1 latest/stable videolan -\n");
        assert_eq!(s, vec![pkg("snap", "vlc", "3.0")]);
        assert_eq!(parse_pacman("firefox 130.0-1\n"), vec![pkg("pacman", "firefox", "130.0-1")]);
        assert_eq!(tabbed("apt", "a\t1\n\nb\n"), vec![pkg("apt", "a", "1"), pkg("apt", "b", "")]);
    }
}
