use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    net::{Ipv4Addr, UdpSocket},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};
static DATA: Mutex<()> = Mutex::new(());
static STATE: Mutex<State> = Mutex::new(State {
    running: None,
    pending: None,
    window_open: false,
    epoch: 0,
});
struct State {
    window_open: bool,
    epoch: u64,
    running: Option<Running>,
    pending: Option<Pending>,
}
impl State {
    fn permits_start(&self, epoch: u64) -> bool {
        self.window_open && self.epoch == epoch
    }
    fn close_window(&mut self) {
        if let Some(run) = self.running.take() {
            let _ = run.stop.send(true);
        }
        self.pending = None;
        self.window_open = false;
        self.epoch = self.epoch.wrapping_add(1);
    }
}
struct Running {
    addr: String,
    code: String,
    stop: tokio::sync::watch::Sender<bool>,
    task: tauri::async_runtime::JoinHandle<()>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub title: String,
    pub content: String,
    #[serde(default = "default_minutes")]
    pub minutes: u64,
    #[serde(default)]
    pub pinned: bool,
}
fn default_minutes() -> u64 {
    1440
}
impl Draft {
    pub fn validate(&mut self) -> Result<(), String> {
        self.title = self.title.trim().into();
        self.content = self.content.trim().into();
        if self.title.is_empty()
            || self.title.chars().count() > 100
            || self.title.chars().any(char::is_control)
        {
            return Err("标题须为1–100字，不含换行".into());
        }
        if self.content.is_empty()
            || self.content.chars().count() > 6000
            || self.content.contains('\0')
        {
            return Err("正文须为1–6000字".into());
        }
        if !(1..=10080).contains(&self.minutes) {
            return Err("有效期须为1分钟到7天".into());
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub pinned: bool,
    pub created_at: u64,
    pub expires_at: u64,
    pub withdrawn: bool,
}
#[derive(Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    next_id: u64,
    #[serde(default)]
    posts: Vec<Post>,
}
impl Store {
    fn publish(&mut self, draft: Draft, time: u64) -> Result<u64, String> {
        if self.posts.len() >= 200 {
            return Err("广播已达到200条存储上限".into());
        }
        let id = self.next_id.checked_add(1).ok_or("广播编号已达上限")?;
        let expires_at = time
            .checked_add(draft.minutes.checked_mul(60).ok_or("有效期过大")?)
            .ok_or("时间无效")?;
        self.posts.insert(
            0,
            Post {
                id,
                title: draft.title,
                content: draft.content,
                pinned: draft.pinned,
                created_at: time,
                expires_at,
                withdrawn: false,
            },
        );
        self.next_id = id;
        Ok(id)
    }
    fn withdraw(&mut self, id: u64) -> Result<(), String> {
        self.posts
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or("广播不存在")?
            .withdrawn = true;
        Ok(())
    }
    fn visible(&self, time: u64) -> Vec<Post> {
        let mut items: Vec<_> = self
            .posts
            .iter()
            .filter(|p| !p.withdrawn && p.expires_at > time)
            .cloned()
            .collect();
        items.sort_by_key(|p| {
            (
                !p.pinned,
                std::cmp::Reverse(p.created_at),
                std::cmp::Reverse(p.id),
            )
        });
        items
    }
}
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Operation {
    Publish { draft: Draft },
    Withdraw { post: Post },
}
#[derive(Clone, Serialize)]
struct Pending {
    token: String,
    created_at: u64,
    #[serde(flatten)]
    operation: Operation,
}
impl Pending {
    fn authorized(&self, token: &str, time: u64) -> bool {
        self.token == token && time.saturating_sub(self.created_at) <= 600
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn secret() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn read(app: &AppHandle) -> Result<Store, String> {
    match fs::read(crate::config_dir(app).join("broadcast.json")) {
        Ok(b) => serde_json::from_slice(&b).map_err(|e| format!("广播记录无法读取：{e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(e) => Err(e.to_string()),
    }
}
fn save(app: &AppHandle, store: &Store) -> Result<(), String> {
    let path = crate::config_dir(app).join("broadcast.json");
    let next = path.with_extension("json.next");
    fs::write(&next, serde_json::to_vec(store).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    fs::rename(next, path).map_err(|e| e.to_string())
}
pub fn visible(app: &AppHandle) -> Result<Vec<Post>, String> {
    let _lock = DATA.lock().map_err(|e| e.to_string())?;
    Ok(read(app)?.visible(now()))
}
pub fn list(app: &AppHandle) -> Result<Value, String> {
    let _lock = DATA.lock().map_err(|e| e.to_string())?;
    let store = read(app)?;
    Ok(
        json!({"posts":store.posts.iter().take(30).map(|p|json!({"id":p.id,"title":p.title,"preview":p.content.chars().take(120).collect::<String>(),"pinned":p.pinned,"created_at":p.created_at,"expires_at":p.expires_at,"withdrawn":p.withdrawn})).collect::<Vec<_>>(),"total":store.posts.len()}),
    )
}
pub fn read_post(app: &AppHandle, id: u64) -> Result<Value, String> {
    let _lock = DATA.lock().map_err(|e| e.to_string())?;
    let store = read(app)?;
    Ok(json!(store
        .posts
        .iter()
        .find(|p| p.id == id)
        .ok_or("广播不存在")?))
}
pub fn status(app: &AppHandle) -> Result<Value, String> {
    let state = STATE.lock().map_err(|e| e.to_string())?;
    let running = state.running.as_ref();
    Ok(
        json!({"sharing":running.is_some(),"url":running.map(|r|format!("http://{}",r.addr)),"visible_count":visible(app)?.len()}),
    )
}
pub fn open(app: &AppHandle) -> Result<Value, String> {
    crate::builtin_plugins::require("broadcast")?;
    if let Some(w) = app.get_webview_window("broadcast") {
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
    } else {
        let window = tauri::WebviewWindowBuilder::new(
            app,
            "broadcast",
            tauri::WebviewUrl::App("broadcast.html".into()),
        )
        .title("广播站 · 闪念")
        .visible(false)
        .inner_size(680.0, 760.0)
        .min_inner_size(420.0, 520.0)
        .build()
        .map_err(|e| e.to_string())?;
        window.on_window_event(|event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                if let Ok(mut state) = STATE.lock() {
                    state.close_window();
                }
            }
        });
        {
            let mut state = STATE.lock().map_err(|e| e.to_string())?;
            state.window_open = true;
            state.epoch = state.epoch.wrapping_add(1);
        }
        if let Err(error) = window.show() {
            if let Ok(mut state) = STATE.lock() {
                state.close_window();
            }
            return Err(error.to_string());
        }
    }
    Ok(json!({"opened":true,"sharing_started":false}))
}
fn request(app: &AppHandle, operation: Operation) -> Result<Value, String> {
    let token = secret()?;
    {
        let mut state = STATE.lock().map_err(|e| e.to_string())?;
        if state.pending.is_some() {
            return Err("请先确认或取消当前广播预览".into());
        }
        state.pending = Some(Pending {
            token: token.clone(),
            created_at: now(),
            operation,
        });
    }
    if let Err(error) = open(app) {
        if let Ok(mut state) = STATE.lock() {
            if state.pending.as_ref().map(|p| p.token.as_str()) == Some(&token) {
                state.pending = None;
            }
        }
        return Err(error);
    }
    Ok(json!({"confirmation_opened":true,"published":false}))
}
pub fn prepare(app: &AppHandle, mut draft: Draft) -> Result<Value, String> {
    draft.validate()?;
    request(app, Operation::Publish { draft })
}
pub fn request_withdraw(app: &AppHandle, id: u64) -> Result<Value, String> {
    let post: Post = serde_json::from_value(read_post(app, id)?).map_err(|e| e.to_string())?;
    request(app, Operation::Withdraw { post })
}
fn own(w: &tauri::WebviewWindow) -> Result<(), String> {
    if w.label() == "broadcast" {
        Ok(())
    } else {
        Err("仅广播站窗口可确认发布或控制共享".into())
    }
}
fn detected_ip() -> String {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|s| {
            s.connect("192.0.2.1:80")?;
            s.local_addr()
        })
        .ok()
        .map(|a| a.ip())
        .filter(|a| matches!(a,std::net::IpAddr::V4(v)if v.is_private()))
        .map(|a| a.to_string())
        .unwrap_or_default()
}
#[tauri::command]
pub fn broadcast_snapshot(app: AppHandle, window: tauri::WebviewWindow) -> Result<Value, String> {
    own(&window)?;
    let mut state = STATE.lock().map_err(|e| e.to_string())?;
    if state
        .pending
        .as_ref()
        .map(|p| now().saturating_sub(p.created_at) > 600)
        .unwrap_or(false)
    {
        state.pending = None;
    }
    let _lock = DATA.lock().map_err(|e| e.to_string())?;
    let store = read(&app)?;
    Ok(
        json!({"posts":store.posts,"pending":state.pending,"sharing":state.running.is_some(),"address":state.running.as_ref().map(|r|format!("http://{}",r.addr)),"code":state.running.as_ref().map(|r|r.code.clone()),"share_url":state.running.as_ref().map(|r|format!("http://{}/#{}",r.addr,r.code)),"suggested_ip":detected_ip()}),
    )
}
#[tauri::command]
pub fn broadcast_prepare(
    app: AppHandle,
    window: tauri::WebviewWindow,
    draft: Draft,
) -> Result<Value, String> {
    own(&window)?;
    prepare(&app, draft)
}
#[tauri::command]
pub fn broadcast_withdraw(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: u64,
) -> Result<Value, String> {
    own(&window)?;
    request_withdraw(&app, id)
}
#[tauri::command]
pub fn broadcast_confirm(
    app: AppHandle,
    window: tauri::WebviewWindow,
    token: String,
    accepted: bool,
) -> Result<Value, String> {
    own(&window)?;
    let mut state = STATE.lock().map_err(|e| e.to_string())?;
    let pending = state
        .pending
        .as_ref()
        .filter(|p| p.authorized(&token, now()))
        .ok_or("预览已过期，请重新准备广播")?;
    if !accepted {
        state.pending = None;
        return Ok(json!({"cancelled":true}));
    }
    let _lock = DATA.lock().map_err(|e| e.to_string())?;
    let mut store = read(&app)?;
    let result = match &pending.operation {
        Operation::Publish { draft } => {
            json!({"published":true,"id":store.publish(draft.clone(),now())?})
        }
        Operation::Withdraw { post } => {
            store.withdraw(post.id)?;
            json!({"withdrawn":true,"id":post.id})
        }
    };
    save(&app, &store)?;
    state.pending = None;
    Ok(result)
}
pub fn stop() {
    if let Ok(mut state) = STATE.lock() {
        if let Some(run) = state.running.take() {
            let _ = run.stop.send(true);
            let _ = &run.task;
        }
    }
}
#[tauri::command]
pub fn broadcast_stop(window: tauri::WebviewWindow) -> Result<(), String> {
    own(&window)?;
    stop();
    Ok(())
}
#[tauri::command]
pub async fn broadcast_start(
    app: AppHandle,
    window: tauri::WebviewWindow,
    ip: String,
) -> Result<(), String> {
    own(&window)?;
    let epoch = {
        let state = STATE.lock().map_err(|e| e.to_string())?;
        if !state.window_open {
            return Err("广播窗口已关闭，未开启共享".into());
        }
        state.epoch
    };
    let ip: Ipv4Addr = ip.parse().map_err(|_| "请填写这台电脑的局域网IPv4地址")?;
    if !ip.is_private() {
        return Err("只允许绑定局域网IPv4地址".into());
    }
    let listener = tokio::net::TcpListener::bind((ip, 0))
        .await
        .map_err(|e| format!("无法开启共享，请确认地址属于本机：{e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .to_string();
    let code = secret()?;
    let (tx, rx) = tokio::sync::watch::channel(false);
    let mut state = STATE.lock().map_err(|e| e.to_string())?;
    if !state.permits_start(epoch) {
        return Err("窗口已关闭或已重新打开，未开启共享".into());
    }
    if state.running.is_some() {
        return Err("共享已经开启，请先停止再切换地址".into());
    }
    let task_addr = addr.clone();
    let task_code = code.clone();
    let task = tauri::async_runtime::spawn(async move {
        crate::broadcast_http::serve(listener, app, task_addr, task_code.clone(), rx).await;
        if let Ok(mut state) = STATE.lock() {
            if state.running.as_ref().map(|r| r.code.as_str()) == Some(&task_code) {
                state.running = None;
            }
        }
    });
    state.running = Some(Running {
        addr,
        code,
        stop: tx,
        task,
    });
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn draft() -> Draft {
        Draft {
            title: "通知".into(),
            content: "下午三点见".into(),
            minutes: 60,
            pinned: false,
        }
    }
    #[test]
    fn publishes_only_validated_bounded_drafts_and_orders_expiry_and_withdrawal() {
        let mut d = draft();
        d.validate().unwrap();
        let mut store = Store::default();
        let a = store.publish(d.clone(), 100).unwrap();
        d.pinned = true;
        let b = store.publish(d, 110).unwrap();
        assert_eq!(
            store.visible(120).iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![b, a]
        );
        store.withdraw(b).unwrap();
        assert_eq!(store.visible(120).len(), 1);
        assert!(store.visible(4000).is_empty());
        assert!(store.withdraw(999).is_err());
        let loaded: Store = serde_json::from_str(&serde_json::to_string(&store).unwrap()).unwrap();
        assert_eq!(loaded.next_id, b);
    }
    #[test]
    fn closing_and_reopening_invalidates_an_in_flight_start() {
        let mut state = State {
            running: None,
            pending: None,
            window_open: true,
            epoch: 7,
        };
        let started_epoch = state.epoch;
        assert!(state.permits_start(started_epoch));
        state.close_window();
        assert!(!state.permits_start(started_epoch));
        state.window_open = true;
        assert!(!state.permits_start(started_epoch));
        assert!(state.permits_start(state.epoch));
    }
    #[test]
    fn preview_approval_is_bound_to_the_exact_token_and_expiry() {
        let pending = Pending {
            token: "native-only-token".into(),
            created_at: 100,
            operation: Operation::Publish { draft: draft() },
        };
        assert!(pending.authorized("native-only-token", 700));
        assert!(!pending.authorized("native-only-token", 701));
        assert!(!pending.authorized("model-confirmed-true", 101));
    }
    #[test]
    fn rejects_unknown_fields_and_invalid_drafts() {
        assert!(serde_json::from_value::<Draft>(
            json!({"title":"通知","content":"正文","confirmed":true})
        )
        .is_err());
        let mut d = draft();
        d.minutes = 0;
        assert!(d.validate().is_err());
        d = draft();
        d.title = "\n".into();
        assert!(d.validate().is_err());
        d = draft();
        d.content = "字".repeat(6001);
        assert!(d.validate().is_err());
        assert_ne!(secret().unwrap(), secret().unwrap());
    }
}
