pub mod archive;
pub mod fetch;
pub mod input;
pub mod sniff;
pub mod system;
pub mod trace;
pub mod util;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Unpack");
}
