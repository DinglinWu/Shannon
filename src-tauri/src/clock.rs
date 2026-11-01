use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

static CLOCK_ACTIVE: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn clock_active(app: AppHandle) -> bool {
    app.get_webview_window("clock").is_some() && CLOCK_ACTIVE.load(Ordering::Relaxed)
}

#[tauri::command]
pub async fn open_clock(app: AppHandle, mode: String, seconds: u64, restart: bool) -> Result<bool, String> {
    crate::builtin_plugins::require("clock")?;
    if mode != "up" && mode != "down" {
        return Err("无效的计时方式".into());
    }
    if seconds > 86400 {
        return Err("倒计时最长为 24 小时".into());
    }
    if let Some(window) = app.get_webview_window("clock") {
        if restart {
            // 复用窗口，先由前端播放退出动画，再重新打开设置。
            window.emit("clock-restart", serde_json::json!({ "mode": mode, "seconds": seconds }))
                .map_err(|e| e.to_string())?;
            return Ok(true);
        }
        if CLOCK_ACTIVE.load(Ordering::Relaxed) {
            return Ok(false);
        }
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(true);
    }
    let main = app.get_webview_window("main").ok_or("未找到主窗口")?;
    let monitor = main.current_monitor().map_err(|e| e.to_string())?.ok_or("未找到显示器")?;
    let scale = monitor.scale_factor();
    let width = (400.0 * scale).round() as u32;
    let height = (510.0 * scale).round() as u32;
    let origin = monitor.position();
    let screen = monitor.size();
    let window = WebviewWindowBuilder::new(
        &app,
        "clock",
        WebviewUrl::App(format!("clock.html?mode={mode}&seconds={seconds}").into()),
    )
    .title("闪念 · 计时器")
    .inner_size(400.0, 510.0)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()
    .map_err(|e| e.to_string())?;
    let result = (|| {
        window.set_position(PhysicalPosition::new(
            origin.x + (screen.width as i64 - width as i64).max(0) as i32 / 2,
            origin.y + (screen.height as i64 - height as i64).max(0) as i32 / 2,
        ))?;
        window.set_size(PhysicalSize::new(width, height))?;
        window.show()?;
        window.set_focus()
    })();
    if let Err(err) = result {
        let _ = window.destroy();
        return Err(err.to_string());
    }
    CLOCK_ACTIVE.store(false, Ordering::Relaxed);
    Ok(true)
}

#[tauri::command]
pub async fn reset_clock(app: AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("clock").ok_or("未找到计时器")?;
    let monitor = window.current_monitor().map_err(|e| e.to_string())?.ok_or("未找到显示器")?;
    let scale = monitor.scale_factor();
    let width = (400.0 * scale).round() as u32;
    let height = (510.0 * scale).round() as u32;
    let origin = monitor.position();
    window.set_size(PhysicalSize::new(width, height)).map_err(|e| e.to_string())?;
    window.set_position(PhysicalPosition::new(
        origin.x + (monitor.size().width as i64 - width as i64).max(0) as i32 / 2,
        origin.y + (monitor.size().height as i64 - height as i64).max(0) as i32 / 2,
    )).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    CLOCK_ACTIVE.store(false, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn dock_clock(app: AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("clock").ok_or("未找到计时器")?;
    let monitor = window.current_monitor().map_err(|e| e.to_string())?.ok_or("未找到显示器")?;
    let scale = monitor.scale_factor();
    let width = (260.0 * scale).round() as u32;
    let height = (54.0 * scale).round() as u32;
    // 使用显示器物理边缘（非工作区），适配副屏负坐标及不同 DPI。
    let origin = monitor.position();
    let target = PhysicalPosition::new(
        origin.x + (monitor.size().width as i64 - width as i64) as i32 / 2,
        origin.y,
    );
    let from = window.outer_position().map_err(|e| e.to_string())?;
    let original_size = window.inner_size().map_err(|e| e.to_string())?;
    // 页面此时透明；一次定位后由 WebView 从顶部中央展开岛体。
    let result = (|| {
        window.set_always_on_top(true)?;
        window.set_size(PhysicalSize::new(width, height))?;
        window.set_position(target)
    })();
    if let Err(err) = result {
        // 定位失败时恢复设置面板所在的窗口。
        let _ = window.set_position(from);
        let _ = window.set_size(original_size);
        return Err(err.to_string());
    }
    CLOCK_ACTIVE.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn close_clock(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("clock") {
        window.destroy().map_err(|e| e.to_string())?;
    }
    CLOCK_ACTIVE.store(false, Ordering::Relaxed);
    Ok(())
}
