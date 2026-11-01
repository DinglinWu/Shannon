//! Optional microphone worker. Built separately, never linked into the launcher.
#[cfg(windows)]
#[path = "../src/voice_local.rs"]
mod engine;

#[cfg(windows)]
fn main() {
    use std::{io::{self, BufRead, Write}, path::PathBuf, sync::atomic::{AtomicBool, AtomicU64, Ordering}};
    static STOP: AtomicBool = AtomicBool::new(false);
    static SESSION: AtomicU64 = AtomicU64::new(0);
    fn emit(id: u64, state: &str, text: &str) {
        let mut out = io::stdout().lock();
        let _ = writeln!(out, "{}", serde_json::json!({"id":id,"state":state,"text":text}));
        let _ = out.flush();
    }
    let Some(root) = std::env::args_os().nth(1).map(PathBuf::from) else { return; };
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break; };
        let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else { continue; };
        let id = message["id"].as_u64().unwrap_or(0);
        match message["op"].as_str() {
            Some("stop") if SESSION.load(Ordering::SeqCst) == id => STOP.store(true, Ordering::SeqCst),
            Some("start") if id != 0 => {
                if SESSION.compare_exchange(0,id,Ordering::SeqCst,Ordering::SeqCst).is_err() {
                    emit(id,"error","上一段语音尚未结束"); continue;
                }
                STOP.store(false, Ordering::SeqCst);
                let root = root.clone();
                std::thread::spawn(move || {
                    let outcome = std::panic::catch_unwind(|| engine::recognize(&root, &STOP, |state,text| emit(id,state,text)))
                        .unwrap_or_else(|_| Err("本地语音组件异常，请重试".into()));
                    SESSION.store(0,Ordering::SeqCst);
                    match outcome { Ok(()) => emit(id,"stopped",""), Err(e) => emit(id,"error",&e) }
                });
            },
            _ => {},
        }
    }
    // Parent exit closes stdin. Terminate the worker and its microphone threads.
    STOP.store(true, Ordering::SeqCst);
}

#[cfg(not(windows))]
fn main() { eprintln!("The local voice component currently supports Windows only."); }
