//! Move to a new computer: export the apps installed with Unpack as JSON, and on the other side work out
//! how each one can be installed again (Store id, the original link, or not at all).
use crate::input::{self, Source};
use crate::plan::{manager_name, safe_ident, Uninstall};
use crate::store::App;
use crate::system::System;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Item {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub manager: Option<String>,
    #[serde(default)]
    pub ident: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct List {
    /// Format version.
    pub unpack: u32,
    pub apps: Vec<Item>,
}

/// How an exported app comes back on this computer.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Way {
    /// `store_plan(source, id)`.
    Store { source: String, id: String },
    /// `analyze(input)`: the original link or git address.
    Input { input: String },
    None { why: String },
}

#[derive(Serialize, Debug)]
pub struct Imported {
    pub item: Item,
    pub way: Way,
}

const MAX_LIST: u64 = 1024 * 1024;

pub fn export(apps: &[App]) -> List {
    let apps = apps
        .iter()
        .filter(|a| a.status == "installed")
        .map(|a| {
            let (manager, ident) = match &a.uninstall {
                Uninstall::Manager { manager, ident, .. } => (Some(manager.clone()), Some(ident.clone())),
                _ => (None, None),
            };
            Item { name: a.name.clone(), kind: a.kind.clone(), source: a.source.clone(), manager, ident }
        })
        .collect();
    List { unpack: 1, apps }
}

pub fn default_path() -> PathBuf {
    crate::util::home().join("unpack-apps.json")
}

pub fn write(apps: &[App], path: &Path) -> Result<usize, String> {
    let list = export(apps);
    std::fs::write(path, serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(list.apps.len())
}

pub fn way(item: &Item, sys: &System) -> Way {
    let native = sys.native.map(manager_name);
    let store = |source: &str, id: &str| Way::Store { source: source.into(), id: id.into() };
    match (item.manager.as_deref(), item.ident.as_deref()) {
        (_, Some(id)) if !safe_ident(id) => return Way::None { why: "Its package name is not valid.".into() },
        (Some("flatpak"), Some(id)) => return store("flathub", id),
        (Some("snap"), Some(id)) => return store("snap", id),
        // Only a package that came from the repositories (the Store) can be fetched again by name.
        (Some(m), Some(id)) if Some(m) == native && item.source.as_deref().is_some_and(|s| s.starts_with(&format!("{m}:"))) => return store(m, id),
        _ => {}
    }
    match item.source.as_deref().map(input::parse) {
        Some(Ok(Source::Url { pipe_to_shell: false, .. } | Source::Git { .. })) => Way::Input { input: item.source.clone().unwrap_or_default() },
        Some(Ok(Source::Url { .. })) => Way::None { why: "It came from a download-and-run command; run that again only if you still trust it.".into() },
        Some(Ok(Source::File(_))) => Way::None { why: "It was installed from a file on the old computer. Copy the file over and open it.".into() },
        _ => Way::None { why: "Unpack does not know where it came from.".into() },
    }
}

pub fn read(path: &Path, sys: &System) -> Result<Vec<Imported>, String> {
    let size = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    if size > MAX_LIST {
        return Err("That file is too large to be an Unpack app list.".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let list: List = serde_json::from_str(&text).map_err(|e| format!("Not an Unpack app list: {e}"))?;
    Ok(list.apps.into_iter().map(|item| Imported { way: way(&item, sys), item }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::Native;

    fn item(manager: Option<&str>, ident: Option<&str>, source: &str) -> Item {
        Item { name: "x".into(), kind: "k".into(), source: Some(source.into()), manager: manager.map(String::from), ident: ident.map(String::from) }
    }

    #[test]
    fn round_trip_and_ways() {
        let mut sys = crate::system::detect();
        sys.native = Some(Native::Apt);
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("l.json");
        let apps = vec![App { id: 1, name: "VLC".into(), version: None, kind: "flathub".into(), source: Some("flathub:org.videolan.VLC".into()), sha256: None, installed_at: 0, status: "installed".into(), replaced_by: None, uninstall: Uninstall::Manager { manager: "flatpak".into(), ident: "org.videolan.VLC".into(), user: true } }];
        assert_eq!(write(&apps, &f).unwrap(), 1);
        let back = read(&f, &sys).unwrap();
        assert_eq!(back[0].way, Way::Store { source: "flathub".into(), id: "org.videolan.VLC".into() });

        assert_eq!(way(&item(Some("apt"), Some("vlc"), "apt:vlc"), &sys), Way::Store { source: "apt".into(), id: "vlc".into() });
        assert!(matches!(way(&item(Some("apt"), Some("tool"), "/home/u/tool.deb"), &sys), Way::None { .. }));
        assert!(matches!(way(&item(Some("dnf"), Some("vlc"), "dnf:vlc"), &sys), Way::None { .. }));
        assert_eq!(way(&item(None, None, "https://example.com/a.AppImage"), &sys), Way::Input { input: "https://example.com/a.AppImage".into() });
        assert!(matches!(way(&item(None, None, "curl -fsSL https://x.sh | sh"), &sys), Way::None { .. }));
        assert!(matches!(way(&item(Some("flatpak"), Some("--x"), ""), &sys), Way::None { .. }));
        std::fs::write(&f, "{}").unwrap();
        assert!(read(&f, &sys).is_err());
    }
}
