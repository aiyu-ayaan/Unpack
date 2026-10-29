//! The Store: apps to browse and search, from Flathub and from the system's own package sources.
//! Read only. Installing goes through `plan::plan_repo`, which checks the id again.
use crate::system::{Native, System};
use crate::util::{capture, has};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Entry {
    /// `flathub`, `snap`, or the native manager (`apt`, `dnf`…): what `plan::plan_repo` takes.
    pub source: String,
    pub id: String,
    pub name: String,
    pub summary: String,
    /// An `https://dl.flathub.org/` image, or none.
    pub icon: Option<String>,
}

const FLATHUB: &str = "https://flathub.org/api/v2";
const ICONS: &str = "https://dl.flathub.org/";
/// Native search answers are long (every library matching the word); the first ones are enough.
const NATIVE_MAX: usize = 40;

#[derive(Deserialize)]
struct Hits {
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    app_id: String,
    name: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    icon: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(20)).user_agent(concat!("Unpack/", env!("CARGO_PKG_VERSION"))).build()
}

fn flathub_entries(body: &str) -> Result<Vec<Entry>, String> {
    let h: Hits = serde_json::from_str(body).map_err(|e| format!("Unexpected answer from Flathub: {e}"))?;
    Ok(h.hits
        .into_iter()
        .map(|h| Entry {
            source: "flathub".into(),
            id: h.app_id,
            name: h.name,
            summary: h.summary.unwrap_or_default(),
            // The page only loads images from Flathub's own media host (see the CSP).
            icon: h.icon.filter(|i| i.starts_with(ICONS)),
        })
        .collect())
}

fn flathub_get(path: &str) -> Result<String, String> {
    agent().get(&format!("{FLATHUB}/{path}")).call().map_err(|e| format!("Could not reach Flathub: {e}"))?.into_string().map_err(|e| e.to_string())
}

/// Desktop apps most people look for, offered from the native repositories on the front page when they are there.
const POPULAR_NATIVE: [&str; 12] = ["firefox", "vlc", "gimp", "libreoffice", "thunderbird", "inkscape", "krita", "obs-studio", "audacity", "blender", "keepassxc", "transmission-gtk"];

/// Flathub entries plus `rest`; an error only when there is nothing at all.
fn with_flathub(flat: Result<Vec<Entry>, String>, rest: Vec<Entry>) -> Result<Vec<Entry>, String> {
    match flat {
        Ok(mut v) => {
            v.extend(rest);
            Ok(v)
        }
        Err(e) if rest.is_empty() => Err(e),
        Err(_) => Ok(rest),
    }
}

/// The Store's front page: Flathub's popular apps, the Snap Store's featured ones, and well-known apps from the
/// native repositories.
pub fn popular(sys: &System) -> Result<Vec<Entry>, String> {
    let (flat, rest) = std::thread::scope(|s| {
        let flat = s.spawn(|| flathub_entries(&flathub_get("collection/popular?page=1&per_page=60")?));
        let snaps = s.spawn(|| if has("snap") { capture("snap", &["find", "--section=featured"]).map(|o| parse_snap(&o)).unwrap_or_default() } else { vec![] });
        let native: Vec<_> = sys.native.map(|n| POPULAR_NATIVE.map(|name| s.spawn(move || native_search(n, name).into_iter().find(|e| e.id == name))).into_iter().collect()).unwrap_or_default();
        let mut rest: Vec<Entry> = snaps.join().unwrap_or_default().into_iter().take(NATIVE_MAX).collect();
        rest.extend(native.into_iter().filter_map(|h| h.join().ok().flatten()));
        (flat.join().unwrap_or_else(|_| Err("Flathub stopped".into())), rest)
    });
    with_flathub(flat, rest)
}

/// Store categories: Flathub's name, the label, and the Snap Store section that matches, if any.
pub const CATEGORIES: [(&str, &str, Option<&str>); 10] = [
    ("AudioVideo", "Audio & Video", Some("photo-and-video")),
    ("Development", "Development", Some("development")),
    ("Education", "Education", Some("education")),
    ("Game", "Games", Some("games")),
    ("Graphics", "Graphics", Some("art-and-design")),
    ("Network", "Internet", Some("social")),
    ("Office", "Office", Some("productivity")),
    ("Science", "Science", Some("science")),
    ("System", "System", None),
    ("Utility", "Utilities", Some("utilities")),
];

/// One category's apps from Flathub and the Snap Store.
pub fn category(name: &str) -> Result<Vec<Entry>, String> {
    let (_, _, section) = CATEGORIES.iter().find(|c| c.0 == name).ok_or("Unknown category")?;
    let (flat, rest) = std::thread::scope(|s| {
        let flat = s.spawn(|| flathub_entries(&flathub_get(&format!("collection/category/{name}?page=1&per_page=60"))?));
        let rest = match section {
            Some(sec) if has("snap") => capture("snap", &["find", &format!("--section={sec}")]).map(|o| parse_snap(&o)).unwrap_or_default().into_iter().take(NATIVE_MAX).collect(),
            _ => vec![],
        };
        (flat.join().unwrap_or_else(|_| Err("Flathub stopped".into())), rest)
    });
    with_flathub(flat, rest)
}

/// What the Store shows about one app before it is installed.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Detail {
    pub description: String,
    /// `https://dl.flathub.org/` images.
    pub screenshots: Vec<String>,
    /// What the app may reach, in plain words. Empty when the source does not say.
    pub permissions: Vec<String>,
    pub developer: Option<String>,
    pub license: Option<String>,
    pub download_size: Option<u64>,
    pub installed_size: Option<u64>,
}

#[derive(Deserialize, Default)]
struct Appstream {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    screenshots: Vec<Shot>,
    #[serde(default)]
    developer_name: Option<String>,
    #[serde(default)]
    project_license: Option<String>,
}

#[derive(Deserialize)]
struct Shot {
    #[serde(default)]
    sizes: Vec<ShotSize>,
}

#[derive(Deserialize)]
struct ShotSize {
    src: String,
    #[serde(default)]
    width: Option<String>,
}

#[derive(Deserialize, Default)]
struct Summary {
    #[serde(default)]
    download_size: Option<u64>,
    #[serde(default)]
    installed_size: Option<u64>,
    #[serde(default)]
    metadata: Option<SummaryMeta>,
}

#[derive(Deserialize, Default)]
struct SummaryMeta {
    #[serde(default)]
    permissions: Option<Permissions>,
}

#[derive(Deserialize, Default)]
struct Permissions {
    #[serde(default)]
    shared: Vec<String>,
    #[serde(default)]
    sockets: Vec<String>,
    #[serde(default)]
    devices: Vec<String>,
    #[serde(default)]
    filesystems: Vec<String>,
}

/// AppStream description markup (`<p>`, `<ul><li>`) as plain text.
pub fn plain_text(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else { break };
        match rest[i + 1..i + j].trim_start_matches('/').split_whitespace().next().unwrap_or("") {
            "li" if !rest[i + 1..].starts_with('/') => out.push_str("\n• "),
            "p" | "ul" | "ol" | "li" | "br" => out.push('\n'),
            _ => {}
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    let out = out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'");
    let lines: Vec<String> = out.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    let mut text = lines.join("\n");
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    text.replace("\n\n• ", "\n• ").trim().to_string()
}

/// A Flatpak's sandbox holes in plain words, most far-reaching first.
fn permission_words(p: &Permissions) -> Vec<String> {
    let mut v: Vec<String> = vec![];
    for f in &p.filesystems {
        let (place, ro) = f.split_once(':').map_or((f.as_str(), false), |(a, m)| (a, m == "ro"));
        let what = match place {
            "host" => "all your files".to_string(),
            "host-os" => "the system's programs and libraries".into(),
            "host-etc" => "the system's settings (/etc)".into(),
            "home" => "your home folder".into(),
            p if p.starts_with("xdg-download") => "your Downloads folder".into(),
            p if p.starts_with("xdg-documents") => "your Documents folder".into(),
            p if p.starts_with("xdg-pictures") => "your Pictures folder".into(),
            p if p.starts_with("xdg-music") => "your Music folder".into(),
            p if p.starts_with("xdg-videos") => "your Videos folder".into(),
            p if p.starts_with("xdg-run/") || p.starts_with("xdg-config/") || p.starts_with("xdg-data/") || p.starts_with("xdg-cache/") => continue,
            p => format!("files in {p}"),
        };
        v.push(format!("{} {what}", if ro { "Can read" } else { "Can read and change" }));
    }
    if p.devices.iter().any(|d| d == "all") {
        v.push("Can use all devices (webcam, USB, game controllers)".into());
    }
    if p.shared.iter().any(|s| s == "network") {
        v.push("Can use the internet".into());
    }
    for s in &p.sockets {
        match s.as_str() {
            "x11" => v.push("Uses X11, where apps can see each other's windows and keystrokes".into()),
            "pulseaudio" => v.push("Can play and record sound".into()),
            "session-bus" => v.push("Can talk to every app and service in your session".into()),
            "system-bus" => v.push("Can talk to system services".into()),
            "ssh-auth" => v.push("Can use your SSH keys".into()),
            "cups" => v.push("Can print".into()),
            "pcsc" => v.push("Can use smart cards".into()),
            _ => {}
        }
    }
    if v.is_empty() {
        v.push("No special permissions: it stays in its sandbox".into());
    }
    v
}

fn flathub_detail(id: &str) -> Result<Detail, String> {
    let (app, sum) = std::thread::scope(|s| {
        let sum = s.spawn(|| flathub_get(&format!("summary/{id}")).ok().and_then(|b| serde_json::from_str::<Summary>(&b).ok()).unwrap_or_default());
        let app = flathub_get(&format!("appstream/{id}")).and_then(|b| serde_json::from_str::<Appstream>(&b).map_err(|e| format!("Unexpected answer from Flathub: {e}")));
        (app, sum.join().unwrap_or_default())
    });
    let app = app?;
    // The size nearest 752 px wide, which fits the page without scaling much.
    let width = |z: &ShotSize| z.width.as_deref().and_then(|w| w.parse::<i64>().ok()).map_or(i64::MAX, |w| (w - 752).abs());
    let screenshots = app.screenshots.iter().filter_map(|s| s.sizes.iter().filter(|z| z.src.starts_with(ICONS)).min_by_key(|z| width(z)).map(|z| z.src.clone())).take(8).collect();
    Ok(Detail {
        description: app.description.as_deref().map(plain_text).unwrap_or_default(),
        screenshots,
        permissions: sum.metadata.and_then(|m| m.permissions).map(|p| permission_words(&p)).unwrap_or_default(),
        developer: app.developer_name,
        license: app.project_license,
        download_size: sum.download_size,
        installed_size: sum.installed_size,
    })
}

/// The `Description` field of a package manager's info output, with its indented continuation lines
/// (`apt-cache show`, `dnf info`, `zypper info`, `pacman -Si`, `snap info`).
pub fn description_field(out: &str) -> String {
    let mut lines = out.lines().skip_while(|l| !l.to_lowercase().starts_with("description"));
    let Some(first) = lines.next() else { return String::new() };
    let mut text = vec![first.split_once(':').map(|x| x.1).unwrap_or("").trim().trim_start_matches('|').trim().to_string()];
    for l in lines.take_while(|l| l.starts_with(char::is_whitespace)) {
        let t = l.trim().trim_start_matches(':').trim();
        text.push(if t == "." { String::new() } else { t.to_string() });
    }
    text.join("\n").trim().to_string()
}

/// Details for a Store entry; everything but Flathub has a description only.
pub fn detail(source: &str, id: &str, sys: &System) -> Result<Detail, String> {
    if !crate::plan::safe_ident(id) {
        return Err("Unknown app".into());
    }
    let info = |prog: &str, args: &[&str]| Detail { description: capture(prog, args).map(|o| description_field(&o)).unwrap_or_default(), ..Detail::default() };
    Ok(match source {
        "flathub" => flathub_detail(id)?,
        "snap" => info("snap", &["info", id]),
        _ => match sys.native.filter(|n| crate::plan::manager_name(*n) == source) {
            Some(Native::Apt) => info("apt-cache", &["show", "--no-all-versions", id]),
            Some(Native::Dnf) => info(if has("dnf") { "dnf" } else { "yum" }, &["info", "-q", id]),
            Some(Native::Zypper) => info("zypper", &["--non-interactive", "--no-refresh", "info", id]),
            Some(Native::Pacman) => info("pacman", &["-Si", id]),
            _ => Detail::default(),
        },
    })
}

fn flathub_search(q: &str) -> Result<Vec<Entry>, String> {
    let body = agent().post(&format!("{FLATHUB}/search")).set("Content-Type", "application/json").send_string(&serde_json::json!({ "query": q }).to_string()).map_err(|e| format!("Could not reach Flathub: {e}"))?;
    flathub_entries(&body.into_string().map_err(|e| e.to_string())?)
}

fn entry(source: &str, id: &str, summary: &str) -> Entry {
    Entry { source: source.into(), id: id.into(), name: id.into(), summary: summary.trim().into(), icon: None }
}

/// `name - summary` (apt-cache search).
pub fn parse_apt(out: &str) -> Vec<Entry> {
    out.lines().filter_map(|l| l.split_once(" - ").map(|(n, s)| entry("apt", n.trim(), s))).collect()
}

/// `name<TAB>summary` (our repoquery format).
fn parse_tabbed(source: &str, out: &str) -> Vec<Entry> {
    out.lines().filter_map(|l| l.split_once('\t').map(|(n, s)| entry(source, n.trim(), s))).collect()
}

/// `repo/name version [installed]` then an indented summary line (pacman -Ss).
pub fn parse_pacman(out: &str) -> Vec<Entry> {
    let mut v = vec![];
    let mut lines = out.lines().peekable();
    while let Some(l) = lines.next() {
        if l.starts_with(' ') {
            continue;
        }
        let Some(name) = l.split_whitespace().next().and_then(|n| n.split_once('/')).map(|x| x.1) else { continue };
        let summary = lines.next_if(|n| n.starts_with(' ')).unwrap_or("");
        v.push(entry("pacman", name, summary));
    }
    v
}

/// `S | Name | Summary | Type` table rows (zypper search).
pub fn parse_zypper(out: &str) -> Vec<Entry> {
    out.lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split('|').map(str::trim).collect();
            (c.len() >= 4 && c[3] == "package").then(|| entry("zypper", c[1], c[2]))
        })
        .collect()
}

/// `name-1.2-r0 - summary` (apk search -v).
pub fn parse_apk(out: &str) -> Vec<Entry> {
    out.lines()
        .filter_map(|l| {
            let (nv, s) = l.split_once(" - ")?;
            let mut parts = nv.rsplitn(3, '-');
            let (_rel, _ver, name) = (parts.next()?, parts.next()?, parts.next()?);
            Some(entry("apk", name, s))
        })
        .collect()
}

/// `Name Version Publisher Notes Summary` table (snap find).
pub fn parse_snap(out: &str) -> Vec<Entry> {
    out.lines()
        .skip(1)
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let name = w.next()?;
            let summary: Vec<&str> = w.skip(3).collect();
            Some(entry("snap", name, &summary.join(" ")))
        })
        .collect()
}

/// `[-] name-1.2_1   summary` (xbps-query -Rs).
pub fn parse_xbps(out: &str) -> Vec<Entry> {
    out.lines()
        .filter_map(|l| {
            let rest = l.get(4..)?.trim_start();
            let (nv, s) = rest.split_once(char::is_whitespace)?;
            Some(entry("xbps", nv.rsplit_once('-')?.0, s))
        })
        .collect()
}

/// `*  cat/name` then an indented `Description:` line (emerge --search). The name alone is what emerge takes.
pub fn parse_emerge(out: &str) -> Vec<Entry> {
    let mut v: Vec<Entry> = vec![];
    for l in out.lines() {
        if let Some(atom) = l.strip_prefix("*  ") {
            let name = atom.trim().split(' ').next().unwrap_or_default();
            v.push(entry("emerge", name.rsplit('/').next().unwrap_or(name), ""));
        } else if let (Some(d), Some(last)) = (l.trim().strip_prefix("Description:"), v.last_mut()) {
            last.summary = d.trim().into();
        }
    }
    v
}

fn native_search(n: Native, q: &str) -> Vec<Entry> {
    let found = match n {
        Native::Apt => capture("apt-cache", &["search", "--names-only", q]).map(|o| parse_apt(&o)),
        Native::Dnf => capture(if has("dnf") { "dnf" } else { "yum" }, &["repoquery", "-q", "--latest-limit=1", "--qf", "%{name}\t%{summary}\n", &format!("*{q}*")]).map(|o| parse_tabbed("dnf", &o)),
        Native::Zypper => capture("zypper", &["--non-interactive", "--no-refresh", "search", q]).map(|o| parse_zypper(&o)),
        Native::Pacman => capture("pacman", &["-Ss", q]).map(|o| parse_pacman(&o)),
        Native::Apk => capture("apk", &["search", "-v", q]).map(|o| parse_apk(&o)),
        Native::Xbps => capture("xbps-query", &["-Rs", q]).map(|o| parse_xbps(&o)),
        Native::Portage => capture("emerge", &["--search", "--color=n", q]).map(|o| parse_emerge(&o)),
    };
    let mut v = found.unwrap_or_default();
    v.dedup_by(|a, b| a.id == b.id);
    // Names that contain the word first, then the rest, as the manager listed them.
    let ql = q.to_lowercase();
    v.sort_by_key(|e| !e.id.to_lowercase().contains(&ql));
    v.truncate(NATIVE_MAX);
    v
}

/// A configured Flatpak source.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Remote {
    pub name: String,
    pub url: String,
    /// In the user's own installation (no admin password to use or remove).
    pub user: bool,
}

/// `name<TAB>url<TAB>options` (flatpak remotes --columns=name,url,options).
pub fn parse_remotes(out: &str) -> Vec<Remote> {
    out.lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            let name = c.first()?.trim();
            (!name.is_empty()).then(|| Remote { name: name.into(), url: c.get(1).unwrap_or(&"").trim().into(), user: c.get(2).is_some_and(|o| o.split(',').any(|x| x.trim() == "user")) })
        })
        .collect()
}

pub fn remotes() -> Vec<Remote> {
    if !has("flatpak") {
        return vec![];
    }
    capture("flatpak", &["remotes", "--columns=name,url,options"]).map(|o| parse_remotes(&o)).unwrap_or_default()
}

/// `application<TAB>name<TAB>description<TAB>remotes` (flatpak search). Flathub rows are left to its own API.
pub fn parse_flatpak_search(out: &str) -> Vec<Entry> {
    let mut v = vec![];
    for l in out.lines() {
        let c: Vec<&str> = l.split('\t').map(str::trim).collect();
        let [id, name, summary, remotes] = c.as_slice() else { continue };
        for r in remotes.split(',').map(str::trim).filter(|r| !r.is_empty() && *r != "flathub") {
            v.push(Entry { source: format!("flatpak:{r}"), id: id.to_string(), name: name.to_string(), summary: summary.to_string(), icon: None });
        }
    }
    v
}

/// Search every source this system has. Flathub needs the network; the others ask local tools.
/// A source that fails is left out; only "nothing could be asked" is an error.
pub fn search(q: &str, sys: &System) -> Result<Vec<Entry>, String> {
    let q = q.trim();
    if q.is_empty() || q.len() > 100 || q.starts_with('-') {
        return Err("Type a few letters of an app's name.".into());
    }
    let (flat, rest) = std::thread::scope(|s| {
        let flat = s.spawn(|| flathub_search(q));
        let mut rest = sys.native.map(|n| native_search(n, q)).unwrap_or_default();
        if has("snap") {
            rest.extend(capture("snap", &["find", q]).map(|o| parse_snap(&o)).unwrap_or_default().into_iter().take(NATIVE_MAX));
        }
        // Other Flatpak sources the user added, searched in their local AppStream copy.
        if remotes().iter().any(|r| r.name != "flathub") {
            rest.extend(capture("flatpak", &["search", "--columns=application,name,description,remotes", q]).map(|o| parse_flatpak_search(&o)).unwrap_or_default().into_iter().take(NATIVE_MAX));
        }
        (flat.join().unwrap_or_else(|_| Err("Flathub search stopped".into())), rest)
    });
    with_flathub(flat, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_manager() {
        assert_eq!(parse_apt("vlc - multimedia player\nvlc-bin - binaries\n")[0], entry("apt", "vlc", "multimedia player"));
        let p = parse_pacman("extra/vlc 3.0.21-1 [installed]\n    Multi-platform player\ncore/zlib 1.3-1\n    Compression\n");
        assert_eq!(p, vec![entry("pacman", "vlc", "Multi-platform player"), entry("pacman", "zlib", "Compression")]);
        assert_eq!(parse_zypper("S | Name | Summary | Type\n--+------+---------+--------\n  | vlc  | Player  | package\n  | vlc  | src     | srcpackage\n"), vec![entry("zypper", "vlc", "Player")]);
        assert_eq!(parse_apk("vlc-3.0.21-r0 - player\nlibvlc-dev-3.0-r1 - dev\n")[1].id, "libvlc-dev");
        assert_eq!(parse_snap("Name Version Publisher Notes Summary\nvlc 3.0 videolan✓ - The ultimate player\n"), vec![entry("snap", "vlc", "The ultimate player")]);
        assert_eq!(parse_tabbed("dnf", "vlc\tPlayer\n"), vec![entry("dnf", "vlc", "Player")]);
        assert_eq!(parse_xbps("[-] vlc-3.0.21_1           Free and portable multimedia player\n[*] lib-x-1.0_2 x\n"), vec![entry("xbps", "vlc", "Free and portable multimedia player"), entry("xbps", "lib-x", "x")]);
        assert_eq!(parse_emerge("*  media-video/vlc\n      Latest version available: 3.0\n      Description:   VLC media player\n*  media-video/vlc-x [ Masked ]\n"), vec![entry("emerge", "vlc", "VLC media player"), entry("emerge", "vlc-x", "")]);
    }

    #[test]
    fn details_in_plain_words() {
        assert_eq!(plain_text("<p>\n  VLC plays &amp; streams.\n</p>\n<ul><li>Fast</li><li>Free</li></ul><p>End</p>"), "VLC plays & streams.\n• Fast\n• Free\n\nEnd");
        let p = Permissions { shared: vec!["network".into(), "ipc".into()], sockets: vec!["x11".into(), "wayland".into()], devices: vec!["dri".into()], filesystems: vec!["xdg-download".into(), "home:ro".into(), "xdg-run/gvfs".into()] };
        assert_eq!(permission_words(&p), ["Can read and change your Downloads folder", "Can read your home folder", "Can use the internet", "Uses X11, where apps can see each other's windows and keystrokes"]);
        assert_eq!(permission_words(&Permissions::default()).len(), 1);
        assert_eq!(description_field("Package: vlc\nDescription-en: multimedia player\n VLC plays files.\n .\n It streams.\nHomepage: x\n"), "multimedia player\nVLC plays files.\n\nIt streams.");
        assert_eq!(description_field("Name : vlc\nDescription  : The player\n             : for all.\n"), "The player\nfor all.");
        assert_eq!(description_field("name: vlc\ndescription: |\n  Plays everything.\ncommands:\n"), "Plays everything.");
    }

    #[test]
    fn flatpak_sources() {
        assert_eq!(parse_remotes("flathub\thttps://dl.flathub.org/repo/\tsystem\nfedora\toci+https://registry.fedoraproject.org\tuser,disabled\n"), vec![
            Remote { name: "flathub".into(), url: "https://dl.flathub.org/repo/".into(), user: false },
            Remote { name: "fedora".into(), url: "oci+https://registry.fedoraproject.org".into(), user: true },
        ]);
        let found = parse_flatpak_search("org.a.A\tA\tAn app\tflathub,kde\norg.b.B\tB\tOnly flathub\tflathub\n");
        assert_eq!(found, vec![Entry { source: "flatpak:kde".into(), id: "org.a.A".into(), name: "A".into(), summary: "An app".into(), icon: None }]);
    }

    #[test]
    fn keeps_only_flathub_icons() {
        let v = flathub_entries(r#"{"hits":[{"app_id":"org.a.A","name":"A","summary":"s","icon":"https://dl.flathub.org/x.png"},{"app_id":"org.b.B","name":"B","icon":"https://evil.example/x.png"}]}"#).unwrap();
        assert_eq!(v[0].icon.as_deref(), Some("https://dl.flathub.org/x.png"));
        assert_eq!(v[1].icon, None);
        assert_eq!(v[1].summary, "");
    }
}
