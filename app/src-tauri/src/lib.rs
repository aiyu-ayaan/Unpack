pub mod analyze;
pub mod archive;
pub mod fetch;
pub mod input;
pub mod libcheck;
pub mod library;
pub mod plan;
pub mod runner;
pub mod sniff;
pub mod store;
pub mod system;
pub mod trace;
pub mod updates;
pub mod util;

use plan::{Plan, Uninstall};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use store::{App, Event, Store};
use system::System;
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    plans: Mutex<HashMap<String, Plan>>,
    store: Mutex<Store>,
    /// The last update check's answer; install takes it from here, never from the page.
    offer: Mutex<Option<updates::Offer>>,
}

type Shared<'a> = State<'a, Arc<AppState>>;

#[derive(Serialize, Clone)]
struct LogLine<'a> {
    plan_id: &'a str,
    stream: &'a str,
    line: &'a str,
}

#[derive(Serialize)]
struct RunResult {
    ok: bool,
    app_id: Option<i64>,
    error: Option<String>,
    /// Problems found after a successful install, such as missing libraries.
    notes: Vec<String>,
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn detect_system() -> Result<System, String> {
    blocking(system::detect).await
}

#[tauri::command]
async fn analyze(input: String, allow_http: bool, state: Shared<'_>) -> Result<Plan, String> {
    let st = state.inner().clone();
    blocking(move || {
        let plan = analyze::analyze(&input, &system::detect(), allow_http)?;
        st.plans.lock().map_err(|e| e.to_string())?.insert(plan.id.clone(), plan.clone());
        Ok(plan)
    })
    .await?
}

#[tauri::command]
async fn execute(plan_id: String, as_root: bool, app: AppHandle, state: Shared<'_>) -> Result<RunResult, String> {
    let st = state.inner().clone();
    blocking(move || {
        let plan = st.plans.lock().map_err(|e| e.to_string())?.get(&plan_id).cloned().ok_or("Unknown plan; analyze again")?;
        if !plan.supported {
            return Err("This plan can't be run".to_string());
        }
        let emit = |stream: &str, line: &str| {
            let _ = app.emit("plan-log", LogLine { plan_id: &plan_id, stream, line });
        };
        let sys = system::detect();
        let result = runner::run(&plan, as_root, &sys, &emit);
        let db = st.store.lock().map_err(|e| e.to_string())?;
        let out = match result {
            Ok(o) => {
                let id = db.add_app(&plan.name, plan.version.as_deref(), &plan.kind, &plan.source, plan.sha256.as_deref(), &o.uninstall)?;
                db.event(Some(id), "info", &format!("Installed from {}", plan.source));
                for n in &o.notes {
                    db.event(Some(id), "warn", n);
                }
                RunResult { ok: true, app_id: Some(id), error: None, notes: o.notes }
            }
            Err(e) => {
                db.event(None, "error", &format!("Install of {} failed: {e}", plan.name));
                RunResult { ok: false, app_id: None, error: Some(e), notes: vec![] }
            }
        };
        let _ = app.emit("plan-done", &plan_id);
        Ok(out)
    })
    .await?
}

#[tauri::command]
fn list_apps(state: Shared<'_>) -> Result<Vec<App>, String> {
    state.store.lock().map_err(|e| e.to_string())?.list()
}

#[tauri::command]
fn history(app_id: Option<i64>, state: Shared<'_>) -> Result<Vec<Event>, String> {
    state.store.lock().map_err(|e| e.to_string())?.events(app_id)
}

#[tauri::command]
async fn scan_system() -> Result<Vec<library::SysPkg>, String> {
    blocking(library::scan).await
}

#[tauri::command]
async fn uninstall_app(app_id: i64, app: AppHandle, state: Shared<'_>) -> Result<RunResult, String> {
    let st = state.inner().clone();
    blocking(move || {
        let rec = st.store.lock().map_err(|e| e.to_string())?.get(app_id)?;
        let emit = |stream: &str, line: &str| {
            let _ = app.emit("plan-log", LogLine { plan_id: "uninstall", stream, line });
        };
        let healed = runner::heal(&rec.uninstall, rec.source.as_deref());
        let r = runner::uninstall(&healed, &system::detect(), &emit);
        let db = st.store.lock().map_err(|e| e.to_string())?;
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
    })
    .await?
}

/// Remove a package the system reports (not tracked by Unpack).
#[tauri::command]
async fn uninstall_system(manager: String, ident: String, app: AppHandle) -> Result<RunResult, String> {
    blocking(move || {
        let emit = |stream: &str, line: &str| {
            let _ = app.emit("plan-log", LogLine { plan_id: "uninstall", stream, line });
        };
        let user = manager == "flatpak";
        let r = runner::uninstall(&Uninstall::Manager { manager, ident, user }, &system::detect(), &emit);
        Ok(RunResult { ok: r.is_ok(), app_id: None, error: r.err(), notes: vec![] })
    })
    .await?
}

#[derive(Serialize)]
struct UpdateInfo {
    version: &'static str,
    channel: updates::Channel,
    /// The AppImage replaces itself; a package is saved and opened instead.
    self_install: bool,
    skipped: Option<String>,
}

#[tauri::command]
fn update_info() -> UpdateInfo {
    UpdateInfo { version: updates::version(), channel: updates::channel(), self_install: updates::appimage().is_some(), skipped: updates::load().skipped }
}

#[tauri::command]
async fn update_check(state: Shared<'_>) -> Result<Option<updates::Offer>, String> {
    let offer = blocking(updates::check).await??;
    *state.offer.lock().map_err(|e| e.to_string())? = offer.clone();
    Ok(offer)
}

/// Downloads the offer from the last check (progress arrives as `update-progress`, 0..1, null when over).
/// Returns where a package was saved, or null when the app is restarting into the new AppImage.
#[tauri::command]
async fn update_install(app: AppHandle, state: Shared<'_>) -> Result<Option<String>, String> {
    let offer = state.offer.lock().map_err(|e| e.to_string())?.clone().ok_or("No update to install. Check again.")?;
    let handle = app.clone();
    let done = blocking(move || {
        let _ = handle.emit("update-progress", 0.0);
        let r = updates::install(&offer, &|f| {
            let _ = handle.emit("update-progress", f);
        });
        let _ = handle.emit("update-progress", Option::<f64>::None);
        r
    })
    .await??;
    match done {
        updates::Installed::Restart(file) => {
            util::command(file).spawn().map_err(|e| format!("Updated, but could not restart: {e}"))?;
            app.exit(0);
            Ok(None)
        }
        updates::Installed::Opened(file) => Ok(Some(file.display().to_string())),
    }
}

#[tauri::command]
fn update_set_channel(channel: updates::Channel) -> Result<(), String> {
    let mut s = updates::load();
    s.channel = Some(channel);
    updates::save(&s)
}

/// "Later" on the start-up offer is remembered for that version.
#[tauri::command]
fn update_skip(state: Shared<'_>) -> Result<(), String> {
    let Some(offer) = state.offer.lock().map_err(|e| e.to_string())?.clone() else { return Ok(()) };
    let mut s = updates::load();
    s.skipped = Some(offer.version);
    updates::save(&s)
}

/// Opens one of our own links in the browser. The page names it; it never sends an address.
#[tauri::command]
fn open_link(which: String) -> Result<(), String> {
    let url = match which.as_str() {
        "repo" => updates::REPO_URL,
        "issues" => "https://github.com/aiyu-ayaan/Unpack/issues",
        "developer" => "https://github.com/aiyu-ayaan",
        "site" => "https://aiyu.co.in",
        _ => return Err("Unknown link".into()),
    };
    util::command("xdg-open").arg(url).spawn().map(|_| ()).map_err(|e| format!("Could not open your browser: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let store = Store::open(&util::data_dir().join("unpack.db"))?;
            app.manage(Arc::new(AppState { plans: Mutex::new(HashMap::new()), store: Mutex::new(store), offer: Mutex::new(None) }));
            std::thread::spawn(analyze::clean_old_work);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![detect_system, analyze, execute, list_apps, history, scan_system, uninstall_app, uninstall_system, update_info, update_check, update_install, update_set_channel, update_skip, open_link])
        .run(tauri::generate_context!())
        .expect("error while running Unpack");
}
