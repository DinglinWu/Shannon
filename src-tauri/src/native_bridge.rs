use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin},
    sync::oneshot,
};
type Pending = Arc<Mutex<BTreeMap<u64, oneshot::Sender<Result<Value, String>>>>>;
struct Runtime {
    child: Child,
    input: ChildStdin,
    pending: Pending,
    alive: Arc<AtomicBool>,
}
static RUNTIME: tokio::sync::Mutex<BTreeMap<String, Runtime>> = tokio::sync::Mutex::const_new(BTreeMap::new());
static SEQUENCE: AtomicU64 = AtomicU64::new(1);
fn route(name: &str, args: &Value) -> Result<String, String> {
    let id = crate::builtin_plugins::tool_owner(name).or_else(|| owner(name))
        .or_else(|| match name {
            "voice_reset" => Some(if args["gesture"]==true {"gesture"} else {"voice"}),
            "feature_can_remove" | "feature_shutdown" => args["id"].as_str(),
            "component_status" | "component_install" | "components_status" | "components_install" => args["name"].as_str().or(Some(if crate::builtin_plugins::enabled("voice") {"voice"} else {"gesture"})),
            _ => None,
        }).ok_or("未知插件入口")?;
    crate::builtin_plugins::definition(id)?;
    Ok(id.to_owned())
}
/// Terminate only the removed feature, including its background tasks.
pub async fn stop(app: &AppHandle, id: &str) {
    if id=="download" && RUNTIME.lock().await.contains_key(id) {
        let _=tokio::time::timeout(std::time::Duration::from_secs(10),call(app,"command","feature_shutdown",json!({"id":id}))).await;
    }
    if let Some(mut runtime) = RUNTIME.lock().await.remove(id) {
        let _ = runtime.child.kill().await;
    }
}
pub fn start_background(app: &AppHandle) {
    let app=app.clone();
    tauri::async_runtime::spawn(async move {
        for id in ["wechat","gesture"] {
            if crate::builtin_plugins::enabled(id) {
                let mut runtimes=RUNTIME.lock().await;
                if crate::builtin_plugins::enabled(id) {let _=ensure(&app,id,&mut runtimes).await;}
            }
        }
    });
}
async fn ensure(app: &AppHandle, id: &str, slot: &mut BTreeMap<String, Runtime>) -> Result<(), String> {
    if let Some(r) = slot.get_mut(id) {
        if r.alive.load(Ordering::SeqCst)
            && r.child.try_wait().map_err(|e| e.to_string())?.is_none()
        {
            return Ok(());
        }
        let _ = r.child.start_kill();
    }
    let path = crate::official_distribution::verified_runtime(app,id)?;
    let mut command = tokio::process::Command::new(path);
    command
        .arg("--native-service").arg("--plugin-id").arg(id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let input = child.stdin.take().ok_or("插件输入管道不可用")?;
    let mut output = BufReader::new(child.stdout.take().ok_or("插件输出管道不可用")?);
    let mut ready = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        output.read_line(&mut ready),
    )
    .await
    .map_err(|_| "插件启动超时")?
    .map_err(|e| e.to_string())?;
    let ready: Value = serde_json::from_str(&ready).map_err(|_| "插件握手无效")?;
    if ready["ready"] != true || ready["protocol"] != 1 || ready["plugin_id"] != id || ready["version"] != crate::official_distribution::runtime_spec(id)?["version"] {
        return Err("插件运行版本与主程序不一致，请更新插件".into());
    }
    let pending: Pending = Arc::new(Mutex::new(BTreeMap::new()));
    let alive = Arc::new(AtomicBool::new(true));
    let reader_pending = pending.clone();
    let reader_alive = alive.clone();
    let app = app.clone();
    tokio::spawn(async move {
        loop {
            let mut line = String::new();
            match output.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if line.len() > 4 * 1024 * 1024 {
                break;
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                break;
            };
            if let Some(event) = v["event"].as_str() {
                if event == "runtime-show-main" {
                    crate::show_window(&app, None);
                } else if [
                    "voice-input",
                    "gesture-voice",
                    "gesture-status",
                    "component-progress",
                    "tool-status",
                ]
                .contains(&event)
                {
                    let _ = app.emit(event, v["payload"].clone());
                }
            } else if let Some(id) = v["id"].as_u64() {
                if let Some(sender) = reader_pending.lock().ok().and_then(|mut p| p.remove(&id)) {
                    let _ = sender.send(if v["ok"] == true {
                        Ok(v["data"].clone())
                    } else {
                        Err(crate::plugin_protocol::error_message(&v["error"]))
                    });
                }
            }
        }
        reader_alive.store(false, Ordering::SeqCst);
        if let Ok(mut p) = reader_pending.lock() {
            for (_, sender) in std::mem::take(&mut *p) {
                let _ = sender.send(Err(
                    "outcome_unknown:插件进程已退出，请查询状态后再重试".into()
                ));
            }
        }
    });
    slot.insert(id.to_owned(), Runtime {
        child,
        input,
        pending,
        alive,
    });
    Ok(())
}
pub async fn call(app: &AppHandle, kind: &str, name: &str, args: Value) -> Result<Value, String> {
    if ["component_status","components_status"].contains(&name)
        && !["voice","gesture"].iter().any(|id| crate::builtin_plugins::enabled(id)) {
        return Ok(json!({"components":[],"installing":false}));
    }
let plugin = route(name, &args)?;
    let mut runtime = RUNTIME.lock().await;
    if !["feature_can_remove", "feature_shutdown", "voice_reset", "stop_voice_input"].contains(&name) {
        crate::builtin_plugins::require(&plugin)?;
    }
    ensure(app, &plugin, &mut runtime).await?;
    let id = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let pending = runtime.get(&plugin).unwrap().pending.clone();
    let (sender, receiver) = oneshot::channel();
    pending
        .lock()
        .map_err(|e| e.to_string())?
        .insert(id, sender);
    let line = format!("{}\n", json!({"id":id,"kind":kind,"name":name,"args":args}));
    if let Err(e) = runtime
        .get_mut(&plugin)
        .unwrap()
        .input
        .write_all(line.as_bytes())
        .await
    {
        pending.lock().ok().map(|mut p| p.remove(&id));
        return Err(format!("outcome_unknown:插件管道写入失败：{e}"));
    }
    drop(runtime);
    let result = tokio::time::timeout(std::time::Duration::from_secs(180), receiver)
        .await
        .map_err(|_| "outcome_unknown:插件响应超时，修改可能已经完成".to_string())
        .and_then(|r| r.map_err(|_| "插件已退出".to_string()))
        .and_then(|r| r);
    pending.lock().ok().map(|mut p| p.remove(&id));
    result
}
pub async fn can_remove(app: &AppHandle, id: &str) -> Result<(), String> {
    if !RUNTIME.lock().await.contains_key(id) {
        return Ok(());
    }
    call(app, "command", "feature_can_remove", json!({"id":id}))
        .await
        .map(|_| ())
}
pub fn reset_voice(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if RUNTIME.lock().await.contains_key("voice") {
            let _ = call(&app, "command", "voice_reset", json!({})).await;
        }
        if RUNTIME.lock().await.contains_key("gesture") {
            let _ = call(&app, "command", "voice_reset", json!({"gesture":true})).await;
        }
    });
}
pub async fn shutdown() {
    let runtimes=std::mem::take(&mut *RUNTIME.lock().await);
    let mut tasks=Vec::new();
    for (_,mut r) in runtimes {
        // EOF lets the worker run its normal download/device cleanup.
        drop(r.input);
        tasks.push(tokio::spawn(async move {
            if tokio::time::timeout(std::time::Duration::from_secs(10),r.child.wait()).await.is_err() {
                let _=r.child.kill().await;
            }
        }));
    }
    for task in tasks {let _=task.await;}
}
fn owner(command: &str) -> Option<&'static str> {
    match command {
        "open_notes" | "close_notes" | "list_notes" | "set_note_done" => Some("note"),
        "open_clock" | "clock_active" => Some("clock"),
        "gesture_control" => Some("gesture"),
        "start_voice_input" | "stop_voice_input" => Some("voice"),
        "component_status" | "component_install" => None,
        _ => None,
    }
}
#[cfg(test)] mod routing_tests {
    use super::*;
    #[test] fn feature_processes_are_selected_from_fixed_owners() {
        assert_eq!(route("notes_add",&json!({})).unwrap(),"note");
        assert_eq!(route("components_install",&json!({"name":"gesture"})).unwrap(),"gesture");
        assert!(route("components_install",&json!({"name":"shell"})).is_err());
        assert!(route("run_shell",&json!({})).is_err());
    }
}
#[tauri::command]
pub async fn plugin_native_call(
    app: AppHandle,
    window: tauri::WebviewWindow,
    command: String,
    args: Value,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("仅主搜索框可使用插件入口".into());
    }
    if ![
        "open_notes",
        "close_notes",
        "list_notes",
        "set_note_done",
        "open_clock",
        "clock_active",
        "gesture_control",
        "start_voice_input",
        "stop_voice_input",
        "component_status",
        "component_install",
    ]
    .contains(&command.as_str())
    {
        return Err("未知插件入口".into());
    }
    if let Some(id) = owner(&command) {
        crate::builtin_plugins::require(id)?;
    }
    if command == "start_voice_input" && !window.is_focused().map_err(|e| e.to_string())? {
        return Err("请先聚焦搜索框".into());
    }
    if command == "component_status"
        && !["voice","gesture"].iter().any(|id| crate::builtin_plugins::enabled(id) && crate::official_distribution::verified_runtime(&app,id).is_ok())
    {
        return Ok(json!({"components":[],"installing":false}));
    }
    call(&app, "command", &command, args).await
}
