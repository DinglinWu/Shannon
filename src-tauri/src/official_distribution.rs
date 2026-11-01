use serde_json::{json, Value};
use std::{fs, io::Read, path::PathBuf};
use tauri::{AppHandle, Manager};
pub fn metadata() -> Value {
    serde_json::from_str(include_str!("../official-plugins.json"))
        .expect("checked official catalog")
}
pub fn runtime_spec(id: &str) -> Result<Value, String> {
    crate::builtin_plugins::definition(id)?;
    let spec = metadata()["plugins"].as_array().and_then(|p| p.iter().find(|p| p["id"] == id)).map(|p| p["runtime"].clone()).unwrap_or(Value::Null);
    if spec.is_null() {
        return Err("官方插件分发包尚未构建，请先生成并上传发行附件".into());
    }
    if spec["id"]!=id || spec["protocol"]!=1 {return Err("官方插件身份或协议不匹配".into());}
    if !spec["url"]
        .as_str()
        .is_some_and(crate::release_sources::official_url)
        || spec["sha256"].as_str().is_none_or(|s| s.len() != 64)
        || spec["executable_sha256"]
            .as_str()
            .is_none_or(|s| s.len() != 64)
    {
        return Err("官方分发目录无效".into());
    }
    Ok(spec)
}
pub fn runtime_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    let spec = runtime_spec(id)?;
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("official-plugins").join(id)
        .join(spec["sha256"].as_str().unwrap())
        .join("runtime.exe"))
}
pub fn verified_runtime(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    let p = runtime_path(app, id)?;
    let data = fs::read(&p).map_err(|_| "尚未下载官方插件，请在插件管理中添加")?;
    if crate::plugin_package::digest(&data)
        != runtime_spec(id)?["executable_sha256"].as_str().unwrap()
    {
        return Err("官方插件文件已变化，请重新添加插件".into());
    }
    Ok(p)
}
pub fn read_local_archive(id:&str,source:&str)->Result<Vec<u8>,String>{read_local_with_spec(source,&runtime_spec(id)?)}
fn read_local_with_spec(source:&str,spec:&Value)->Result<Vec<u8>,String>{
    let path=std::path::Path::new(source);
    if source.chars().count()>2048||source.contains('\0')||!path.is_absolute()||!path.extension().is_some_and(|e|e.eq_ignore_ascii_case("zip")){return Err("请输入官方ZIP包的完整本地路径".into());}
    let expected=spec["bytes"].as_u64().filter(|n|*n>0&&*n<=256*1024*1024).ok_or("官方插件大小声明无效")?;
    let mut bytes=Vec::new();fs::File::open(path).map_err(|_|"无法打开本地官方插件包")?.take(expected+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len()as u64!=expected||crate::plugin_package::digest(&bytes)!=spec["sha256"].as_str().ok_or("缺少插件包校验信息")?{return Err("本地官方包与当前版本不匹配，请使用当前插件对应版本的官方ZIP".into());}
    decode_archive(&bytes,spec)?;
    Ok(bytes)
}
pub async fn ensure_from_source(app:&AppHandle,id:&str,source:Option<&str>)->Result<(),String>{
    if let Some(source)=source{let bytes=read_local_archive(id,source)?;install_archive(app,id,&bytes,&runtime_spec(id)?)}else{ensure_installed(app,id).await}
}
pub async fn ensure_installed(app: &AppHandle, id: &str) -> Result<(), String> {
    let ui_valid=id!="voice" || fs::read(runtime_path(app,id)?.parent().unwrap().join("voice-ui.js"))
        .map(|data| crate::plugin_package::digest(&data)==runtime_spec(id).ok().and_then(|s|s["voice_ui_sha256"].as_str().map(str::to_owned)).unwrap_or_default()).unwrap_or(false);
    if verified_runtime(app,id).is_ok() && ui_valid {
        return Ok(());
    }
    let spec = runtime_spec(id)?;
    let expected = spec["bytes"].as_u64().ok_or("缺少下载大小")?;
    if expected == 0 || expected > 256 * 1024 * 1024 {
        return Err("插件包大小无效".into());
    }
    let cfg = crate::load_config(app);
    let mut builder = reqwest::Client::builder().https_only(true)
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(180));
    if !cfg.ai.use_system_proxy {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|e| e.to_string())?;
    let mut errors = Vec::new();
    for url in crate::release_sources::download_urls(spec["url"].as_str().unwrap()) {
        match download_archive(&client, &url, expected, &spec).await {
            Ok(bytes) => return install_archive(app, id, &bytes, &spec),
            Err(error) => errors.push(error),
        }
    }
    Err(format!("官方插件所有下载来源均失败：{}", errors.join("；")))
}
async fn download_archive(client: &reqwest::Client, url: &str, expected: u64, spec: &Value) -> Result<Vec<u8>, String> {
    let mut response = client.get(url).send().await.map_err(|e| format!("插件下载失败：{e}"))?
        .error_for_status().map_err(|e| format!("插件附件不可用：{e}"))?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "插件下载中断")? {
        if bytes.len() + chunk.len() > expected as usize {
            return Err("插件包超过目录声明的大小".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() as u64 != expected
        || crate::plugin_package::digest(&bytes) != spec["sha256"].as_str().unwrap()
    {
        return Err("插件包大小或SHA-256校验失败".into());
    }
    decode_archive(&bytes, spec)?;
    Ok(bytes)
}
fn install_archive(app: &AppHandle, id: &str, bytes: &[u8], spec: &Value) -> Result<(), String> {
    let output = decode_archive(bytes, spec)?;
    let base = runtime_path(app, id)?.parent().unwrap().to_path_buf();
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    for (name, data) in output {
        let p = base.join(name);
        let tmp = p.with_extension("next");
        fs::write(&tmp, data).map_err(|e| e.to_string())?;
        fs::rename(tmp, p).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn decode_archive(bytes: &[u8], spec: &Value) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let identified = spec["id"].as_str();
    let expected = if identified.is_some() { if spec.get("voice_ui_sha256").is_some() {4} else {3} } else {2};
    if zip.len() != expected {
        return Err("官方运行包内容不完整".into());
    }
    let mut output = Vec::new();
    let mut identity_seen=false;
    for i in 0..zip.len() {
        let file = zip.by_index(i).map_err(|e| e.to_string())?;
        if file.name()=="README.md" {
            if identified.is_none() || output.iter().any(|(name,_)| *name=="README.md") || file.size()>crate::plugin_readme::MAX_README as u64 {return Err("官方README.md无效".into());}
            let mut data=Vec::new();file.take(crate::plugin_readme::MAX_README as u64+1).read_to_end(&mut data).map_err(|e|e.to_string())?;
            let text=std::str::from_utf8(&data).map_err(|_|"README.md必须为UTF-8")?;
            if spec["readme"].as_str()!=Some(text) || spec["readme_sha256"].as_str()!=Some(crate::plugin_package::digest(&data).as_str()) {return Err("官方README.md校验失败".into());}
            let m=&spec["readme_metadata"];
            crate::plugin_readme::validate(text,m["id"].as_str().unwrap_or(""),m["name"].as_str().unwrap_or(""),m["author"].as_str().unwrap_or(""),m["version"].as_str().unwrap_or(""),m["description"].as_str().unwrap_or(""))?;
            output.push(("README.md",data));continue;
        }
        if file.name()=="official-plugin.json" {
            if identified.is_none() || identity_seen || file.size()>4096 {return Err("插件身份清单无效".into());}
            let mut data=Vec::new();file.take(4097).read_to_end(&mut data).map_err(|e|e.to_string())?;
            let manifest:Value=serde_json::from_slice(&data).map_err(|_|"插件身份清单无效")?;
            if manifest["id"]!=spec["id"] || manifest["version"]!=spec["version"] || manifest["protocol"]!=1 {return Err("插件身份或协议不匹配".into());}
            identity_seen=true;continue;
        }
        let (name, hash, limit) = match file.name() {
            "runtime.exe" => ("runtime.exe", "executable_sha256", 256 * 1024 * 1024),
            "voice-ui.js" => ("voice-ui.js", "voice_ui_sha256", 1024 * 1024),
            _ => return Err("官方运行包包含未知文件".into()),
        };
        if output.iter().any(|(n, _)| *n == name) || file.size() > limit {
            return Err("官方运行包文件无效".into());
        }
        let mut data = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if crate::plugin_package::digest(&data)
            != spec[hash].as_str().ok_or("缺少执行文件校验信息")?
        {
            return Err("官方插件执行文件校验失败".into());
        }
        output.push((name, data));
    }
    if identified.is_some() && (!identity_seen || !output.iter().any(|(n,_)|*n=="README.md")) {return Err("缺少插件身份清单".into());}
    Ok(output)
}
#[tauri::command]
pub fn plugin_runtime_mode() -> Value {
    json!({"native":cfg!(feature="native-runtime")})
}

#[tauri::command]
pub fn plugin_ui_source(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: String,
) -> Result<Value, String> {
    if window.label() != "main" || id != "voice" {
        return Err("仅支持已安装的官方输入扩展".into());
    }
    crate::builtin_plugins::require("voice")?;
    let p = runtime_path(&app, &id)?.parent().unwrap().join("voice-ui.js");
    let data = fs::read(p).map_err(|_| "尚未下载语音界面扩展")?;
    if data.len() > 1024 * 1024
        || crate::plugin_package::digest(&data)
            != runtime_spec(&id)?["voice_ui_sha256"]
                .as_str()
                .ok_or("缺少校验信息")?
    {
        return Err("语音扩展校验失败".into());
    }
    Ok(json!({"source":String::from_utf8(data).map_err(|e|e.to_string())?}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn official_zip_requires_the_published_readme() {
        let readme="---\nid: note\nname: 便签\nauthor: Dinglin\nversion: 1.0.0\ndescription: 功能\n---\n# 功能\n添加便签";
        let spec=json!({"id":"note","version":"1.0.0","executable_sha256":crate::plugin_package::digest(b"exe"),"readme":readme,"readme_sha256":crate::plugin_package::digest(readme.as_bytes()),"readme_metadata":{"id":"note","name":"便签","author":"Dinglin","version":"1.0.0","description":"功能"}});
        let make=|text:Option<&str>| {
            let mut zip=zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (name,data) in [("runtime.exe",b"exe".to_vec()),("official-plugin.json",serde_json::to_vec(&json!({"id":"note","version":"1.0.0","protocol":1})).unwrap())] {
                zip.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();zip.write_all(&data).unwrap();
            }
            if let Some(text)=text {zip.start_file("README.md",zip::write::SimpleFileOptions::default()).unwrap();zip.write_all(text.as_bytes()).unwrap();}
            zip.finish().unwrap().into_inner()
        };
        assert!(decode_archive(&make(Some(readme)),&spec).is_ok());
        assert!(decode_archive(&make(None),&spec).is_err());
        assert!(decode_archive(&make(Some("伪造说明")),&spec).is_err());
    }
    #[test]
    fn every_official_plugin_has_accurate_publication_metadata() {
        let data = metadata();
        let plugins = data["plugins"].as_array().unwrap();
        assert_eq!(plugins.len(), 8);
        for p in plugins {
            assert!(!p["author"].as_str().unwrap().is_empty());
            assert!(!p["version"].as_str().unwrap().is_empty());
            assert!(!p["description"].as_str().unwrap().is_empty());
            assert!(!p["features"].as_array().unwrap().is_empty());
            assert_eq!(p["distribution"], "gitee");
        }
    }
    fn archive(name: &str) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        z.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(b"exe").unwrap();
        z.start_file("voice-ui.js", zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(b"ui").unwrap();
        z.finish().unwrap().into_inner()
    }
    #[test]
    fn offline_import_requires_exact_published_archive_and_absolute_zip_path(){
        let bytes=archive("runtime.exe");let spec=json!({"bytes":bytes.len(),"sha256":crate::plugin_package::digest(&bytes),"executable_sha256":crate::plugin_package::digest(b"exe"),"voice_ui_sha256":crate::plugin_package::digest(b"ui")});
        let mut random=[0u8;12];getrandom::fill(&mut random).unwrap();let key:String=random.iter().map(|b|format!("{b:02x}")).collect();
        let file=std::env::temp_dir().join(format!("official-offline-{key}.zip"));fs::write(&file,&bytes).unwrap();
        assert_eq!(read_local_with_spec(file.to_str().unwrap(),&spec).unwrap(),bytes);
        assert!(read_local_with_spec("relative.zip",&spec).is_err());
        let mut wrong=spec.clone();wrong["sha256"]=json!("0".repeat(64));assert!(read_local_with_spec(file.to_str().unwrap(),&wrong).is_err());
        fs::write(&file,b"tampered").unwrap();assert!(read_local_with_spec(file.to_str().unwrap(),&spec).is_err());fs::remove_file(file).unwrap();
    }
    #[test]
    fn runtime_packages_reject_paths_hash_changes_and_extra_files() {
        let spec = json!({"executable_sha256":crate::plugin_package::digest(b"exe"),"voice_ui_sha256":crate::plugin_package::digest(b"ui")});
        assert_eq!(
            decode_archive(&archive("runtime.exe"), &spec)
                .unwrap()
                .len(),
            2
        );
        assert!(decode_archive(&archive("../runtime.exe"), &spec).is_err());
        let mut bad = spec.clone();
        bad["executable_sha256"] = json!("a".repeat(64));
        assert!(decode_archive(&archive("runtime.exe"), &bad).is_err());
    }
}
