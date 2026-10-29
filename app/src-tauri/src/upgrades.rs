//! What can be updated: packages the system's managers report as upgradable, and apps Unpack installed from a
//! GitHub release (the recorded link, or the update address an AppImage carries in its `.upd_info` section: a
//! GitHub release pattern or a plain `.zsync` link).
use crate::plan::{safe_ident, Plan, Track, Uninstall};
use crate::store::App;
use crate::system::{Native, System};
use crate::updates::Release;
use crate::util::{capture, command, has};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Upgrade {
    /// `apt`, `dnf`, `zypper`, `pacman`, `apk`, `flatpak`, `snap`, or `download` (a newer release file).
    pub manager: String,
    pub name: String,
    pub current: String,
    pub latest: String,
    /// Flatpak: the user installation rather than the system one.
    pub user: bool,
    /// The Unpack record this belongs to, if any.
    pub app_id: Option<i64>,
    /// The user said not to update this, or to skip this version (`hold`).
    pub held: bool,
}

/// One package the page picked to update, as `list` reported it.
#[derive(Deserialize, Debug)]
pub struct Pick {
    pub manager: String,
    pub name: String,
    #[serde(default)]
    pub user: bool,
}

fn up(manager: &str, name: &str, current: &str, latest: &str) -> Upgrade {
    Upgrade { manager: manager.into(), name: name.into(), current: current.into(), latest: latest.into(), user: false, app_id: None, held: false }
}

/// Whether `held` (from the settings) turns down `u`: the app always, or this version of it.
pub fn is_held(held: &[String], u: &Upgrade) -> bool {
    let key = format!("{}:{}", u.manager, u.name);
    held.iter().any(|h| *h == key || h.strip_prefix(&key).and_then(|r| r.strip_prefix('@')) == Some(u.latest.as_str()))
}

/// Never update `manager:name`, or (with `version`) skip that one version. Replaces what was there for it.
pub fn hold(manager: &str, name: &str, version: Option<&str>) -> Result<(), String> {
    let mut s = crate::updates::load();
    let key = format!("{manager}:{name}");
    s.held.retain(|h| h != &key && !h.starts_with(&format!("{key}@")));
    s.held.push(version.map_or(key.clone(), |v| format!("{key}@{v}")));
    crate::updates::save(&s)
}

/// Offer updates for `manager:name` again.
pub fn unhold(manager: &str, name: &str) -> Result<(), String> {
    let mut s = crate::updates::load();
    let key = format!("{manager}:{name}");
    s.held.retain(|h| h != &key && !h.starts_with(&format!("{key}@")));
    crate::updates::save(&s)
}

// ---- parsers -----------------------------------------------------------------------------------

/// `vlc/noble-updates 3.0.21-1 amd64 [upgradable from: 3.0.20-1]` (apt list --upgradable).
pub fn parse_apt(out: &str) -> Vec<Upgrade> {
    out.lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let name = w.next()?.split_once('/')?.0;
            let latest = w.next()?;
            let current = l.split_once("upgradable from: ").map(|x| x.1.trim_end_matches(']')).unwrap_or("");
            Some(up("apt", name, current, latest))
        })
        .collect()
}

/// `vlc.x86_64   3.0.21-1.fc42   updates` (dnf check-update).
pub fn parse_dnf(out: &str) -> Vec<Upgrade> {
    let mut v = vec![];
    for l in out.lines() {
        // A header like "Obsoleting packages" ends the list of updates.
        if l.starts_with("Obsoleting") || l.starts_with("Security:") {
            break;
        }
        let w: Vec<&str> = l.split_whitespace().collect();
        if let [na, ver, _repo] = w.as_slice() {
            if let Some((name, _arch)) = na.rsplit_once('.') {
                v.push(up("dnf", name, "", ver));
            }
        }
    }
    v
}

/// `v | repo | name | current | available | arch` rows (zypper list-updates).
pub fn parse_zypper(out: &str) -> Vec<Upgrade> {
    out.lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split('|').map(str::trim).collect();
            (c.len() >= 6 && c[0] == "v").then(|| up("zypper", c[2], c[3], c[4]))
        })
        .collect()
}

/// `name old -> new` (checkupdates, pacman -Qu).
pub fn parse_pacman(out: &str) -> Vec<Upgrade> {
    out.lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            match w.as_slice() {
                [n, old, "->", new, ..] => Some(up("pacman", n, old, new)),
                _ => None,
            }
        })
        .collect()
}

/// `name-1.2-r0` → (`name`, `1.2-r0`).
pub(crate) fn apk_split(nv: &str) -> Option<(&str, &str)> {
    let mut i = nv.rmatch_indices('-');
    let (_, _) = i.next()?;
    let (at, _) = i.next()?;
    Some((&nv[..at], &nv[at + 1..]))
}

/// `name-2.0-r0 x86_64 {origin} (MIT) [upgradable from: name-1.0-r0]` (apk list --upgradable).
pub fn parse_apk(out: &str) -> Vec<Upgrade> {
    out.lines()
        .filter_map(|l| {
            let (name, latest) = apk_split(l.split_whitespace().next()?)?;
            let current = l.split_once("upgradable from: ").and_then(|x| apk_split(x.1.trim_end_matches(']'))).map(|x| x.1).unwrap_or("");
            Some(up("apk", name, current, latest))
        })
        .collect()
}

/// `application<TAB>version` (flatpak remote-ls --updates).
fn parse_flatpak(out: &str, user: bool) -> Vec<Upgrade> {
    out.lines()
        .filter_map(|l| {
            let (id, ver) = l.split_once('\t').unwrap_or((l, ""));
            let id = id.trim();
            (!id.is_empty()).then(|| Upgrade { user, ..up("flatpak", id, "", ver.trim()) })
        })
        .collect()
}

/// `name-1.2_1 update x86_64 <repo> <size> <size>` (xbps-install -Mun).
pub fn parse_xbps(out: &str) -> Vec<Upgrade> {
    out.lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            let (name, latest) = w.first()?.rsplit_once('-')?;
            (w.get(1) == Some(&"update")).then(|| up("xbps", name, "", latest))
        })
        .collect()
}

/// `Name Version Rev Size Publisher Notes` (snap refresh --list).
pub fn parse_snap(out: &str) -> Vec<Upgrade> {
    out.lines()
        .skip(1)
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            Some(up("snap", w.next()?, "", w.next()?))
        })
        .collect()
}

/// stdout of a command whatever its exit code: `dnf check-update` exits 100 when there are updates.
fn stdout(program: &str, args: &[&str]) -> String {
    command(program).args(args).env("LC_ALL", "C").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// Upgradable packages every manager on this system reports. Lists come from each manager's last refresh.
pub fn system(sys: &System) -> Vec<Upgrade> {
    // A read-only base is updated as a whole by its own tool (rpm-ostree upgrade, transactional-update up).
    let mut v = match sys.native.filter(|_| !sys.immutable) {
        Some(Native::Apt) => parse_apt(&stdout("apt", &["list", "--upgradable"])),
        Some(Native::Dnf) => parse_dnf(&stdout(if has("dnf") { "dnf" } else { "yum" }, &["check-update", "-q"])),
        Some(Native::Zypper) => parse_zypper(&stdout("zypper", &["--non-interactive", "--no-refresh", "list-updates"])),
        // checkupdates (pacman-contrib) syncs a private copy of the database; plain -Qu reads the last sync.
        Some(Native::Pacman) => parse_pacman(&if has("checkupdates") { stdout("checkupdates", &[]) } else { stdout("pacman", &["-Qu"]) }),
        Some(Native::Apk) => parse_apk(&stdout("apk", &["list", "--upgradable"])),
        Some(Native::Xbps) => parse_xbps(&stdout("xbps-install", &["-Mun"])),
        // shortcut: Gentoo's update list needs a full dependency calculation (`emerge -puDN @world`, minutes); not listed.
        Some(Native::Portage) | None => vec![],
    };
    if has("flatpak") {
        for (flag, user) in [("--user", true), ("--system", false)] {
            v.extend(parse_flatpak(&capture("flatpak", &["remote-ls", flag, "--updates", "--app", "--columns=application,version"]).unwrap_or_default(), user));
        }
    }
    if has("snap") {
        v.extend(parse_snap(&capture("snap", &["refresh", "--list"]).unwrap_or_default()));
    }
    v
}

// ---- apps from GitHub releases -----------------------------------------------------------------

/// `(owner, repo, tag, file)` from `https://github.com/<owner>/<repo>/releases/download/<tag>/<file>`.
pub fn github_asset(url: &str) -> Option<(&str, &str, &str, &str)> {
    let rest = url.strip_prefix("https://github.com/")?;
    let p: Vec<&str> = rest.split('/').collect();
    match p.as_slice() {
        [o, r, "releases", "download", tag, file] if !file.is_empty() => Some((o, r, tag, file)),
        _ => None,
    }
}

/// `file` with digits collapsed, so `App-1.2.3-x86_64.AppImage` and `App-1.3.0-x86_64.AppImage` look alike.
fn shape(file: &str) -> String {
    let mut out = String::new();
    for c in file.chars() {
        if c.is_ascii_digit() {
            if !out.ends_with('#') {
                out.push('#');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The asset of a newer release that replaces `file` from release `old_tag`.
pub fn matching_asset<'a>(file: &str, old_tag: &str, new_tag: &str, names: &[&'a str]) -> Option<&'a str> {
    let (old_v, new_v) = (old_tag.trim_start_matches('v'), new_tag.trim_start_matches('v'));
    if !old_v.is_empty() && file.contains(old_v) {
        let want = file.replace(old_v, new_v);
        if let Some(n) = names.iter().find(|n| **n == want) {
            return Some(n);
        }
    }
    let s = shape(file);
    names.iter().find(|n| shape(n) == s).copied()
}

/// `*` matches any run of characters; nothing else is special. Enough for AppImage update patterns.
pub fn glob(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if parts.len() == 1 {
        return pattern == name;
    }
    if !name.starts_with(first) || !name[first.len()..].ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

/// The named section of a little-endian 64-bit ELF file (an AppImage's `.upd_info`), at most 4 KiB.
pub fn elf_section(path: &Path, want: &str) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = [0u8; 64];
    f.read_exact(&mut h).ok()?;
    if !h.starts_with(b"\x7fELF") || h[4] != 2 || h[5] != 1 {
        return None;
    }
    let shoff = u64::from_le_bytes(h[0x28..0x30].try_into().ok()?);
    let (shentsize, shnum, shstrndx) = (u16::from_le_bytes([h[0x3A], h[0x3B]]) as u64, u16::from_le_bytes([h[0x3C], h[0x3D]]) as u64, u16::from_le_bytes([h[0x3E], h[0x3F]]) as u64);
    if shentsize < 64 || shnum > 512 || shstrndx >= shnum {
        return None;
    }
    let mut read_at = |off: u64, len: u64| -> Option<Vec<u8>> {
        if len > 1 << 16 {
            return None;
        }
        let mut b = vec![0u8; len as usize];
        f.seek(SeekFrom::Start(off)).ok()?;
        f.read_exact(&mut b).ok()?;
        Some(b)
    };
    let headers = read_at(shoff, shentsize * shnum)?;
    let sec = |i: u64| {
        let s = &headers[(i * shentsize) as usize..];
        (u32::from_le_bytes(s[0..4].try_into().unwrap()) as usize, u64::from_le_bytes(s[0x18..0x20].try_into().unwrap()), u64::from_le_bytes(s[0x20..0x28].try_into().unwrap()))
    };
    let (_, str_off, str_len) = sec(shstrndx);
    let names = read_at(str_off, str_len)?;
    for i in 0..shnum {
        let (n, off, len) = sec(i);
        let name = names.get(n..)?.split(|b| *b == 0).next()?;
        if name == want.as_bytes() {
            return read_at(off, len.min(4096));
        }
    }
    None
}

/// The update address an AppImage carries in its `.upd_info` section.
#[derive(Debug, PartialEq)]
pub enum UpdInfo {
    /// `gh-releases-zsync|owner|repo|latest|App-*x86_64.AppImage.zsync`: the release asset matching the pattern.
    GitHub { owner: String, repo: String, tag: String, pattern: String },
    /// `zsync|https://example.com/App-latest-x86_64.AppImage.zsync`: a fixed `.zsync` file that names the newest build.
    Zsync(String),
}

pub fn parse_upd_info(text: &str) -> Option<UpdInfo> {
    let t = text.trim_end_matches('\0').trim();
    let p: Vec<&str> = t.split('|').collect();
    match p.as_slice() {
        ["gh-releases-zsync", o, r, tag, pat] => Some(UpdInfo::GitHub { owner: o.to_string(), repo: r.to_string(), tag: tag.to_string(), pattern: pat.trim_end_matches(".zsync").to_string() }),
        ["zsync", url] if url.starts_with("https://") => Some(UpdInfo::Zsync(url.to_string())),
        _ => None,
    }
}

/// `(SHA-1, file name, download link)` from a `.zsync` header (`Key: value` lines up to the first blank line).
/// `URL:` may be relative to the `.zsync` file.
pub fn parse_zsync(header: &str, zsync_url: &str) -> Option<(String, String, String)> {
    let get = |k: &str| header.lines().take_while(|l| !l.is_empty()).find_map(|l| l.strip_prefix(k)?.strip_prefix(':').map(|v| v.trim().to_string()));
    let (sha1, url) = (get("SHA-1")?, get("URL")?);
    let name = get("Filename").unwrap_or_else(|| url.rsplit('/').next().unwrap_or_default().to_string());
    let url = if url.contains("://") { url } else { format!("{}/{url}", zsync_url.rsplit_once('/')?.0) };
    Some((sha1.to_lowercase(), name, url))
}

/// The header of a `.zsync` file; the block checksums after it are not needed.
fn zsync_header(url: &str) -> Result<String, String> {
    let r = ureq::get(url).set("User-Agent", concat!("Unpack/", env!("CARGO_PKG_VERSION"))).timeout(std::time::Duration::from_secs(15)).call().map_err(|e| e.to_string())?;
    let mut buf = vec![];
    r.into_reader().take(16 * 1024).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn release(owner: &str, repo: &str, tag: &str) -> Result<Release, String> {
    if ![owner, repo, tag].iter().all(|s| safe_ident(s)) {
        return Err("Bad release address".into());
    }
    let which = if tag == "latest" { "latest".to_string() } else { format!("tags/{tag}") };
    let body = ureq::get(&format!("https://api.github.com/repos/{owner}/{repo}/releases/{which}"))
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", concat!("Unpack/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .map_err(|e| format!("Could not reach GitHub: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| format!("Unexpected answer from GitHub: {e}"))
}

/// A newer release file for an app Unpack installed: `(current, latest version, download link)`.
pub fn newer_download(app: &App) -> Option<(String, String, String)> {
    if let Some((o, r, tag, file)) = app.source.as_deref().and_then(github_asset) {
        let rel = release(o, r, "latest").ok()?;
        if rel.tag_name == tag {
            return None;
        }
        let names: Vec<&str> = rel.assets.iter().map(|a| a.name.as_str()).collect();
        let pick = matching_asset(file, tag, &rel.tag_name, &names)?;
        let url = rel.assets.iter().find(|a| a.name == pick)?.browser_download_url.clone();
        return Some((tag.to_string(), rel.tag_name, url));
    }
    let Uninstall::Files { paths } = &app.uninstall else { return None };
    let image = paths.iter().map(Path::new).find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("appimage")))?;
    let have = image.file_name()?.to_string_lossy().into_owned();
    let current = app.version.clone().unwrap_or_else(|| have.clone());
    match parse_upd_info(&String::from_utf8_lossy(&elf_section(image, ".upd_info")?))? {
        UpdInfo::GitHub { owner, repo, tag, pattern } => {
            let rel = release(&owner, &repo, &tag).ok()?;
            let asset = rel.assets.iter().find(|a| glob(&pattern, &a.name))?;
            (asset.name != have).then(|| (current, rel.tag_name, asset.browser_download_url.clone()))
        }
        // The .zsync names one build; it is newer when its checksum is not this file's.
        UpdInfo::Zsync(url) => {
            let (sha1, name, link) = parse_zsync(&zsync_header(&url).ok()?, &url)?;
            let mine = capture("sha1sum", &[&image.to_string_lossy()]).ok()?;
            (mine.split_whitespace().next()? != sha1 && link.starts_with("https://")).then_some((current, name, link))
        }
    }
}

/// Everything that can be updated, with Unpack's own records marked. Asks GitHub once per tracked download.
pub fn list(apps: &[App], sys: &System) -> Vec<Upgrade> {
    let live: Vec<&App> = apps.iter().filter(|a| a.status == "installed").collect();
    let (mut v, downloads) = std::thread::scope(|s| {
        let found: Vec<_> = live.iter().map(|a| s.spawn(move || newer_download(a).map(|n| (a, n)))).collect();
        let v = system(sys);
        (v, found.into_iter().filter_map(|h| h.join().ok().flatten()).collect::<Vec<_>>())
    });
    for u in &mut v {
        u.app_id = live.iter().find(|a| matches!(&a.uninstall, Uninstall::Manager { ident, .. } if *ident == u.name)).map(|a| a.id);
    }
    v.extend(downloads.into_iter().map(|(a, (cur, new, _))| Upgrade { app_id: Some(a.id), ..up("download", &a.name, &cur, &new) }));
    let held = crate::updates::load().held;
    for u in &mut v {
        u.held = is_held(&held, u);
    }
    v.sort_by_key(|u| (u.app_id.is_none(), u.manager.clone(), u.name.to_lowercase()));
    v
}

/// One plan that updates the picked packages: every root step in one admin prompt.
pub fn plan(picks: &[Pick], sys: &System) -> Plan {
    let mut p = Plan::new("update", "Updates", "package managers");
    p.track = Track::Nothing;
    if let Some(bad) = picks.iter().find(|k| !safe_ident(&k.name)) {
        p.refuse(&format!("Refusing unsafe package name: {:?}", bad.name), &[]);
        return p;
    }
    let names = |m: &str, user: Option<bool>| -> Vec<&str> { picks.iter().filter(|k| k.manager == m && user.is_none_or(|u| k.user == u)).map(|k| k.name.as_str()).collect() };
    let native = sys.native.filter(|_| !sys.immutable).map(crate::plan::manager_name);
    for m in ["apt", "dnf", "zypper", "pacman", "apk", "xbps"] {
        let n = names(m, None);
        if n.is_empty() {
            continue;
        }
        if native != Some(m) {
            p.refuse(&format!("{m} is not this system's package manager."), &[]);
            return p;
        }
        match m {
            "apt" => {
                p.cmd("Refresh package lists", true, "env", &["DEBIAN_FRONTEND=noninteractive", "apt-get", "update"], None);
                let mut a = vec!["DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "--only-upgrade", "-y"];
                a.extend(&n);
                p.cmd("Update packages", true, "env", &a, None);
            }
            "dnf" => p.cmd("Update packages", true, if has("dnf") { "dnf" } else { "yum" }, &[&["upgrade", "-y"][..], &n].concat(), None),
            "zypper" => p.cmd("Update packages", true, "zypper", &[&["--non-interactive", "update"][..], &n].concat(), None),
            "pacman" => {
                // Arch supports only whole-system upgrades; updating one package alone can break others.
                p.warnings.push("Arch Linux updates the whole system at once, so every package with an update will be updated.".into());
                p.cmd("Update the system", true, "pacman", &["-Syu", "--noconfirm"], None);
            }
            "xbps" => p.cmd("Update packages", true, "xbps-install", &[&["-Suy"][..], &n].concat(), None),
            _ => p.cmd("Update packages", true, "apk", &[&["upgrade"][..], &n].concat(), None),
        }
    }
    for (user, flag) in [(true, "--user"), (false, "--system")] {
        let n = names("flatpak", Some(user));
        if !n.is_empty() {
            p.cmd("Update Flatpak apps", !user, "flatpak", &[&["update", flag, "-y", "--noninteractive"][..], &n].concat(), None);
        }
    }
    let n = names("snap", None);
    if !n.is_empty() {
        p.cmd("Update snaps", true, "snap", &[&["refresh"][..], &n].concat(), None);
    }
    if p.steps.is_empty() && p.supported {
        p.refuse("Nothing to update.", &[]);
    }
    let count = picks.len();
    p.name = if count == 1 { picks[0].name.clone() } else { format!("{count} updates") };
    p.title = format!("Update {}", p.name);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_manager() {
        assert_eq!(parse_apt("Listing...\nvlc/noble-updates 3.0.21-1 amd64 [upgradable from: 3.0.20-1]\n"), vec![up("apt", "vlc", "3.0.20-1", "3.0.21-1")]);
        assert_eq!(parse_dnf("\nvlc.x86_64   1:3.0.21-1.fc42   updates\nObsoleting packages\nold.x86_64 1 x\n"), vec![up("dnf", "vlc", "", "1:3.0.21-1.fc42")]);
        assert_eq!(parse_zypper("S | Repository | Name | Current | Available | Arch\n--+--\nv | repo-oss | vlc | 3.0.20 | 3.0.21 | x86_64\n"), vec![up("zypper", "vlc", "3.0.20", "3.0.21")]);
        assert_eq!(parse_pacman("vlc 3.0.20-1 -> 3.0.21-1\n"), vec![up("pacman", "vlc", "3.0.20-1", "3.0.21-1")]);
        assert_eq!(parse_apk("lib-foo-2.0-r0 x86_64 {foo} (MIT) [upgradable from: lib-foo-1.0-r3]\n"), vec![up("apk", "lib-foo", "1.0-r3", "2.0-r0")]);
        assert_eq!(parse_snap("Name Version Rev Size Publisher Notes\nvlc 3.0.21 3777 300MB videolan -\n"), vec![up("snap", "vlc", "", "3.0.21")]);
        assert!(parse_flatpak("org.a.A\t1.2\n", true)[0].user);
        assert_eq!(parse_xbps("vlc-3.0.21_2 update x86_64 https://repo-default.voidlinux.org/current 1MB 2MB\nfoo-1_1 install x86_64 r 1 1\n"), vec![up("xbps", "vlc", "", "3.0.21_2")]);
    }

    #[test]
    fn holds_an_app_or_one_version() {
        let u = up("apt", "vlc", "1", "2");
        assert!(is_held(&["apt:vlc".into()], &u));
        assert!(is_held(&["apt:vlc@2".into()], &u));
        assert!(!is_held(&["apt:vlc@3".into(), "snap:vlc".into(), "apt:vlc-bin".into()], &u));
    }

    #[test]
    fn finds_the_next_release_file() {
        assert_eq!(github_asset("https://github.com/o/r/releases/download/v1.2.3/App-1.2.3-x86_64.AppImage"), Some(("o", "r", "v1.2.3", "App-1.2.3-x86_64.AppImage")));
        assert_eq!(github_asset("https://example.com/o/r/releases/download/v1/x"), None);
        let names = ["App-1.3.0-aarch64.AppImage", "App-1.3.0-x86_64.AppImage", "App-1.3.0.tar.gz"];
        assert_eq!(matching_asset("App-1.2.3-x86_64.AppImage", "v1.2.3", "v1.3.0", &names), Some("App-1.3.0-x86_64.AppImage"));
        // Version not in the file name: fall back to the same shape.
        assert_eq!(matching_asset("app_amd64.deb", "nightly-5", "nightly-6", &["app_arm64.deb", "app_amd64.deb"]), Some("app_amd64.deb"));
        assert_eq!(matching_asset("tool.deb", "1", "2", &["tool.rpm"]), None);
    }

    #[test]
    fn globs_and_update_info() {
        assert!(glob("Subsurface-*x86_64.AppImage", "Subsurface-5.0-x86_64.AppImage"));
        assert!(!glob("Subsurface-*x86_64.AppImage", "Subsurface-5.0-aarch64.AppImage"));
        assert!(glob("a*b*c", "a1b2c") && !glob("a*b*c", "ac") && glob("x", "x"));
        assert_eq!(parse_upd_info("gh-releases-zsync|o|r|latest|A-*x86_64.AppImage.zsync\0\0"), Some(UpdInfo::GitHub { owner: "o".into(), repo: "r".into(), tag: "latest".into(), pattern: "A-*x86_64.AppImage".into() }));
        assert_eq!(parse_upd_info("zsync|https://x/A.AppImage.zsync"), Some(UpdInfo::Zsync("https://x/A.AppImage.zsync".into())));
        assert_eq!(parse_upd_info("zsync|http://x/A.AppImage.zsync"), None);
        let head = "zsync: 0.6.2\nFilename: A-2.AppImage\nSHA-1: ABC123\nURL: A-2.AppImage\n\nURL: ignored";
        assert_eq!(parse_zsync(head, "https://x/d/A.AppImage.zsync"), Some(("abc123".into(), "A-2.AppImage".into(), "https://x/d/A-2.AppImage".into())));
        assert_eq!(parse_zsync("SHA-1: a\nURL: https://y/B\n", "https://x/A.zsync").unwrap().2, "https://y/B");
    }

    #[test]
    fn reads_an_elf_section() {
        let d = tempfile::tempdir().unwrap();
        let info = d.path().join("info");
        std::fs::write(&info, "gh-releases-zsync|o|r|latest|A-*.AppImage.zsync").unwrap();
        let out = d.path().join("a.AppImage");
        let made = std::process::Command::new("objcopy").arg(format!("--add-section=.upd_info={}", info.display())).arg("/bin/true").arg(&out).status();
        if !made.is_ok_and(|s| s.success()) {
            return; // no binutils here
        }
        let got = elf_section(&out, ".upd_info").unwrap();
        assert!(matches!(parse_upd_info(&String::from_utf8_lossy(&got)), Some(UpdInfo::GitHub { pattern, .. }) if pattern == "A-*.AppImage"));
        assert!(elf_section(&out, ".nope").is_none());
    }

    #[test]
    fn plans_one_admin_prompt_and_refuses_bad_names() {
        let mut sys = crate::system::detect();
        sys.native = Some(Native::Apt);
        let pick = |m: &str, n: &str, user: bool| Pick { manager: m.into(), name: n.into(), user };
        let p = plan(&[pick("apt", "vlc", false), pick("flatpak", "org.a.A", true), pick("snap", "x", false)], &sys);
        assert!(p.supported, "{:?}", p.warnings);
        assert_eq!(p.track, Track::Nothing);
        assert!(p.steps.iter().any(|s| s.display.contains("--only-upgrade -y vlc")));
        assert!(p.steps.iter().any(|s| !s.needs_root && s.display.contains("flatpak update --user")));
        assert!(!plan(&[pick("apt", "-o=x", false)], &sys).supported);
        assert!(!plan(&[pick("dnf", "vlc", false)], &sys).supported);
    }
}
