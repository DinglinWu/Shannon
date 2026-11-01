use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
pub fn open(app: &AppHandle) -> Result<Value, String> {
    if let Some(w) = app.get_webview_window("plugin-manager") {
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
    } else {
        tauri::WebviewWindowBuilder::new(
            app,
            "plugin-manager",
            tauri::WebviewUrl::App("plugin-manager.html".into()),
        )
        .title("闪念 · 插件管理")
        .inner_size(940., 720.)
        .min_inner_size(660., 480.)
        .build()
        .map_err(|e| e.to_string())?;
    }
    Ok(json!({"opened":true}))
}
fn own(w: &tauri::WebviewWindow) -> Result<(), String> {
    if ["main", "plugin-manager"].contains(&w.label()) {
        Ok(())
    } else {
        Err("仅插件管理与搜索框可执行此操作".into())
    }
}
pub fn catalog() -> Result<Value, String> {
    let mut value = crate::builtin_plugins::list();
    value["plugins"] = crate::plugins::list()?["plugins"].clone();
    Ok(value)
}
#[tauri::command]
pub async fn plugin_manager_open(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Value, String> {
    own(&window)?;
    open(&app)
}
#[tauri::command]
pub fn plugin_manager_catalog(window: tauri::WebviewWindow) -> Result<Value, String> {
    own(&window)?;
    catalog()
}
#[tauri::command]
pub async fn builtin_plugin_change(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    action: String,
    source: Option<String>,
) -> Result<Value, String> {
    own(&window)?;
    crate::plugins::request_builtin_change_from(&app, &id, &action, source.as_deref())
}
