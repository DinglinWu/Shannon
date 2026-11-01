//! Pinned optional components: download, verify, stage, then atomically activate.
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Read, path::{Path, PathBuf}, sync::{atomic::{AtomicBool, Ordering}, Mutex}};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Spec { pub name: String, pub version: String, pub url: String, pub sha256: String, pub bytes: u64, pub unpacked_bytes: u64 }
#[derive(Deserialize)]
struct Catalog { schema: u32, components: Vec<Spec> }
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest { component: String, version: String, files: BTreeMap<String,String> }
static BUSY: AtomicBool = AtomicBool::new(false);
static JOBS: Mutex<BTreeMap<String,Value>> = Mutex::new(BTreeMap::new());
#[derive(Clone, Serialize)]
pub struct Confirmation { token: String, component: String, bytes: u64 }
static PENDING: Mutex<Option<Confirmation>> = Mutex::new(None);
struct Installing;
impl Drop for Installing { fn drop(&mut self) { BUSY.store(false,Ordering::SeqCst); } }

pub fn validate_name(name: &str) -> Result<(),String> {
    if ["voice","gesture"].contains(&name) { Ok(()) } else { Err("组件名称仅支持 voice 或 gesture".into()) }
}
fn catalog() -> Result<Vec<Spec>,String> {
    let catalog: Catalog = serde_json::from_str(include_str!("../component-catalog.json")).map_err(|e| e.to_string())?;
    if catalog.schema != 1 { return Err("组件目录版本不支持".into()); }
    Ok(catalog.components)
}
fn spec(name: &str) -> Result<Spec,String> {
    validate_name(name)?;
    let spec = catalog()?.into_iter().find(|s|s.name==name).ok_or("此版本尚未配置组件发布包，请联系开发者更新应用")?;
    if spec.version.split('.').any(|part|part.is_empty() || !part.bytes().all(|c|c.is_ascii_digit()))
        || spec.sha256.len()!=64 || !spec.sha256.bytes().all(|c|c.is_ascii_hexdigit())
        || !spec.url.starts_with("https://gitee.com/wu-dinglin/shannon/releases/download/")
        || spec.bytes==0 || spec.bytes>1024*1024*1024 || spec.unpacked_bytes>2*1024*1024*1024 {
        return Err("组件发布信息无效".into());
    }
    Ok(spec)
}
pub fn root(app: &AppHandle) -> Result<PathBuf,String> {
    app.path().app_local_data_dir().map(|p|p.join("components")).map_err(|e|e.to_string())
}
fn required(name: &str) -> &'static [&'static str] {
    if name=="voice" { &["shannon-voice.exe","assets/model.int8.onnx","assets/tokens.txt","assets/silero_vad.onnx"] }
    else { &["python/python.exe","control.py","app.py","models/hand_landmarker.task"] }
}
pub fn installed_dir(app: &AppHandle, name: &str) -> Result<PathBuf,String> {
    let spec = spec(name)?;
    let target = root(app)?.join(name).join(&spec.version);
    let ready = fs::read_to_string(target.join("ready.json")).ok().and_then(|s|serde_json::from_str::<Value>(&s).ok());
    if ready.as_ref().and_then(|s|s["sha256"].as_str()) != Some(spec.sha256.as_str()) || required(name).iter().any(|f|!target.join(f).is_file()) {
        return Err(format!("尚未安装{}组件，请输入 /component install {name}", if name=="voice" {"语音"} else {"手势"}));
    }
    Ok(target)
}
pub fn status(app: &AppHandle) -> Result<Value,String> {
    let jobs = JOBS.lock().map_err(|e|e.to_string())?;
    let items: Vec<Value> = ["voice","gesture"].iter().map(|name| {
        let spec = spec(name).ok();
        json!({"name":name,"installed":installed_dir(app,name).is_ok(),"version":spec.as_ref().map(|s|&s.version),
            "download_bytes":spec.as_ref().map(|s|s.bytes),"job":jobs.get(*name)})
    }).collect();
    Ok(json!({"components":items,"installing":BUSY.load(Ordering::SeqCst)}))
}
fn progress(app: &AppHandle, name: &str, phase: &str, received: u64, total: u64, error: Option<String>) {
    let value = json!({"name":name,"phase":phase,"received":received,"total":total,"error":error});
    if let Ok(mut jobs) = JOBS.lock() { jobs.insert(name.to_owned(),value.clone()); }
    let _ = app.emit_to("main","component-progress",value);
}
fn digest(path: &Path) -> Result<String,String> {
    let mut file = fs::File::open(path).map_err(|e|e.to_string())?;
    let mut hash = Sha256::new(); let mut buffer = [0u8;65536];
    loop { let n = file.read(&mut buffer).map_err(|e|e.to_string())?; if n==0 {break;} hash.update(&buffer[..n]); }
    Ok(format!("{:x}",hash.finalize()))
}
fn safe_path(name: &str) -> Result<PathBuf,String> {
    if name.is_empty() || name.contains('\\') || name.starts_with('/') { return Err("组件包含非法路径".into()); }
    let trimmed = name.trim_end_matches('/');
    for part in trimmed.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.is_empty() || part=="." || part==".." || part.contains(':') || part.ends_with(['.',' '])
            || matches!(stem.as_str(),"CON"|"PRN"|"AUX"|"NUL")
            || (stem.len()==4 && (stem.starts_with("COM")||stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit()) {
            return Err("组件包含非法路径".into());
        }
    }
    Ok(PathBuf::from(trimmed))
}
fn extract(archive: &Path, stage: &Path, spec: &Spec) -> Result<(),String> {
    if digest(archive)? != spec.sha256 { return Err("组件校验失败，未执行安装".into()); }
    let mut zip = zip::ZipArchive::new(fs::File::open(archive).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if zip.len()>30000 { return Err("组件文件数量异常".into()); }
    fs::create_dir_all(stage).map_err(|e|e.to_string())?;
    let mut files = BTreeMap::new(); let mut total=0u64;
    for index in 0..zip.len() {
        let entry = zip.by_index(index).map_err(|e|e.to_string())?;
        let relative = safe_path(entry.name())?;
        if entry.unix_mode().map(|m|m & 0o170000 == 0o120000).unwrap_or(false) {return Err("组件不允许符号链接".into());}
        let path = stage.join(&relative);
        if entry.is_dir() { fs::create_dir_all(path).map_err(|e|e.to_string())?; continue; }
        total=total.checked_add(entry.size()).ok_or("组件尺寸异常")?;
        if total>spec.unpacked_bytes || total>2*1024*1024*1024 {return Err("组件解压尺寸超出声明".into());}
        if files.insert(entry.name().to_owned(),String::new()).is_some() {return Err("组件包含重复文件".into());}
        fs::create_dir_all(path.parent().ok_or("组件路径无效")?).map_err(|e|e.to_string())?;
        let mut target=fs::File::create(path).map_err(|e|e.to_string())?;
        let size=entry.size();
        let written=std::io::copy(&mut entry.take(size+1),&mut target).map_err(|e|e.to_string())?;
        if written!=size {return Err("组件文件解压尺寸不符".into());}
    }
    let manifest_path=stage.join("component.json");
    if fs::metadata(&manifest_path).map_err(|e|e.to_string())?.len()>8*1024*1024 {return Err("组件文件清单过大".into());}
    let manifest: Manifest=serde_json::from_slice(&fs::read(manifest_path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if manifest.component!=spec.name || manifest.version!=spec.version || manifest.files.len()+1!=files.len() {return Err("组件身份或文件清单不符".into());}
    for (name,hash) in &manifest.files {
        if !files.contains_key(name) || digest(&stage.join(safe_path(name)?))? != *hash {return Err(format!("组件文件校验失败：{name}"));}
    }
    if required(&spec.name).iter().any(|f|!manifest.files.contains_key(*f)) {return Err("组件缺少必要文件".into());}
    Ok(())
}

async fn install(app: &AppHandle, spec: &Spec) -> Result<(),String> {
    let base=root(app)?.join(&spec.name);
    tokio::fs::create_dir_all(&base).await.map_err(|e|e.to_string())?;
    let archive=base.join("download.zip.part");
    let stage=base.join("install.stage");
    if stage.exists() {fs::remove_dir_all(&stage).map_err(|e|e.to_string())?;}
    progress(app,&spec.name,"downloading",0,spec.bytes,None);
    #[cfg(debug_assertions)]
    let local={
        let path=Path::new(env!("CARGO_MANIFEST_DIR")).join("../release/components").join(format!("{}-{}-windows-x64.zip",spec.name,spec.version));
        if path.is_file() {Some(path)} else {None}
    };
    #[cfg(not(debug_assertions))]
    let local: Option<PathBuf>=None;
    if let Some(path)=local {
        tokio::fs::copy(path,&archive).await.map_err(|e|e.to_string())?;
    } else {
        use tokio::io::AsyncWriteExt;
        let client=reqwest::Client::builder().connect_timeout(std::time::Duration::from_secs(20)).timeout(std::time::Duration::from_secs(1800)).build().map_err(|e|e.to_string())?;
        let response=client.get(&spec.url).send().await.map_err(|e|format!("组件下载失败：{e}"))?.error_for_status().map_err(|e|format!("组件发行包暂不可下载：{e}"))?;
        let mut stream=response.bytes_stream(); let mut file=tokio::fs::File::create(&archive).await.map_err(|e|e.to_string())?;
        let mut received=0u64; let mut reported=0u64;
        while let Some(chunk)=stream.next().await {
            let chunk=chunk.map_err(|e|e.to_string())?; received+=chunk.len() as u64;
            if received>spec.bytes {return Err("组件下载尺寸超出声明".into());}
            file.write_all(&chunk).await.map_err(|e|e.to_string())?;
            if received-reported>=1024*1024 {progress(app,&spec.name,"downloading",received,spec.bytes,None); reported=received;}
        }
        file.flush().await.map_err(|e|e.to_string())?;
        if received!=spec.bytes {return Err("组件下载不完整，请重新安装".into());}
    }
    progress(app,&spec.name,"verifying",spec.bytes,spec.bytes,None);
    let (zip_path,stage_path,description)=(archive.clone(),stage.clone(),spec.clone());
    let unpack=tauri::async_runtime::spawn_blocking(move||extract(&zip_path,&stage_path,&description)).await.map_err(|e|e.to_string())?;
    if let Err(error)=unpack {let _=fs::remove_dir_all(&stage);return Err(error);}
    fs::write(stage.join("ready.json"),json!({"sha256":spec.sha256}).to_string()).map_err(|e|e.to_string())?;
    let target=base.join(&spec.version);
    if target.exists() {
        let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        fs::rename(&target,base.join(format!("previous-{stamp}"))).map_err(|e|e.to_string())?;
    }
    fs::rename(stage,target).map_err(|e|e.to_string())?;
    let _=tokio::fs::remove_file(archive).await;
    Ok(())
}
pub fn start_install(app: &AppHandle, name: &str) -> Result<Value,String> {
    let spec=spec(name)?;
    if installed_dir(app,name).is_ok() {return Ok(json!({"component":name,"installed":true,"started":false}));}
    if BUSY.swap(true,Ordering::SeqCst) {return Err("已有组件正在安装，请稍候".into());}
    progress(app,name,"queued",0,spec.bytes,None);
    let app=app.clone();
    tauri::async_runtime::spawn(async move {
        let guard=Installing;
        let result=install(&app,&spec).await;
        drop(guard);
        match result {
            Ok(())=>progress(&app,&spec.name,"installed",spec.bytes,spec.bytes,None),
            Err(error)=>progress(&app,&spec.name,"error",0,spec.bytes,Some(error)),
        }
    });
    Ok(json!({"component":name,"started":true,"installed":false}))
}
pub fn request_install(app: &AppHandle, name: &str) -> Result<Value,String> {
    let spec=spec(name)?;
    if installed_dir(app,name).is_ok() {return Ok(json!({"component":name,"installed":true,"confirmation_opened":false}));}
    let mut pending=PENDING.lock().map_err(|e|e.to_string())?;
    if let Some(current)=pending.as_ref() {
        if current.component!=name {return Err("请先处理当前组件安装确认".into());}
    } else {
        *pending=Some(Confirmation {token:format!("{:?}",std::time::SystemTime::now()),component:name.into(),bytes:spec.bytes});
    }
    if let Some(window)=app.get_webview_window("component-install") {let _=window.show();let _=window.set_focus();}
    else {
        match WebviewWindowBuilder::new(app,"component-install",WebviewUrl::App("components.html".into()))
            .title("闪念 · 安装可选组件").inner_size(430.0,300.0).resizable(false).center().build() {
            Ok(window)=>window.on_window_event(|event| {if matches!(event,tauri::WindowEvent::Destroyed) {if let Ok(mut pending)=PENDING.try_lock() {*pending=None;}}}),
            Err(error)=>{*pending=None;return Err(error.to_string());}
        }
    }
    Ok(json!({"component":name,"confirmation_opened":true,"installed":false}))
}
#[tauri::command]
pub fn component_status(app: AppHandle) -> Result<Value,String> {status(&app)}
#[tauri::command]
pub fn component_install(app: AppHandle, window: tauri::WebviewWindow, name: String) -> Result<Value,String> {
    if window.label()!="main" {return Err("仅搜索框可以发起组件安装".into());}
    start_install(&app,&name)
}
#[tauri::command]
pub fn component_confirmation(window: tauri::WebviewWindow) -> Result<Confirmation,String> {
    if window.label()!="component-install" {return Err("仅安装确认窗口可读取确认请求".into());}
    PENDING.lock().map_err(|e|e.to_string())?.clone().ok_or("安装请求已失效".into())
}
#[tauri::command]
pub fn component_confirm(app: AppHandle, window: tauri::WebviewWindow, token: String, accepted: bool) -> Result<Value,String> {
    if window.label()!="component-install" {return Err("安装必须由应用确认窗口批准".into());}
    let mut pending=PENDING.lock().map_err(|e|e.to_string())?;
    let request=pending.as_ref().filter(|r|r.token==token).ok_or("安装确认已失效")?;
    let result=if accepted {start_install(&app,&request.component)?} else {json!({"cancelled":true})};
    *pending=None;drop(pending);let _=window.destroy();Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn rejects_unsafe_windows_paths_and_component_names() {
        for name in ["../escape","/absolute","C:/evil","ok/../evil","a\\b","a:stream","NUL.txt","dir/COM1","a.","a "] {assert!(safe_path(name).is_err(),"{name}");}
        assert_eq!(safe_path("python/Lib/site-packages/a.py").unwrap(),PathBuf::from("python/Lib/site-packages/a.py"));
        assert!(validate_name("shell").is_err());assert!(validate_name("voice").is_ok());
    }
    #[test] fn bad_archive_hash_is_rejected_before_extraction() {
        let dir=std::env::temp_dir().join(format!("shannon-hash-test-{}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();let archive=dir.join("bad.zip");fs::write(&archive,b"tampered").unwrap();
        let spec=Spec{name:"voice".into(),version:"1".into(),url:String::new(),sha256:"0".repeat(64),bytes:8,unpacked_bytes:16};
        assert!(extract(&archive,&dir.join("stage"),&spec).unwrap_err().contains("校验失败"));
        assert!(!dir.join("stage").exists());fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    #[ignore="Validate generated component ZIP files, no camera or microphone"]
    fn validates_prepared_components() {
        let base=Path::new(env!("CARGO_MANIFEST_DIR")).join("target/component-validation/中文路径");
        for spec in catalog().unwrap() {
            let zip=Path::new(env!("CARGO_MANIFEST_DIR")).join("../release/components").join(format!("{}-{}-windows-x64.zip",spec.name,spec.version));
            let stage=base.join(&spec.name);
            if stage.exists() {fs::remove_dir_all(&stage).unwrap();}
            extract(&zip,&stage,&spec).unwrap();
            assert!(required(&spec.name).iter().all(|f|stage.join(f).is_file()));
        }
    }
}
