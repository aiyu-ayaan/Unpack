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

/// Flathub's popular apps, for the Store's front page.
pub fn popular() -> Result<Vec<Entry>, String> {
    let body = agent().get(&format!("{FLATHUB}/collection/popular?page=1&per_page=60")).call().map_err(|e| format!("Could not reach Flathub: {e}"))?;
    flathub_entries(&body.into_string().map_err(|e| e.to_string())?)
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

fn native_search(n: Native, q: &str) -> Vec<Entry> {
    let found = match n {
        Native::Apt => capture("apt-cache", &["search", "--names-only", q]).map(|o| parse_apt(&o)),
        Native::Dnf => capture(if has("dnf") { "dnf" } else { "yum" }, &["repoquery", "-q", "--latest-limit=1", "--qf", "%{name}\t%{summary}\n", &format!("*{q}*")]).map(|o| parse_tabbed("dnf", &o)),
        Native::Zypper => capture("zypper", &["--non-interactive", "--no-refresh", "search", q]).map(|o| parse_zypper(&o)),
        Native::Pacman => capture("pacman", &["-Ss", q]).map(|o| parse_pacman(&o)),
        Native::Apk => capture("apk", &["search", "-v", q]).map(|o| parse_apk(&o)),
    };
    let mut v = found.unwrap_or_default();
    v.dedup_by(|a, b| a.id == b.id);
    // Names that contain the word first, then the rest, as the manager listed them.
    let ql = q.to_lowercase();
    v.sort_by_key(|e| !e.id.to_lowercase().contains(&ql));
    v.truncate(NATIVE_MAX);
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
        (flat.join().unwrap_or_else(|_| Err("Flathub search stopped".into())), rest)
    });
    match flat {
        Ok(mut v) => {
            v.extend(rest);
            Ok(v)
        }
        Err(e) if rest.is_empty() => Err(e),
        Err(_) => Ok(rest),
    }
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
    }

    #[test]
    fn keeps_only_flathub_icons() {
        let v = flathub_entries(r#"{"hits":[{"app_id":"org.a.A","name":"A","summary":"s","icon":"https://dl.flathub.org/x.png"},{"app_id":"org.b.B","name":"B","icon":"https://evil.example/x.png"}]}"#).unwrap();
        assert_eq!(v[0].icon.as_deref(), Some("https://dl.flathub.org/x.png"));
        assert_eq!(v[1].icon, None);
        assert_eq!(v[1].summary, "");
    }
}
