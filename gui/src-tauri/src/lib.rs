use serde_json::Value;
use std::process::Command;

#[tauri::command]
async fn call_api(request: Value) -> Result<Value, String> {
    let json_arg = serde_json::to_string(&request).map_err(|e| e.to_string())?;

    let output = Command::new("kiro-cc-plugins-api")
        .arg(&json_arg)
        .output()
        .map_err(|e| format!("Failed to run sidecar: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Sidecar error: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).map_err(|e| format!("Invalid JSON: {e}\n{stdout}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![call_api])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
