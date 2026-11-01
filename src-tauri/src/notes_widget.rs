//! A bounded geometry API for the notes widget only.
use serde::{Deserialize, Serialize};
use std::{fs, sync::atomic::Ordering};
use tauri::{AppHandle, Manager};
#[derive(Clone, Serialize, Deserialize)]
pub struct Anchor {
    pub x: i32,
    pub y: i32,
}
pub fn initial_state(app: &AppHandle) -> Result<Anchor, String> {
    let main = app.get_webview_window("main").ok_or("未找到主窗口")?;
    let monitors = main.available_monitors().map_err(|e| e.to_string())?;
    let monitor = main
        .current_monitor()
        .map_err(|e| e.to_string())?
        .or_else(|| monitors.first().cloned())
        .ok_or("未找到显示器")?;
    let area = monitor.work_area();
    let size = (64.0 * monitor.scale_factor()).round() as i32;
    let fallback = Anchor {
        x: area.position.x + (area.size.width as i32 - size).max(0),
        y: area.position.y + 48.min((area.size.height as i32 - size).max(0)),
    };
    let saved = fs::read(super::config_dir(app).join("notes-widget.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Anchor>(&b).ok())
        .unwrap_or(fallback.clone());
    for m in &monitors {
        let a = m.work_area();
        let size = (64.0 * m.scale_factor()).round() as i32;
        if saved.x >= a.position.x
            && saved.y >= a.position.y
            && (saved.x as i64) < a.position.x as i64 + a.size.width as i64
            && (saved.y as i64) < a.position.y as i64 + a.size.height as i64
        {
            return Ok(Anchor {
                x: saved.x.clamp(
                    a.position.x,
                    a.position.x + (a.size.width as i32 - size).max(0),
                ),
                y: saved.y.clamp(
                    a.position.y,
                    a.position.y + (a.size.height as i32 - size).max(0),
                ),
            });
        }
    }
    Ok(fallback)
}
fn notes_only(w: &tauri::WebviewWindow) -> Result<(), String> {
    if w.label() == "notes" {
        Ok(())
    } else {
        Err("仅便签窗口可调整自己的位置".into())
    }
}
#[tauri::command]
pub fn notes_widget_state(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<serde_json::Value, String> {
    notes_only(&window)?;
    Ok(
        serde_json::json!({"anchor":initial_state(&app)?,"reveal":crate::notes::REVEAL_ON_LOAD.swap(false,Ordering::SeqCst)}),
    )
}
#[tauri::command]
pub fn notes_widget_save(
    app: AppHandle,
    window: tauri::WebviewWindow,
    x: i32,
    y: i32,
) -> Result<(), String> {
    notes_only(&window)?;
    if !window
        .available_monitors()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|m| {
            let a = m.work_area();
            x >= a.position.x
                && y >= a.position.y
                && (x as i64) < a.position.x as i64 + a.size.width as i64
                && (y as i64) < a.position.y as i64 + a.size.height as i64
        })
    {
        return Err("便签位置不在屏幕内".into());
    }
    let path = super::config_dir(&app).join("notes-widget.json");
    let temp = path.with_extension("json.next");
    fs::write(
        &temp,
        serde_json::to_vec(&Anchor { x, y }).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn notes_widget_bounds(
    window: tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    notes_only(&window)?;
    if !(40..=4096).contains(&width) || !(40..=4096).contains(&height) {
        return Err("便签尺寸无效".into());
    }
    if !window
        .available_monitors()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|m| {
            let a = m.work_area();
            x as i64 >= a.position.x as i64 - 8
                && y as i64 >= a.position.y as i64 - 8
                && (x as i64 + width as i64) <= a.position.x as i64 + a.size.width as i64 + 8
                && (y as i64 + height as i64) <= a.position.y as i64 + a.size.height as i64 + 8
        })
    {
        return Err("便签边界超出屏幕".into());
    }
    #[cfg(windows)]
    {
        let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as usize;
        let (tx, rx) = tokio::sync::oneshot::channel();
        window
            .run_on_main_thread(move || {
                #[link(name = "user32")]
                extern "system" {
                    fn SetWindowPos(
                        hwnd: *mut std::ffi::c_void,
                        after: *mut std::ffi::c_void,
                        x: i32,
                        y: i32,
                        w: i32,
                        h: i32,
                        flags: u32,
                    ) -> i32;
                }
                let ok = unsafe {
                    SetWindowPos(
                        hwnd as *mut _,
                        std::ptr::null_mut(),
                        x,
                        y,
                        width as i32,
                        height as i32,
                        0x0014,
                    )
                };
                let _ = tx.send(if ok != 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error().to_string())
                });
            })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        window
            .set_size(tauri::PhysicalSize::new(width, height))
            .map_err(|e| e.to_string())?;
        window
            .set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())
    }
}

/// Clip the native drawing AND hit-test region without resizing the WebView.
/// A transparent Windows WebView can otherwise lose one composition frame on shrink.
#[tauri::command]
pub async fn notes_widget_mask(
    window: tauri::WebviewWindow,
    x: Option<i32>,
    y: Option<i32>,
    size: Option<u32>,
) -> Result<(), String> {
    notes_only(&window)?;
    let mask = match (x, y, size) {
        (None, None, None) => None,
        (Some(x), Some(y), Some(size)) => {
            let bounds = window.inner_size().map_err(|e| e.to_string())?;
            if x < 0
                || y < 0
                || !(40..=512).contains(&size)
                || x as u64 + size as u64 > bounds.width as u64 + 2
                || y as u64 + size as u64 > bounds.height as u64 + 2
            {
                return Err("便签裁剪范围无效".into());
            }
            Some((x, y, size as i32))
        }
        _ => return Err("便签裁剪参数不完整".into()),
    };
    #[cfg(windows)]
    {
        let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as usize;
        let (tx, rx) = tokio::sync::oneshot::channel();
        window
            .run_on_main_thread(move || {
                #[link(name = "gdi32")]
                extern "system" {
                    fn CreateRectRgn(
                        left: i32,
                        top: i32,
                        right: i32,
                        bottom: i32,
                    ) -> *mut std::ffi::c_void;
                    fn DeleteObject(object: *mut std::ffi::c_void) -> i32;
                }
                #[link(name = "user32")]
                extern "system" {
                    fn SetWindowRgn(
                        hwnd: *mut std::ffi::c_void,
                        region: *mut std::ffi::c_void,
                        redraw: i32,
                    ) -> i32;
                }
                let region = match mask {
                    Some((x, y, size)) => unsafe { CreateRectRgn(x, y, x + size, y + size) },
                    None => std::ptr::null_mut(),
                };
                if mask.is_some() && region.is_null() {
                    let _ = tx.send(Err("创建便签裁剪区域失败".to_string()));
                    return;
                }
                let ok = unsafe { SetWindowRgn(hwnd as *mut _, region, 1) };
                // Windows owns the region after success. On failure the caller owns it.
                let result = if ok != 0 {
                    Ok(())
                } else {
                    let error = std::io::Error::last_os_error().to_string();
                    if !region.is_null() {
                        unsafe {
                            DeleteObject(region);
                        }
                    }
                    Err(error)
                };
                let _ = tx.send(result);
            })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = mask;
        Err("当前系统不支持便签原生裁剪".into())
    }
}
