use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

static NOTES_LOCK: Mutex<()> = Mutex::new(());
static EXIT_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static EXIT_PENDING: AtomicU64 = AtomicU64::new(0);
pub static REVEAL_ON_LOAD: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: u64,
    pub text: String,
    pub done: bool,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub deleted: bool,
}

fn read(app: &AppHandle) -> Result<Vec<Note>, String> {
    match fs::read(super::config_dir(app).join("notes.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("便签文件无法读取：{e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.to_string()),
    }
}

fn save(app: &AppHandle, notes: &[Note]) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(notes).map_err(|e| e.to_string())?;
    let path = super::config_dir(app).join("notes.json");
    // Keep the previous successful version if writing is interrupted.
    if path.exists() {
        fs::copy(&path, path.with_extension("json.bak")).map_err(|e| e.to_string())?;
    }
    let temporary = path.with_extension("json.next");
    fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())?;
    let _ = app.emit_to("notes", "notes-changed", ());
    Ok(())
}

#[tauri::command]
pub fn list_notes(app: AppHandle) -> Result<Vec<Note>, String> {
    let _guard = NOTES_LOCK.lock().map_err(|e| e.to_string())?;
    Ok(read(&app)?.into_iter().filter(|n| !n.deleted).collect())
}

#[tauri::command]
pub fn set_note_done(app: AppHandle, id: u64, done: bool) -> Result<(), String> {
    let _guard = NOTES_LOCK.lock().map_err(|e| e.to_string())?;
    let mut notes = read(&app)?;
    let note = notes
        .iter_mut()
        .find(|n| n.id == id && !n.deleted)
        .ok_or("未找到这条待办，可能已被删除")?;
    note.done = done;
    save(&app, &notes)
}

#[tauri::command]
pub async fn open_notes(app: AppHandle, text: String) -> Result<(), String> {
    crate::builtin_plugins::require("note")?;
    let text = text.trim();
    if text.chars().count() > 2000 {
        return Err("每条便签最多 2000 字".into());
    }
    if !text.is_empty() {
        add_note(&app, text)?;
    }
    ensure_window(&app, text.is_empty()).await
}
pub async fn show_compact(app: &AppHandle) -> Result<(), String> {
    crate::builtin_plugins::require("note")?;
    ensure_window(app, false).await
}
async fn ensure_window(app: &AppHandle, reveal: bool) -> Result<(), String> {
    EXIT_PENDING.store(0, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("notes") {
        window.show().map_err(|e| e.to_string())?;
        if reveal {
            window.set_focus().map_err(|e| e.to_string())?;
        }
        window
            .emit(
                if reveal {
                    "notes-open"
                } else {
                    "notes-changed"
                },
                (),
            )
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let anchor = crate::notes_widget::initial_state(app)?;
    REVEAL_ON_LOAD.store(reveal, Ordering::SeqCst);
    let window = WebviewWindowBuilder::new(app, "notes", WebviewUrl::App("notes.html".into()))
        .title("闪念 · 便签")
        .inner_size(64.0, 64.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    let result = (|| {
        window.set_position(PhysicalPosition::new(anchor.x, anchor.y))?;
        window.show()?;
        if reveal {
            window.set_focus()?;
        }
        Ok::<(), tauri::Error>(())
    })();
    if let Err(err) = result {
        let _ = window.destroy();
        return Err(err.to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn close_notes(app: AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("notes") else {
        return Ok(());
    };
    let token = EXIT_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    EXIT_PENDING.store(token, Ordering::SeqCst);
    window.emit("notes-exit", ()).map_err(|e| e.to_string())?;
    // Finish the same aggregation into the compact icon for both the button and AI calls.
    tokio::time::sleep(std::time::Duration::from_millis(850)).await;
    if EXIT_PENDING
        .compare_exchange(token, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("便签已重新打开，取消退出".into());
    }
    window.destroy().map_err(|e| e.to_string())
}

pub fn add_note(app: &AppHandle, text: &str) -> Result<Note, String> {
    add_note_dated(app, text, None)
}
pub fn add_note_dated(app: &AppHandle, text: &str, date: Option<String>) -> Result<Note, String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 2000 {
        return Err("便签内容须为1–2000字".into());
    }
    let _guard = NOTES_LOCK.lock().map_err(|e| e.to_string())?;
    let date = normalize_date(date)?;
    let mut notes = read(app)?;
    let id = notes
        .iter()
        .map(|n| n.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("便签ID已耗尽")?;
    let note = Note {
        id,
        text: text.into(),
        done: false,
        date,
        deleted: false,
    };
    notes.push(note.clone());
    save(app, &notes)?;
    Ok(note)
}

pub fn normalize_date(date: Option<String>) -> Result<Option<String>, String> {
    let Some(date) = date else { return Ok(None) };
    let date = date.trim();
    if date.is_empty() {
        return Ok(None);
    }
    let invalid = || "日期须为有效的 YYYY-MM-DD 本地日历日期".to_string();
    if date.len() != 10
        || date.as_bytes()[4] != b'-'
        || date.as_bytes()[7] != b'-'
        || !date
            .bytes()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err(invalid());
    }
    let year: u32 = date[0..4].parse().map_err(|_| invalid())?;
    let month: u32 = date[5..7].parse().map_err(|_| invalid())?;
    let day: u32 = date[8..10].parse().map_err(|_| invalid())?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    if year == 0 || day == 0 || day > max {
        return Err(invalid());
    }
    Ok(Some(date.into()))
}
fn update<T>(
    app: &AppHandle,
    change: impl FnOnce(&mut Vec<Note>) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = NOTES_LOCK.lock().map_err(|e| e.to_string())?;
    let mut notes = read(app)?;
    let result = change(&mut notes)?;
    save(app, &notes)?;
    Ok(result)
}
#[tauri::command]
pub fn notes_snapshot(app: AppHandle) -> Result<serde_json::Value, String> {
    let _g = NOTES_LOCK.lock().map_err(|e| e.to_string())?;
    let n = read(&app)?;
    Ok(
        serde_json::json!({"notes":n.iter().filter(|n|!n.deleted).collect::<Vec<_>>(),"deleted":n.iter().filter(|n|n.deleted).collect::<Vec<_>>()}),
    )
}
#[tauri::command]
pub fn list_deleted_notes(app: AppHandle) -> Result<Vec<Note>, String> {
    let _g = NOTES_LOCK.lock().map_err(|e| e.to_string())?;
    Ok(read(&app)?.into_iter().filter(|n| n.deleted).collect())
}
#[tauri::command]
pub fn set_note_date(app: AppHandle, id: u64, date: Option<String>) -> Result<Note, String> {
    let date = normalize_date(date)?;
    update(&app, |n| {
        let item = n
            .iter_mut()
            .find(|n| n.id == id && !n.deleted)
            .ok_or("未找到这条待办")?;
        item.date = date;
        Ok(item.clone())
    })
}
fn set_deleted(notes: &mut [Note], id: u64, deleted: bool) -> Result<Note, String> {
    let n = notes
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or("未找到这条待办")?;
    n.deleted = deleted;
    Ok(n.clone())
}
fn delete_many(notes: &mut [Note], ids: &[u64]) -> Result<Vec<u64>, String> {
    let unique: BTreeSet<_> = ids.iter().copied().collect();
    if ids.is_empty() || ids.len() > 10000 || unique.len() != ids.len() || ids.contains(&0) {
        return Err("删除ID须为1–10000个不重复的真实ID".into());
    }
    if ids
        .iter()
        .any(|id| !notes.iter().any(|n| n.id == *id && !n.deleted))
    {
        return Err("待办列表已变化，请重新查询后再删除；本次未删除任何项目".into());
    }
    for note in notes.iter_mut() {
        if unique.contains(&note.id) {
            note.deleted = true;
        }
    }
    Ok(ids.to_vec())
}
#[tauri::command]
pub fn delete_notes(app: AppHandle, ids: Vec<u64>) -> Result<Vec<u64>, String> {
    update(&app, |n| delete_many(n, &ids))
}
#[tauri::command]
pub fn delete_note(app: AppHandle, id: u64) -> Result<Note, String> {
    update(&app, |n| {
        delete_many(n, &[id])?;
        Ok(n.iter().find(|n| n.id == id).unwrap().clone())
    })
}
#[tauri::command]
pub fn restore_deleted_note(app: AppHandle, id: u64) -> Result<Note, String> {
    update(&app, |n| set_deleted(n, id, false))
}
fn reorder(notes: &mut Vec<Note>, ids: &[u64]) -> Result<(), String> {
    let expected: BTreeSet<_> = notes.iter().filter(|n| !n.deleted).map(|n| n.id).collect();
    let actual: BTreeSet<_> = ids.iter().copied().collect();
    if ids.len() > 10000 || actual.len() != ids.len() || actual != expected {
        return Err("待办列表已变化或排序ID不完整，请刷新后重试".into());
    }
    let mut map: BTreeMap<_, _> = notes
        .iter()
        .filter(|n| !n.deleted)
        .cloned()
        .map(|n| (n.id, n))
        .collect();
    let mut ordered: Vec<_> = ids
        .iter()
        .map(|id| map.remove(id).expect("validated id"))
        .collect();
    ordered.extend(notes.iter().filter(|n| n.deleted).cloned());
    *notes = ordered;
    Ok(())
}
#[tauri::command]
pub fn reorder_notes(app: AppHandle, ids: Vec<u64>) -> Result<Vec<u64>, String> {
    update(&app, |n| {
        reorder(n, &ids)?;
        Ok(ids)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn n(id: u64) -> Note {
        Note {
            id,
            text: format!("待办{id}"),
            done: false,
            date: None,
            deleted: false,
        }
    }
    #[test]
    fn legacy_notes_keep_ids_and_defaults() {
        let n: Vec<Note> =
            serde_json::from_str(r#"[{"id":12,"text":"旧便签","done":true}]"#).unwrap();
        assert_eq!(n[0].id, 12);
        assert_eq!(n[0].date, None);
        assert!(!n[0].deleted);
    }
    #[test]
    fn dates_validate_real_calendar_days() {
        for d in ["2024-02-29", "2026-10-28", "2000-02-29"] {
            assert!(normalize_date(Some(d.into())).unwrap().is_some());
        }
        for d in [
            "2026-02-29",
            "2100-02-29",
            "2026-13-01",
            "2026-04-31",
            "0000-01-01",
            "2026-1-01",
        ] {
            assert!(normalize_date(Some(d.into())).is_err());
        }
        assert_eq!(normalize_date(Some("".into())).unwrap(), None);
    }
    #[test]
    fn deletion_can_restore_the_same_id_date_and_text() {
        let mut a = vec![n(1), n(2)];
        a[0].date = Some("2026-10-28".into());
        a[0].done = true;
        let original = a[0].clone();
        set_deleted(&mut a, 1, true).unwrap();
        assert!(a[0].deleted);
        set_deleted(&mut a, 1, false).unwrap();
        assert_eq!(a[0], original);
    }
    #[test]
    fn reorder_rejects_stale_or_duplicate_ids_without_losing_data() {
        let mut a = vec![n(1), n(2), n(3)];
        a[2].deleted = true;
        let original = a.clone();
        for ids in [vec![1], vec![1, 1], vec![1, 2, 3], vec![1, 4]] {
            assert!(reorder(&mut a, &ids).is_err());
            assert_eq!(a, original);
        }
        reorder(&mut a, &[2, 1]).unwrap();
        assert_eq!(a.iter().map(|n| n.id).collect::<Vec<_>>(), vec![2, 1, 3]);
    }
    #[test]
    fn bulk_delete_is_atomic_and_preserves_recovery_data() {
        let mut notes = vec![n(1), n(2), n(3)];
        notes[1].date = Some("2026-10-29".into());
        notes[1].done = true;
        let original = notes.clone();
        for ids in [vec![], vec![1, 1], vec![1, 99], vec![0]] {
            assert!(delete_many(&mut notes, &ids).is_err());
            assert_eq!(notes, original);
        }
        assert_eq!(delete_many(&mut notes, &[1, 2, 3]).unwrap(), vec![1, 2, 3]);
        assert!(notes.iter().all(|n| n.deleted));
        for id in [1, 2, 3] {
            set_deleted(&mut notes, id, false).unwrap();
        }
        assert_eq!(notes, original);
    }
    #[test]
    fn delete_more_than_one_ai_round_of_items_in_one_batch() {
        let mut notes: Vec<_> = (1..=40).map(n).collect();
        let ids: Vec<_> = (1..=40).collect();
        let deleted = delete_many(&mut notes, &ids).unwrap();
        assert_eq!(deleted.len(), 40);
        assert!(notes.iter().all(|n| n.deleted));
    }
}
