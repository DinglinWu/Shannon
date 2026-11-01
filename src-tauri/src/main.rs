#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature="native-runtime")]
mod downloads;
#[cfg(feature="native-runtime")]
mod broadcast;
#[cfg(feature="native-runtime")]
mod broadcast_http;

mod config;
#[cfg(feature="native-runtime")]
mod clock;
#[cfg(feature="native-runtime")]
mod notes;
#[cfg(feature="native-runtime")]
mod notes_widget;
#[cfg(feature="native-runtime")]
mod wechat;
#[cfg(feature="native-runtime")]
mod wechat_story;
#[cfg(feature="native-runtime")]
mod wechat_memory;
#[cfg(feature="native-runtime")]
mod wechat_social;
#[cfg(feature="native-runtime")]
mod story;
#[cfg(feature="native-runtime")]
mod story_memory;
#[cfg(feature="native-runtime")] mod story_profile;
#[cfg(feature="native-runtime")]
mod wechat_moments;
#[cfg(feature="native-runtime")]
mod wechat_avatars;
mod notes_bridge;
#[cfg_attr(not(feature="native-runtime"),path="ai_tools_lean.rs")]
mod ai_tools;
#[cfg(feature="native-runtime")] mod native_service;
#[cfg(not(feature="native-runtime"))] mod native_bridge;
mod official_distribution;
mod release_sources;
mod plugin_protocol;
mod plugin_readme;
mod ai_runtime;
#[cfg(feature="native-runtime")]
mod voice;
#[cfg(feature="native-runtime")]
mod components;
mod plugin_schema;
mod plugin_package;
mod plugins;
mod builtin_plugins;
mod plugin_manager;
#[cfg(feature="native-runtime")]
mod voice_gesture;
#[cfg(feature="native-runtime")]
mod gesture;
#[cfg(feature="native-runtime")]
mod gesture_overlay;
mod opener;
mod prompts;
mod selection;
mod online_theme;
mod theme;
mod update;


use serde::{Deserialize, Serialize};
use config::Config;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

// ---------- 会话数据 ----------

#[derive(Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    protocol: Option<ai_runtime::ProtocolTurn>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Session {
    id: String,
    title: String,
    created_at: u64,
    updated_at: u64,
    messages: Vec<ChatMessage>,
}

#[derive(Serialize, Deserialize, Default)]
struct Store {
    sessions: Vec<Session>,
}

// 侧栏只需要这几个字段，不用把所有消息都序列化过去
#[derive(Serialize, Clone)]
struct SessionMeta {
    id: String,
    title: String,
    updated_at: u64,
}

struct AppState {
    store: Mutex<Store>,
    current: Mutex<String>,
    pinned: AtomicBool,
    // 全屏特效的参数：前端 play_screen_fx 放进来，特效窗口加载好后 take_fx_payload 取走
    fx_payload: Mutex<Option<serde_json::Value>>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn new_id() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("s{}", ms)
}

fn make_title(prompt: &str) -> String {
    let t: String = prompt.trim().chars().take(24).collect();
    if t.is_empty() {
        "新对话".to_string()
    } else {
        t
    }
}

// ---------- 磁盘读写 ----------

fn config_dir(app: &AppHandle) -> std::path::PathBuf {
    #[cfg(all(feature="native-runtime",debug_assertions))]
    if native_service::requested(){if let Some(root)=std::env::var_os("SHANNON_PLUGIN_TEST_ROOT"){let p=std::path::PathBuf::from(root);fs::create_dir_all(&p).ok();return p;}}
    let dir = app.path().app_config_dir().expect("no config dir");
    fs::create_dir_all(&dir).ok();
    dir
}

fn config_path(app: &AppHandle) -> std::path::PathBuf {
    config_dir(app).join(config::FILE_NAME)
}

fn prompts_path(app: &AppHandle) -> std::path::PathBuf {
    config_dir(app).join(prompts::FILE_NAME)
}

fn sessions_path(app: &AppHandle) -> std::path::PathBuf {
    config_dir(app).join("sessions.json")
}

fn load_store(app: &AppHandle) -> Store {
    let mut store: Store = fs::read_to_string(sessions_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // 超过保留期的直接丢掉
    let cutoff = now().saturating_sub(load_config(app).retention_days() * 86400);
    store.sessions.retain(|s| s.updated_at >= cutoff);
    store.sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    store
}

fn save_store(app: &AppHandle, store: &Store) {
    if let Ok(json) = serde_json::to_string_pretty(store) {
        if let Ok(mut f) = fs::File::create(sessions_path(app)) {
            let _ = f.write_all(json.as_bytes());
        }
    }
}

// ---------- 配置 ----------
//
// 全部设置都在 config.toml（见 config.rs 和 config_template.toml）。
// 读有缓存，文件没变就不重新解析；写只改那一个值，用户的注释原样保留

fn load_config(app: &AppHandle) -> Config {
    config::load(&config_path(app))
}

fn edit_config(app: &AppHandle, f: impl FnOnce(&mut toml_edit::DocumentMut)) -> Result<(), String> {
    config::edit(&config_path(app), f)
}

#[tauri::command]
fn has_api_key(app: AppHandle) -> bool {
    !load_config(&app).ai.api_key.trim().is_empty()
}

// 取 Cargo.toml 里的版本号，编译期展开。
// 这样界面上显示的永远是这个二进制自己的版本，不会和源码里另写一份对不上
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn get_api_key(app: AppHandle) -> String {
    load_config(&app).ai.api_key
}

#[tauri::command]
fn save_api_key(app: AppHandle, key: String) -> Result<(), String> {
    edit_config(&app, |doc| doc["ai"]["api_key"] = config::str_value(key.trim()))
}

fn load_api_key(app: &AppHandle) -> Option<String> {
    let key = load_config(app).ai.api_key;
    if key.trim().is_empty() {
        None
    } else {
        Some(key.trim().to_string())
    }
}

#[tauri::command]
fn get_deep_thinking(app: AppHandle) -> bool {
    load_config(&app).ai.deep_thinking
}

#[tauri::command]
fn set_deep_thinking(app: AppHandle, value: bool) -> Result<(), String> {
    edit_config(&app, |doc| doc["ai"]["deep_thinking"] = toml_edit::value(value))
}

#[tauri::command]
fn set_ai_model(app: AppHandle, window: tauri::WebviewWindow, value: String) -> Result<(), String> {
    if window.label() != "main" { return Err("仅主窗口设置可切换模型".into()); }
    if !matches!(value.as_str(), "deepseek-flash" | "deepseek-v4-pro") {
        return Err("请选择 V4 Flash 或 V4 Pro".into());
    }
    edit_config(&app, |doc| doc["ai"]["model"] = config::str_value(&value))
}

#[tauri::command]
fn get_web_search(app: AppHandle) -> bool {
    load_config(&app).ai.web_search
}

#[tauri::command]
fn set_web_search(app: AppHandle, value: bool) -> Result<(), String> {
    edit_config(&app, |doc| doc["ai"]["web_search"] = toml_edit::value(value))
}

// 开机自启以配置文件为准，由 tauri-plugin-autostart 落到注册表的 Run 项
fn apply_autostart(app: &AppHandle, want: bool) {
    let launcher = app.autolaunch();
    if launcher.is_enabled().unwrap_or(false) != want {
        let _ = if want { launcher.enable() } else { launcher.disable() };
    }
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    load_config(&app).general.autostart
}

#[tauri::command]
fn set_autostart(app: AppHandle, value: bool) -> Result<(), String> {
    edit_config(&app, |doc| doc["general"]["autostart"] = toml_edit::value(value))?;
    apply_autostart(&app, value);
    Ok(())
}

#[tauri::command]
fn get_use_system_proxy(app: AppHandle) -> bool {
    load_config(&app).ai.use_system_proxy
}

#[tauri::command]
fn set_use_system_proxy(app: AppHandle, value: bool) -> Result<(), String> {
    edit_config(&app, |doc| doc["ai"]["use_system_proxy"] = toml_edit::value(value))
}

#[tauri::command]
fn set_theme_mode(app: AppHandle, mode: String) -> Result<(), String> {
    edit_config(&app, |doc| doc["theme"]["mode"] = config::str_value(mode.trim()))
}

// 前端要用的那部分配置，启动时取一次，配置文件一变就推一次（config-changed）
#[derive(Serialize, Clone)]
struct UiConfig {
    width: u32,
    max_height: u32,
    expanded_width: u32,
    expanded_height: u32,
    placeholder: String,
    input_history: u32,
    theme_mode: String,
    prompt: u32,
    hotkey: String,
    deep_thinking: bool,
    model: String,
    web_search: bool,
    use_system_proxy: bool,
    autostart: bool,
}

fn ui_config(cfg: &Config) -> UiConfig {
    let w = &cfg.window;
    UiConfig {
        width: w.width.clamp(400, 1600),
        max_height: w.max_height.clamp(120, 1400),
        expanded_width: w.expanded_width.clamp(560, 2400),
        expanded_height: w.expanded_height.clamp(360, 1600),
        placeholder: w.placeholder.chars().take(60).collect(),
        input_history: cfg.history.input_history.min(5000),
        theme_mode: cfg.theme.mode.trim().to_string(),
        prompt: cfg.ai.prompt,
        hotkey: HOTKEY.lock().unwrap().clone(),
        deep_thinking: cfg.ai.deep_thinking,
        model: cfg.model(),
        web_search: cfg.ai.web_search,
        use_system_proxy: cfg.ai.use_system_proxy,
        autostart: cfg.general.autostart,
    }
}

#[tauri::command]
fn get_ui_config(app: AppHandle) -> UiConfig {
    ui_config(&load_config(&app))
}

// 配置文件现在有没有问题（语法错、热键注册不上等），没有返回 None
#[tauri::command]
fn config_error(app: AppHandle) -> Option<String> {
    config::error(&config_path(&app)).or_else(|| HOTKEY_WARN.lock().unwrap().clone())
}

// /config：用配置里指定的编辑器打开 config.toml，返回文件路径给前端提示
#[tauri::command]
fn open_config(app: AppHandle) -> Result<String, String> {
    let dir = config_dir(&app);
    config::ensure(&dir, app.autolaunch().is_enabled().unwrap_or(false))?;
    let path = config_path(&app);
    config::open_in_editor(&load_config(&app).general.editor, &path)?;
    Ok(path.to_string_lossy().to_string())
}

// ---------- 系统提示词（prompts.toml，/spt） ----------

#[derive(Serialize)]
struct PromptInfo {
    id: u32,
    name: String,
    preview: String,
}

#[derive(Serialize)]
struct PromptList {
    active: u32,
    items: Vec<PromptInfo>,
    // prompts.toml 现在的错误；有错时 items 是上一次读对的内容
    error: Option<String>,
}

#[tauri::command]
fn list_prompts(app: AppHandle) -> PromptList {
    let (map, error) = prompts::load(&prompts_path(&app));
    PromptList {
        active: load_config(&app).ai.prompt,
        items: map
            .iter()
            .map(|(id, p)| PromptInfo { id: *id, name: p.name.clone(), preview: prompts::preview(&p.text) })
            .collect(),
        error,
    }
}

// 切换提示词，返回它的名字。0 表示不用
#[tauri::command]
fn set_prompt(app: AppHandle, id: u32) -> Result<String, String> {
    let name = if id == 0 {
        "不使用".to_string()
    } else {
        let (map, _) = prompts::load(&prompts_path(&app));
        map.get(&id)
            .map(|p| p.name.clone())
            .ok_or_else(|| format!("提示词 {} 不存在。使用 /spt 查看列表，/spt edit 添加提示词", id))?
    };
    edit_config(&app, |doc| doc["ai"]["prompt"] = toml_edit::value(id as i64))?;
    Ok(name)
}

#[tauri::command]
fn open_prompts(app: AppHandle) -> Result<String, String> {
    let dir = config_dir(&app);
    prompts::ensure(&dir, "")?;
    let path = prompts_path(&app);
    config::open_in_editor(&load_config(&app).general.editor, &path)?;
    Ok(path.to_string_lossy().to_string())
}

// 这次提问要带的系统提示词，没有就是空串
fn active_prompt(app: &AppHandle, cfg: &Config, date: &str, weekday: &str) -> String {
    if cfg.ai.prompt == 0 {
        return String::new();
    }
    let (map, _) = prompts::load(&prompts_path(app));
    map.get(&cfg.ai.prompt)
        .map(|p| prompts::render(&p.text, date, weekday))
        .unwrap_or_default()
}

// ---------- 更新 ----------

#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<update::UpdateInfo>, String> {
    update::check(&app).await
}

#[tauri::command]
fn pending_update() -> Option<update::UpdateInfo> {
    update::pending()
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    update::install(&app).await
}

// 启动 30 秒后检查一次，之后按配置的间隔检查。失败时静默，等下一轮
fn watch_updates(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        loop {
            let cfg = load_config(&app);
            if cfg.update.auto_check {
                if let Ok(Some(info)) = update::check(&app).await {
                    let _ = app.emit("update-available", info);
                }
            }
            tokio::time::sleep(cfg.update_interval()).await;
        }
    });
}

// ---------- 热键 ----------

// 当前注册着的热键原文，以及最近一次热键出问题的说明
static HOTKEY: Mutex<String> = Mutex::new(String::new());
static HOTKEY_WARN: Mutex<Option<String>> = Mutex::new(None);
const DEFAULT_HOTKEY: &str = "Alt+Space";

// 接受 Win / Windows 这种写法，底层库只认 Super
fn parse_hotkey(s: &str) -> Result<Shortcut, String> {
    let norm: Vec<String> = s
        .split('+')
        .map(|t| {
            let t = t.trim();
            match t.to_ascii_lowercase().as_str() {
                "win" | "windows" | "meta" => "Super".to_string(),
                _ => t.to_string(),
            }
        })
        .collect();
    norm.join("+")
        .parse::<Shortcut>()
        .map_err(|e| format!("快捷键「{}」格式无效：{}", s, e))
}

// 换热键：先注销旧的再注册新的；新的注册不上（格式错、被别的程序占了）就把旧的装回去
fn apply_hotkey(app: &AppHandle, want: &str) -> Result<(), String> {
    let want = want.trim();
    let mut cur = HOTKEY.lock().unwrap();
    if *cur == want {
        return Ok(());
    }
    let result = parse_hotkey(want).and_then(|sc| {
        let gs = app.global_shortcut();
        let _ = gs.unregister_all();
        gs.register(sc).map_err(|e| format!("快捷键「{}」注册失败，可能已被其他程序占用：{}", want, e))
    });
    match result {
        Ok(()) => {
            *cur = want.to_string();
            *HOTKEY_WARN.lock().unwrap() = None;
            Ok(())
        }
        Err(e) => {
            // 装回原来的；一个都没有（启动时就失败）就用默认的 Alt+Space
            let fallback = if cur.is_empty() { DEFAULT_HOTKEY.to_string() } else { cur.clone() };
            if let Ok(sc) = parse_hotkey(&fallback) {
                if app.global_shortcut().register(sc).is_ok() {
                    *cur = fallback.clone();
                }
            }
            let msg = format!("{}。当前仍使用「{}」", e, cur);
            *HOTKEY_WARN.lock().unwrap() = Some(msg.clone());
            Err(msg)
        }
    }
}

// 每隔一会儿看一眼 config.toml 的修改时间，变了就重新读、把热键和自启落实、通知前端。
// 不用文件监听库：只看一个文件，轮询的开销可以忽略
fn watch_config(app: AppHandle) {
    std::thread::spawn(move || {
        let path = config_path(&app);
        let stamp = |p: &std::path::Path| fs::metadata(p).and_then(|m| m.modified()).ok();
        let mut last = stamp(&path);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1200));
            let now = stamp(&path);
            if now == last {
                continue;
            }
            // 整个文件被删了：按模板重新生成（沿用当前的自启状态）
            if now.is_none() {
                let _ = config::ensure(&config_dir(&app), load_config(&app).general.autostart);
            }
            last = stamp(&path);

            let cfg = load_config(&app);
            if let Some(err) = config::error(&path) {
                let _ = app.emit("config-error", err);
                continue;
            }
            let hotkey = apply_hotkey(&app, &cfg.general.hotkey);
            apply_autostart(&app, cfg.general.autostart);
            let _ = app.emit("config-changed", ui_config(&cfg));
            if let Err(e) = hotkey {
                let _ = app.emit("config-error", e);
            }
        }
    });
}

// ---------- /o 与 /rmbr ----------
//
// 查找顺序：用户别名 → 内置别名（opener::BUILTIN，没被 /rmbr -d 删掉的）→ 普通匹配。
// 用户别名在设置时就解析成具体路径存下来：一来当场就能告诉用户匹配到了哪个，
// 二来以后 /o 不用每次再扫开始菜单。内置别名每台机器装的东西不一样，只能用的时候现解析

#[derive(Serialize)]
struct AliasList {
    user: Vec<(String, String)>,
    // 同一个应用的几个内置别名合成一行：(["wx","weixin","wechat"], 路径)
    builtin: Vec<(Vec<String>, String)>,
}

// 用户别名或内置别名命中就返回 Some；都不是返回 None，交给普通匹配
fn lookup_alias(cfg: &Config, key: &str, idx: &opener::Index) -> Option<Result<String, String>> {
    if let Some(v) = cfg.aliases.get(key) {
        // 存的是当时解析好的结果。程序卸了、文件夹挪了，这里要说清楚
        if !opener::looks_like_url(v) && !std::path::Path::new(v).exists() {
            return Some(Err(format!(
                "别名「{}」的目标 {} 已不存在，请使用 /rmbr 重新设置",
                key, v
            )));
        }
        return Some(Ok(v.clone()));
    }
    if !cfg.builtin_disabled(key) {
        if let Some(v) = opener::resolve_builtin(key, idx) {
            return Some(Ok(v));
        }
    }
    None
}

// 列内置别名要扫开始菜单，放到 blocking 线程里
#[tauri::command]
async fn list_aliases(app: AppHandle) -> Result<AliasList, String> {
    let cfg = load_config(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let idx = opener::Index::scan();
        let builtin = opener::installed_builtins(&idx, &|n| {
            // 被删掉的、被用户同名别名盖住的都不显示
            cfg.builtin_disabled(n) || cfg.aliases.contains_key(n)
        });
        AliasList {
            user: cfg.aliases.into_iter().collect(),
            builtin,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_alias(app: AppHandle, name: String, target: String) -> Result<String, String> {
    let name = name.trim().to_lowercase();
    if name.is_empty() {
        return Err("别名不能为空".into());
    }
    let cfg = load_config(&app);
    let cfg_for_lookup = cfg.clone();
    let resolved = tauri::async_runtime::spawn_blocking(move || {
        let idx = opener::Index::scan();
        // 目标本身也可以是另一个别名（用户的或内置的），比如 /rmbr 聊天 wx
        let key = opener::strip_quotes(&target).to_lowercase();
        match lookup_alias(&cfg_for_lookup, &key, &idx) {
            Some(r) => r,
            None => opener::resolve(&target, &idx),
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    // 用户自己定义了同名的，就不再算"删掉的内置别名"
    let mut disabled = cfg.builtin_aliases.disabled.clone();
    let was_disabled = disabled.remove(&name);
    edit_config(&app, |doc| {
        if was_disabled {
            config::set_disabled_builtins(doc, &disabled);
        }
        doc["aliases"][name.as_str()] = config::str_value(&resolved);
    })?;
    Ok(resolved)
}

// 删用户别名返回它原来指向的路径；删内置别名只是记下"不要了"，返回 None
#[tauri::command]
fn remove_alias(app: AppHandle, name: String) -> Result<Option<String>, String> {
    let key = name.trim().to_lowercase();
    let cfg = load_config(&app);
    if let Some(old) = cfg.aliases.get(&key).cloned() {
        edit_config(&app, |doc| {
            if let Some(t) = doc["aliases"].as_table_like_mut() {
                t.remove(&key);
            }
        })?;
        return Ok(Some(old));
    }
    let mut disabled: BTreeSet<String> = cfg.builtin_aliases.disabled.clone();
    if opener::is_builtin(&key) && disabled.insert(key) {
        edit_config(&app, |doc| config::set_disabled_builtins(doc, &disabled))?;
        return Ok(None);
    }
    Err(format!("未找到别名「{}」", name.trim()))
}

// 返回实际打开的东西，前端出错时拿来提示。
// 扫开始菜单是阻塞 IO，放到 blocking 线程里，别卡住界面
#[tauri::command]
async fn open_target(app: AppHandle, query: String) -> Result<String, String> {
    let cfg = load_config(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let idx = opener::Index::scan();
        let key = opener::strip_quotes(&query).to_lowercase();
        let target = match lookup_alias(&cfg, &key, &idx) {
            Some(r) => r?,
            None => opener::resolve(&query, &idx)?,
        };
        opener::open(&target)?;
        Ok(target)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------- 会话相关命令 ----------

#[tauri::command]
fn list_sessions(state: State<'_, AppState>) -> Vec<SessionMeta> {
    state
        .store
        .lock()
        .unwrap()
        .sessions
        .iter()
        .map(|s| SessionMeta {
            id: s.id.clone(),
            title: s.title.clone(),
            updated_at: s.updated_at,
        })
        .collect()
}

#[tauri::command]
fn current_session_id(state: State<'_, AppState>) -> String {
    state.current.lock().unwrap().clone()
}

#[tauri::command]
fn get_messages(state: State<'_, AppState>, id: String) -> Vec<ChatMessage> {
    state
        .store
        .lock()
        .unwrap()
        .sessions
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.messages.clone())
        .unwrap_or_default()
}

// 新会话只是把 current 指向一个新 id，记录推迟到第一次提问时才建，
// 这样侧栏不会冒出一堆空会话
#[tauri::command]
fn new_session(state: State<'_, AppState>) -> String {
    let id = new_id();
    *state.current.lock().unwrap() = id.clone();
    id
}

// 兼容旧名字：Ctrl+L 清空 = 开一个新会话
#[tauri::command]
fn reset_conversation(state: State<'_, AppState>) -> String {
    new_session(state)
}

#[tauri::command]
fn switch_session(state: State<'_, AppState>, id: String) -> Vec<ChatMessage> {
    *state.current.lock().unwrap() = id.clone();
    state
        .store
        .lock()
        .unwrap()
        .sessions
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.messages.clone())
        .unwrap_or_default()
}

#[tauri::command]
fn delete_session(app: AppHandle, state: State<'_, AppState>, id: String) {
    {
        let mut store = state.store.lock().unwrap();
        store.sessions.retain(|s| s.id != id);
        save_store(&app, &store);
    }
    let mut cur = state.current.lock().unwrap();
    if *cur == id {
        *cur = new_id();
    }
}

// ---------- 调用 DeepSeek ----------

// reqwest 的 to_string() 只给最外层，真正的原因在 source 链里
fn describe_err(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "超时"
    } else if e.is_connect() {
        "连接失败"
    } else if e.is_request() {
        "请求发送失败"
    } else {
        "网络错误"
    };

    let mut parts = vec![e.to_string()];
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        parts.push(s.to_string());
        src = s.source();
    }
    format!("{}: {}", kind, parts.join(" <- "))
}

static AI_REQUEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tauri::command]
async fn ask_deepseek(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    plugin_targets: Option<Vec<plugins::PluginTarget>>,
    // 本地日期和星期，由前端按用户时区算好，用来替换提示词里的 {date}、{weekday}
    date: Option<String>,
    weekday: Option<String>,
) -> Result<(), String> {
    let _request_guard = AI_REQUEST_LOCK.try_lock().map_err(|_| "AI正在处理上一条请求，请稍候")?;
    let targets=plugin_targets.unwrap_or_default();
    let tools=if targets.is_empty(){ai_tools::registry()}else{ai_tools::selected_registry(&targets)?};
    let api_key = load_api_key(&app).ok_or_else(|| "未设置 API Key，请在设置面板或配置文件中填写".to_string())?;
    let session_id = state.current.lock().unwrap().clone();
    let cfg = load_config(&app);
    let mut system_prompt = active_prompt(
        &app,
        &cfg,
        date.as_deref().unwrap_or(""),
        weekday.as_deref().unwrap_or(""),
    );
    if let Some(today) = date.as_deref().filter(|s|chrono::NaiveDate::parse_from_str(s,"%Y-%m-%d").is_ok()) {
        system_prompt.push_str(&format!("\n当前用户本地日期：{today}。安排待办日期时据此计算今天、明天等相对日期。"));
    }
    if !targets.is_empty(){system_prompt.push_str("\n用户通过@选择了本轮的工具范围。按其真实需求使用提供的工具，必填信息不足时先询问，不编造参数或声称已执行。@只选择工具，不增加权限，也不代表安装、删除或修改的授权。");}

    // 把这次提问写进当前会话，并取出要发给 API 的上下文
    let messages_to_send: Vec<ChatMessage> = {
        let mut store = state.store.lock().unwrap();
        let ts = now();

        if !store.sessions.iter().any(|s| s.id == session_id) {
            store.sessions.insert(
                0,
                Session {
                    id: session_id.clone(),
                    title: make_title(&prompt),
                    created_at: ts,
                    updated_at: ts,
                    messages: Vec::new(),
                },
            );
        }

        let sess = store
            .sessions
            .iter_mut()
            .find(|s| s.id == session_id)
            .expect("session just inserted");
        sess.messages.push(ChatMessage {
            role: "user".to_string(),
            content: prompt.clone(),
            protocol: None,
        });
        sess.updated_at = ts;

        let start = sess.messages.len().saturating_sub(cfg.context_messages());
        sess.messages[start..].to_vec()
    };

    // 默认 no_proxy() 直连。DeepSeek 是国内服务，走代理反而是
    // "别人机器上用不了"的主要原因；强制走代理的内网用户可以在设置里打开
    let mut builder = reqwest::Client::builder().connect_timeout(cfg.connect_timeout());
    if !cfg.ai.use_system_proxy {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|e| e.to_string())?;

    let messages = ai_runtime::history(&messages_to_send, cfg.ai.web_search);
    match ai_runtime::run(&app, &client, &api_key, &cfg, &system_prompt, messages, tools).await {
        Ok(text) => {
            save_reply(&app, &state, &session_id, text);
            let _ = app.emit("stream-done", ());
            Ok(())
        }
        Err(error) => {
            let _ = app.emit("stream-error", error.clone());
            Err(error)
        }
    }
}
// 把这一轮的完整回答写进会话并落盘
fn save_reply(app: &AppHandle, state: &State<'_, AppState>, session_id: &str, answer: ai_runtime::Answer) {
    let mut store = state.store.lock().unwrap();
    if let Some(sess) = store.sessions.iter_mut().find(|s| s.id == session_id) {
        sess.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: answer.text,
            protocol: Some(answer.protocol),
        });
        sess.updated_at = now();
    }
    store.sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    save_store(app, &store);
}

// ---------- 窗口 ----------

static HIDE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PENDING_HIDE: AtomicU64 = AtomicU64::new(0);

#[tauri::command]
fn finish_hide_window(app: AppHandle, token: u64) {
    if token == 0 { return; }
    if PENDING_HIDE.compare_exchange(token, 0, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("window-hidden", ());
        let _ = window.hide();
    }
}

#[tauri::command]
fn hide_window(app: AppHandle) {
    reset_voice(&app);
    if let Some(window) = app.get_webview_window("main") {
        if !window.is_visible().unwrap_or(false) { return; }
        let token = HIDE_SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;
        if PENDING_HIDE.compare_exchange(0, token, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        let _ = window.emit("window-hiding", token);
        // 页面忙碌或重载时也保证最终隐藏；重新唤起会取消旧 token。
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            finish_hide_window(app, token);
        });
    }
}

// 固定：置顶 + 不再失焦自动隐藏。返回切换后的状态给前端更新图标
#[tauri::command]
fn toggle_pin(app: AppHandle, state: State<'_, AppState>) -> bool {
    let pinned = !state.pinned.load(Ordering::Relaxed);
    state.pinned.store(pinned, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_always_on_top(pinned);
    }
    pinned
}

#[tauri::command]
fn is_pinned(state: State<'_, AppState>) -> bool {
    state.pinned.load(Ordering::Relaxed)
}

// 判断鼠标是不是还在窗口范围内（含外侧几像素的缩放边框）。
// 抓窗口边缘缩放时，系统会把焦点从 webview 上拿走，触发 Focused(false)，
// 光靠焦点判断会导致"一碰边缘窗口就没了"
fn cursor_inside(window: &tauri::Window) -> bool {
    let handle = window.app_handle();
    let (Ok(cursor), Ok(origin), Ok(size)) = (
        handle.cursor_position(),
        window.outer_position(),
        window.outer_size(),
    ) else {
        return false;
    };

    const MARGIN: f64 = 10.0; // 缩放边框在窗口外侧，留点余量
    let left = origin.x as f64 - MARGIN;
    let top = origin.y as f64 - MARGIN;
    let right = origin.x as f64 + size.width as f64 + MARGIN;
    let bottom = origin.y as f64 + size.height as f64 + MARGIN;

    cursor.x >= left && cursor.x <= right && cursor.y >= top && cursor.y <= bottom
}

// selected：唤起前在别的程序里选中的文字，前端拿到后填进输入框；没有就是 None
fn show_window(app: &AppHandle, selected: Option<String>) {
    #[cfg(feature="native-runtime")] if native_service::requested(){let _=app.emit("runtime-show-main",());return;}
    PENDING_HIDE.store(0, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("window-shown", selected);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

// 托盘点击用：这时前台是任务栏，没有选区可取
fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) && PENDING_HIDE.load(Ordering::SeqCst) == 0 {
            hide_window(app.clone());
        } else {
            show_window(app, None);
        }
    }
}

// 正在取选区时又按了一次热键，别再开第二个线程去抢剪贴板
static GRABBING: AtomicBool = AtomicBool::new(false);

// 热键唤起：先趁焦点还在原程序里把选中的文字取过来，再弹窗口。
// 取选区要等对方响应 Ctrl+C（最多几百毫秒），放到单独线程里，不卡事件循环
fn on_hotkey(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if PENDING_HIDE.load(Ordering::SeqCst) != 0 {
        show_window(app, None);
        return;
    }
    if window.is_visible().unwrap_or(false) {
        hide_window(app.clone());
        return;
    }
    if GRABBING.swap(true, Ordering::SeqCst) {
        return;
    }
    let cfg = load_config(app);
    let (enabled, wait_ms) = (cfg.selection.enabled, cfg.selection_wait_ms());
    let app = app.clone();
    std::thread::spawn(move || {
        let selected = if enabled { selection::grab(wait_ms) } else { None };
        GRABBING.store(false, Ordering::SeqCst);
        show_window(&app, selected);
    });
}

// ---------- 主题 ----------

#[tauri::command]
fn themes_dir(app: AppHandle) -> String {
    theme::dir(&config_dir(&app)).to_string_lossy().to_string()
}

#[tauri::command]
fn list_user_themes(app: AppHandle) -> Vec<theme::UserTheme> {
    theme::list(&config_dir(&app))
}

#[tauri::command]
fn list_online_themes(app: AppHandle) -> Vec<theme::UserTheme> {
    theme::list_dir(&online_theme::dir(&config_dir(&app)))
}

// source："online" 读在线主题缓存，其余读自定义主题目录
#[tauri::command]
fn theme_asset(app: AppHandle, id: String, file: String, source: Option<String>) -> Result<String, String> {
    let dir = config_dir(&app);
    match source.as_deref() {
        Some("online") => theme::asset_in(&online_theme::dir(&dir), &id, &file),
        _ => theme::asset(&dir, &id, &file),
    }
}

// 在线主题清单用发布安装包的同一把密钥签名，公钥取自 tauri.conf.json，不另存一份
fn updater_pubkey(app: &AppHandle) -> Option<String> {
    app.config()
        .plugins
        .0
        .get("updater")?
        .get("pubkey")?
        .as_str()
        .map(String::from)
}

async fn sync_online_themes(app: &AppHandle) -> Result<online_theme::Report, String> {
    let pubkey = updater_pubkey(app).ok_or("未配置签名公钥，无法同步在线主题")?;
    let ua = format!("Shannon/{}", app.package_info().version);
    let report = online_theme::sync(&config_dir(app), &pubkey, &ua).await?;
    if report.changed {
        let _ = app.emit("themes-changed", ());
    }
    Ok(report)
}

#[tauri::command]
async fn sync_themes(app: AppHandle) -> Result<online_theme::Report, String> {
    sync_online_themes(&app).await
}

// 启动 20 秒后同步一次，之后与版本检查间隔相同。失败时静默，等下一轮
fn watch_online_themes(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        loop {
            let cfg = load_config(&app);
            if cfg.theme.online {
                let _ = sync_online_themes(&app).await;
            }
            tokio::time::sleep(cfg.update_interval()).await;
        }
    });
}

// 全屏特效：临时建一个铺满当前屏幕的透明窗口，置顶、鼠标穿透，放完由页面调 fx_done 销毁。
// 建窗口必须在 async 命令里做，同步命令里建在 Windows 上会卡死
static FX_GUARD_UNTIL: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[tauri::command]
async fn play_screen_fx(
    app: AppHandle,
    state: State<'_, AppState>,
    mut payload: serde_json::Value,
) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("未找到主窗口")?;
    let monitor = main
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("未找到显示器")?;
    let scale = monitor.scale_factor();
    let pos = monitor.position().to_logical::<f64>(scale);
    let size = monitor.size().to_logical::<f64>(scale);

    if payload["greeting"].is_object() {
        let origin = main.outer_position().map_err(|e| e.to_string())?;
        let main_scale = main.scale_factor().map_err(|e| e.to_string())?;
        let anchor = &payload["greeting"]["anchor"];
        let left = anchor["left"].as_f64().unwrap_or(12.0);
        let top = anchor["top"].as_f64().unwrap_or(24.0);
        let width = anchor["width"].as_f64().unwrap_or(640.0);
        payload["greeting"]["anchor"] = serde_json::json!({
            "left": (origin.x as f64 - monitor.position().x as f64 + left * main_scale) / scale,
            "top": (origin.y as f64 - monitor.position().y as f64 + top * main_scale) / scale,
            "width": width * main_scale / scale,
        });
    }
    // 同一场粒子特效不叠加，但允许手动预览时替换上方寄语。
    if let Some(fx) = app.get_webview_window("fx") {
        if !payload["greeting"].is_null() {
            fx.emit("show-greeting", &payload["greeting"]).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    let duration = payload["duration"].as_u64().unwrap_or(3000).min(8000);
    *state.fx_payload.lock().unwrap() = Some(payload);
    FX_GUARD_UNTIL.store(now_ms() + 800, Ordering::Relaxed);

    let fx = tauri::WebviewWindowBuilder::new(&app, "fx", tauri::WebviewUrl::App("fx.html".into()))
        .title("闪念特效")
        .position(pos.x, pos.y)
        .inner_size(size.width, size.height)
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
    let ready = (|| {
        fx.set_ignore_cursor_events(true)?;
        fx.set_position(*monitor.position())?;
        fx.set_size(*monitor.size())?;
        fx.show()
    })();
    if let Err(err) = ready {
        let _ = fx.destroy();
        *state.fx_payload.lock().unwrap() = None;
        return Err(err.to_string());
    }
    let _ = main.set_focus();

    // 保险：页面万一没调 fx_done，到点也把窗口收掉
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(duration + 6000)).await;
        if let Some(w) = app2.get_webview_window("fx") {
            let _ = w.destroy();
        }
    });
    Ok(())
}

#[tauri::command]
fn take_fx_payload(state: State<'_, AppState>) -> Option<serde_json::Value> {
    state.fx_payload.lock().unwrap().take()
}

#[tauri::command]
fn fx_done(app: AppHandle) {
    if let Some(w) = app.get_webview_window("fx") {
        let _ = w.destroy();
    }
}

#[tauri::command]
fn dismiss_theme_greeting(app: AppHandle, instant: bool) {
    if let Some(fx) = app.get_webview_window("fx") {
        let _ = fx.emit("dismiss-greeting", instant);
    }
}


#[cfg(feature="native-runtime")] fn app_handler()->impl Fn(tauri::ipc::Invoke)->bool+Send+Sync+'static {tauri::generate_handler![official_distribution::plugin_runtime_mode,official_distribution::plugin_ui_source,
            gesture::gesture_control,
            gesture_overlay::gesture_overlay_snapshot,
            gesture_overlay::gesture_overlay_control,
            clock::open_clock,
            clock::clock_active,
            clock::dock_clock,
            clock::reset_clock,
            clock::close_clock,
            ai_tools::list_ai_tools,
            ai_tools::mention_catalog,
            ai_tools::builtin_mention_run,
            voice::start_voice_input,
            voice::stop_voice_input,
            components::component_status,
            components::component_install,
            components::component_confirmation,
            components::component_confirm,
            plugin_manager::plugin_manager_open, plugin_manager::plugin_manager_catalog, plugin_manager::builtin_plugin_change,
            plugins::plugin_list,
            plugins::plugin_info,
            plugins::plugin_install,
            plugins::plugin_change,
            plugins::plugin_run,
            plugins::plugin_review,
            plugins::plugin_confirm,
            plugins::plugin_result,
            plugins::plugin_host_call,
            notes::open_notes,
            notes::close_notes,
            notes::list_notes,
            notes::set_note_done,
            broadcast::broadcast_snapshot,
            broadcast::broadcast_prepare,
            broadcast::broadcast_withdraw,
            broadcast::broadcast_confirm,
            broadcast::broadcast_start,
            broadcast::broadcast_stop,
            wechat::wechat_activity,
            wechat::wechat_social_snapshot,
            wechat::wechat_save_social,
            wechat::wechat_memory_edit,
            wechat::wechat_contact_pause,
            wechat::wechat_mark_read,
            story::story_memory_search, story::story_memory_edit,
            story::story_delete_prepare, story::story_delete_pending, story::story_delete_cancel, story::story_delete_confirm, story::story_trash, story::story_restore,
            story::story_list, story::story_read, story::story_create, story::story_update,
            story::story_player_message, story::story_advance, story::story_history, story::story_select, story::story_role_choices, story::story_open_wechat,
            wechat::wechat_snapshot,
            wechat::wechat_avatar_import,
            wechat::wechat_avatar_data,
            wechat::wechat_home,
            wechat::wechat_remove_friend,
            wechat::wechat_restore_friend,
            wechat::wechat_moments,
            wechat::wechat_post,
            wechat::wechat_react,
            wechat::wechat_friend_post,
            wechat::wechat_select,
            wechat::wechat_save_role,
            wechat::wechat_send,
            wechat::wechat_close,
            notes::notes_snapshot,
            notes::list_deleted_notes,
            notes::set_note_date,
            notes::delete_note,
            notes::delete_notes,
            notes::restore_deleted_note,
            notes::reorder_notes,
            notes_widget::notes_widget_state,
            notes_widget::notes_widget_save,
            notes_widget::notes_widget_bounds,
            notes_widget::notes_widget_mask,
            app_version,
            has_api_key,
            get_api_key,
            save_api_key,
            get_use_system_proxy,
            set_use_system_proxy,
            get_autostart,
            set_autostart,
            get_deep_thinking,
            set_deep_thinking,
            set_ai_model,
            downloads::downloads_snapshot,
            downloads::downloads_prepare,
            downloads::downloads_confirm,
            downloads::downloads_control,
            downloads::downloads_install,
            get_web_search,
            set_web_search,
            set_theme_mode,
            get_ui_config,
            config_error,
            open_config,
            list_prompts,
            set_prompt,
            open_prompts,
            check_update,
            pending_update,
            install_update,
            ask_deepseek,
            reset_conversation,
            hide_window,
            finish_hide_window,
            toggle_pin,
            is_pinned,
            list_sessions,
            current_session_id,
            get_messages,
            new_session,
            switch_session,
            delete_session,
            list_aliases,
            set_alias,
            remove_alias,
            open_target,
            themes_dir,
            list_user_themes,
            list_online_themes,
            sync_themes,
            theme_asset,
            play_screen_fx,
            take_fx_payload,
            fx_done,
            dismiss_theme_greeting
        ]}
#[cfg(not(feature="native-runtime"))] fn app_handler()->impl Fn(tauri::ipc::Invoke)->bool+Send+Sync+'static {tauri::generate_handler![official_distribution::plugin_runtime_mode,official_distribution::plugin_ui_source,ai_tools::list_ai_tools,ai_tools::mention_catalog,ai_tools::builtin_mention_run,plugin_manager::plugin_manager_open,plugin_manager::plugin_manager_catalog,plugin_manager::builtin_plugin_change,plugins::plugin_list,plugins::plugin_info,plugins::plugin_install,plugins::plugin_change,plugins::plugin_run,plugins::plugin_review,plugins::plugin_confirm,plugins::plugin_result,plugins::plugin_host_call,app_version,has_api_key,get_api_key,save_api_key,get_use_system_proxy,set_use_system_proxy,get_autostart,set_autostart,get_deep_thinking,set_deep_thinking,set_ai_model,get_web_search,set_web_search,set_theme_mode,get_ui_config,config_error,open_config,list_prompts,set_prompt,open_prompts,check_update,pending_update,install_update,ask_deepseek,reset_conversation,hide_window,finish_hide_window,toggle_pin,is_pinned,list_sessions,current_session_id,get_messages,new_session,switch_session,delete_session,list_aliases,set_alias,remove_alias,open_target,themes_dir,list_user_themes,list_online_themes,sync_themes,theme_asset,play_screen_fx,take_fx_payload,fx_done,dismiss_theme_greeting,native_bridge::plugin_native_call]}
fn reset_voice(app:&AppHandle){
 #[cfg(feature="native-runtime")] {let _=app;voice::reset();}
 #[cfg(not(feature="native-runtime"))] {native_bridge::reset_voice(app);}
}
fn main() {
    #[cfg(feature="native-runtime")]
    if std::env::args().any(|a|a=="--export-plugin-tools") {println!("{}",serde_json::to_string(&ai_tools::distribution_registry()).unwrap());return;}
    let mut context=tauri::generate_context!();
    #[cfg(feature="native-runtime")]
    if native_service::requested(){context.config_mut().app.windows[0].url=tauri::WebviewUrl::App("runtime.html".into());
        #[cfg(debug_assertions)] if let Some(root)=std::env::var_os("SHANNON_PLUGIN_TEST_ROOT"){context.config_mut().app.windows[0].data_directory=Some(std::path::PathBuf::from(root).join("webview"));}
    }
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        on_hotkey(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
.invoke_handler(app_handler())
        .setup(|app| {
            builtin_plugins::init(app.handle())?;
            #[cfg(feature="native-runtime")] let service=native_service::requested();
            #[cfg(not(feature="native-runtime"))] let service=false;
            if !service {if let Err(error)=plugins::init(app.handle()) { eprintln!("插件目录加载失败：{error}"); }}
            let handle = app.handle().clone();

            // 第一次运行新版：生成 config.toml，把老 config.json 里的设置搬过去
            if let Err(e) = config::ensure(
                &config_dir(&handle),
                handle.autolaunch().is_enabled().unwrap_or(false),
            ) {
                eprintln!("生成 config.toml 失败：{}", e);
            }
            // 第一次运行带提示词功能的版本：生成 prompts.toml，
            // 上一版写在 config.toml 里的 system_prompt 搬过去，config 里换成 prompt = 编号
            let dir = config_dir(&handle);
            if !dir.join(prompts::FILE_NAME).exists() {
                match prompts::ensure(&dir, &load_config(&handle).ai.system_prompt) {
                    Ok(id) => {
                        let _ = config::upgrade_prompt_key(&config_path(&handle), id);
                    }
                    Err(e) => eprintln!("生成 prompts.toml 失败：{}", e),
                }
            }
            let cfg = load_config(&handle);

            // 历史会话在这里一次性读进内存，之后每次回答完再落盘
            app.manage(AppState {
                store: Mutex::new(load_store(&handle)),
                current: Mutex::new(new_id()),
                pinned: AtomicBool::new(false),
                fx_payload: Mutex::new(None),
            });

            #[cfg(feature="native-runtime")]
            if native_service::requested(){native_service::start(handle.clone())?;return Ok(());}
            // 热键来自 config.toml；写错或被占用时退回 Alt+Space，错误在窗口里提示
            let _ = apply_hotkey(&handle, &cfg.general.hotkey);
            apply_autostart(&handle, cfg.general.autostart);
            #[cfg(feature="native-runtime")] gesture_overlay::watch(handle.clone());
            notes_bridge::watch(handle.clone());
            #[cfg(feature="native-runtime")] wechat::watch(handle.clone());
            #[cfg(not(feature="native-runtime"))] native_bridge::start_background(&handle);
            watch_config(handle.clone());
            watch_updates(handle.clone());
            watch_online_themes(handle.clone());

            let show_item = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let plugins_item=MenuItem::with_id(app,"plugins","插件管理",true,None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &plugins_item, &sep, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                // 鼠标悬停在托盘图标上时显示
                .tooltip("闪念")
                .menu(&menu)
                // 菜单只在右键时弹出；左键留给"显示/隐藏窗口"
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => toggle_window(app),
                    "plugins" => {let app=app.clone();tauri::async_runtime::spawn(async move{let _=plugin_manager::open(&app);});}
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // 只认左键抬起，右键的按下/抬起都不要触发显隐
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_window(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| match event {
            // 下面这些只针对主窗口，全屏特效窗口自己管自己
            _ if window.label() != "main" => {}
            // 失焦即隐藏：用完即走。固定之后不隐藏；
            // 鼠标还在窗口上时也不隐藏，否则拖边缘缩放会被自己隐藏掉
            WindowEvent::Focused(false) => {
                reset_voice(window.app_handle());
                // 刚建全屏特效窗口的这一小会儿，焦点可能被它抢走，抢回来而不是隐藏
                if now_ms() < FX_GUARD_UNTIL.load(Ordering::Relaxed) {
                    let _ = window.set_focus();
                    return;
                }
                let pinned = window
                    .app_handle()
                    .state::<AppState>()
                    .pinned
                    .load(Ordering::Relaxed);
                if !pinned && !cursor_inside(window) {
                    hide_window(window.app_handle().clone());
                }
            }
            // 点右上角关闭按钮也只是隐藏，进程继续常驻
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                hide_window(window.app_handle().clone());
            }
            _ => {}
        })
        .build(context)
        .expect("error while building tauri application")
        .run(|_, event| { if matches!(event, tauri::RunEvent::Exit) { #[cfg(feature="native-runtime")] tauri::async_runtime::block_on(downloads::shutdown());
            #[cfg(not(feature="native-runtime"))] tauri::async_runtime::block_on(native_bridge::shutdown()); } });
}
