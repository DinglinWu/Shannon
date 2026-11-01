// 在线主题：从 Gitee 仓库的 themes/ 目录同步到本地缓存 %APPDATA%\com.brynn.ailauncher\online-themes\。
//
// 仓库中放置两个文件：
//   index.json      主题清单，列出每个主题的每个文件及其大小和 SHA-256
//   index.json.sig  清单的签名，使用发布安装包的同一把私钥生成
// 同步时先用内置公钥（tauri.conf.json 的 plugins.updater.pubkey）验证清单签名，再逐个核对本地缓存，
// 只下载缺失或内容有变化的文件；每个文件下载后核对 SHA-256，一个主题的文件全部通过后才替换旧版本。
// 清单中已移除的主题同时从缓存删除。
//
// 签名保证清单出自发布者，清单中的哈希保证每个文件未被改动，因此仓库或传输过程被篡改时，
// 篡改的内容不会进入缓存。主题本身仅含数据（JSON + 图片），见 THEME_SPEC.md。
// 清单由 scripts/themes.py 生成并签名，发布步骤见 RELEASE.md。

use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::theme;

pub const BASE_URL: &str = "https://gitee.com/wu-dinglin/shannon/raw/master/themes/";
const INDEX_FILE: &str = "index.json";
const SPEC_VERSION: u32 = 1;

const MAX_INDEX_BYTES: u64 = 256 * 1024;
const MAX_SIG_BYTES: u64 = 4 * 1024;
const MAX_THEMES: usize = 32;
const MAX_FILES: usize = 16;
const MAX_THEME_BYTES: u64 = 8 * 1024 * 1024;

static SYNCING: AtomicBool = AtomicBool::new(false);

// ---------- 清单 ----------

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Index {
    pub spec: u32,
    // 生成时间（Unix 秒）。比本地缓存的清单旧时拒绝，防止旧清单被重放
    pub generated: u64,
    pub themes: Vec<Entry>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub files: BTreeMap<String, FileEntry>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct FileEntry {
    pub size: u64,
    pub sha256: String,
}

#[derive(Serialize, Debug, Default)]
pub struct Report {
    // 缓存是否有变化（新增、更新或删除了主题）
    pub changed: bool,
    // 清单中的主题数
    pub count: usize,
    // 未能同步的主题及原因；下次同步时重试
    pub failed: Vec<String>,
}

pub fn dir(config_dir: &Path) -> PathBuf {
    let d = config_dir.join("online-themes");
    fs::create_dir_all(&d).ok();
    d
}

// 替换主题时先写到这里，全部写好再整体换过去；不放在缓存目录内，以免被当成主题列出
fn staging_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("online-themes.staging")
}

fn valid_name(name: &str) -> bool {
    name == "theme.json" || theme::valid_file(name)
}

fn valid_sha(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{:02x}", b)).collect()
}

// 验证清单签名。公钥与签名均为 tauri signer 输出的 base64 文本
pub fn verify(data: &[u8], sig_b64: &str, pubkey_b64: &str) -> Result<(), String> {
    let decode = |s: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(s.trim())
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
    };
    let pk = decode(pubkey_b64)
        .and_then(|s| PublicKey::decode(&s).ok())
        .ok_or("内置公钥无效")?;
    let sig = decode(sig_b64)
        .and_then(|s| Signature::decode(&s).ok())
        .ok_or("在线主题清单的签名格式有误")?;
    pk.verify(data, &sig, true)
        .map_err(|_| "在线主题清单签名校验未通过，已忽略本次同步".to_string())
}

// 解析并校验清单。任何一项不合规都拒绝整份清单：清单由发布脚本生成，出错说明来源有问题
pub fn parse(data: &[u8]) -> Result<Index, String> {
    let text = std::str::from_utf8(data).map_err(|_| "在线主题清单不是有效的 UTF-8 文本")?;
    let index: Index = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("在线主题清单格式有误：{}", e))?;
    if index.spec != SPEC_VERSION {
        return Err("在线主题清单的版本不受支持，请更新闪念".into());
    }
    if index.themes.len() > MAX_THEMES {
        return Err(format!("在线主题数量超过上限 {}", MAX_THEMES));
    }
    let mut seen = BTreeSet::new();
    for t in &index.themes {
        if !theme::valid_id(&t.id) || !seen.insert(t.id.as_str()) {
            return Err(format!("在线主题 ID 无效或重复：{}", t.id));
        }
        if !t.files.contains_key("theme.json") {
            return Err(format!("在线主题 {} 缺少 theme.json", t.id));
        }
        if t.files.len() > MAX_FILES {
            return Err(format!("在线主题 {} 的文件数超过上限 {}", t.id, MAX_FILES));
        }
        let mut total = 0;
        for (name, f) in &t.files {
            let limit = if name == "theme.json" { theme::MAX_MANIFEST_BYTES } else { theme::MAX_ASSET_BYTES };
            if !valid_name(name) || !valid_sha(&f.sha256) || f.size > limit {
                return Err(format!("在线主题 {} 中的文件 {} 无效", t.id, name));
            }
            total += f.size;
        }
        if total > MAX_THEME_BYTES {
            return Err(format!("在线主题 {} 的总大小超过上限", t.id));
        }
    }
    Ok(index)
}

// 本地缓存中上一份清单的生成时间
fn cached_generated(root: &Path) -> Option<u64> {
    let data = fs::read(root.join(INDEX_FILE)).ok()?;
    parse(&data).ok().map(|i| i.generated)
}

// ---------- 核对与写入（不涉及网络，便于测试） ----------

// 核对本地缓存中的一个主题。返回 None 表示与清单一致；
// 返回 Some(需下载的文件) 表示需要重建（文件缺失、内容不同，或存在清单外的多余文件）
pub fn plan(root: &Path, entry: &Entry) -> Option<Vec<String>> {
    let tdir = root.join(&entry.id);
    let mut need = Vec::new();
    for (name, f) in &entry.files {
        let ok = fs::read(tdir.join(name)).map(|d| sha256_hex(&d) == f.sha256).unwrap_or(false);
        if !ok {
            need.push(name.clone());
        }
    }
    let extra = fs::read_dir(&tdir)
        .map(|rd| {
            rd.flatten()
                .any(|e| !entry.files.contains_key(e.file_name().to_string_lossy().as_ref()))
        })
        .unwrap_or(false);
    if need.is_empty() && !extra {
        None
    } else {
        Some(need)
    }
}

// 用下载到的文件（已核对哈希）和缓存中未变化的文件重建一个主题，整体替换旧版本
pub fn commit(
    config_dir: &Path,
    entry: &Entry,
    mut downloaded: BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let root = dir(config_dir);
    let old = root.join(&entry.id);
    let staging = staging_dir(config_dir).join(&entry.id);
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    for (name, f) in &entry.files {
        let data = match downloaded.remove(name) {
            Some(d) => d,
            None => fs::read(old.join(name)).map_err(|_| format!("缺少文件 {}", name))?,
        };
        if sha256_hex(&data) != f.sha256 {
            return Err(format!("文件 {} 校验未通过", name));
        }
        fs::write(staging.join(name), data).map_err(|e| e.to_string())?;
    }
    if old.exists() {
        fs::remove_dir_all(&old).map_err(|e| format!("无法替换旧版本：{}", e))?;
    }
    fs::rename(&staging, &old).map_err(|e| e.to_string())?;
    Ok(())
}

// 删除清单中已不存在的主题。返回是否删除了内容
pub fn prune(root: &Path, index: &Index) -> bool {
    let keep: BTreeSet<&str> = index.themes.iter().map(|t| t.id.as_str()).collect();
    let mut removed = false;
    if let Ok(rd) = fs::read_dir(root) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if e.path().is_dir() && !keep.contains(name.as_str()) {
                removed |= fs::remove_dir_all(e.path()).is_ok();
            }
        }
    }
    removed
}

// ---------- 网络 ----------

enum FetchError {
    NotFound,
    Other(String),
}

async fn fetch(client: &reqwest::Client, name: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
    let url = format!("{}{}", BASE_URL, name);
    let mut resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| FetchError::Other(format!("网络连接失败：{}", e)))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(FetchError::NotFound);
    }
    if !resp.status().is_success() {
        return Err(FetchError::Other(format!("服务器返回 {}", resp.status())));
    }
    if resp.content_length().is_some_and(|n| n > limit) {
        return Err(FetchError::Other(format!("{} 超过大小上限", name)));
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| FetchError::Other(format!("下载中断：{}", e)))?
    {
        body.extend_from_slice(&chunk);
        if body.len() as u64 > limit {
            return Err(FetchError::Other(format!("{} 超过大小上限", name)));
        }
    }
    Ok(body)
}

fn message(e: FetchError, what: &str) -> String {
    match e {
        FetchError::NotFound => format!("{}不存在", what),
        FetchError::Other(s) => s,
    }
}

async fn download_theme(
    client: &reqwest::Client,
    entry: &Entry,
    need: &[String],
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut out = BTreeMap::new();
    for name in need {
        let f = &entry.files[name];
        let data = fetch(client, &format!("{}/{}", entry.id, name), f.size)
            .await
            .map_err(|e| message(e, &format!("文件 {} ", name)))?;
        if sha256_hex(&data) != f.sha256 {
            return Err(format!("文件 {} 校验未通过", name));
        }
        out.insert(name.clone(), data);
    }
    Ok(out)
}

pub async fn sync(config_dir: &Path, pubkey: &str, user_agent: &str) -> Result<Report, String> {
    if SYNCING.swap(true, Ordering::SeqCst) {
        return Err("正在同步在线主题，请稍候".into());
    }
    let result = run_sync(config_dir, pubkey, user_agent).await;
    SYNCING.store(false, Ordering::SeqCst);
    result
}

async fn run_sync(config_dir: &Path, pubkey: &str, user_agent: &str) -> Result<Report, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .user_agent(user_agent)
        .build()
        .map_err(|e| e.to_string())?;

    let index_bytes = fetch(&client, INDEX_FILE, MAX_INDEX_BYTES).await.map_err(|e| match e {
        FetchError::NotFound => "暂无在线主题".to_string(),
        FetchError::Other(s) => s,
    })?;
    // 文件不存在时部分服务器返回 200 和一个网页，而不是 404
    let body = index_bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&index_bytes);
    if body.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return Err("暂无在线主题".into());
    }
    let sig = fetch(&client, &format!("{}.sig", INDEX_FILE), MAX_SIG_BYTES)
        .await
        .map_err(|e| message(e, "在线主题清单的签名文件"))?;
    let sig = String::from_utf8(sig).map_err(|_| "在线主题清单的签名格式有误")?;
    verify(&index_bytes, &sig, pubkey)?;
    let index = parse(&index_bytes)?;

    let root = dir(config_dir);
    if cached_generated(&root).is_some_and(|prev| index.generated < prev) {
        return Err("在线主题清单早于本地已同步的版本，已忽略".into());
    }

    let mut report = Report { count: index.themes.len(), ..Default::default() };
    report.changed = prune(&root, &index);
    for entry in &index.themes {
        let Some(need) = plan(&root, entry) else { continue };
        let result = match download_theme(&client, entry, &need).await {
            Ok(files) => commit(config_dir, entry, files),
            Err(e) => Err(e),
        };
        match result {
            Ok(()) => report.changed = true,
            Err(e) => report.failed.push(format!("{}：{}", entry.id, e)),
        }
    }
    let _ = fs::remove_dir_all(staging_dir(config_dir));
    // 全部同步成功才记下这份清单；有失败的主题时下次整份重试
    if report.failed.is_empty() {
        fs::write(root.join(INDEX_FILE), &index_bytes).map_err(|e| e.to_string())?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("shannon-online-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn entry(id: &str, files: &[(&str, &[u8])]) -> Entry {
        Entry {
            id: id.into(),
            files: files
                .iter()
                .map(|(n, d)| (n.to_string(), FileEntry { size: d.len() as u64, sha256: sha256_hex(d) }))
                .collect(),
        }
    }

    fn index_json(themes: Vec<Entry>) -> Vec<u8> {
        serde_json::to_vec(&Index { spec: 1, generated: 1, themes }).unwrap()
    }

    #[test]
    fn sha256_known_value() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn parse_rejects_bad_entries() {
        let ok = entry("mid-autumn", &[("theme.json", b"{}"), ("moon.svg", b"<svg/>")]);
        assert!(parse(&index_json(vec![ok.clone()])).is_ok());

        let no_manifest = entry("a", &[("moon.svg", b"x")]);
        assert!(parse(&index_json(vec![no_manifest])).is_err());

        let traversal = entry("a", &[("theme.json", b"{}"), ("../x.svg", b"x")]);
        assert!(parse(&index_json(vec![traversal])).is_err());

        let script = entry("a", &[("theme.json", b"{}"), ("x.js", b"x")]);
        assert!(parse(&index_json(vec![script])).is_err());

        let bad_id = entry("../evil", &[("theme.json", b"{}")]);
        assert!(parse(&index_json(vec![bad_id])).is_err());

        assert!(parse(&index_json(vec![ok.clone(), ok])).is_err(), "重复 ID");

        let future = br#"{"spec":2,"generated":1,"themes":[]}"#;
        assert!(parse(future).is_err());
    }

    #[test]
    fn plan_commit_prune() {
        let cfg = tmp("sync");
        let root = dir(&cfg);
        let v1 = entry("demo", &[("theme.json", b"{\"spec\":1}"), ("a.svg", b"<svg>1</svg>")]);

        // 首次：全部需要下载
        let need = plan(&root, &v1).unwrap();
        assert_eq!(need.len(), 2);
        let files: BTreeMap<_, _> = [
            ("theme.json".to_string(), b"{\"spec\":1}".to_vec()),
            ("a.svg".to_string(), b"<svg>1</svg>".to_vec()),
        ]
        .into();
        commit(&cfg, &v1, files).unwrap();
        assert!(plan(&root, &v1).is_none(), "写入后与清单一致");

        // 更新一张图片：只需下载变化的文件，未变化的从缓存沿用
        let v2 = entry("demo", &[("theme.json", b"{\"spec\":1}"), ("a.svg", b"<svg>2</svg>")]);
        assert_eq!(plan(&root, &v2).unwrap(), vec!["a.svg".to_string()]);
        commit(&cfg, &v2, [("a.svg".to_string(), b"<svg>2</svg>".to_vec())].into()).unwrap();
        assert_eq!(fs::read(root.join("demo/a.svg")).unwrap(), b"<svg>2</svg>");

        // 下载内容与清单不符时不替换旧版本
        let v3 = entry("demo", &[("theme.json", b"{\"spec\":1}"), ("a.svg", b"<svg>3</svg>")]);
        assert!(commit(&cfg, &v3, [("a.svg".to_string(), b"tampered".to_vec())].into()).is_err());
        assert_eq!(fs::read(root.join("demo/a.svg")).unwrap(), b"<svg>2</svg>");

        // 删除了一张图片：清单外的多余文件触发重建
        let v4 = entry("demo", &[("theme.json", b"{\"spec\":1}")]);
        assert_eq!(plan(&root, &v4), Some(vec![]));
        commit(&cfg, &v4, BTreeMap::new()).unwrap();
        assert!(!root.join("demo/a.svg").exists());

        // 下架：清单中没有的主题从缓存删除
        let empty = Index { spec: 1, generated: 2, themes: vec![] };
        assert!(prune(&root, &empty));
        assert!(!root.join("demo").exists());

        let _ = fs::remove_dir_all(&cfg);
    }

    #[test]
    fn verify_rejects_garbage() {
        let pk = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDE2OUZCQzhGRERENjNEREUKUldUZVBkYmRqN3lmRmxZNWNMdE1ZcW9nT0l4WWU1MmtpZGorWXRTQys0ZEUwelVOeTZVcUVjbmUK";
        assert!(verify(b"{}", "not-base64!", pk).is_err());
        assert!(verify(b"{}", "", pk).is_err());
    }
}
