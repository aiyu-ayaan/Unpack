pub mod analyze;
pub mod archive;
pub mod fetch;
pub mod input;
pub mod library;
pub mod plan;
pub mod runner;
pub mod sniff;
pub mod store;
pub mod system;
pub mod trace;
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
                RunResult { ok: true, app_id: Some(id), error: None }
            }
            Err(e) => {
                db.event(None, "error", &format!("Install of {} failed: {e}", plan.name));
                RunResult { ok: false, app_id: None, error: Some(e) }
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
        let r = runner::uninstall(&rec.uninstall, &system::detect(), &emit);
        let db = st.store.lock().map_err(|e| e.to_string())?;
        Ok(match r {
            Ok(()) => {
                db.set_status(app_id, "removed")?;
                db.event(Some(app_id), "info", "Uninstalled");
                RunResult { ok: true, app_id: Some(app_id), error: None }
            }
            Err(e) => {
                db.event(Some(app_id), "error", &format!("Uninstall failed: {e}"));
                RunResult { ok: false, app_id: Some(app_id), error: Some(e) }
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
        Ok(RunResult { ok: r.is_ok(), app_id: None, error: r.err() })
    })
    .await?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let store = Store::open(&util::data_dir().join("unpack.db"))?;
            app.manage(Arc::new(AppState { plans: Mutex::new(HashMap::new()), store: Mutex::new(store) }));
            std::thread::spawn(analyze::clean_old_work);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![detect_system, analyze, execute, list_apps, history, scan_system, uninstall_app, uninstall_system])
        .run(tauri::generate_context!())
        .expect("error while running Unpack");
}
