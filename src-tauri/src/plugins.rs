//! Third-party tool plugins: reviewed packages and an opaque-origin JS worker.
use crate::plugin_package::{self, Manifest, Package, Tool};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    hash::{BuildHasher, Hasher},
    io::Read,
    path::PathBuf,
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Installed {
    #[serde(default)]
    readme: String,
    manifest: Manifest,
    sha256: String,
    code_hash: String,
    source: String,
    enabled: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginTarget {
    pub id: String,
    pub tool: Option<String>,
    #[serde(default)]
    pub builtin: bool,
}
struct Store {
    root: PathBuf,
    installed: BTreeMap<String, Installed>,
}
static STORE: Mutex<Option<Store>> = Mutex::new(None);
#[derive(Clone, Serialize)]
pub struct Review {
    readme: String,
    #[serde(default)]
    builtin: bool,
    #[serde(default)]
    builtin_status: Option<String>,
    #[serde(default)]
    official_source: Option<String>,
    token: String,
    action: String,
    id: String,
    manifest: Manifest,
    source: String,
    sha256: String,
}
struct Pending {
    review: Review,
    package: Option<Package>,
}
static REVIEW: Mutex<Option<Pending>> = Mutex::new(None);
struct Invocation {
    plugin: String,
    sha256: String,
    permissions: Vec<String>,
    sender: oneshot::Sender<Result<Value, String>>,
    calls: usize,
}
static RUNNING: Mutex<BTreeMap<String, Invocation>> = Mutex::new(BTreeMap::new());
fn token() -> String {
    format!(
        "{:016x}{:016x}",
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish(),
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
    )
}
fn main_only(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("仅搜索框可管理和执行插件".into())
    }
}
fn store_mut<T>(f: impl FnOnce(&mut Store) -> Result<T, String>) -> Result<T, String> {
    let mut lock = STORE.lock().map_err(|e| e.to_string())?;
    f(lock.as_mut().ok_or("插件系统未就绪，请重启应用")?)
}
fn code_path(store: &Store, item: &Installed) -> PathBuf {
    store
        .root
        .join("packages")
        .join(&item.manifest.id)
        .join(&item.sha256)
        .join("main.js")
}
fn save(store: &Store, next: &BTreeMap<String, Installed>) -> Result<(), String> {
    let path = store.root.join("plugins.next.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(next).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(path, store.root.join("plugins.json")).map_err(|e| e.to_string())
}
pub fn init(app: &AppHandle) -> Result<(), String> {
    let root = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("plugins");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let file = root.join("plugins.json");
    let installed: BTreeMap<String, Installed> = if file.exists() {
        if fs::metadata(&file).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
            return Err("插件目录过大".into());
        }
        serde_json::from_slice(&fs::read(file).map_err(|e| e.to_string())?)
            .map_err(|e| format!("插件目录损坏：{e}"))?
    } else {
        BTreeMap::new()
    };
    if installed.len() > 32
        || installed
            .values()
            .map(|p| p.manifest.tools.len())
            .sum::<usize>()
            > 64
    {
        return Err("插件或工具目录超过容量限制".into());
    }
    let mut store = Store { root, installed };
    for (id, item) in &mut store.installed {
        plugin_package::validate_manifest(&item.manifest)?;
        if id != &item.manifest.id || !hash_valid(&item.sha256) || !hash_valid(&item.code_hash) {
            return Err("插件目录记录无效".into());
        }
        let path = store
            .root
            .join("packages")
            .join(id)
            .join(&item.sha256)
            .join("main.js");
        if fs::read(&path)
            .map(|b| plugin_package::digest(&b) != item.code_hash)
            .unwrap_or(true)
        {
            item.enabled = false;
        }
    }
    *STORE.lock().map_err(|e| e.to_string())? = Some(store);
    Ok(())
}
fn hash_valid(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn public_info(item: &Installed) -> Value {
    // Local source paths and private download-query tokens are not model data.
    json!({"manifest":item.manifest,"sha256":item.sha256,"enabled":item.enabled})
}
pub fn list() -> Result<Value, String> {
    store_mut(|s| {
        Ok(
            json!({"plugins":s.installed.values().map(public_info).collect::<Vec<_>>(),"api_version":1}),
        )
    })
}
pub fn info(id: &str) -> Result<Value, String> {
    store_mut(|s| Ok(public_info(s.installed.get(id).ok_or("插件不存在")?)))
}
pub fn registry() -> Vec<crate::ai_tools::ToolSpec> {
    store_mut(|s| {
        Ok(s.installed
            .values()
            .filter(|p| p.enabled)
            .flat_map(|p| {
                p.manifest.tools.iter().map(|t| crate::ai_tools::ToolSpec {
                    name: plugin_package::tool_name(&p.manifest.id, &t.name),
                    description: format!("第三方插件「{}」：{}", p.manifest.name, t.description),
                    effect: t.effect.clone(),
                    parameters: t.parameters.clone(),
                })
            })
            .collect())
    })
    .unwrap_or_default()
}
pub fn selected_tools(targets: &[PluginTarget]) -> Result<Vec<crate::ai_tools::ToolSpec>, String> {
    store_mut(|s| select_from(&s.installed, targets))
}
fn select_from(
    installed: &BTreeMap<String, Installed>,
    targets: &[PluginTarget],
) -> Result<Vec<crate::ai_tools::ToolSpec>, String> {
    if targets.is_empty() || targets.len() > 8 {
        return Err("请选择1–8个插件或工具".into());
    }
    let mut result = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for target in targets {
        if target.builtin {return Err("内置工具不能作为第三方插件执行".into());}
        let item = installed
            .get(&target.id)
            .filter(|p| p.enabled)
            .ok_or("@选择的插件不存在或已停用，请重新选择")?;
        let tools: Vec<_> = item
            .manifest
            .tools
            .iter()
            .filter(|t| target.tool.as_ref().map_or(true, |name| name == &t.name))
            .collect();
        if tools.is_empty() {
            return Err("@选择的插件工具不存在，请重新选择".into());
        }
        for tool in tools {
            let name = plugin_package::tool_name(&item.manifest.id, &tool.name);
            if names.insert(name.clone()) {
                result.push(crate::ai_tools::ToolSpec {
                    name,
                    description: format!(
                        "第三方插件「{}」：{}",
                        item.manifest.name, tool.description
                    ),
                    effect: tool.effect.clone(),
                    parameters: tool.parameters.clone(),
                });
            }
        }
    }
    Ok(result)
}
fn find(name: &str) -> Result<(Installed, Tool), String> {
    store_mut(|s| {
        for p in s.installed.values().filter(|p| p.enabled) {
            for t in &p.manifest.tools {
                if plugin_package::tool_name(&p.manifest.id, &t.name) == name {
                    return Ok((p.clone(), t.clone()));
                }
            }
        }
        Err("插件工具不存在或已停用".into())
    })
}
pub fn validate_tool(name: &str, args: &Value) -> Result<(), String> {
    let (_, tool) = find(name)?;
    crate::plugin_schema::validate(&tool.parameters, args)
}

async fn load_source(source: &str) -> Result<Package, String> {
    if source.is_empty() || source.len() > 2048 {
        return Err("插件来源无效".into());
    }
    let bytes = if source.starts_with("https://") {
        use futures_util::StreamExt;
        let url = reqwest::Url::parse(source).map_err(|_| "插件下载地址无效")?;
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err("下载地址不允许凭据或片段".into());
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(3))
            .https_only(true)
            .timeout(std::time::Duration::from_secs(45))
            .build()
            .map_err(|e| e.to_string())?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|_| "插件下载失败")?
            .error_for_status()
            .map_err(|_| "插件下载地址不可用")?;
        if response
            .content_length()
            .map_or(false, |n| n > plugin_package::MAX_ZIP as u64)
        {
            return Err("插件ZIP超过4MiB".into());
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "插件下载中断")?;
            if bytes.len() + chunk.len() > plugin_package::MAX_ZIP {
                return Err("插件ZIP超过4MiB".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        bytes
    } else {
        if source.contains("://") {
            return Err("在线插件仅支持HTTPS".into());
        }
        let path = PathBuf::from(source);
        if !path.is_absolute()
            || path
                .extension()
                .and_then(|s| s.to_str())
                .map_or(true, |s| !s.eq_ignore_ascii_case("zip"))
        {
            return Err("请输入插件ZIP的完整路径或HTTPS下载地址".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|_| "无法打开插件ZIP")?
            .take(plugin_package::MAX_ZIP as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        bytes
    };
    plugin_package::read_package(&bytes)
}
pub async fn request_install(app: &AppHandle, source: &str) -> Result<Value, String> {
    let package = load_source(source).await?;
    store_mut(|s| check_capacity(s, &package.manifest))?;
    let review = Review {
        readme: package.readme.clone(),
        builtin: false, builtin_status: None, official_source:None,
        token: token(),
        action: "install".into(),
        id: package.manifest.id.clone(),
        manifest: package.manifest.clone(),
        source: source.into(),
        sha256: package.sha256.clone(),
    };
    show_review(
        app,
        Pending {
            review,
            package: Some(package),
        },
    )
}
pub fn request_builtin_change(app:&AppHandle,id:&str,action:&str)->Result<Value,String>{request_builtin_change_from(app,id,action,None)}
pub fn request_builtin_change_from(app:&AppHandle,id:&str,action:&str,source:Option<&str>)->Result<Value,String>{
    if let Some(source)=source{if !["install","enable"].contains(&action){return Err("本地包只用于添加或启用".into());}crate::official_distribution::read_local_archive(id,source)?;}

    crate::builtin_plugins::validate_action(id,action)?;
    crate::builtin_plugins::definition(id)?;
    let catalog=crate::official_distribution::metadata();
    let meta=catalog["plugins"].as_array().and_then(|items|items.iter().find(|item|item["id"]==id)).ok_or("官方插件信息缺失")?;
    show_review(app,Pending{review:Review{readme:crate::official_distribution::runtime_spec(id)?.get("readme").and_then(|r|r.as_str()).unwrap_or("").into(),builtin:true,official_source:source.map(str::to_owned),builtin_status:Some(crate::builtin_plugins::state_name(crate::builtin_plugins::status(id)).into()),
        token:token(),action:action.into(),id:id.into(),source:source.unwrap_or("https://gitee.com/wu-dinglin/shannon/releases").into(),sha256:crate::official_distribution::runtime_spec(id).ok().and_then(|s|s["sha256"].as_str().map(str::to_owned)).unwrap_or_default(),
        manifest:Manifest{api_version:1,id:id.into(),name:meta["name"].as_str().unwrap_or("").into(),version:meta["version"].as_str().unwrap_or("").into(),description:meta["description"].as_str().unwrap_or("").into(),author:meta["author"].as_str().unwrap_or("").into(),entry:"builtin".into(),permissions:vec![],tools:vec![]}},package:None})
}
fn manage_only(window:&tauri::WebviewWindow)->Result<(),String>{if ["main","plugin-manager"].contains(&window.label()){Ok(())}else{Err("仅插件管理和搜索框可管理插件".into())}}
pub async fn request_change(app: &AppHandle, id: &str, action: &str) -> Result<Value, String> {
    if !["enable", "disable", "uninstall"].contains(&action) {
        return Err("插件操作无效".into());
    }
    let item = store_mut(|s| s.installed.get(id).cloned().ok_or("插件不存在".into()))?;
    show_review(
        app,
        Pending {
            review: Review {
                readme: item.readme.clone(),
                builtin: false, builtin_status: None, official_source:None,
                token: token(),
                action: action.into(),
                id: id.into(),
                manifest: item.manifest,
                source: item.source,
                sha256: item.sha256,
            },
            package: None,
        },
    )
}
fn show_review(app: &AppHandle, pending: Pending) -> Result<Value, String> {
    let id = pending.review.id.clone();
    let action = pending.review.action.clone();
    {
        let mut lock = REVIEW.lock().map_err(|e| e.to_string())?;
        if lock.is_some() {
            return Err("请先处理当前插件确认窗口".into());
        }
        *lock = Some(pending);
    }
    let window =
        WebviewWindowBuilder::new(app, "plugin-review", WebviewUrl::App("plugins.html".into()))
            .title("闪念 · 插件确认")
            .inner_size(520.0, 580.0)
            .min_inner_size(460.0, 420.0)
            .center()
            .build();
    match window {
        Ok(w) => {
            w.on_window_event(|event| {
                if matches!(event, tauri::WindowEvent::Destroyed) {
                    if let Ok(mut lock) = REVIEW.lock() {
                        *lock = None;
                    }
                }
            });
            let _ = w.set_focus();
        }
        Err(e) => {
            *REVIEW.lock().map_err(|e| e.to_string())? = None;
            return Err(e.to_string());
        }
    }
    Ok(json!({"plugin":id,"action":action,"confirmation_opened":true,"executed":false}))
}
fn check_capacity(s: &Store, manifest: &Manifest) -> Result<(), String> {
    if !s.installed.contains_key(&manifest.id) && s.installed.len() >= 32 {
        return Err("最多安装32个插件".into());
    }
    if s.installed
        .values()
        .filter(|p| p.manifest.id != manifest.id)
        .map(|p| p.manifest.tools.len())
        .sum::<usize>()
        + manifest.tools.len()
        > 64
    {
        return Err("插件工具总数最多64个".into());
    }
    Ok(())
}
fn apply_review(pending: Pending) -> Result<Value, String> {
    store_mut(|s| apply_review_to(s, pending))
}
fn apply_review_to(s: &mut Store, pending: Pending) -> Result<Value, String> {
    if pending.review.builtin{return Err("内置插件须由内置配置执行器处理".into());}
    let mut next = s.installed.clone();
    let id = &pending.review.id;
    if pending.review.action == "install" {
        let package = pending.package.ok_or("插件包已失效")?;
        check_capacity(s, &package.manifest)?;
        let mut source = pending.review.source;
        if let Ok(mut url) = reqwest::Url::parse(&source) {
            if url.scheme() == "https" {
                url.set_query(None);
                url.set_fragment(None);
                source = url.to_string();
            }
        }
        let item = Installed {
            readme: package.readme.clone(),
            code_hash: plugin_package::digest(package.code.as_bytes()),
            manifest: package.manifest,
            sha256: package.sha256,
            source,
            enabled: true,
        };
        let path = code_path(s, &item);
        fs::create_dir_all(path.parent().ok_or("插件路径无效")?).map_err(|e| e.to_string())?;
        // Only a fixed main.js is persisted; no ZIP-supplied filesystem paths.
        let temp = path.with_extension("js.next");
        fs::write(&temp, package.code.as_bytes()).map_err(|e| e.to_string())?;
        fs::rename(temp, &path).map_err(|e| e.to_string())?;
        next.insert(id.clone(), item);
    } else {
        let item = next.get_mut(id).ok_or("插件不存在")?;
        if item.sha256 != pending.review.sha256 {
            return Err("插件已变化，请重新确认".into());
        }
        if pending.review.action == "uninstall" {
            next.remove(id);
        } else {
            item.enabled = pending.review.action == "enable";
        }
    }
    save(s, &next)?;
    s.installed = next;
    // Code and scoped data are retained for recovery; uninstall unregisters
    // all commands/tools immediately. No arbitrary recursive file deletion.
    Ok(json!({"plugin":id,"action":pending.review.action,"completed":true,"data_preserved":true}))
}

struct RunningGuard(String);
impl Drop for RunningGuard {
    fn drop(&mut self) {
        if let Ok(mut pending) = RUNNING.lock() {
            pending.remove(&self.0);
        }
    }
}
pub async fn execute(app: &AppHandle, name: &str, args: Value) -> Result<Value, String> {
    let (item, tool) = find(name)?;
    crate::plugin_schema::validate(&tool.parameters, &args)?;
    let code = store_mut(|s| {
        fs::read(code_path(s, &item)).map_err(|_| "插件文件缺失，请重新安装".into())
    })?;
    if code.len() > 512 * 1024 || plugin_package::digest(&code) != item.code_hash {
        return Err("插件文件已变化，请重新安装并确认".into());
    }
    let code = String::from_utf8(code).map_err(|_| "插件文件无效")?;
    let request = token();
    let (sender, receiver) = oneshot::channel();
    {
        let mut pending = RUNNING.lock().map_err(|e| e.to_string())?;
        if pending.len() >= 4 {
            return Err("插件正在忙，请稍后重试".into());
        }
        pending.insert(
            request.clone(),
            Invocation {
                plugin: item.manifest.id.clone(),
                sha256: item.sha256.clone(),
                permissions: tool.permissions,
                sender,
                calls: 0,
            },
        );
    }
    let _guard = RunningGuard(request.clone());
    app.emit_to("main","plugin-execute",json!({"request":request,"plugin":item.manifest.id,"tool":tool.name,"code":code,"args":args})).map_err(|e|e.to_string())?;
    let result = tokio::time::timeout(std::time::Duration::from_secs(12), receiver).await;
    let _ = app.emit_to("main", "plugin-cancel", json!({"request":request}));
    match result {
        Ok(Ok(Ok(data))) => {
            let (current, _) = find(name).map_err(|_| {
                "outcome_unknown:插件状态已变化，旧调用结果已丢弃；请先查询修改结果".to_string()
            })?;
            if current.sha256 != item.sha256 {
                return Err(
                    "outcome_unknown:插件版本已变化，旧调用结果已丢弃；请先查询修改结果".into(),
                );
            }
            Ok(data)
        }
        Ok(Ok(Err(error))) => Err(error),
        _ => Err("outcome_unknown:插件执行超时或已取消；修改可能已完成，请先查询状态再重试".into()),
    }
}

#[tauri::command]
pub fn plugin_list(window: tauri::WebviewWindow) -> Result<Value, String> {
    manage_only(&window)?;
    list()
}
#[tauri::command]
pub fn plugin_info(window: tauri::WebviewWindow, id: String) -> Result<Value, String> {
    manage_only(&window)?;
    info(&id)
}
#[tauri::command]
pub async fn plugin_install(
    app: AppHandle,
    window: tauri::WebviewWindow,
    source: String,
) -> Result<Value, String> {
    manage_only(&window)?;
    request_install(&app, &source).await
}
#[tauri::command]
pub async fn plugin_change(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    action: String,
) -> Result<Value, String> {
    manage_only(&window)?;
    request_change(&app, &id, &action).await
}
#[tauri::command]
pub async fn plugin_run(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    tool: String,
    args: Value,
) -> Result<Value, String> {
    main_only(&window)?;
    execute(&app, &plugin_package::tool_name(&id, &tool), args).await
}
#[tauri::command]
pub fn plugin_review(window: tauri::WebviewWindow) -> Result<Review, String> {
    if window.label() != "plugin-review" {
        return Err("仅确认窗口可读取安装申请".into());
    }
    REVIEW
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .map(|p| p.review.clone())
        .ok_or("确认已失效".into())
}
#[tauri::command]
pub async fn plugin_confirm(
    app: AppHandle,
    window: tauri::WebviewWindow,
    token: String,
    accepted: bool,
) -> Result<Value, String> {
    if window.label() != "plugin-review" {
        return Err("仅确认窗口可批准插件变更".into());
    }
    let pending = {
        let mut lock = REVIEW.lock().map_err(|e| e.to_string())?;
        if lock.as_ref().map(|p| p.review.token.as_str()) != Some(&token) {
            return Err("确认已失效".into());
        }
        lock.take().ok_or("确认已失效")?
    };
    let result = if accepted && pending.review.builtin {
        #[cfg(not(feature="native-runtime"))]
        let installed=if ["install","enable"].contains(&pending.review.action.as_str()){crate::official_distribution::ensure_from_source(&app,&pending.review.id,pending.review.official_source.as_deref()).await}else{crate::native_bridge::can_remove(&app,&pending.review.id).await};
        #[cfg(feature="native-runtime")] let installed=if pending.review.official_source.is_some(){crate::official_distribution::ensure_from_source(&app,&pending.review.id,pending.review.official_source.as_deref()).await}else{Ok(())};
        let result=installed.and_then(|_|crate::builtin_plugins::change(&app,&pending.review.id,&pending.review.action,pending.review.builtin_status.as_deref().unwrap_or("")));
        #[cfg(not(feature="native-runtime"))]
        if result.is_ok() && ["disable","uninstall"].contains(&pending.review.action.as_str()) {
            crate::native_bridge::stop(&app,&pending.review.id).await;
        }
        #[cfg(not(feature="native-runtime"))]
        if result.is_ok() && ["enable","install"].contains(&pending.review.action.as_str()) {
            crate::native_bridge::start_background(&app);
        }
        result
    } else if accepted {
        apply_review(pending)
    } else {
        Ok(json!({"cancelled":true}))
    };
    if let Ok(value) = &result {
        if let Some(id) = value["plugin"].as_str().filter(|_|value["builtin"]!=true) {
            cancel_plugin(&app, id);
        }
    }
    let _ = app.emit(
        "plugin-changed",
        match &result {
            Ok(value) => value.clone(),
            Err(error) => json!({"error":error}),
        },
    );
    let _ = window.destroy();
    result
}
#[tauri::command]
pub fn plugin_result(
    window: tauri::WebviewWindow,
    request: String,
    output: Value,
) -> Result<(), String> {
    main_only(&window)?;
    if output.to_string().len() > 65536 {
        return Err("插件结果超过64KiB".into());
    }
    let call = RUNNING
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&request)
        .ok_or("插件调用已失效")?;
    let result = if output["ok"] == true {
        Ok(output.get("data").cloned().unwrap_or(Value::Null))
    } else {
        Err(output["error"]
            .as_str()
            .unwrap_or("插件执行失败")
            .chars()
            .take(1000)
            .collect())
    };
    let _ = call.sender.send(result);
    Ok(())
}
#[tauri::command]
pub async fn plugin_host_call(
    app: AppHandle,
    window: tauri::WebviewWindow,
    request: String,
    method: String,
    args: Value,
) -> Result<Value, String> {
    main_only(&window)?;
    if args.to_string().len() > 32768 {
        return Err("插件宿主参数过大".into());
    }
    let (id, sha256, permissions) = {
        let mut pending = RUNNING.lock().map_err(|e| e.to_string())?;
        let call = pending.get_mut(&request).ok_or("插件调用已失效")?;
        call.calls += 1;
        if call.calls > 32 {
            return Err("单次插件宿主调用超过32次".into());
        }
        (
            call.plugin.clone(),
            call.sha256.clone(),
            call.permissions.clone(),
        )
    };
    store_mut(|s| {
        if s.installed
            .get(&id)
            .map_or(false, |p| p.enabled && p.sha256 == sha256)
        {
            Ok(())
        } else {
            Err("插件已停用或版本已变化".into())
        }
    })?;
    require_permission(&permissions, &method)?;
    if method.starts_with("storage_") {
        return storage(&id, &method, args);
    }
    let output = Box::pin(crate::ai_tools::dispatch(&app, &method, args)).await;
    if output["ok"] == true {
        Ok(output["data"].clone())
    } else {
        Err(output["error"]["message"]
            .as_str()
            .unwrap_or("宿主调用失败")
            .into())
    }
}
fn permission_for(method: &str) -> Option<&'static str> {
    Some(match method {
        "storage_get" => "storage.read",
        "storage_set" => "storage.write",
        "notes_list" => "notes.read",
        "notes_add" | "notes_set_done" => "notes.write",
        "notes_set_date" | "notes_reorder" => "notes.organize",
        "notes_delete" | "notes_delete_many" | "notes_restore_deleted" => "notes.delete",
        "notes_trash" => "notes.trash.read",
        "notes_show" | "notes_close" => "notes.display",
        "clock_status" => "clock.read",
        "clock_open" => "clock.display",
        _ => return None,
    })
}
fn require_permission(permissions: &[String], method: &str) -> Result<(), String> {
    let permission = permission_for(method).ok_or("插件不能调用此宿主能力")?;
    if !permissions.iter().any(|p| p == permission) {
        return Err(format!("插件工具未获授权：{permission}"));
    }
    Ok(())
}
fn cancel_plugin(app: &AppHandle, id: &str) {
    if let Ok(mut pending) = RUNNING.lock() {
        let keys: Vec<_> = pending
            .iter()
            .filter(|(_, call)| call.plugin == id)
            .map(|(key, _)| key.clone())
            .collect();
        for key in keys {
            if let Some(call) = pending.remove(&key) {
                let _ = call.sender.send(Err(
                    "outcome_unknown:插件状态已变化，调用已取消；已完成的修改不会回滚".into(),
                ));
                let _ = app.emit_to("main", "plugin-cancel", json!({"request":key}));
            }
        }
    }
}
fn storage(id: &str, method: &str, args: Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Get {
        key: String,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Set {
        key: String,
        value: Value,
    }
    let (key, value) = if method == "storage_get" {
        let a: Get = serde_json::from_value(args).map_err(|e| e.to_string())?;
        (a.key, None)
    } else {
        let a: Set = serde_json::from_value(args).map_err(|e| e.to_string())?;
        (a.key, Some(a.value))
    };
    if key.is_empty() || key.len() > 100 || key.chars().any(|c| c.is_control()) {
        return Err("存储键无效".into());
    }
    store_mut(|s| {
        let dir = s.root.join("data");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{id}.json"));
        let mut data: BTreeMap<String, Value> = if path.exists() {
            let raw = fs::read(&path).map_err(|e| e.to_string())?;
            if raw.len() > 262144 {
                return Err("插件存储超限".into());
            }
            serde_json::from_slice(&raw).map_err(|e| e.to_string())?
        } else {
            BTreeMap::new()
        };
        if let Some(value) = value {
            data.insert(key, value);
            let bytes = serde_json::to_vec(&data).map_err(|e| e.to_string())?;
            if bytes.len() > 262144 {
                return Err("每个插件最多存储256KiB".into());
            }
            let temp = dir.join(format!("{id}.next.json"));
            fs::write(&temp, bytes).map_err(|e| e.to_string())?;
            fs::rename(temp, path).map_err(|e| e.to_string())?;
            Ok(json!({"saved":true}))
        } else {
            Ok(data.get(&key).cloned().unwrap_or(Value::Null))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mention_scope_only_contains_selected_enabled_tools() {
        let mut manifest = plugin_package::sample_manifest();
        let mut second = manifest.tools[0].clone();
        second.name = "another".into();
        manifest.tools.push(second);
        let item = Installed {
            readme: String::new(),
            manifest,
            sha256: "a".repeat(64),
            code_hash: "b".repeat(64),
            source: "test.zip".into(),
            enabled: true,
        };
        let mut installed = BTreeMap::from([("sample".into(), item)]);
        let selected = select_from(
            &installed,
            &[PluginTarget {
                id: "sample".into(),
                builtin: false,
                tool: Some("hello".into()),
            }],
        )
        .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "plg__sample__hello");
        assert_eq!(
            select_from(
                &installed,
                &[PluginTarget {
                    id: "sample".into(),
                    builtin: false,
                    tool: None
                }]
            )
            .unwrap()
            .len(),
            2
        );
        assert!(select_from(
            &installed,
            &[PluginTarget {
                id: "sample".into(),
                builtin: false,
                tool: Some("missing".into())
            }]
        )
        .is_err());
        installed.get_mut("sample").unwrap().enabled = false;
        assert!(select_from(
            &installed,
            &[PluginTarget {
                id: "sample".into(),
                builtin: false,
                tool: None
            }]
        )
        .is_err());
    }
    #[test]
    fn only_scoped_recoverable_host_capabilities_are_available() {
        for name in [
            "shell",
            "invoke",
            "read_file",
            "start_voice_input",
            "component_confirm",
            "plugin_confirm",
            "fetch",
            "plugins_install",
        ] {
            assert!(permission_for(name).is_none());
        }
        assert_eq!(permission_for("notes_add"), Some("notes.write"));
        assert!(require_permission(&["notes.write".into()], "notes_delete").is_err());
        assert!(require_permission(&["notes.write".into()], "notes_reorder").is_err());
        assert!(require_permission(&["notes.delete".into()], "notes_delete").is_ok());
        assert!(require_permission(&["notes.organize".into()], "notes_set_date").is_ok());
        assert_eq!(permission_for("storage_get"), Some("storage.read"));
        assert!(require_permission(&["notes.read".into()], "notes_add").is_err());
        assert!(require_permission(&[], "notes_list").is_err());
        assert!(require_permission(&["notes.read".into()], "notes_list").is_ok());
        assert!(require_permission(&["notes.write".into()], "plugin_confirm").is_err());
    }
    #[test]
    fn install_update_disable_enable_uninstall_and_recovery_are_consistent() {
        let root = std::env::temp_dir().join(format!("shannon-plugin-test-{}", token()));
        fs::create_dir_all(&root).unwrap();
        let mut store = Store {
            root: root.clone(),
            installed: BTreeMap::new(),
        };
        let package = Package {
            readme: String::new(),
            manifest: plugin_package::sample_manifest(),
            code: "Shannon.register({hello:()=>42});".into(),
            sha256: "a".repeat(64),
        };
        let review = |action: &str, package: &Package| Review {
            readme: package.readme.clone(),
            builtin: false, builtin_status: None, official_source:None,
            token: token(),
            action: action.into(),
            id: package.manifest.id.clone(),
            manifest: package.manifest.clone(),
            source: "test.zip".into(),
            sha256: package.sha256.clone(),
        };
        apply_review_to(
            &mut store,
            Pending {
                review: review("install", &package),
                package: Some(package.clone()),
            },
        )
        .unwrap();
        assert!(store.installed["sample"].enabled);
        assert_eq!(
            fs::read_to_string(code_path(&store, &store.installed["sample"])).unwrap(),
            package.code
        );
        for action in ["disable", "enable"] {
            apply_review_to(
                &mut store,
                Pending {
                    review: review(action, &package),
                    package: None,
                },
            )
            .unwrap();
            assert_eq!(store.installed["sample"].enabled, action == "enable");
        }
        let mut update = package.clone();
        update.sha256 = "b".repeat(64);
        update.manifest.version = "1.1.0".into();
        update.code = "Shannon.register({hello:()=>43});".into();
        apply_review_to(
            &mut store,
            Pending {
                review: review("install", &update),
                package: Some(update.clone()),
            },
        )
        .unwrap();
        let saved: BTreeMap<String, Installed> =
            serde_json::from_slice(&fs::read(root.join("plugins.json")).unwrap()).unwrap();
        assert_eq!(saved["sample"].manifest.version, "1.1.0");
        assert!(apply_review_to(
            &mut store,
            Pending {
                review: review("disable", &package),
                package: None
            }
        )
        .is_err());
        apply_review_to(
            &mut store,
            Pending {
                review: review("uninstall", &update),
                package: None,
            },
        )
        .unwrap();
        assert!(store.installed.is_empty());
        assert!(root.join("packages/sample").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
