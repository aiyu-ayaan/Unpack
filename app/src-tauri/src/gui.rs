//! The desktop window: Tauri commands over the same core the command line uses.
use crate::plan::{Plan, Uninstall};
use crate::store::{App, Event, Store};
use crate::system::{self, System};
use crate::{analyze, fetch, library, ops, runner, updates, util};
use ops::RunResult;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    plans: Mutex<HashMap<String, Plan>>,
    store: Mutex<Store>,
    /// The last update check's answer; install takes it from here, never from the page.
    offer: Mutex<Option<updates::Offer>>,
    /// Set by `cancel_analyze`; the running analyze checks it.
    cancel: Arc<AtomicBool>,
}

type Shared<'a> = State<'a, Arc<AppState>>;

#[derive(Serialize, Clone)]
struct LogLine<'a> {
    plan_id: &'a str,
    stream: &'a str,
    line: &'a str,
}

#[derive(Serialize, Clone)]
struct Fetched {
    done: u64,
    total: Option<u64>,
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn detect_system() -> Result<System, String> {
    blocking(system::detect).await
}

/// Download progress arrives as `analyze-progress`. Cancel answers at once; the worker stops at its next
/// check (download chunk, git clone poll, end) and removes its work folder.
#[tauri::command]
async fn analyze(input: String, allow_http: bool, app: AppHandle, state: Shared<'_>) -> Result<Plan, String> {
    let st = state.inner().clone();
    st.cancel.store(false, Ordering::SeqCst);
    let cancel = st.cancel.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let shown = std::sync::atomic::AtomicU64::new(0);
        let progress = |done: u64, total: Option<u64>| {
            // About every 256 KiB, not every 64 KiB chunk.
            if done == 0 || done >= shown.load(Ordering::Relaxed) + (1 << 18) {
                shown.store(done, Ordering::Relaxed);
                let _ = app.emit("analyze-progress", Fetched { done, total });
            }
            !cancel.load(Ordering::SeqCst)
        };
        let _ = tx.send(analyze::analyze(&input, &system::detect(), allow_http, &progress));
    });
    let cancel = st.cancel.clone();
    let plan = blocking(move || loop {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(r) => return r,
            Err(_) if cancel.load(Ordering::SeqCst) => return Err(fetch::CANCELLED.to_string()),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Err("Analyze stopped unexpectedly".into()),
            Err(_) => {}
        }
    })
    .await??;
    st.plans.lock().map_err(|e| e.to_string())?.insert(plan.id.clone(), plan.clone());
    Ok(plan)
}

#[tauri::command]
fn cancel_analyze(state: Shared<'_>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
async fn execute(plan_id: String, as_root: bool, app: AppHandle, state: Shared<'_>) -> Result<RunResult, String> {
    let st = state.inner().clone();
    blocking(move || {
        let plan = st.plans.lock().map_err(|e| e.to_string())?.get(&plan_id).cloned().ok_or("Unknown plan; analyze again")?;
        let emit = |stream: &str, line: &str| {
            let _ = app.emit("plan-log", LogLine { plan_id: &plan_id, stream, line });
        };
        let out = ops::install(&plan, as_root, &system::detect(), &st.store, &emit)?;
        if out.ok {
            st.plans.lock().map_err(|e| e.to_string())?.remove(&plan_id);
        }
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
        let emit = |stream: &str, line: &str| {
            let _ = app.emit("plan-log", LogLine { plan_id: "uninstall", stream, line });
        };
        ops::uninstall(app_id, &system::detect(), &st.store, &emit)
    })
    .await?
}

/// Settings, cache and data folders a removed app left behind, for the page to show before removing them.
#[tauri::command]
fn leftovers(app_id: i64, state: Shared<'_>) -> Result<Vec<String>, String> {
    let app = state.store.lock().map_err(|e| e.to_string())?.get(app_id)?;
    Ok(ops::leftovers(&app).iter().map(|p| p.display().to_string()).collect())
}

/// Removes what `leftovers` finds now; the page names the app, never the paths.
#[tauri::command]
async fn remove_leftovers(app_id: i64, app: AppHandle, state: Shared<'_>) -> Result<Vec<String>, String> {
    let st = state.inner().clone();
    blocking(move || {
        let emit = |stream: &str, line: &str| {
            let _ = app.emit("plan-log", LogLine { plan_id: "uninstall", stream, line });
        };
        ops::remove_leftovers(app_id, &st.store, &emit)
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
            app.manage(Arc::new(AppState { plans: Mutex::new(HashMap::new()), store: Mutex::new(store), offer: Mutex::new(None), cancel: Arc::new(AtomicBool::new(false)) }));
            std::thread::spawn(analyze::clean_old_work);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![detect_system, analyze, cancel_analyze, execute, list_apps, history, scan_system, uninstall_app, leftovers, remove_leftovers, uninstall_system, update_info, update_check, update_install, update_set_channel, update_skip, open_link])
        .run(tauri::generate_context!())
        .expect("error while running Unpack");
}
