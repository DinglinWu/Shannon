// 自动更新。
//
// 分别读取 Gitee / GitHub 更新清单，下载同一签名附件时可切换镜像。
// 后台定期检查，发现新版本时通知前端显示提示；/update install 时下载安装包、用内置公钥验签，
// 通过后运行安装程序。Windows 上安装程序启动时闪念会退出，安装完成后自动重新启动。
// 发布步骤见 docs/release/RELEASE.md。

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Error, Update, UpdaterExt};

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: String,
    pub date: Option<String>,
}

// 最近一次检查到的新版本；没有新版本或还没检查过时为 None
static PENDING: Mutex<Option<UpdateInfo>> = Mutex::new(None);
static INSTALLING: AtomicBool = AtomicBool::new(false);

// 插件的报错是英文的，换成界面上显示的说明
fn describe(e: Error) -> String {
    match e {
        Error::ReleaseNotFound => "暂未获取到版本信息，请稍后重试".into(),
        Error::Reqwest(_) | Error::Network(_) => format!("网络连接失败：{}", e),
        Error::TargetNotFound(_) => "版本信息中缺少 Windows 安装包".into(),
        Error::Serialization(_) | Error::Semver(_) => "版本信息格式有误".into(),
        Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_) => {
            "安装包签名校验未通过，已停止更新".into()
        }
        other => format!("{}", other),
    }
}

pub fn pending() -> Option<UpdateInfo> {
    PENDING.lock().unwrap().clone()
}

async fn find_update(app: &AppHandle) -> Result<Option<Update>, String> {
    let endpoints = app.config().plugins.0.get("updater")
        .and_then(|v| v["endpoints"].as_array())
        .ok_or("未配置自动更新来源")?;
    let mut errors = Vec::new();
    let mut checked = false;
    // Separate requests also handle timeouts, malformed JSON and stale mirror manifests.
    for endpoint in endpoints {
        let url = endpoint.as_str().ok_or("更新来源格式无效")?
            .parse().map_err(|_| "更新来源地址无效")?;
        let updater = app.updater_builder().endpoints(vec![url]).map_err(describe)?
            .timeout(std::time::Duration::from_secs(15)).build().map_err(describe)?;
        match updater.check().await {
            Ok(Some(mut update)) => {
                update.timeout = Some(std::time::Duration::from_secs(180));
                return Ok(Some(update));
            }
            Ok(None) => checked = true,
            Err(error) => errors.push(describe(error)),
        }
    }
    if checked { Ok(None) } else { Err(format!("所有更新来源均不可用：{}", errors.join("；"))) }
}

pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let found = find_update(app).await?;
    let info = found.map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone().unwrap_or_default(),
        date: u.date.map(|d| d.date().to_string()),
    });
    *PENDING.lock().unwrap() = info.clone();
    Ok(info)
}

// 下载、验签、安装。下载进度通过 update-progress 事件发给前端：(已下载字节, 总字节)
pub async fn install(app: &AppHandle) -> Result<(), String> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("更新正在进行中".into());
    }
    let result = run_install(app).await;
    INSTALLING.store(false, Ordering::SeqCst);
    result
}

async fn run_install(app: &AppHandle) -> Result<(), String> {
    let Some(mut update) = find_update(app).await? else {
        return Err("当前已是最新版本".into());
    };
    let mut errors = Vec::new();
    for url in crate::release_sources::download_urls(update.download_url.as_str()) {
        update.download_url = url.parse().map_err(|_| "更新下载地址无效")?;
        let mut received: u64 = 0;
        let _ = app.emit("update-progress", (0_u64, Option::<u64>::None));
        match update.download(
            |chunk, total| {
                received += chunk as u64;
                let _ = app.emit("update-progress", (received, total));
            }, || {},
        ).await {
            Ok(bytes) => {
                // download() has verified the original signature; never retry installer failures.
                let _ = app.emit("update-downloaded", ());
                update.install(bytes).map_err(describe)?;
                app.restart()
            }
            Err(error) => errors.push(describe(error)),
        }
    }
    Err(format!("所有下载来源均失败：{}", errors.join("；")))
}
