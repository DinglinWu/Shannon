//! Local per-user request files let assistants use notes without UI automation.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Add {
        text: String,
        #[serde(default)]
        date: Option<String>,
    },
    SetDate {
        id: u64,
        date: String,
    },
    DeleteMany {
        ids: Vec<u64>,
    },
    Delete {
        id: u64,
    },
    Trash,
    RestoreDeleted {
        id: u64,
    },
    Reorder {
        ids: Vec<u64>,
    },
    List,
    Show,
    Close,
    Complete {
        id: u64,
    },
    Restore {
        id: u64,
    },
}

#[derive(Deserialize)]
struct Request {
    expires_at: u64,
    request: Action,
}

async fn execute(app: &AppHandle, action: Action) -> Result<Value, String> {
    let (name, args) = match action {
        Action::List => ("notes_list", json!({})),
        Action::Add { text, date } => (
            "notes_add",
            if let Some(date) = date {
                json!({"text":text,"date":date})
            } else {
                json!({"text":text})
            },
        ),
        Action::SetDate { id, date } => ("notes_set_date", json!({"id":id,"date":date})),
        Action::DeleteMany { ids } => ("notes_delete_many", json!({"ids":ids})),
        Action::Delete { id } => ("notes_delete", json!({"id":id})),
        Action::Trash => ("notes_trash", json!({})),
        Action::RestoreDeleted { id } => ("notes_restore_deleted", json!({"id":id})),
        Action::Reorder { ids } => ("notes_reorder", json!({"ids":ids})),
        Action::Show => ("notes_show", json!({})),
        Action::Close => ("notes_close", json!({})),
        Action::Complete { id } => ("notes_set_done", json!({"id":id,"done":true})),
        Action::Restore { id } => ("notes_set_done", json!({"id":id,"done":false})),
    };
    let result = crate::ai_tools::dispatch(app, name, args).await;
    if result["ok"] == true {
        Ok(result["data"].clone())
    } else {
        Err(result["error"]["message"]
            .as_str()
            .unwrap_or("便签调用失败")
            .into())
    }
}
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let dir = super::config_dir(&app).join("notes-bridge");
        if let Err(err) = fs::create_dir_all(&dir) {
            eprintln!("便签接口初始化失败：{err}");
            return;
        }
        loop {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                let Some(id) = name.strip_suffix(".request.json") else {
                    continue;
                };
                if id.len() != 32 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
                    continue;
                }
                let working = dir.join(format!("{id}.processing"));
                let response = dir.join(format!("{id}.response.json"));
                if response.exists() || working.exists() {
                    continue;
                }
                // Atomic claim prevents overlapping app instances processing twice.
                // A stale processing file is deliberately never replayed after a crash.
                if fs::rename(&path, &working).is_err() {
                    continue;
                }
                let result = async {
                    if fs::metadata(&working).map_err(|e| e.to_string())?.len() > 32768 {
                        return Err("便签请求过大".to_string());
                    }
                    let bytes = fs::read(&working).map_err(|e| e.to_string())?;
                    let request: Request =
                        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    if request.expires_at <= now {
                        return Err("请求已过期，请确认闪念正在运行".into());
                    }
                    execute(&app, request.request).await
                }
                .await;
                let value = match result {
                    Ok(data) => json!({"ok": true, "data": data}),
                    Err(error) => json!({"ok": false, "error": error}),
                };
                let temporary = dir.join(format!("{id}.response.tmp"));
                if fs::write(&temporary, value.to_string()).is_ok()
                    && fs::rename(&temporary, &response).is_ok()
                {
                    let _ = fs::remove_file(&working);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_unicode_and_rejects_unknown_actions() {
        let parsed: Request = serde_json::from_str(
            r#"{"expires_at":123,"request":{"action":"add","text":"买牛奶\n带面包"}}"#,
        )
        .unwrap();
        assert!(matches!(parsed.request, Action::Add { text,.. } if text == "买牛奶\n带面包"));
        assert!(serde_json::from_str::<Request>(
            r#"{"expires_at":123,"request":{"action":"delete"}}"#
        )
        .is_err());
        assert!(serde_json::from_str::<Request>(
            r#"{"expires_at":123,"request":{"action":"complete","id":-1}}"#
        )
        .is_err());
    }
    #[test]
    fn new_note_actions_keep_the_bridge_protocol_and_reject_extra_fields() {
        for request in [
            r#"{"action":"add","text":"报告","date":"2026-10-29"}"#,
            r#"{"action":"set_date","id":1,"date":""}"#,
            r#"{"action":"delete","id":1}"#,
            r#"{"action":"trash"}"#,
            r#"{"action":"restore_deleted","id":1}"#,
            r#"{"action":"reorder","ids":[2,1]}"#,
        ] {
            assert!(serde_json::from_str::<Action>(request).is_ok());
        }
        assert!(
            serde_json::from_str::<Action>(r#"{"action":"delete","id":1,"permanent":true}"#)
                .is_err()
        );
    }
}
