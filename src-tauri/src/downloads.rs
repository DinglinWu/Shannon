//! Experimental HTTP download plugin backed by a pinned, optional Gopeed process.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager};
const VERSION: &str = "1.9.3";
const URL: &str = "https://api.github.com/repos/GopeedLab/gopeed/releases/assets/376411346";
const HASH: &str = "ee3d41fa19c366b820a2483619328156579108ac03c53ea35cdb33327320f756";
const SIZE: usize = 40411930;
const EXE_HASH: &str = "71ae0d89f7ac1f3a5e481aa2ee836ce110738f606cc61b68764ca9131aa27bc4";
static CREATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static VERIFIED: Mutex<Option<(PathBuf, std::time::SystemTime, u64)>> = Mutex::new(None);
static ENGINE: tokio::sync::Mutex<Option<Engine>> = tokio::sync::Mutex::const_new(None);
static INSTALLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static PENDING: Mutex<Option<Review>> = Mutex::new(None);
static JOB: Mutex<Option<String>> = Mutex::new(None);
struct Engine {
    child: Child,
    port: u16,
    token: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub url: String,
    #[serde(default = "connections")]
    pub connections: u8,
}
fn connections() -> u8 {
    4
}
impl Draft {
    pub fn validate(&self) -> Result<(), String> {
        let u = reqwest::Url::parse(&self.url).map_err(|_| "请输入完整的 HTTP(S) 下载链接")?;
        if !["http", "https"].contains(&u.scheme())
            || u.host_str().is_none()
            || !u.username().is_empty()
            || u.password().is_some()
            || self.url.len() > 4096
            || self.url.chars().any(|c| c.is_control())
            || ![1, 4].contains(&self.connections)
        {
            return Err("仅支持HTTP(S)链接和1或4个连接，不支持链接内的用户名密码".into());
        }
        Ok(())
    }
}
#[derive(Clone, Serialize)]
struct Review {
    token: String,
    draft: Draft,
    folder: String,
}
fn nonce() -> String {
    let mut b = [0u8; 24];
    getrandom::fill(&mut b).expect("random");
    b.iter().map(|b| format!("{b:02x}")).collect()
}
fn root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|p| p.join("components/gopeed/1.9.3"))
        .map_err(|e| e.to_string())
}
fn folder(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .download_dir()
        .map(|p| p.join("Shannon"))
        .map_err(|e| e.to_string())
}
fn only(w: &tauri::WebviewWindow) -> Result<(), String> {
    if w.label() == "downloads" {
        Ok(())
    } else {
        Err("仅下载窗口可确认操作".into())
    }
}
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())
}
fn checksum(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn executable(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = root(app)?;
    let p = dir.join("gopeed.exe");
    let meta = fs::metadata(&p).map_err(|_| "尚未安装下载组件，请在下载窗口安装")?;
    let stamp = meta.modified().map_err(|e| e.to_string())?;
    if VERIFIED.lock().map_err(|e| e.to_string())?.as_ref() == Some(&(p.clone(), stamp, meta.len()))
    {
        return Ok(p);
    }

    let b = fs::read(&p).map_err(|_| "下载组件缺失，请重新安装")?;
    if checksum(&b) != EXE_HASH {
        return Err("下载组件校验失败，请重新安装".into());
    }
    *VERIFIED.lock().map_err(|e| e.to_string())? = Some((p.clone(), stamp, meta.len()));
    Ok(p)
}
pub fn open(app: &AppHandle) -> Result<Value, String> {
    crate::builtin_plugins::require("download")?;
    if let Some(w) = app.get_webview_window("downloads") {
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
    } else {
        tauri::WebviewWindowBuilder::new(
            app,
            "downloads",
            tauri::WebviewUrl::App("downloads.html".into()),
        )
        .title("下载 · 闪念")
        .inner_size(740., 700.)
        .min_inner_size(520., 480.)
        .build()
        .map_err(|e| e.to_string())?;
    }
    Ok(json!({"opened":true}))
}
pub fn prepare(app: &AppHandle, draft: Draft) -> Result<Value, String> {
    draft.validate()?;
    let path = folder(app)?;
    let mut pending = PENDING.lock().map_err(|e| e.to_string())?;
    if let Some(p) = pending.as_ref() {
        if p.draft.url != draft.url || p.draft.connections != draft.connections {
            return Err("已有待确认下载，请先处理下载窗口中的申请".into());
        }
    } else {
        *pending = Some(Review {
            token: nonce(),
            draft,
            folder: path.display().to_string(),
        });
    }
    drop(pending);
    let display_error = open(app).err();
    let _ = app.emit("downloads-updated", ());
    Ok(json!({"pending":true,"started":false,"display_error":display_error}))
}
async fn rpc(
    port: u16,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let c = client()?;
    let mut req = c
        .request(method, format!("http://127.0.0.1:{port}/api/v1/{path}"))
        .header("X-Api-Token", token);
    if path == "info" {
        req = req.timeout(std::time::Duration::from_millis(600));
    }
    if path == "tasks/pause" {
        req = req.timeout(std::time::Duration::from_secs(2));
    }
    if let Some(b) = body {
        req = req.json(&b);
    }
    let response = req
        .send()
        .await
        .map_err(|_| "下载引擎未响应；操作结果可能未知，请先刷新任务，不要重复提交")?;
    if !response.status().is_success() {
        return Err("下载引擎接口返回错误".into());
    }
    let v: Value = response.json().await.map_err(|_| "下载引擎响应格式错误")?;
    decode(v)
}
fn decode(v: Value) -> Result<Value, String> {
    if v["code"].as_i64() != Some(0) {
        return Err("Gopeed 未能完成操作，请检查下载任务或引擎状态".into());
    }
    Ok(v["data"].clone())
}
async fn engine(app: &AppHandle) -> Result<(u16, String), String> {
    let mut state = ENGINE.lock().await;
    if let Some(e) = state.as_mut() {
        if matches!(e.child.try_wait(), Ok(None)) {
            return Ok((e.port, e.token.clone()));
        }
        *state = None;
    }
    let exe = executable(app)?;
    let dir = root(app)?;
    let downloads = folder(app)?;
    fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    let socket = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = socket.local_addr().unwrap().port();
    drop(socket);
    let token = nonce();
    let cfg = json!({"address":"127.0.0.1","port":port,"apiToken":token,"username":"shannon","password":nonce(),"storageDir":dir.join("storage"),"whiteDownloadDirs":[downloads],"downloadConfig":{"downloadDir":downloads,"maxRunning":3,"autoTorrent":{"enable":false},"archive":{"autoExtract":false},"script":{"enable":false}}});
    fs::write(dir.join("runtime.json"), serde_json::to_vec(&cfg).unwrap())
        .map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("-c")
        .arg(dir.join("runtime.json"))
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(|_| "无法启动下载组件")?;
    for _ in 0..20 {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("下载引擎启动失败，请检查端口或组件")?;
        }
        if rpc(port, &token, reqwest::Method::GET, "info", None)
            .await
            .is_ok()
        {
            *state = Some(Engine {
                child,
                port,
                token: token.clone(),
            });
            return Ok((port, token));
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    let _ = child.kill();
    let _ = child.wait();
    Err("下载引擎启动超时".into())
}
pub async fn list(app: &AppHandle) -> Result<Value, String> {
    if executable(app).is_err() {
        return Ok(json!({"installed":false,"tasks":[]}));
    }
    let (p, t) = engine(app).await?;
    let tasks = rpc(p, &t, reqwest::Method::GET, "tasks", None).await?;
    let tasks=tasks.as_array().ok_or("下载列表格式错误")?.iter().map(|v|json!({"id":v["id"],"name":v["name"],"status":v["status"],"progress":v["progress"],"size":v["meta"]["res"]["size"],"connections":v["meta"]["opts"]["extra"]["connections"]})).collect::<Vec<_>>();
    Ok(json!({"installed":true,"tasks":tasks,"folder":folder(app)?.display().to_string()}))
}
pub fn valid_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 80
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        Err("下载任务ID无效，请先查询任务列表".into())
    } else {
        Ok(())
    }
}
pub async fn control(app: &AppHandle, id: &str, action: &str) -> Result<Value, String> {
    valid_id(id)?;
    if !["pause", "continue", "cancel"].contains(&action) {
        return Err("下载操作无效".into());
    }
    let (p, t) = engine(app).await?;
    rpc(p, &t, reqwest::Method::GET, &format!("tasks/{id}"), None).await?;
    let (method, path) = if action == "cancel" {
        (reqwest::Method::DELETE, format!("tasks/{id}?force=false"))
    } else {
        (reqwest::Method::PUT, format!("tasks/{id}/{action}"))
    };
    rpc(p, &t, method, &path, None).await?;
    Ok(json!({"id":id,"action":action,"files_deleted":false}))
}
#[tauri::command]
pub async fn downloads_snapshot(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Value, String> {
    only(&window)?;
    let mut v = list(&app).await?;
    v["pending"] =
        serde_json::to_value(PENDING.lock().map_err(|e| e.to_string())?.clone()).unwrap();
    v["installing"] = json!(INSTALLING.load(std::sync::atomic::Ordering::SeqCst));
    v["install_error"] = json!(JOB.lock().map_err(|e| e.to_string())?.clone());
    v["engine_version"] = json!(VERSION);
    v["component_bytes"] = json!(SIZE);
    v["folder"] = json!(folder(&app)?.display().to_string());
    Ok(v)
}
#[tauri::command]
pub fn downloads_prepare(
    app: AppHandle,
    window: tauri::WebviewWindow,
    draft: Draft,
) -> Result<Value, String> {
    only(&window)?;
    prepare(&app, draft)
}
#[tauri::command]
pub async fn downloads_control(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    action: String,
) -> Result<Value, String> {
    only(&window)?;
    control(&app, &id, &action).await
}
#[tauri::command]
pub async fn downloads_confirm(
    app: AppHandle,
    window: tauri::WebviewWindow,
    token: String,
    approve: bool,
) -> Result<Value, String> {
    only(&window)?;
    let _serial = CREATE.lock().await;
    let review = {
        let mut pending = PENDING.lock().map_err(|e| e.to_string())?;
        if pending.as_ref().map(|r| r.token.as_str()) != Some(token.as_str()) {
            return Err("下载申请已失效，请刷新")?;
        }
        pending.take().unwrap()
    };
    if !approve {
        return Ok(json!({"started":false,"cancelled":true}));
    }
    review.draft.validate()?;
    let (p, t) = engine(&app).await?;
    // Same URL in this private engine is reused, even after completion. Never silently duplicate a download.
    let existing = rpc(p, &t, reqwest::Method::GET, "tasks", None).await?;
    if let Some(task) = existing.as_array().and_then(|a| {
        a.iter()
            .find(|v| v["meta"]["req"]["url"].as_str() == Some(&review.draft.url))
    }) {
        return Ok(json!({"id":task["id"],"existing":true,"started":false}));
    }
    let id=rpc(p,&t,reqwest::Method::POST,"tasks",Some(json!({"req":{"url":review.draft.url},"opts":{"path":folder(&app)?,"extra":{"connections":review.draft.connections,"autoTorrent":false,"autoExtract":false}}}))).await?;
    Ok(json!({"id":id,"started":true}))
}
#[tauri::command]
pub fn downloads_install(app: AppHandle, window: tauri::WebviewWindow) -> Result<Value, String> {
    only(&window)?;
    if INSTALLING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Ok(json!({"started":false,"running":true}));
    }
    *JOB.lock().map_err(|e| e.to_string())? = None;
    tauri::async_runtime::spawn(async move {
        let r = install(&app).await;
        *JOB.lock().unwrap() = r.err();
        INSTALLING.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = app.emit("downloads-updated", ());
    });
    Ok(json!({"started":true}))
}
async fn install(app: &AppHandle) -> Result<(), String> {
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    return Err("试用组件仅支持Windows x64".into());
    let c = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = c
        .get(URL)
        .header("Accept", "application/octet-stream")
        .header("User-Agent", "Shannon-Download-Plugin")
        .send()
        .await
        .map_err(|_| "组件下载失败，请检查GitHub网络连接后重试")?
        .error_for_status()
        .map_err(|_| "组件下载地址不可用")?;
    let mut data = Vec::with_capacity(SIZE);
    while let Some(b) = response.chunk().await.map_err(|_| "组件下载中断")? {
        if data.len() + b.len() > SIZE {
            return Err("组件包大小异常".into());
        }
        data.extend_from_slice(&b);
    }
    install_bytes(app, &data)
}
fn install_bytes(app: &AppHandle, data: &[u8]) -> Result<(), String> {
    if data.len() != SIZE || checksum(data) != HASH {
        return Err("组件包哈希校验失败".into());
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| e.to_string())?;
    let names: Vec<_> = zip
        .file_names()
        .filter(|n| *n == "gopeed-web-v1.9.3-windows-amd64/gopeed.exe")
        .map(String::from)
        .collect();
    if names.len() != 1 {
        return Err("组件包结构异常".into());
    }
    let mut exe = Vec::new();
    zip.by_name(&names[0])
        .map_err(|e| e.to_string())?
        .take(128 * 1024 * 1024 + 1)
        .read_to_end(&mut exe)
        .map_err(|e| e.to_string())?;
    if exe.len() > 128 * 1024 * 1024 || !exe.starts_with(b"MZ") {
        return Err("组件程序格式异常".into());
    }
    let dir = root(app)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("gopeed.exe.part"), &exe).map_err(|e| e.to_string())?;
    fs::rename(dir.join("gopeed.exe.part"), dir.join("gopeed.exe")).map_err(|e| e.to_string())?;
    fs::write(
        dir.join("LICENSE"),
        include_str!("../../third_party/gopeed/LICENSE"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(dir.join("SOURCE.txt"),"Gopeed 1.9.3, unmodified upstream binary. GPL-3.0. Corresponding source: https://github.com/GopeedLab/gopeed/tree/v1.9.3\nhttps://github.com/GopeedLab/gopeed/archive/refs/tags/v1.9.3.zip\n").map_err(|e|e.to_string())?;
    Ok(())
}
pub async fn shutdown() {
    let mut state = ENGINE.lock().await;
    if let Some(mut e) = state.take() {
        let _ = rpc(e.port, &e.token, reqwest::Method::PUT, "tasks/pause", None).await;
        let _ = e.child.kill();
        let _ = e.child.wait();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_download_requests() {
        for url in [
            "file:///C:/a",
            "ftp://example.com/a",
            "https://user:pass@example.com/a",
            "https://example.com/a\n",
        ] {
            assert!(Draft {
                url: url.into(),
                connections: 4
            }
            .validate()
            .is_err());
        }
        assert!(Draft {
            url: "https://example.com/a.zip".into(),
            connections: 4
        }
        .validate()
        .is_ok());
        assert!(Draft {
            url: "https://example.com/a".into(),
            connections: 16
        }
        .validate()
        .is_err());
    }
    #[test]
    fn bounded_ids_and_explicit_result() {
        assert!(valid_id("../pause").is_err());
        assert!(valid_id("a?force=true").is_err());
        assert!(valid_id("abc_-123").is_ok());
        assert!(decode(json!({"code":1000,"msg":"secret"})).is_err());
        assert_eq!(decode(json!({"code":0,"data":{"id":1}})).unwrap()["id"], 1);
        assert!(decode(json!({"data":[]})).is_err());
    }
}

#[cfg(test)]
mod rpc_tests {
    use super::*;
    #[tokio::test]
    async fn local_api_auth_envelope_and_no_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = server.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (mut stream, _) = server.accept().await.unwrap();
            let mut b = [0; 4096];
            let n = stream.read(&mut b).await.unwrap();
            let req = String::from_utf8_lossy(&b[..n]).to_lowercase();
            assert!(req.starts_with("get /api/v1/tasks"));
            assert!(req.contains("x-api-token: test-secret"));
            let body = r#"{"code":0,"data":[{"id":"real-task"}]}"#;
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
        });
        assert_eq!(
            rpc(port, "test-secret", reqwest::Method::GET, "tasks", None)
                .await
                .unwrap()[0]["id"],
            "real-task"
        );
        task.await.unwrap();
    }
}
