//! The desktop window: Tauri commands over the same core the command line uses.
use crate::plan::Plan;
use crate::store::{App, Event, Store};
use crate::system::{self, System};
use crate::{analyze, catalog, fetch, fix, library, mime, ops, plan, sandbox, transfer, updates, upgrades, util};
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
    /// The file `unpack open <file>` started the window with, until the page takes it.
    opened: Mutex<Option<String>>,
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
async fn analyze(input: String, allow_http: bool, as_package: bool, app: AppHandle, state: Shared<'_>) -> Result<Plan, String> {
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
        let _ = tx.send(analyze::analyze(&input, &system::detect(), allow_http, as_package, &progress));
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

/// Keep a plan built by the backend (not by `analyze`) so `execute` can run it by id.
fn keep(st: &AppState, mut plan: Plan) -> Result<Plan, String> {
    plan.id = analyze::new_id();
    st.plans.lock().map_err(|e| e.to_string())?.insert(plan.id.clone(), plan.clone());
    Ok(plan)
}

#[tauri::command]
async fn store_popular() -> Result<Vec<catalog::Entry>, String> {
    blocking(|| catalog::popular(&system::detect())).await?
}

#[tauri::command]
async fn store_category(category: String) -> Result<Vec<catalog::Entry>, String> {
    blocking(move || catalog::category(&category)).await?
}

#[tauri::command]
async fn store_detail(source: String, id: String) -> Result<catalog::Detail, String> {
    blocking(move || catalog::detail(&source, &id, &system::detect())).await?
}

#[tauri::command]
async fn flatpak_remotes() -> Result<Vec<catalog::Remote>, String> {
    blocking(catalog::remotes).await
}

/// Removing a Flatpak source is a reviewed plan like any other change.
#[tauri::command]
async fn remote_remove_plan(name: String, state: Shared<'_>) -> Result<Plan, String> {
    let p = blocking(move || plan::plan_remote_remove(&name)).await?;
    keep(&state, p)
}

#[tauri::command]
async fn store_search(query: String) -> Result<Vec<catalog::Entry>, String> {
    blocking(move || catalog::search(&query, &system::detect())).await?
}

/// The page names a source and an id from the Store; the backend builds and checks the plan.
#[tauri::command]
async fn store_plan(source: String, id: String, state: Shared<'_>) -> Result<Plan, String> {
    let p = blocking(move || plan::plan_repo(&source, &id, &system::detect())).await?;
    keep(&state, p)
}

#[tauri::command]
async fn list_upgrades(state: Shared<'_>) -> Result<Vec<upgrades::Upgrade>, String> {
    let apps = state.store.lock().map_err(|e| e.to_string())?.list()?;
    blocking(move || upgrades::list(&apps, &system::detect())).await
}

/// One plan for the packages the page picked from `list_upgrades`.
#[tauri::command]
async fn upgrade_plan(picks: Vec<upgrades::Pick>, state: Shared<'_>) -> Result<Plan, String> {
    let p = blocking(move || upgrades::plan(&picks, &system::detect())).await?;
    keep(&state, p)
}

/// Analyze the newer release file of a tracked app; installing it replaces the old record.
#[tauri::command]
async fn update_app_plan(app_id: i64, app: AppHandle, state: Shared<'_>) -> Result<Plan, String> {
    let rec = state.store.lock().map_err(|e| e.to_string())?.get(app_id)?;
    let plan = blocking(move || {
        let (_, _, url) = upgrades::newer_download(&rec).ok_or("No newer version found")?;
        let progress = |done: u64, total: Option<u64>| {
            let _ = app.emit("analyze-progress", Fetched { done, total });
            true
        };
        analyze::analyze(&url, &system::detect(), false, false, &progress)
    })
    .await??;
    let mut plan = plan;
    plan.track = plan::Track::Replace(app_id);
    plan.title = format!("Update {}", plan.name);
    state.plans.lock().map_err(|e| e.to_string())?.insert(plan.id.clone(), plan.clone());
    Ok(plan)
}

/// What keeps a tracked app from starting, as a plan to review (unsupported with advice when nothing is wrong).
#[tauri::command]
async fn fix_plan(app_id: i64, state: Shared<'_>) -> Result<Plan, String> {
    let rec = state.store.lock().map_err(|e| e.to_string())?.get(app_id)?;
    let p = blocking(move || fix::plan(&rec, &system::detect())).await?;
    keep(&state, p)
}

/// The file this window was opened with ("Open with Unpack"), once.
#[tauri::command]
fn take_opened(state: Shared<'_>) -> Result<Option<String>, String> {
    Ok(state.opened.lock().map_err(|e| e.to_string())?.take())
}

#[tauri::command]
fn file_handler() -> bool {
    mime::enabled()
}

#[tauri::command]
async fn set_file_handler(on: bool) -> Result<(), String> {
    blocking(move || if on { mime::enable() } else { mime::disable() }).await?
}

/// Save the installed apps to `~/unpack-apps.json`; returns where.
#[tauri::command]
fn export_apps(state: Shared<'_>) -> Result<String, String> {
    let apps = state.store.lock().map_err(|e| e.to_string())?.list()?;
    let path = transfer::default_path();
    transfer::write(&apps, &path)?;
    Ok(path.display().to_string())
}

/// Read an exported list and say how each app can come back here; installing goes through Store or Install.
#[tauri::command]
async fn import_apps(path: String) -> Result<Vec<transfer::Imported>, String> {
    blocking(move || {
        let path = path.trim();
        let p = path.strip_prefix("~/").map(|r| util::home().join(r)).unwrap_or_else(|| path.into());
        transfer::read(&p, &system::detect())
    })
    .await?
}

/// Run a script plan in a bubblewrap sandbox (read-only system, no network, throwaway home) and report.
#[tauri::command]
async fn sandbox_preview(plan_id: String, state: Shared<'_>) -> Result<sandbox::Report, String> {
    let plan = state.plans.lock().map_err(|e| e.to_string())?.get(&plan_id).cloned().ok_or("Unknown plan; analyze again")?;
    blocking(move || sandbox::preview(&plan)).await?
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
        Ok(ops::uninstall_system(&manager, &ident, &system::detect(), &emit))
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
pub fn run(open: Option<String>) {
    tauri::Builder::default()
        // A second start (another "Open with Unpack", or the menu entry) hands its file to this window and exits.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = (w.unminimize(), w.show(), w.set_focus());
            }
            if let [_, cmd, rest @ ..] = argv.as_slice() {
                if cmd == "open" && !rest.is_empty() {
                    if let Ok(mut o) = app.state::<Arc<AppState>>().opened.lock() {
                        *o = Some(rest.join(" "));
                    }
                    let _ = app.emit("opened", ());
                }
            }
        }))
        .setup(move |app| {
            let store = Store::open(&util::data_dir().join("unpack.db"))?;
            app.manage(Arc::new(AppState { plans: Mutex::new(HashMap::new()), store: Mutex::new(store), offer: Mutex::new(None), cancel: Arc::new(AtomicBool::new(false)), opened: Mutex::new(open) }));
            std::thread::spawn(analyze::clean_old_work);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![detect_system, analyze, cancel_analyze, flatpak_remotes, remote_remove_plan, store_popular, store_category, store_detail, store_search, store_plan, list_upgrades, upgrade_plan, update_app_plan, fix_plan, take_opened, file_handler, set_file_handler, export_apps, import_apps, sandbox_preview, execute, list_apps, history, scan_system, uninstall_app, leftovers, remove_leftovers, uninstall_system, update_info, update_check, update_install, update_set_channel, update_skip, open_link])
        .run(tauri::generate_context!())
        .expect("error while running Unpack");
}
