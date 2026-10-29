//! Self-update from GitHub releases on a channel (alpha / beta / stable).
//!
//! Release tags are `0.2.0-alpha.1`, `0.2.0-beta.1`, `0.2.0` (a leading `v` is fine). A channel also takes every
//! steadier one: beta sees beta and stable, alpha sees all. An AppImage replaces its own file and restarts; a
//! `.deb` or `.rpm` is downloaded to the Downloads folder and handed to the desktop's installer.

use crate::util;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const REPO_URL: &str = "https://github.com/aiyu-ayaan/Unpack";
const RELEASES: &str = "https://api.github.com/repos/aiyu-ayaan/Unpack/releases?per_page=30";
/// The only place an update may be downloaded from.
const DOWNLOADS: &str = "https://github.com/aiyu-ayaan/Unpack/releases/download/";
const MAX_UPDATE: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Alpha,
    Beta,
    Stable,
}

/// (major, minor, patch, stage, pre-release number). A stable release sorts above all of its pre-releases.
type Key = (u64, u64, u64, Channel, u64);

fn version_key(text: &str) -> Option<Key> {
    let t = text.trim().trim_start_matches('v');
    let (core, pre) = match t.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (t, None),
    };
    let mut nums = core.split('.').map(|n| n.parse::<u64>().ok());
    let (major, minor, patch) = (nums.next()??, nums.next()??, nums.next()??);
    if nums.next().is_some() {
        return None;
    }
    let (stage, n) = match pre {
        None => (Channel::Stable, u64::MAX),
        Some(p) => {
            let (name, num) = p.split_once('.')?;
            let stage = match name {
                "alpha" => Channel::Alpha,
                "beta" => Channel::Beta,
                _ => return None,
            };
            (stage, num.parse().ok()?)
        }
    };
    Some((major, minor, patch, stage, n))
}

/// The channel a build belongs to: whoever installed an alpha wants alphas.
pub fn channel_of(version: &str) -> Channel {
    version_key(version).map_or(Channel::Stable, |k| k.3)
}

#[derive(Deserialize)]
pub struct Asset {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
}

#[derive(Deserialize)]
pub struct Release {
    pub(crate) tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    pub(crate) assets: Vec<Asset>,
}

#[derive(Clone, Serialize, PartialEq, Debug)]
pub struct Offer {
    pub version: String,
    pub url: String,
    pub notes: Option<String>,
}

/// The newest release on `channel` that is newer than `installed` and has an asset ending in `suffix`.
pub fn pick(releases: &[Release], installed: &str, channel: Channel, suffix: &str) -> Option<Offer> {
    let current = version_key(installed)?;
    let mut best: Option<(Key, &Release, &Asset)> = None;
    for r in releases {
        let Some(key) = version_key(&r.tag_name) else { continue };
        let Some(asset) = r.assets.iter().find(|a| a.name.ends_with(suffix)) else { continue };
        if key.3 < channel || key <= best.map_or(current, |b| b.0) {
            continue;
        }
        best = Some((key, r, asset));
    }
    best.map(|(_, r, a)| Offer {
        version: r.tag_name.trim_start_matches('v').to_string(),
        url: a.browser_download_url.clone(),
        notes: r.body.as_deref().map(str::trim).filter(|b| !b.is_empty()).map(String::from),
    })
}

// ---- settings (channel, skipped version, held apps) --------------------------------------------

#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    pub channel: Option<Channel>,
    pub skipped: Option<String>,
    /// Updates the user turned down: `manager:name` (never) or `manager:name@version` (not that version).
    #[serde(default)]
    pub held: Vec<String>,
}

fn settings_path() -> PathBuf {
    util::data_dir().join("settings.json")
}

pub fn load() -> Settings {
    std::fs::read_to_string(settings_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<(), String> {
    std::fs::create_dir_all(util::data_dir()).map_err(|e| e.to_string())?;
    std::fs::write(settings_path(), serde_json::to_string_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn channel() -> Channel {
    load().channel.unwrap_or_else(|| channel_of(version()))
}

/// The AppImage file this process runs from, if it is one.
pub fn appimage() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The release file this install can take: its own AppImage, else the package for the system's manager.
fn suffix() -> Option<String> {
    let arch = std::env::consts::ARCH;
    if appimage().is_some() {
        return Some(format!("_{}.AppImage", if arch == "aarch64" { "aarch64" } else { "amd64" }));
    }
    if util::has("dpkg") {
        return Some(format!("_{}.deb", if arch == "aarch64" { "arm64" } else { "amd64" }));
    }
    util::has("rpm").then(|| format!(".{arch}.rpm"))
}

/// A dev build updates by rebuilding it, so it never offers one.
pub fn check() -> Result<Option<Offer>, String> {
    let Some(suffix) = suffix().filter(|_| !cfg!(debug_assertions)) else { return Ok(None) };
    let body = ureq::get(RELEASES)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", concat!("Unpack/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .map_err(|e| format!("Could not reach GitHub: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let releases: Vec<Release> = serde_json::from_str(&body).map_err(|e| format!("Unexpected answer from GitHub: {e}"))?;
    Ok(pick(&releases, version(), channel(), &suffix))
}

pub enum Installed {
    /// The AppImage was replaced; start this file and quit.
    Restart(PathBuf),
    /// A package was saved here and opened in the desktop's installer.
    Opened(PathBuf),
}

fn downloads_dir() -> PathBuf {
    let from_xdg = crate::util::command("xdg-user-dir").arg("DOWNLOAD").output().ok().filter(|o| o.status.success());
    from_xdg
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
        .filter(|p| p.is_dir() && p != &util::home())
        .unwrap_or_else(|| util::home().join("Downloads"))
}

/// Download `offer` (progress 0..1 through `progress`) and put it in place. Only GitHub release URLs are accepted.
pub fn install(offer: &Offer, progress: &dyn Fn(f64)) -> Result<Installed, String> {
    if !offer.url.starts_with(DOWNLOADS) {
        return Err("Refusing to download an update from outside this project's GitHub releases.".into());
    }
    let own = appimage();
    let name = Path::new(offer.url.rsplit('/').next().unwrap_or("")).file_name().ok_or("Bad update address")?;
    let target = match &own {
        Some(p) => p.clone(),
        None => {
            let dir = downloads_dir();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            dir.join(name)
        }
    };
    let mut part = target.clone().into_os_string();
    part.push(".part");
    let part = PathBuf::from(part);
    let result = download(&offer.url, &part, own.is_some(), progress);
    if let Err(e) = result.and_then(|_| std::fs::rename(&part, &target).map_err(|e| e.to_string())) {
        let _ = std::fs::remove_file(&part);
        return Err(format!("Update failed: {e}"));
    }
    if own.is_some() {
        return Ok(Installed::Restart(target));
    }
    crate::util::command("xdg-open").arg(&target).spawn().map_err(|e| format!("Saved to {} but could not open it: {e}", target.display()))?;
    Ok(Installed::Opened(target))
}

fn download(url: &str, part: &Path, executable: bool, progress: &dyn Fn(f64)) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let resp = ureq::AgentBuilder::new()
        .redirects(5)
        .timeout_connect(std::time::Duration::from_secs(20))
        .build()
        .get(url)
        .set("User-Agent", concat!("Unpack/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    // Without a Content-Length the bar stays at 0; GitHub release assets always send one.
    let total: u64 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(if executable { 0o755 } else { 0o644 })
        .open(part)
        .map_err(|e| e.to_string())?;
    let mut reader = resp.into_reader().take(MAX_UPDATE + 1);
    let (mut buf, mut done, mut shown) = (vec![0u8; 1 << 16], 0u64, 0u64);
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("download failed: {e}"))?;
        if n == 0 {
            break;
        }
        done += n as u64;
        if done > MAX_UPDATE {
            return Err("the file is larger than the 1 GiB safety limit".into());
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        let percent = (done * 100).checked_div(total).unwrap_or(0);
        if percent != shown {
            shown = percent;
            progress(percent as f64 / 100.0);
        }
    }
    drop(out);
    if executable {
        // An error page saved under the right name must not become the app.
        let mut head = [0u8; 4];
        std::fs::File::open(part).and_then(|mut f| f.read_exact(&mut head)).map_err(|e| e.to_string())?;
        if head != *b"\x7fELF" {
            return Err("the download is not an AppImage".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, names: &[&str]) -> Release {
        Release {
            tag_name: format!("v{tag}"),
            body: Some(format!("notes {tag}")),
            assets: names.iter().map(|n| Asset { name: n.to_string(), browser_download_url: format!("u/{n}") }).collect(),
        }
    }

    fn sample() -> Vec<Release> {
        vec![
            release("0.0.2-alpha.1", &["a.AppImage"]),
            release("0.0.1", &["b.AppImage", "b.deb"]),
            release("0.0.1-beta.2", &["c.AppImage"]),
            release("0.0.3", &["d.rpm"]),
        ]
    }

    fn offer(version: &str, url: &str) -> Option<Offer> {
        Some(Offer { version: version.into(), url: url.into(), notes: Some(format!("notes {version}")) })
    }

    #[test]
    fn alpha_takes_the_newest() {
        assert_eq!(pick(&sample(), "0.0.1-alpha.1", Channel::Alpha, ".AppImage"), offer("0.0.2-alpha.1", "u/a.AppImage"));
    }

    #[test]
    fn beta_takes_stable_but_not_alpha() {
        assert_eq!(pick(&sample(), "0.0.1-beta.2", Channel::Beta, ".AppImage"), offer("0.0.1", "u/b.AppImage"));
        assert_eq!(pick(&sample(), "0.0.1-beta.2", Channel::Beta, ".deb"), offer("0.0.1", "u/b.deb"));
    }

    #[test]
    fn nothing_newer_or_bad_version_gives_none() {
        assert_eq!(pick(&sample(), "0.0.1", Channel::Stable, ".AppImage"), None);
        assert_eq!(pick(&sample(), "not-a-version", Channel::Alpha, ".AppImage"), None);
    }

    #[test]
    fn stable_sorts_above_its_prereleases() {
        assert!(version_key("1.0.0").unwrap() > version_key("1.0.0-beta.9").unwrap());
        assert!(version_key("1.0.0-beta.1").unwrap() > version_key("1.0.0-alpha.9").unwrap());
        assert_eq!(version_key("1.0"), None);
        assert_eq!(version_key("1.0.0-rc.1"), None);
    }

    #[test]
    fn channel_follows_the_installed_build() {
        assert_eq!(channel_of("0.0.1-alpha.1"), Channel::Alpha);
        assert_eq!(channel_of("1.2.3"), Channel::Stable);
    }

    #[test]
    fn refuses_foreign_download_urls() {
        let o = Offer { version: "9.9.9".into(), url: "https://evil.example/x.AppImage".into(), notes: None };
        assert!(install(&o, &|_| {}).is_err());
    }
}
