//! Only fresh, active physical gesture telemetry can request microphone input.
use serde_json::Value;
use std::sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Mutex};
use tauri::{AppHandle, Emitter, Manager};

static HELD: AtomicBool = AtomicBool::new(false);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static GATE: Mutex<Gate> = Mutex::new(Gate { since: None, released: None, held: false });
pub fn held() -> bool { HELD.load(Ordering::SeqCst) }

struct Gate { since: Option<f64>, released: Option<f64>, held: bool }
impl Gate {
    fn update(&mut self, status: &Value, now: f64) -> Option<bool> {
        let age = now - status["updated_at"].as_f64().unwrap_or(0.0);
        let enabled = status["phase"] == "running" && status["process_running"] == true
            && status["recognition_active"] == true && status["camera_connected"] == true
            && status["dry_run"] != true && status["telemetry_stale"] != true
            && (0.0..0.8).contains(&age);
        let point = enabled && status["voice_point"] == true;
        if point {
            self.released = None;
            let since = *self.since.get_or_insert(now);
            if !self.held && now - since >= 0.45 { self.held = true; return Some(true); }
        } else {
            self.since = None;
            let released = *self.released.get_or_insert(now);
            if self.held && (!enabled || now - released >= 0.25) {
                self.held = false;
                return Some(false);
            }
        }
        None
    }
}

pub fn update(app: &AppHandle, status: &Value) {
    if !crate::builtin_plugins::enabled("voice")||!crate::builtin_plugins::enabled("gesture"){
        if HELD.swap(false,Ordering::SeqCst) { crate::voice::reset(); }
        if let Ok(mut gate)=GATE.lock() { *gate=Gate{since:None,released:None,held:false}; }
        return;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64();
    let change = GATE.lock().ok().and_then(|mut gate| gate.update(status, now));
    let Some(active) = change else { return; };
    HELD.store(active, Ordering::SeqCst);
    let sequence = SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;
    if !active {
        crate::voice::stop_gesture_input();
        let _ = app.emit_to("main", "gesture-voice", serde_json::json!({"active":false}));
        return;
    }
    // Do not steal an existing mouse session or reselect text in a focused editor.
    if crate::voice::active() { return; }
    let focused = app.get_webview_window("main").map(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false)).unwrap_or(false);
    if !focused { crate::show_window(app, None); }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Let the existing window reveal and focus complete before starting dictation.
        tokio::time::sleep(std::time::Duration::from_millis(180)).await;
        if held() && SEQUENCE.load(Ordering::SeqCst) == sequence {
            let _ = app.emit_to("main", "gesture-voice", serde_json::json!({"active":true}));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn status(t: f64, point: bool) -> Value { serde_json::json!({"updated_at":t,"phase":"running","process_running":true,"recognition_active":true,"camera_connected":true,"voice_point":point}) }
    fn gate() -> Gate { Gate { since:None, released:None, held:false } }
    #[test]
    fn stable_hold_starts_once_and_release_finishes() {
        let mut g = gate();
        assert_eq!(g.update(&status(1.0,true),1.0),None);
        assert_eq!(g.update(&status(1.5,true),1.5),Some(true));
        assert_eq!(g.update(&status(2.0,true),2.0),None);
        assert_eq!(g.update(&status(2.1,false),2.1),None);
        assert_eq!(g.update(&status(2.2,true),2.2),None);
        assert_eq!(g.update(&status(2.3,false),2.3),None);
        assert_eq!(g.update(&status(2.6,false),2.6),Some(false));
    }
    #[test]
    fn stale_camera_pause_and_dry_run_cannot_record() {
        for key in ["dry_run","telemetry_stale","camera_connected","recognition_active","process_running"] {
            let mut g = gate();
            g.update(&status(1.0,true),1.0);
            assert_eq!(g.update(&status(1.5,true),1.5),Some(true));
            let mut s = status(2.0,true);
            s[key] = serde_json::json!(key == "dry_run" || key == "telemetry_stale");
            assert_eq!(g.update(&s,2.0),Some(false));
        }
        let mut g = gate(); g.update(&status(1.0,true),1.0);
        assert_eq!(g.update(&status(1.0,true),2.0),None);
        assert_eq!(g.update(&status(2.1,true),2.1),None);
        assert_eq!(g.update(&status(2.6,true),2.6),Some(true));
        assert_eq!(g.update(&status(2.6,true),3.5),Some(false));
    }
}
