//! Install and remove with their SQLite record. Shared by the window and the command line.
use crate::plan::{Plan, Track, Uninstall};
use crate::runner::{self, Emit};
use crate::store::{App, Store};
use crate::system::System;
use crate::util::{cache_dir, home, xdg};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Serialize, Debug)]
pub struct RunResult {
    pub ok: bool,
    pub app_id: Option<i64>,
    pub error: Option<String>,
    /// Problems found after a successful install, such as missing libraries.
    pub notes: Vec<String>,
}

fn lock(store: &Mutex<Store>) -> Result<std::sync::MutexGuard<'_, Store>, String> {
    store.lock().map_err(|e| e.to_string())
}

/// Run a plan and record it. The store is locked only after the run, so a long install never blocks reads.
pub fn install(plan: &Plan, as_root: bool, sys: &System, store: &Mutex<Store>, emit: Emit) -> Result<RunResult, String> {
    if !plan.supported {
        return Err("This plan can't be run".into());
    }
    let result = runner::run(plan, as_root, sys, emit);
    let db = lock(store)?;
    Ok(match result {
        Ok(o) => {
            // The download and build folder is no longer needed; a failed run keeps it for a retry.
            let _ = std::fs::remove_dir_all(cache_dir().join("work").join(&plan.id));
            if plan.track == Track::Nothing {
                db.event(None, "info", &format!("{} done", plan.title));
                return Ok(RunResult { ok: true, app_id: None, error: None, notes: o.notes });
            }
            let id = db.add_app(&plan.name, plan.version.as_deref(), &plan.kind, &plan.source, plan.sha256.as_deref(), &o.uninstall)?;
            db.event(Some(id), "info", &format!("Installed from {}", plan.source));
            for n in &o.notes {
                db.event(Some(id), "warn", n);
            }
            if let Track::Replace(old) = plan.track {
                let rec = db.get(old)?;
                drop(db);
                replace_old(&rec, &o.uninstall, sys, emit);
                let db = lock(store)?;
                db.set_status(old, "updated")?;
                db.event(Some(old), "info", &format!("Updated: replaced by app {id}"));
            }
            RunResult { ok: true, app_id: Some(id), error: None, notes: o.notes }
        }
        Err(e) => {
            db.event(None, "error", &format!("Install of {} failed: {e}", plan.name));
            RunResult { ok: false, app_id: None, error: Some(e), notes: vec![] }
        }
    })
}

/// After an update that was a new download: delete the old version's files the new one did not reuse.
/// A package manager already replaced its own package in place.
fn replace_old(old: &App, new: &Uninstall, sys: &System, emit: Emit) {
    let Uninstall::Files { paths } = &old.uninstall else { return };
    let keep: &[String] = match new {
        Uninstall::Files { paths } => paths,
        _ => &[],
    };
    let stale: Vec<String> = paths.iter().filter(|p| !keep.contains(p)).cloned().collect();
    if let Err(e) = runner::uninstall(&Uninstall::Files { paths: stale }, sys, emit) {
        emit("warn", &format!("The old version's files were not all removed: {e}"));
    }
}

pub fn uninstall(app_id: i64, sys: &System, store: &Mutex<Store>, emit: Emit) -> Result<RunResult, String> {
    let rec = lock(store)?.get(app_id)?;
    let healed = runner::heal(&rec.uninstall, rec.source.as_deref());
    let r = runner::uninstall(&healed, sys, emit);
    let db = lock(store)?;
    Ok(match r {
        Ok(()) => {
            db.set_status(app_id, "removed")?;
            db.event(Some(app_id), "info", "Uninstalled");
            RunResult { ok: true, app_id: Some(app_id), error: None, notes: vec![] }
        }
        Err(e) => {
            db.event(Some(app_id), "error", &format!("Uninstall failed: {e}"));
            RunResult { ok: false, app_id: Some(app_id), error: Some(e), notes: vec![] }
        }
    })
}

// ------------------------------------------------------------ leftovers

/// Folders apps keep their settings, caches and data in, one folder per app.
fn leftover_roots() -> Vec<PathBuf> {
    vec![xdg("XDG_CONFIG_HOME", ".config"), xdg("XDG_CACHE_HOME", ".cache"), xdg("XDG_DATA_HOME", ".local/share"), xdg("XDG_STATE_HOME", ".local/state"), home().join(".var/app")]
}

/// Shared folders in those roots that belong to no single app.
const SHARED: [&str; 14] = ["applications", "icons", "fonts", "mime", "flatpak", "systemd", "autostart", "bin", "trash", "Trash", "recently-used.xbel", "unpack", "unpack-dev", "dconf"];

fn plausible(name: &str) -> bool {
    name.len() > 1 && !name.starts_with('.') && !name.contains('/') && !SHARED.contains(&name)
}

/// Settings, cache and data folders named after a removed app, for the user to confirm before they go.
pub fn leftovers(app: &App) -> Vec<PathBuf> {
    let mut names = vec![app.name.clone(), app.name.to_lowercase()];
    if let Uninstall::Manager { ident, .. } = &app.uninstall {
        names.push(ident.clone());
    }
    names.retain(|n| plausible(n));
    names.dedup();
    let mut out: Vec<PathBuf> = vec![];
    for root in leftover_roots() {
        for n in &names {
            let p = root.join(n);
            if p.symlink_metadata().is_ok_and(|m| m.is_dir()) && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Delete what `leftovers` finds now (never a list from elsewhere), each a direct child of a known root.
pub fn remove_leftovers(app_id: i64, store: &Mutex<Store>, emit: Emit) -> Result<Vec<String>, String> {
    let app = lock(store)?.get(app_id)?;
    let roots = leftover_roots();
    let mut removed = vec![];
    for p in leftovers(&app) {
        if !p.parent().is_some_and(|parent| roots.iter().any(|r| r == parent)) {
            continue;
        }
        std::fs::remove_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        emit("info", &format!("Removed {}", p.display()));
        removed.push(p.display().to_string());
    }
    if !removed.is_empty() {
        lock(store)?.event(Some(app_id), "info", &format!("Removed leftovers: {}", removed.join(", ")));
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_and_removes_only_named_folders() {
        let _g = crate::util::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let h = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", h.path());
        for v in ["XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"] {
            std::env::remove_var(v);
        }
        for d in [".config/Tool", ".cache/tool", ".config/other", ".local/share/applications", ".var/app/org.x.Tool"] {
            std::fs::create_dir_all(h.path().join(d)).unwrap();
        }
        let store = Mutex::new(Store::memory().unwrap());
        let u = Uninstall::Manager { manager: "flatpak".into(), ident: "org.x.Tool".into(), user: true };
        let id = store.lock().unwrap().add_app("Tool", None, "flatpak", "x", None, &u).unwrap();
        let found = leftovers(&store.lock().unwrap().get(id).unwrap());
        assert_eq!(found.len(), 3, "{found:?}");
        let gone = remove_leftovers(id, &store, &|_, _| {}).unwrap();
        assert_eq!(gone.len(), 3);
        assert!(h.path().join(".config/other").exists() && h.path().join(".local/share/applications").exists());
        assert!(!h.path().join(".config/Tool").exists() && !h.path().join(".var/app/org.x.Tool").exists());

        // A name that is a shared folder is never a leftover.
        let id2 = store.lock().unwrap().add_app("applications", None, "x", "x", None, &Uninstall::Traced).unwrap();
        assert!(leftovers(&store.lock().unwrap().get(id2).unwrap()).is_empty());
    }
}
