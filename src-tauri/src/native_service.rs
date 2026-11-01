//! Private parent/child JSON pipe. No public socket and no arbitrary native invocation.
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};
use tauri::{AppHandle, Listener, Manager};
pub fn requested() -> bool {
    std::env::args().any(|a| a == "--native-service")
}
pub fn plugin_id() -> Option<String> {
    let mut args=std::env::args();
    while let Some(arg)=args.next() { if arg=="--plugin-id" { return args.next(); } }
    None
}
fn allowed(name: &str, args: &Value, id: &str) -> bool {
    if let Some(owner)=crate::builtin_plugins::tool_owner(name) { return owner==id; }
    match name {
        "open_notes"|"close_notes"|"list_notes"|"set_note_done" => id=="note",
        "open_clock"|"clock_active" => id=="clock",
        "start_voice_input"|"stop_voice_input" => id=="voice",
        "voice_reset" => id=="voice" || id=="gesture",
        "gesture_control" => id=="gesture",
        "feature_can_remove" | "feature_shutdown" => args["id"]==id,
        "component_status"|"components_status" => ["voice","gesture"].contains(&id),
        "component_install"|"components_install" => args["name"]==id,
        _ => false,
    }
}
fn send(value: Value) {
    let out = std::io::stdout();
    let mut lock = out.lock();
    let _ = writeln!(lock, "{}", value);
    let _ = lock.flush();
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: u64,
    kind: String,
    name: String,
    args: Value,
}
fn number(a: &Value, k: &str) -> Result<u64, String> {
    a[k].as_u64().ok_or_else(|| format!("缺少{k}"))
}
fn string(a: &Value, k: &str) -> Result<String, String> {
    a[k].as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("缺少{k}"))
}
async fn command(app: &AppHandle, name: &str, a: Value) -> Result<Value, String> {
    let owner = match name {
        "open_notes" | "close_notes" | "list_notes" | "set_note_done" => Some("note"),
        "open_clock" | "clock_active" => Some("clock"),
        "gesture_control" => Some("gesture"),
        "start_voice_input" => Some("voice"),
        _ => None,
    };
    if let Some(id) = owner {
        if !["close_notes", "clock_active"].contains(&name) {
            crate::builtin_plugins::require(id)?;
        }
    }
    match name {
        "open_notes" => {
            crate::notes::open_notes(app.clone(), a["text"].as_str().unwrap_or("").into()).await?;
            Ok(Value::Null)
        }
        "close_notes" => {
            crate::notes::close_notes(app.clone()).await?;
            Ok(Value::Null)
        }
        "list_notes" => {
            serde_json::to_value(crate::notes::list_notes(app.clone())?).map_err(|e| e.to_string())
        }
        "set_note_done" => {
            crate::notes::set_note_done(
                app.clone(),
                number(&a, "id")?,
                a["done"].as_bool().ok_or("缺少done")?,
            )?;
            Ok(Value::Null)
        }
        "clock_active" => Ok(json!(crate::clock::clock_active(app.clone()))),
        "open_clock" => Ok(json!(
            crate::clock::open_clock(
                app.clone(),
                string(&a, "mode")?,
                number(&a, "seconds")?,
                a["restart"].as_bool().unwrap_or(false)
            )
            .await?
        )),
        "gesture_control" => {
            crate::gesture::gesture_control(
                app.clone(),
                string(&a, "action")?,
                a.get("payload").cloned(),
            )
            .await
        }
        "component_status" => crate::components::status(app),
        "component_install" => crate::components::request_install(app, &string(&a, "name")?),
        "start_voice_input" => {
            crate::voice::start_voice_input(
                app.clone(),
                app.get_webview_window("main").ok_or("缺少运行窗口")?,
                number(&a, "id")?,
                a["source"].as_str().map(str::to_owned),
            )?;
            Ok(Value::Null)
        }
        "stop_voice_input" | "voice_reset" => {
            crate::voice::reset();
            Ok(Value::Null)
        }
        "feature_shutdown" => {
            if plugin_id().as_deref()==Some("download") {crate::downloads::shutdown().await;}
            crate::voice::reset();
            Ok(json!({"stopped":true}))
        }
        "feature_can_remove" => {
            let id = string(&a, "id")?;
            let d = crate::builtin_plugins::definition(&id)?;
            if !d.window.is_empty() && app.get_webview_window(d.window).is_some() {
                return Err(format!("请先关闭{}窗口", d.name));
            }
            if id == "story" && crate::story::any_running() {
                return Err("故事仍在演绎".into());
            }
            if id == "voice" && crate::voice::active() {
                return Err("请先结束语音输入".into());
            }
            if id == "gesture"
                && crate::gesture_overlay::gesture_overlay_snapshot()["process_running"] == true
            {
                return Err("请先停止手势控制".into());
            }
            Ok(json!({"idle":true}))
        }
        _ => Err("不支持的插件入口".into()),
    }
}
pub fn start(app: AppHandle) -> Result<(), String> {
    let id=plugin_id().ok_or("缺少插件身份")?;
    crate::builtin_plugins::definition(&id)?;
    for name in [
        "voice-input",
        "gesture-voice",
        "gesture-status",
        "component-progress",
        "runtime-show-main",
        "tool-status",
    ] {
        let key = name.to_owned();
        app.listen_any(name, move |event| {
            if let Ok(payload) = serde_json::from_str::<Value>(event.payload()) {
                send(json!({"event":key,"payload":payload}));
            }
        });
    }
    send(json!({"ready":true,"protocol":1,"plugin_id":id,"version":app.package_info().version.to_string()}));
    if !(cfg!(debug_assertions) && std::env::var_os("SHANNON_PLUGIN_TEST_ROOT").is_some()) {
        if id=="gesture" { crate::gesture_overlay::watch(app.clone()); }
        if id=="wechat" { crate::wechat::watch(app.clone()); }
    }
    std::thread::Builder::new()
        .name("official-plugin-pipe".into())
        .spawn(move || {
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            loop {
                let mut line = String::new();
                match input.by_ref().take(65537).read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                if line.len() > 65536 {
                    break;
                }
                let request: Request = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let app = app.clone();
                let id=id.clone();
                tauri::async_runtime::spawn(async move {
                    let result = match if allowed(&request.name,&request.args,&id) { crate::builtin_plugins::init(&app) } else { Err("插件身份不允许此入口".into()) } {
                        Err(e) => Err(e),
                        Ok(()) => match request.kind.as_str() {
                            "tool"
                                if crate::builtin_plugins::tool_owner(&request.name).is_some()
                                    || request.name.starts_with("components_") =>
                            {
                                let output =
                                    crate::ai_tools::dispatch(&app, &request.name, request.args)
                                        .await;
                                if output["ok"] == true {
                                    Ok(output["data"].clone())
                                } else {
                                    Err(crate::plugin_protocol::error_message(&output["error"]))
                                }
                            }
                            "command" => command(&app, &request.name, request.args).await,
                            _ => Err("插件通信仅接受固定工具与入口".into()),
                        },
                    };
                    send(match result {
                        Ok(data) => json!({"id":request.id,"ok":true,"data":data}),
                        Err(error) => json!({"id":request.id,"ok":false,"error":crate::plugin_protocol::error_value(&error)}),
                    });
                });
            }
            app.exit(0);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)] mod boundary_tests {
    use super::*;
    #[test] fn worker_identity_cannot_route_other_plugins_or_management() {
        assert!(allowed("notes_list",&json!({}),"note"));
        assert!(!allowed("notes_add",&json!({"text":"x"}),"clock"));
        assert!(!allowed("plugins_install",&json!({}),"note"));
        assert!(allowed("component_install",&json!({"name":"voice"}),"voice"));
        assert!(!allowed("component_install",&json!({"name":"gesture"}),"voice"));
        assert!(!allowed("feature_can_remove",&json!({"id":"story"}),"note"));
    }
}
