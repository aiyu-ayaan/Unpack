//! Update checks while Unpack is closed: a daily systemd user timer (or, without systemd, a check at login)
//! runs `unpack updates --notify`, which shows a desktop notification. With auto-update on, updates that need
//! no admin password (user Flatpaks, apps from release downloads in the home folder) are installed first.
use crate::plan::Track;
use crate::store::{App, Store};
use crate::system::System;
use crate::upgrades::{self, Pick, Upgrade};
use crate::util::{command, has, xdg};
use crate::{analyze, ops, updates};
use std::path::PathBuf;
use std::sync::Mutex;

/// `unpack-updates`, or `unpack-dev-updates` for the dev flavor, so the two never share a timer.
fn unit() -> String {
    format!("{}-updates", crate::util::data_dir().file_name().unwrap_or_default().to_string_lossy())
}

fn systemd_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("systemd/user")
}

fn autostart() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("autostart").join(format!("{}.desktop", unit()))
}

/// A path as one word on a systemd `ExecStart=` line.
fn systemd_quote(p: &str) -> String {
    format!("\"{}\"", p.replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%").replace('$', "$$"))
}

pub fn service(exe: &str) -> String {
    format!("[Unit]\nDescription=Check for app updates (Unpack)\n\n[Service]\nType=oneshot\nExecStart={} updates --notify\n", systemd_quote(exe))
}

pub const TIMER: &str = "[Unit]\nDescription=Check for app updates every day (Unpack)\n\n[Timer]\nOnCalendar=daily\nPersistent=true\nRandomizedDelaySec=30min\n\n[Install]\nWantedBy=timers.target\n";

pub fn autostart_entry(exe: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName=Unpack update check\nExec={} updates --notify\nNoDisplay=true\nX-GNOME-Autostart-Delay=120\n", crate::runner::exec_quote(std::path::Path::new(exe)))
}

pub fn enabled() -> bool {
    systemd_dir().join(format!("{}.timer", unit())).is_file() || autostart().is_file()
}

fn systemctl(args: &[&str]) -> Result<(), String> {
    let out = command("systemctl").arg("--user").args(args).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

fn write(path: &PathBuf, text: &str) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().expect("has parent")).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Turn the background check on (a systemd user timer, else an autostart entry) or off.
pub fn set(on: bool) -> Result<(), String> {
    let name = unit();
    let (svc, timer) = (systemd_dir().join(format!("{name}.service")), systemd_dir().join(format!("{name}.timer")));
    if !on {
        if timer.is_file() {
            let _ = systemctl(&["disable", "--now", &format!("{name}.timer")]);
        }
        for p in [&svc, &timer, &autostart()] {
            if p.exists() {
                std::fs::remove_file(p).map_err(|e| e.to_string())?;
            }
        }
        let _ = systemctl(&["daemon-reload"]);
        return Ok(());
    }
    let exe = crate::mime::program()?.to_string_lossy().into_owned();
    if has("systemctl") && systemctl(&["show-environment"]).is_ok() {
        write(&svc, &service(&exe))?;
        write(&timer, TIMER)?;
        systemctl(&["daemon-reload"])?;
        return systemctl(&["enable", "--now", &format!("{name}.timer")]).map_err(|e| format!("Could not start the daily check: {e}"));
    }
    write(&autostart(), &autostart_entry(&exe))
}

/// A desktop notification, when `notify-send` is there.
pub fn notify(title: &str, body: &str) {
    if has("notify-send") {
        let _ = command("notify-send").args(["--app-name=Unpack", "--icon=unpack", title, body]).output();
    }
}

/// Install the updates that need no admin password and have nothing to warn about; returns their names.
pub fn auto_update(all: &[Upgrade], apps: &[App], sys: &System, store: &Mutex<Store>) -> Vec<String> {
    let quiet = |_: &str, _: &str| {};
    let mut done = vec![];
    let flat: Vec<&Upgrade> = all.iter().filter(|u| u.manager == "flatpak" && u.user && !u.held).collect();
    if !flat.is_empty() {
        let picks: Vec<Pick> = flat.iter().map(|u| Pick { manager: u.manager.clone(), name: u.name.clone(), user: true }).collect();
        let mut plan = upgrades::plan(&picks, sys);
        plan.id = analyze::new_id();
        if plan.supported && !plan.needs_root && ops::install(&plan, false, sys, store, &quiet).is_ok_and(|r| r.ok) {
            done.extend(flat.iter().map(|u| u.name.clone()));
        }
    }
    for u in all.iter().filter(|u| u.manager == "download" && !u.held) {
        let Some(app) = apps.iter().find(|a| Some(a.id) == u.app_id) else { continue };
        let Some((_, _, url)) = upgrades::newer_download(app) else { continue };
        let Ok(mut plan) = analyze::analyze(&url, sys, false, false, &|_, _| true) else { continue };
        // Only what installs into the home folder, with no password and no warning a person should read first.
        if !plan.supported || plan.needs_root || plan.root_optional || !plan.warnings.is_empty() {
            let _ = std::fs::remove_dir_all(crate::util::cache_dir().join("work").join(&plan.id));
            continue;
        }
        plan.track = Track::Replace(app.id);
        if ops::install(&plan, false, sys, store, &quiet).is_ok_and(|r| r.ok) {
            done.push(u.name.clone());
        }
    }
    done
}

/// The notification for one background check: what was updated, and what is waiting.
pub fn summary(done: &[String], waiting: &[&Upgrade]) -> Option<(String, String)> {
    let names = |v: Vec<&str>| {
        let more = v.len().saturating_sub(3);
        let head = v.into_iter().take(3).collect::<Vec<_>>().join(", ");
        if more > 0 { format!("{head} and {more} more") } else { head }
    };
    match (done.is_empty(), waiting.is_empty()) {
        (true, true) => None,
        (false, true) => Some((format!("Updated {} app{}", done.len(), if done.len() > 1 { "s" } else { "" }), names(done.iter().map(String::as_str).collect()))),
        (_, false) => {
            let mut body = format!("{}. Open Unpack to review them.", names(waiting.iter().map(|u| u.name.as_str()).collect()));
            if !done.is_empty() {
                body = format!("Updated {}. {body}", names(done.iter().map(String::as_str).collect()));
            }
            Some((format!("{} update{} available", waiting.len(), if waiting.len() > 1 { "s" } else { "" }), body))
        }
    }
}

/// What `unpack updates --notify` does: check, auto-update when that is on, and notify.
pub fn check(store: &Mutex<Store>, sys: &System) -> Result<(), String> {
    let apps = store.lock().map_err(|e| e.to_string())?.list()?;
    let all: Vec<Upgrade> = upgrades::list(&apps, sys).into_iter().filter(|u| !u.held).collect();
    let done = if updates::load().auto_update { auto_update(&all, &apps, sys, store) } else { vec![] };
    let waiting: Vec<&Upgrade> = all.iter().filter(|u| !done.contains(&u.name)).collect();
    if let Some((title, body)) = summary(&done, &waiting) {
        notify(&title, &body);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_and_summary() {
        assert!(service("/home/a b/Un%pack$.AppImage").contains("ExecStart=\"/home/a b/Un%%pack$$.AppImage\" updates --notify\n"));
        assert!(TIMER.contains("OnCalendar=daily") && TIMER.contains("WantedBy=timers.target"));
        assert!(autostart_entry("/opt/unpack").contains("Exec=\"/opt/unpack\" updates --notify\n"));
        let u = |n: &str| Upgrade { manager: "apt".into(), name: n.into(), current: String::new(), latest: String::new(), user: false, app_id: None, held: false };
        let (a, b, c, d) = (u("a"), u("b"), u("c"), u("d"));
        assert_eq!(summary(&[], &[]), None);
        assert_eq!(summary(&[], &[&a, &b, &c, &d]).unwrap(), ("4 updates available".into(), "a, b, c and 1 more. Open Unpack to review them.".into()));
        assert_eq!(summary(&["x".into()], &[&a]).unwrap().1, "Updated x. a. Open Unpack to review them.");
        assert_eq!(summary(&["x".into()], &[]).unwrap().0, "Updated 1 app");
    }
}
