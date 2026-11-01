// 磁盘上的主题：自定义主题（%APPDATA%\com.brynn.ailauncher\themes\<id>\）
// 和在线主题的本地缓存（online-themes\<id>\，由 online_theme.rs 维护），目录结构相同。
// 网页读不到磁盘，这里负责列目录、读清单、把图片转成 data URL 交给前端。
// 校验和应用都在前端 theme.js 里做，这里只管"安全地把文件读出来"。

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

// 单张图片上限。主题图都是小装饰，太大的多半是放错了文件
pub const MAX_ASSET_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_THEMES: usize = 64;

#[derive(Serialize)]
pub struct UserTheme {
    id: String,
    manifest: Option<serde_json::Value>,
    // 读不出来或不是合法 JSON 时写原因，/theme 里会列出来
    error: Option<String>,
}

pub fn dir(config_dir: &Path) -> PathBuf {
    let d = config_dir.join("themes");
    fs::create_dir_all(&d).ok();
    d
}

// 主题 id 就是文件夹名，只许小写字母、数字、短横线，杜绝 ..\ 之类的路径穿越
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
}

fn mime_of(file: &str) -> Option<&'static str> {
    let ext = file.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return None,
    })
}

pub fn valid_file(file: &str) -> bool {
    !file.is_empty()
        && file.len() <= 68
        && !file.starts_with('.')
        && file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && mime_of(file).is_some()
}

pub fn list(config_dir: &Path) -> Vec<UserTheme> {
    list_dir(&dir(config_dir))
}

pub fn list_dir(root: &Path) -> Vec<UserTheme> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten().take(MAX_THEMES) {
        if !e.path().is_dir() {
            continue;
        }
        let id = e.file_name().to_string_lossy().to_string();
        if !valid_id(&id) {
            out.push(UserTheme {
                id,
                manifest: None,
                error: Some("文件夹名仅可包含小写字母、数字和连字符".into()),
            });
            continue;
        }
        let path = e.path().join("theme.json");
        let result = match fs::metadata(&path) {
            Err(_) => Err("文件夹中缺少 theme.json".to_string()),
            Ok(m) if m.len() > MAX_MANIFEST_BYTES => Err("theme.json 超过大小上限".to_string()),
            Ok(_) => fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|s| {
                    // 记事本存的 UTF-8 可能带 BOM，serde_json 不认
                    serde_json::from_str(s.trim_start_matches('\u{feff}'))
                        .map_err(|e| format!("JSON 格式错误：{}", e))
                }),
        };
        match result {
            Ok(v) => out.push(UserTheme { id, manifest: Some(v), error: None }),
            Err(err) => out.push(UserTheme { id, manifest: None, error: Some(err) }),
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub fn asset(config_dir: &Path, id: &str, file: &str) -> Result<String, String> {
    asset_in(&dir(config_dir), id, file)
}

pub fn asset_in(root: &Path, id: &str, file: &str) -> Result<String, String> {
    if !valid_id(id) || !valid_file(file) {
        return Err("主题文件名无效".into());
    }
    let path = root.join(id).join(file);
    let meta = fs::metadata(&path).map_err(|_| format!("主题 {} 中缺少文件 {}", id, file))?;
    if meta.len() > MAX_ASSET_BYTES {
        return Err(format!("{} 超过 2 MB 上限", file));
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    Ok(format!("data:{};base64,{}", mime_of(file).unwrap(), base64(&bytes)))
}

// 标准 base64。就这一处用，不值得为它加一个依赖
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn rejects_traversal() {
        assert!(!valid_id(".."));
        assert!(!valid_id("a/b"));
        assert!(!valid_file("../x.png"));
        assert!(!valid_file("a\\b.png"));
        assert!(!valid_file("x.exe"));
        assert!(valid_file("lantern.svg"));
        assert!(valid_id("national-day"));
    }
}
