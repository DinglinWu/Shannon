use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};

static SESSION: AtomicU64 = AtomicU64::new(0);
static STOP: AtomicBool = AtomicBool::new(false);
static GESTURE_SESSION: AtomicU64 = AtomicU64::new(0);
pub fn reset() { STOP.store(true, Ordering::SeqCst); }
pub fn active() -> bool { SESSION.load(Ordering::SeqCst) != 0 }
pub fn stop_gesture_input() {
    let id = GESTURE_SESSION.load(Ordering::SeqCst);
    if id != 0 && SESSION.load(Ordering::SeqCst) == id { reset(); }
}
fn event(app: &AppHandle, id: u64, state: &str, text: &str) {
    let _ = app.emit_to("main", "voice-input", serde_json::json!({"id":id,"state":state,"text":text}));
}

#[tauri::command]
pub fn stop_voice_input(window: tauri::WebviewWindow, id: u64) -> Result<(), String> {
    if window.label() != "main" { return Err("仅主搜索框可以控制语音输入".into()); }
    if SESSION.load(Ordering::SeqCst) == id { reset(); }
    Ok(())
}

#[tauri::command]
pub fn start_voice_input(app: AppHandle, window: tauri::WebviewWindow, id: u64, source: Option<String>) -> Result<(), String> {
    crate::builtin_plugins::require("voice")?;
    let gesture = match source.as_deref().unwrap_or("mouse") {
        "mouse" => false,
        "gesture" if crate::voice_gesture::held() => true,
        "gesture" => return Err("已松拳，语音输入已取消".into()),
        _ => return Err("不支持的语音输入来源".into()),
    };
    if window.label() != "main" || (!crate::native_service::requested()&&!window.is_focused().map_err(|e| e.to_string())?) {
        return Err("请先将光标放在搜索框中".into());
    }
    crate::components::installed_dir(&app, "voice")?;
    if id == 0 || SESSION.compare_exchange(0, id, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("上一段语音尚未结束，请稍候".into());
    }
    STOP.store(false, Ordering::SeqCst);
    GESTURE_SESSION.store(if gesture { id } else { 0 }, Ordering::SeqCst);
    if gesture && !crate::voice_gesture::held() { reset(); }
    let worker = std::thread::Builder::new().name("sensevoice".into()).spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| recognize(&app, id)))
            .unwrap_or_else(|_| Err("本地语音识别异常，请重新尝试".into()));
        SESSION.compare_exchange(id, 0, Ordering::SeqCst, Ordering::SeqCst).ok();
        match result {
            Ok(()) => event(&app, id, "stopped", ""),
            Err(error) => event(&app, id, "error", &error),
        }
    });
    if let Err(error) = worker { SESSION.store(0, Ordering::SeqCst); return Err(error.to_string()); }
    Ok(())
}

struct Worker {
    child: std::process::Child,
    input: std::process::ChildStdin,
    output: std::sync::mpsc::Receiver<serde_json::Value>,
    root: std::path::PathBuf,
}
impl Drop for Worker { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }
static WORKER: std::sync::Mutex<Option<Worker>> = std::sync::Mutex::new(None);

fn recognize(app: &AppHandle, id: u64) -> Result<(), String> {
    use std::{io::{BufRead, BufReader, Write}, process::{Command, Stdio}, time::{Duration, Instant}};
    let root = crate::components::installed_dir(app, "voice")?;
    let mut slot = WORKER.lock().map_err(|_| "语音组件异常，请重启应用")?;
    let replace = slot.as_mut().map(|w| w.root != root || w.child.try_wait().ok().flatten().is_some()).unwrap_or(true);
    if replace {
        *slot = None;
        let mut command = Command::new(root.join("shannon-voice.exe"));
        command.arg(root.join("assets")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
        let mut child = command.spawn().map_err(|e| format!("语音组件启动失败：{e}，请通过 /component install voice 重新安装"))?;
        let input = child.stdin.take().ok_or("无法连接语音组件")?;
        let stdout = child.stdout.take().ok_or("无法读取语音组件")?;
        let (tx, output) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.len() > 65536 { break; }
                if let Ok(value) = serde_json::from_str(&line) { if tx.send(value).is_err() { break; } }
            }
        });
        *slot = Some(Worker { child, input, output, root });
    }
    let worker = slot.as_mut().unwrap();
    writeln!(worker.input,"{}",serde_json::json!({"op":"start","id":id})).map_err(|e|e.to_string())?;
    worker.input.flush().map_err(|e|e.to_string())?;
    let mut stopping = false;
    let started = Instant::now();
    loop {
        if STOP.load(Ordering::SeqCst) && !stopping {
            writeln!(worker.input,"{}",serde_json::json!({"op":"stop","id":id})).map_err(|e|e.to_string())?;
            worker.input.flush().map_err(|e|e.to_string())?;
            stopping = true;
        }
        if started.elapsed() > Duration::from_secs(150) {
            *slot = None; return Err("语音组件响应超时，已停止录音".into());
        }
        match worker.output.recv_timeout(Duration::from_millis(50)) {
            Ok(value) if value["id"].as_u64() == Some(id) => {
                let state = value["state"].as_str().unwrap_or("");
                let text = value["text"].as_str().unwrap_or("");
                match state {
                    "stopped" => return Ok(()),
                    "error" => return Err(text.to_owned()),
                    "loading" | "listening" | "stopping" | "result" => event(app,id,state,text),
                    _ => {},
                }
            },
            Ok(_) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {},
            Err(_) => { *slot = None; return Err("语音组件意外退出，已停止录音".into()); },
        }
    }
}

