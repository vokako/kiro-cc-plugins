//! Tauri backend — calls kiro_cc_core::api::dispatch() directly (in-process, no subprocess).

use serde_json::Value;

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
async fn call_api(request: Value) -> Result<Value, String> {
    // dispatch is synchronous (does git/fs I/O), run on blocking thread
    tauri::async_runtime::spawn_blocking(move || kiro_cc_core::api::dispatch(&request))
        .await
        .map_err(|e| format!("join error: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![call_api, app_version])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
