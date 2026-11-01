//! Persistent availability of bundled features; removal never deletes user data.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager};
pub struct Definition {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub window: &'static str,
}
pub const DEFINITIONS: &[Definition] = &[
    Definition {
        id: "note",
        name: "便签",
        description: "桌面便签、待办与日期安排",
        window: "notes",
    },
    Definition {
        id: "clock",
        name: "计时器",
        description: "正计时与倒计时",
        window: "clock",
    },
    Definition {
        id: "wechat",
        name: "微信模拟聊天",
        description: "虚拟人物聊天、朋友圈与后台日常",
        window: "wechat",
    },
    Definition {
        id: "story",
        name: "故事舞台",
        description: "场景演绎、人物、记忆与角色微信",
        window: "story",
    },
    Definition {
        id: "broadcast",
        name: "广播站",
        description: "在局域网分享内容",
        window: "broadcast",
    },
    Definition {
        id: "download",
        name: "下载",
        description: "Gopeed下载任务与进度管理",
        window: "downloads",
    },
    Definition {
        id: "voice",
        name: "语音输入",
        description: "可选离线语音组件，添加后仍需单独安装组件",
        window: "",
    },
    Definition {
        id: "gesture",
        name: "手势控制",
        description: "可选手势组件，添加后仍需安装组件和手动开启",
        window: "gesture-status",
    },
];
#[derive(Clone, Copy, Default, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Enabled,
    Disabled,
    Removed,
}
struct State {
    path: PathBuf,
    states: BTreeMap<String, Status>,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
pub fn definition(id: &str) -> Result<&'static Definition, String> {
    DEFINITIONS
        .iter()
        .find(|d| d.id == id)
        .ok_or("未知内置插件".into())
}
pub fn init(app: &AppHandle) -> Result<(), String> {
    let path = crate::config_dir(app).join("builtin-plugins.json");
    let states = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<BTreeMap<String, Status>>(&bytes)
            .map_err(|e| format!("内置插件配置损坏：{e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(e.to_string()),
    };
    *STATE.lock().map_err(|e| e.to_string())? = Some(State { path, states });
    Ok(())
}
pub fn status(id: &str) -> Status {
    #[cfg(feature="native-runtime")]
    if crate::native_service::requested() {
        // A worker may use declared business dependencies internally, but may
        // never start unrelated features or resurrect missing availability.
        let identity=crate::native_service::plugin_id();
        let allowed=identity.as_deref()==Some(id)
            || (identity.as_deref()==Some("story") && id=="wechat")
            || (identity.as_deref()==Some("gesture") && id=="voice");
        if !allowed {return Status::Removed;}
    }
    STATE
        .lock()
        .ok()
        .and_then(|s| s.as_ref().and_then(|s| s.states.get(id).copied()))
        .unwrap_or(if cfg!(feature = "native-runtime") && !is_service() {
            Status::Enabled
        } else {
            Status::Removed
        })
}
fn is_service() -> bool {
    #[cfg(feature="native-runtime")] {crate::native_service::requested()}
    #[cfg(not(feature="native-runtime"))] {false}
}
pub fn enabled(id: &str) -> bool {
    status(id) == Status::Enabled
}
pub fn require(id: &str) -> Result<(), String> {
    let d = definition(id)?;
    if enabled(id) {
        Ok(())
    } else {
        Err(format!("{}已停用或移除，请在插件管理中添加或启用", d.name))
    }
}
pub fn tool_owner(name: &str) -> Option<&'static str> {
    [
        ("notes_", "note"),
        ("clock_", "clock"),
        ("wechat_", "wechat"),
        ("story_", "story"),
        ("broadcast_", "broadcast"),
        ("downloads_", "download"),
    ]
    .into_iter()
    .find(|(prefix, _)| name.starts_with(prefix))
    .map(|(_, id)| id)
}
pub fn require_tool(name: &str) -> Result<(), String> {
    if let Some(id) = tool_owner(name) {
        require(id)
    } else {
        Ok(())
    }
}
pub fn state_name(s: Status) -> &'static str {
    match s {
        Status::Enabled => "enabled",
        Status::Disabled => "disabled",
        Status::Removed => "removed",
    }
}
pub fn list() -> Value {
    let metadata = crate::official_distribution::metadata();
    let mut value = json!({"builtins":DEFINITIONS.iter().map(|d|json!({"id":d.id,"name":d.name,"description":d.description,"status":status(d.id),"builtin":true})).collect::<Vec<_>>()});
    if let Some(items) = value["builtins"].as_array_mut() {
        for item in items {
            if let Some(meta) = metadata["plugins"]
                .as_array()
                .and_then(|all| all.iter().find(|p| p["id"] == item["id"]))
            {
                for key in [
                    "author",
                    "version",
                    "features",
                    "source",
                    "dependencies",
                    "distribution",
                ] {
                    item[key] = meta[key].clone();
                }
            }
        }
    }
    value
}
pub fn validate_action(id: &str, action: &str) -> Result<Status, String> {
    definition(id)?;
    match action {
        "enable" | "install" => Ok(Status::Enabled),
        "disable" => Ok(Status::Disabled),
        "uninstall" => Ok(Status::Removed),
        _ => Err("内置插件操作无效".into()),
    }
}
pub fn change(app: &AppHandle, id: &str, action: &str, expected: &str) -> Result<Value, String> {
    let target = validate_action(id, action)?;
    let d = definition(id)?;
    if target != Status::Enabled {
        if !d.window.is_empty() && app.get_webview_window(d.window).is_some() {
            return Err(format!(
                "请先关闭{}窗口，再停用或移除，避免中断未保存操作",
                d.name
            ));
        }
        #[cfg(feature = "native-runtime")]
        if id == "story" && crate::story::any_running() {
            return Err("故事仍在后台演绎，请完成后重试".into());
        }
        #[cfg(feature = "native-runtime")]
        if id == "voice" && crate::voice::active() {
            return Err("请先结束当前语音输入".into());
        }
        #[cfg(feature = "native-runtime")]
        if id == "gesture"
            && crate::gesture_overlay::gesture_overlay_snapshot()["process_running"] == true
        {
            return Err("请先停止手势控制".into());
        }
    }
    let mut lock = STATE.lock().map_err(|e| e.to_string())?;
    let state = lock.as_mut().ok_or("内置插件配置尚未就绪")?;
    persist_change(state, id, action, expected)?;
    drop(lock);
    let value = json!({"plugin":id,"builtin":true,"action":action,"status":target,"completed":true,"data_preserved":true});
    if id == "wechat" && target != Status::Enabled {
        app.remove_tray_by_id("wechat-plugin");
    }
    Ok(value)
}

fn persist_change(state: &mut State, id: &str, action: &str, expected: &str) -> Result<(), String> {
    let target = validate_action(id, action)?;
    let current = state
        .states
        .get(id)
        .copied()
        .unwrap_or(if cfg!(feature = "native-runtime") {
            Status::Enabled
        } else {
            Status::Removed
        });
    if state_name(current) != expected {
        return Err("插件状态已变化，请重新确认".into());
    }
    let mut next = state.states.clone();
    next.insert(id.into(), target);
    let tmp = state.path.with_extension("json.next");
    fs::write(&tmp, serde_json::to_vec(&next).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    fs::rename(tmp, &state.path).map_err(|e| e.to_string())?;
    state.states = next;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_catalog_covers_all_tools_and_components() {
        let mut ids = std::collections::BTreeSet::new();
        for d in DEFINITIONS {
            assert!(ids.insert(d.id));
        }
        assert_eq!(ids.len(), 8);
        for name in [
            "notes_add",
            "clock_status",
            "story_advance",
            "wechat_open",
            "downloads_control",
            "broadcast_prepare",
        ] {
            assert!(definition(tool_owner(name).unwrap()).is_ok());
        }
        assert!(tool_owner("plugins_open").is_none());
        assert!(tool_owner("builtin_plugins_change").is_none());
        assert!(validate_action("shell", "enable").is_err());
        assert!(validate_action("note", "destroy").is_err());
    }
    #[test]
    fn availability_survives_restart_and_readding_keeps_other_states() {
        let mut random = [0u8; 12];
        getrandom::fill(&mut random).unwrap();
        let id: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("shannon-builtins-{id}"));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("builtin-plugins.json");
        let mut s = State {
            path: path.clone(),
            states: BTreeMap::new(),
        };
        let initial = if cfg!(feature = "native-runtime") {
            "enabled"
        } else {
            "removed"
        };
        persist_change(&mut s, "note", "disable", initial).unwrap();
        persist_change(&mut s, "story", "uninstall", initial).unwrap();
        let saved = fs::read(&path).unwrap();
        let next: BTreeMap<String, Status> = serde_json::from_slice(&saved).unwrap();
        assert_eq!(next["note"], Status::Disabled);
        assert_eq!(next["story"], Status::Removed);
        assert!(persist_change(&mut s, "story", "install", "enabled").is_err());
        assert_eq!(fs::read(&path).unwrap(), saved);
        persist_change(&mut s, "story", "install", "removed").unwrap();
        assert_eq!(s.states["story"], Status::Enabled);
        assert_eq!(s.states["note"], Status::Disabled);
        let before = s.states.clone();
        s.path = dir.join("missing").join("unwritable.json");
        assert!(persist_change(&mut s, "note", "enable", "disabled").is_err());
        assert_eq!(s.states, before);
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
