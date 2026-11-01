//! Local Gesture Mouse bridge. Does not send commands through a shell.
use serde_json::Value;
use std::{path::PathBuf, time::Duration};
use tauri::AppHandle;

pub fn root(app: &AppHandle) -> Result<PathBuf,String> {
    if let Some(path)=std::env::var_os("GESTURE_MOUSE_HOME") {return Ok(PathBuf::from(path));}
    crate::components::installed_dir(app,"gesture")
}

#[tauri::command]
pub async fn gesture_control(app: AppHandle, action: String, payload: Option<Value>) -> Result<Value, String> {
    if !["status","stop"].contains(&action.as_str()){crate::builtin_plugins::require("gesture")?;}
    if action == "start" || action == "restart" {
        crate::gesture_overlay::starting(&app);
    }
    let result = execute(&app, action.clone(), payload).await;
    match &result {
        Ok(value) if ["start", "restart", "stop", "status"].contains(&action.as_str()) => {
            crate::gesture_overlay::publish(&app, value.clone());
        }
        Err(error) if action == "start" || action == "restart" => {
            crate::gesture_overlay::failed(&app, error);
        }
        _ => {}
    }
    result
}

async fn execute(app: &AppHandle, action: String, payload: Option<Value>) -> Result<Value, String> {
    if !["start", "stop", "status", "restart", "cameras", "config", "configure"].contains(&action.as_str()) {
        return Err("不支持的手势命令".into());
    }
    let root = root(app)?;
    let portable = root.join("python/python.exe");
    let python = if portable.is_file() {portable} else {root.join(".venv/Scripts/python.exe")};
    let script = root.join("control.py");
    if !python.is_file() || !script.is_file() {
        return Err("手势组件不完整，请输入 /component install gesture".into());
    }
    let body = payload.unwrap_or_else(|| serde_json::json!({}));
    if !body.is_object() {
        return Err("手势参数必须是 JSON 对象".into());
    }
    let mut command = tokio::process::Command::new(python);
    command.arg(script).arg(action).arg("--json").arg(body.to_string())
        .current_dir(root).env("PYTHONIOENCODING", "utf-8")
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let output = tokio::time::timeout(Duration::from_secs(45), command.output())
        .await.map_err(|_| "手势接口调用超时，请用 /gesture status 查询实际状态".to_string())?
        .map_err(|e| format!("无法调用手势程序：{e}"))?;
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("手势接口响应无效：{e}；{}", String::from_utf8_lossy(&output.stderr)))?;
    if !output.status.success() || value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(value.get("error").and_then(Value::as_str).unwrap_or("手势调用失败").to_string());
    }
    Ok(value["data"].clone())
}
