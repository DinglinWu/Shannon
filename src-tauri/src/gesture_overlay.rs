//! A passive status island matching the timer. Reads local telemetry, not a Python process per frame.
use serde_json::{json, Value};
use std::{path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, Mutex}, time::{Duration, SystemTime, UNIX_EPOCH}};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "gesture-status";
static STATUS: Mutex<Option<Value>> = Mutex::new(None);
static CLOSING: AtomicBool = AtomicBool::new(false);
static CREATING: Mutex<()> = Mutex::new(());

fn now() -> f64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64() }
fn root(app: &AppHandle) -> Option<PathBuf> {
    crate::gesture::root(app).ok()
}
fn timestamp(status: &Value) -> f64 { status["updated_at"].as_f64().unwrap_or(0.0) }
fn visible(status: &Value) -> bool {
    status["phase"].as_str() == Some("running")
        && status["process_running"].as_bool() == Some(true)
        && status["recognition_active"].as_bool() == Some(true)
        && status["telemetry_stale"].as_bool() != Some(true)
        && now() - timestamp(status) < 3.0
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn GetExitCodeProcess(handle: *mut std::ffi::c_void, exit_code: *mut u32) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    unsafe {
        let handle = OpenProcess(0x1000, 0, pid); // Query only; never terminates a process.
        if handle.is_null() { return false; }
        let mut code = 0;
        let alive = GetExitCodeProcess(handle, &mut code) != 0 && code == 259;
        CloseHandle(handle);
        alive
    }
}
#[cfg(not(windows))]
fn process_alive(_pid: u32) -> bool { true }

fn position(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window(LABEL) else { return Ok(()); };
    let monitor = window.current_monitor().map_err(|e| e.to_string())?
        .or(app.primary_monitor().map_err(|e| e.to_string())?).ok_or("未找到显示器")?;
    let scale = monitor.scale_factor();
    let origin = monitor.position();
    let width = (340.0 * scale).round() as i32;
    let screen_width = monitor.size().width as i32;
    let timer_here = crate::clock::clock_active(app.clone()) && app.get_webview_window("clock")
        .and_then(|clock| clock.current_monitor().ok().flatten())
        .map(|m| m.position() == monitor.position()).unwrap_or(false);
    let (left, top) = if timer_here && screen_width as f64 / scale >= 1000.0 {
        ((screen_width / 2 + (142.0 * scale).round() as i32).min(screen_width - width), 0)
    } else if timer_here {
        ((screen_width - width) / 2, (60.0 * scale).round() as i32)
    } else { ((screen_width - width) / 2, 0) };
    let target = PhysicalPosition::new(origin.x + left.max(0), origin.y + top);
    if window.outer_position().map_err(|e| e.to_string())? != target {
        window.set_position(target).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn ensure_window(app: &AppHandle) -> Result<(), String> {
    let _guard = CREATING.lock().map_err(|e| e.to_string())?;
    if app.get_webview_window(LABEL).is_some() { return position(app); }
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("gesture-status.html".into()))
        .title("闪念 · 手势状态").inner_size(340.0, 86.0)
        .decorations(false).transparent(true).shadow(false).resizable(false)
        .always_on_top(true).skip_taskbar(true).focused(false).visible(false)
        .build().map_err(|e| e.to_string())?;
    if let Some(main) = app.get_webview_window("main") {
        if let Ok(Some(monitor)) = main.current_monitor() {
            let scale = monitor.scale_factor();
            let _ = window.set_position(PhysicalPosition::new(
                monitor.position().x + (monitor.size().width as i32 - (340.0 * scale) as i32) / 2,
                monitor.position().y));
        }
    }
    position(app)?;
    window.show().map_err(|e| e.to_string())
}

fn sync_window(app: &AppHandle, status: &Value) {
    if visible(status) {
        if let Err(error) = ensure_window(app) { eprintln!("手势状态窗：{error}"); }
        let _ = app.emit_to(LABEL, "gesture-status", status);
    } else if let Some(window) = app.get_webview_window(LABEL) {
        if CLOSING.swap(true, Ordering::SeqCst) { return; }
        let _ = window.emit("gesture-exit", ());
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let keep = STATUS.lock().ok().and_then(|s| s.clone()).map(|s| visible(&s)).unwrap_or(false);
            if !keep { if let Some(window) = app.get_webview_window(LABEL) { let _ = window.destroy(); } }
            CLOSING.store(false, Ordering::SeqCst);
        });
    }
}

pub fn publish(app: &AppHandle, mut status: Value) {
    if !status.is_object() { return; }
    if status["updated_at"].as_f64().is_none() { status["updated_at"] = json!(now()); }
    let changed = if let Ok(mut current) = STATUS.lock() {
        if current.as_ref() == Some(&status) { false } else { *current = Some(status.clone()); true }
    } else { false };
    if changed { sync_window(app, &status); }
}

pub fn starting(app: &AppHandle) {
    publish(app, json!({"phase":"starting", "process_running":true, "recognition_active":false,
        "tracking":"STARTING", "updated_at":now(), "pending":true}));
}
pub fn failed(app: &AppHandle, message: &str) {
    publish(app, json!({"phase":"error", "process_running":false, "recognition_active":false,
        "error":message, "updated_at":now()}));
}

#[tauri::command]
pub fn gesture_overlay_snapshot() -> Value {
    STATUS.lock().ok().and_then(|s| s.clone()).unwrap_or_else(|| json!({"phase":"stopped"}))
}

#[tauri::command]
pub async fn gesture_overlay_control(app: AppHandle, action: String) -> Result<Value, String> {
    if !["stop"].contains(&action.as_str()) { return Err("不支持的状态窗操作".into()); }
    let status = gesture_overlay_snapshot();
    if status["process_running"].as_bool() == Some(false) {
        let stopped = json!({"phase":"stopped", "process_running":false, "updated_at":now()});
        publish(&app, stopped.clone());
        return Ok(stopped);
    }
    if status["managed"].as_bool() == Some(false) {
        let pid = status["pid"].as_u64().ok_or("缺少手势进程编号")?;
        let folder = root(&app).ok_or("尚未安装手势组件，请输入 /component install gesture")?.join(".runtime");
        let command = json!({"pid":pid, "action":action, "id":format!("overlay-{}", now()), "at":now()});
        let temporary = folder.join("overlay-command.tmp");
        tokio::fs::write(&temporary, command.to_string()).await.map_err(|e| e.to_string())?;
        // Windows rename cannot replace an existing destination: use an atomic
        // replace when possible; the worker tolerates a short missing-file gap.
        let destination = folder.join("overlay-command.json");
        if tokio::fs::rename(&temporary, &destination).await.is_err() {
            let _ = tokio::fs::remove_file(&destination).await;
            tokio::fs::rename(&temporary, &destination).await.map_err(|e| e.to_string())?;
        }
        return Ok(status);
    }
    crate::gesture::gesture_control(app, action, None).await
}

pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            if crate::native_service::requested() { let _=crate::builtin_plugins::init(&app); }
            let current = gesture_overlay_snapshot();
            let candidate = if let Some(root)=root(&app) {
                tokio::fs::read(root.join(".runtime/live-status.json")).await.ok()
                    .and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok())
            } else {None};
            if let Some(mut candidate) = candidate {
                let age = now() - timestamp(&candidate);
                if timestamp(&candidate) >= timestamp(&current) && age >= -2.0 {
                    if candidate["process_running"].as_bool() == Some(true) && age > 3.0 {
                        let alive = candidate["pid"].as_u64().map(|pid| process_alive(pid as u32)).unwrap_or(false);
                        candidate["telemetry_stale"] = json!(alive);
                        if !alive { candidate["phase"] = json!("stopped"); candidate["process_running"] = json!(false); }
                    }
                    if age < 60.0 || current["process_running"].as_bool() == Some(true) {
                        publish(&app, candidate);
                    }
                }
            }
            let current = gesture_overlay_snapshot();
            crate::voice_gesture::update(&app, &current);
            if current["pending"].as_bool() == Some(true) && now() - timestamp(&current) > 30.0 {
                failed(&app, "启动超时，请检查摄像头状态");
            } else if visible(&current) {
                let _ = position(&app);
            } else { sync_window(&app, &current); }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
}
