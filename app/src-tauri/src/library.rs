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
    if has("xbps-query") {
        if let Ok(o) = capture("xbps-query", &["-m"]) {
            all.extend(o.lines().filter_map(|l| l.trim().rsplit_once('-')).map(|(n, v)| pkg("xbps", n, v)));
        }
    }
    if has("emerge") {
        // The world set: `cat/name` or `cat/name:slot`; emerge takes the name alone.
        if let Ok(o) = std::fs::read_to_string("/var/lib/portage/world") {
            all.extend(o.lines().filter_map(|l| l.trim().split(':').next()?.rsplit('/').next()).filter(|n| !n.is_empty()).map(|n| pkg("emerge", n, "")));
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

/// `24.9 MB`, `1.2 GiB`, `4567KB`, `3.6\u{a0}kB` as bytes: `k`/`M`/`G` are powers of 1000, `Ki`/`Mi`/`Gi` of 1024.
pub fn parse_size(text: &str) -> Option<u64> {
    let t = text.trim().replace('\u{a0}', " ");
    let at = t.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(t.len());
    let n: f64 = t[..at].parse().ok()?;
    let unit = t[at..].trim().to_lowercase();
    let mul = match unit.trim_end_matches('b') {
        "" => 1.0,
        "k" => 1e3,
        "m" => 1e6,
        "g" => 1e9,
        "ki" => 1024.0,
        "mi" => 1048576.0,
        "gi" => 1073741824.0,
        _ => return None,
    };
    Some((n * mul) as u64)
}

/// Bytes of files and folders on disk, not following links.
fn du(paths: &[String]) -> u64 {
    paths.iter().flat_map(|p| walkdir::WalkDir::new(p).follow_links(false).into_iter().flatten()).filter_map(|e| e.metadata().ok()).filter(|m| !m.is_dir()).map(|m| m.len()).sum()
}

/// `application<TAB>size<TAB>installation` (flatpak list --app --columns=application,size,installation).
pub fn parse_flatpak_sizes(out: &str) -> HashMap<(String, bool), u64> {
    out.lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            Some(((c.first()?.trim().to_string(), c.get(2)?.trim() == "user"), parse_size(c.get(1)?)?))
        })
        .collect()
}

/// The disk space one tracked app takes, when the way it was installed can say.
fn size_of(app: &crate::store::App, flatpaks: &HashMap<(String, bool), u64>) -> Option<u64> {
    use crate::plan::Uninstall;
    let first = |out: String| out.lines().map(str::trim).find(|l| !l.is_empty()).map(String::from);
    let field = |out: String, key: &str| out.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim().to_string()));
    match &app.uninstall {
        Uninstall::Files { paths } => Some(du(paths)),
        Uninstall::Manager { manager, ident, user } if crate::plan::safe_ident(ident) => match manager.as_str() {
            "apt" => capture("dpkg-query", &["-W", "-f", "${Installed-Size}", ident]).ok()?.trim().parse::<u64>().ok().map(|k| k * 1024),
            "dnf" | "zypper" | "rpm-ostree" | "transactional-update" => capture("rpm", &["-q", "--qf", "%{SIZE}", ident]).ok()?.trim().parse().ok(),
            "pacman" => parse_size(&field(capture("pacman", &["-Qi", ident]).ok()?, "Installed Size")?),
            "apk" => parse_size(capture("apk", &["info", "-s", ident]).ok()?.lines().nth(1)?),
            "xbps" => parse_size(&first(capture("xbps-query", &["-p", "installed_size", ident]).ok()?)?),
            "flatpak" => flatpaks.get(&(ident.clone(), *user)).copied(),
            "snap" => {
                let prefix = format!("{ident}_");
                let all = std::fs::read_dir("/var/lib/snapd/snaps").ok()?.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&prefix)).filter_map(|e| e.metadata().ok()).map(|m| m.len());
                Some(all.sum())
            }
            _ => None,
        },
        _ => None,
    }
}

/// Disk space of each installed tracked app, by id; apps whose size can't be told are left out.
pub fn sizes(apps: &[crate::store::App]) -> HashMap<i64, u64> {
    let live: Vec<&crate::store::App> = apps.iter().filter(|a| a.status == "installed").collect();
    let any_flatpak = live.iter().any(|a| matches!(&a.uninstall, crate::plan::Uninstall::Manager { manager, .. } if manager == "flatpak"));
    let flatpaks = if any_flatpak { parse_flatpak_sizes(&capture("flatpak", &["list", "--app", "--columns=application,size,installation"]).unwrap_or_default()) } else { HashMap::new() };
    std::thread::scope(|s| {
        let found: Vec<_> = live.iter().map(|a| (a.id, s.spawn(|| size_of(a, &flatpaks)))).collect();
        found.into_iter().filter_map(|(id, h)| Some((id, h.join().ok()??))).collect()
    })
}

/// What an app is called across formats: `org.videolan.VLC` and the `vlc` package are both `vlc`.
pub fn app_key(manager: &str, ident: &str) -> String {
    let base = if manager == "flatpak" { ident.rsplit('.').next().unwrap_or(ident) } else { ident };
    base.to_lowercase()
}

/// Whether the native package manager has `name` installed.
fn native_has(native: crate::system::Native, name: &str) -> bool {
    use crate::system::Native;
    match native {
        Native::Apt => capture("dpkg-query", &["-W", "-f", "${Status}", name]).is_ok_and(|s| s.ends_with(" installed")),
        Native::Dnf | Native::Zypper => capture("rpm", &["-q", name]).is_ok(),
        Native::Pacman => capture("pacman", &["-Q", name]).is_ok(),
        Native::Apk => capture("apk", &["info", "-e", name]).is_ok_and(|o| !o.trim().is_empty()),
        Native::Xbps => capture("xbps-query", &[name]).is_ok(),
        Native::Portage => false,
    }
}

/// The other formats the app `ident` (installed with `manager`) is already installed as, in words.
pub fn installed_elsewhere(manager: &str, ident: &str, sys: &crate::system::System) -> Vec<String> {
    let key = app_key(manager, ident);
    if key.len() < 2 || !crate::plan::safe_ident(&key) {
        return vec![];
    }
    let mut found = vec![];
    if manager != "flatpak" && has("flatpak") {
        let apps = capture("flatpak", &["list", "--app", "--columns=application"]).unwrap_or_default();
        if apps.lines().any(|a| app_key("flatpak", a.trim()) == key) {
            found.push("a Flatpak".to_string());
        }
    }
    if manager != "snap" && has("snap") && capture("snap", &["list", &key]).is_ok() {
        found.push("a snap".into());
    }
    if let Some(n) = sys.native.filter(|n| crate::plan::manager_name(*n) != manager) {
        if native_has(n, &key) {
            found.push(format!("a system package ({})", crate::plan::manager_name(n)));
        }
    }
    found
}

/// Warn when the app a plan installs is already here in another format.
pub fn duplicate_warning(plan: &mut crate::plan::Plan, sys: &crate::system::System) {
    let crate::plan::Uninstall::Manager { manager, ident, .. } = &plan.uninstall else { return };
    if !plan.supported || plan.track == crate::plan::Track::Nothing {
        return;
    }
    let other = installed_elsewhere(manager, ident, sys);
    if !other.is_empty() {
        plan.warnings.push(format!("{} is already installed as {}. Two copies take twice the space and both show up in your menu; you can remove one in the Library.", plan.name, other.join(" and ")));
    }
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
        assert_eq!(app_key("flatpak", "org.videolan.VLC"), "vlc");
        assert_eq!(parse_size("24.9\u{a0}MB"), Some(24_900_000));
        assert_eq!(parse_size("12.5 MiB"), Some(13_107_200));
        assert_eq!(parse_size("4567KB"), Some(4_567_000));
        assert_eq!(parse_size("3 bytes"), None);
        assert_eq!(parse_size("100"), Some(100));
        let f = parse_flatpak_sizes("org.a.A\t24.9\u{a0}MB\tuser\norg.b.B\t1\u{a0}GB\tsystem\n");
        assert_eq!((f[&("org.a.A".to_string(), true)], f[&("org.b.B".to_string(), false)]), (24_900_000, 1_000_000_000));
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("app")).unwrap();
        std::fs::write(d.path().join("app/bin"), [0u8; 10]).unwrap();
        let db = crate::store::Store::memory().unwrap();
        let id = db.add_app("t", None, "x", "x", None, &crate::plan::Uninstall::Files { paths: vec![d.path().join("app").to_string_lossy().into()] }).unwrap();
        db.add_app("m", None, "x", "x", None, &crate::plan::Uninstall::Traced).unwrap();
        assert_eq!(sizes(&db.list().unwrap()), HashMap::from([(id, 10)]));
        assert_eq!(app_key("apt", "VLC"), "vlc");
    }
}
