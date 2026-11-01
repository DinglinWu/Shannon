// 自动更新。
//
// 版本信息 latest.json 放在 Gitee 公开仓库（地址与公钥写在 tauri.conf.json 的 plugins.updater）。
// 后台定期检查，发现新版本时通知前端显示提示；/update install 时下载安装包、用内置公钥验签，
// 通过后运行安装程序。Windows 上安装程序启动时闪念会退出，安装完成后自动重新启动。
// 发布步骤见项目根目录 RELEASE.md。

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Error, UpdaterExt};

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

pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(describe)?;
    let found = updater.check().await.map_err(describe)?;
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
    let updater = app.updater().map_err(describe)?;
    let Some(update) = updater.check().await.map_err(describe)? else {
        return Err("当前已是最新版本".into());
    };
    let mut received: u64 = 0;
    let progress = app.clone();
    let done = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                received += chunk as u64;
                let _ = progress.emit("update-progress", (received, total));
            },
            move || {
                let _ = done.emit("update-downloaded", ());
            },
        )
        .await
        .map_err(describe)?;
    // Windows 上执行不到这里：安装程序启动后进程已退出，安装完成后由安装程序重新启动闪念
    app.restart()
}
