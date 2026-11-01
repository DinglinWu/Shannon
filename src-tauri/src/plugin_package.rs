use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
};

pub const MAX_ZIP: usize = 4 * 1024 * 1024;
pub const PERMISSIONS: &[&str] = &[
    "storage.read",
    "storage.write",
    "notes.read",
    "notes.write",
    "notes.organize",
    "notes.delete",
    "notes.trash.read",
    "notes.display",
    "clock.read",
    "clock.display",
];
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub effect: String,
    pub parameters: Value,
    #[serde(default)]
    pub permissions: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub entry: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub tools: Vec<Tool>,
}
#[derive(Clone)]
pub struct Package {
    pub manifest: Manifest,
    pub code: String,
    pub sha256: String,
    pub readme: String,
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 24
        && id.as_bytes()[0].is_ascii_lowercase()
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        && !id.ends_with('-')
        && !id.contains("--")
}
pub fn tool_name(id: &str, tool: &str) -> String {
    format!("plg__{}__{tool}", id.replace('-', "_"))
}
pub fn validate_manifest(m: &Manifest) -> Result<(), String> {
    if m.api_version != 1 {
        return Err("插件 API 版本不支持".into());
    }
    if !valid_id(&m.id) {
        return Err("插件ID须为1–24位小写字母/数字/单个短横线，以字母开头".into());
    }
    let version: Vec<_> = m.version.split('.').collect();
    if version.len() != 3
        || version
            .iter()
            .any(|s| s.is_empty() || s.len() > 5 || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("插件版本须为 x.y.z".into());
    }
    if m.name.trim().is_empty()
        || m.name.chars().count() > 60
        || m.description.trim().is_empty()
        || m.description.chars().count() > 500
        || m.author.trim().is_empty()
        || m.author.chars().count() > 100
    {
        return Err("插件名称、描述或作者无效".into());
    }
    safe_name(&m.entry)?;
    if !m.entry.ends_with(".js") {
        return Err("插件入口必须为打包后的 JavaScript 文件".into());
    }
    let perms: BTreeSet<_> = m.permissions.iter().collect();
    if perms.len() != m.permissions.len()
        || m.permissions
            .iter()
            .any(|p| !PERMISSIONS.contains(&p.as_str()))
    {
        return Err("插件申请了未知或重复的权限".into());
    }
    if m.tools.is_empty() || m.tools.len() > 16 {
        return Err("插件须包含1–16个工具".into());
    }
    let mut names = BTreeSet::new();
    for tool in &m.tools {
        if tool.name.is_empty()
            || tool.name.len() > 24
            || !tool.name.as_bytes()[0].is_ascii_lowercase()
            || tool.name.contains("__")
            || !tool
                .name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            || !names.insert(&tool.name)
        {
            return Err("插件工具名称无效或重复".into());
        }
        if tool.description.trim().is_empty()
            || tool.description.chars().count() > 500
            || !["read", "display", "write"].contains(&tool.effect.as_str())
        {
            return Err("工具描述或 effect 无效".into());
        }
        if tool.parameters["type"] != "object" {
            return Err("工具参数必须为对象 Schema".into());
        }
        crate::plugin_schema::check(&tool.parameters)?;
        let mut seen = BTreeSet::new();
        for permission in &tool.permissions {
            if !perms.contains(permission) || !seen.insert(permission) {
                return Err("工具权限必须属于插件已声明的权限且不重复".into());
            }
            if (permission.ends_with(".write") || matches!(permission.as_str(), "notes.organize" | "notes.delete")) && tool.effect != "write" {
                return Err("写权限工具必须声明 effect:write".into());
            }
            if permission.ends_with(".display") && tool.effect == "read" {
                return Err("展示权限工具必须声明 display 或 write".into());
            }
        }
    }
    Ok(())
}
fn safe_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 240 || name.contains('\\') || name.starts_with('/') {
        return Err("插件包路径无效".into());
    }
    for part in name.trim_end_matches('/').split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.contains([':', '<', '>', '"', '|', '?', '*'])
            || part.ends_with(['.', ' '])
            || part.chars().any(|c| c.is_control())
            || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit())
        {
            return Err("插件包路径无效".into());
        }
    }
    Ok(())
}
pub fn read_package(bytes: &[u8]) -> Result<Package, String> {
    if bytes.is_empty() || bytes.len() > MAX_ZIP {
        return Err("插件ZIP须小于4MiB".into());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "不是有效的插件ZIP")?;
    if zip.len() > 128 {
        return Err("插件文件超过128个".into());
    }
    let mut files = std::collections::BTreeMap::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        safe_name(entry.name())?;
        if entry
            .unix_mode()
            .map_or(false, |m| m & 0o170000 == 0o120000)
        {
            return Err("插件不允许符号链接".into());
        }
        if entry.is_dir() {
            continue;
        }
        total = total.checked_add(entry.size()).ok_or("插件解压尺寸超限")?;
        if total > 8 * 1024 * 1024 || entry.size() > 2 * 1024 * 1024 {
            return Err("插件解压尺寸超限".into());
        }
        let name = entry.name().to_owned();
        if files
            .keys()
            .any(|key: &String| key.eq_ignore_ascii_case(&name))
        {
            return Err("插件包含重复文件".into());
        }
        let mut data = Vec::new();
        entry
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() > 2 * 1024 * 1024 {
            return Err("插件文件过大".into());
        }
        files.insert(name, data);
    }
    let manifest = files.get("plugin.json").ok_or("ZIP根目录缺少plugin.json")?;
    if manifest.len() > 65536 {
        return Err("插件清单过大".into());
    }
    let manifest: Manifest =
        serde_json::from_slice(manifest).map_err(|e| format!("插件清单无效：{e}"))?;
    validate_manifest(&manifest)?;
    let readme=String::from_utf8(files.get("README.md").ok_or("ZIP根目录缺少README.md（开发者、版本、功能说明）")?.clone()).map_err(|_|"README.md 必须为UTF-8")?;
    crate::plugin_readme::validate(&readme,&manifest.id,&manifest.name,&manifest.author,&manifest.version,&manifest.description)?;
    let code = String::from_utf8(files.get(&manifest.entry).ok_or("插件入口不存在")?.clone())
        .map_err(|_| "插件入口不是UTF-8")?;
    if code.len() > 512 * 1024 {
        return Err("插件入口超过512KiB".into());
    }
    Ok(Package {
        manifest,
        code,
        sha256: digest(bytes),
        readme,
    })
}

#[cfg(test)]
pub fn sample_manifest() -> Manifest {
    serde_json::from_value(serde_json::json!({"api_version":1,"id":"sample","name":"示例","version":"1.0.0","description":"测试","author":"测试","entry":"main.js","tools":[{"name":"hello","description":"问候","effect":"read","parameters":{"type":"object","properties":{},"additionalProperties":false}}]})).unwrap()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zip_requires_readme_and_rejects_false_identity() {
        use std::io::Write;
        for readme in [None,Some("# 没有开发者和版本"),Some("---\nid: sample\nname: 示例\nauthor: 冒名\nversion: 1.0.0\ndescription: 测试\n---\n正文")] {
            let mut zip=zip::ZipWriter::new(Cursor::new(Vec::new()));
            zip.start_file("plugin.json",zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(&serde_json::to_vec(&sample_manifest()).unwrap()).unwrap();
            zip.start_file("main.js",zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(b"Shannon.register({hello:()=>42})").unwrap();
            if let Some(readme)=readme {zip.start_file("README.md",zip::write::SimpleFileOptions::default()).unwrap();zip.write_all(readme.as_bytes()).unwrap();}
            assert!(read_package(&zip.finish().unwrap().into_inner()).is_err());
        }
    }
    #[test]
    fn manifest_rejects_unsafe_capabilities_and_namespace_collisions() {
        let mut m = sample_manifest();
        validate_manifest(&m).unwrap();
        m.permissions = vec!["shell".into()];
        assert!(validate_manifest(&m).is_err());
        for id in ["../x", "bad_id", "a--b", "A", "a-", ""] {
            assert!(!valid_id(id));
        }
        m.permissions = vec!["notes.write".into()];
        m.tools[0].permissions = m.permissions.clone();
        assert!(validate_manifest(&m).is_err());
        m.tools[0].effect = "write".into();
        validate_manifest(&m).unwrap();
    }
    #[test]
    fn note_organization_and_deletion_require_write_effect() {
        for permission in ["notes.organize", "notes.delete"] {
            let mut m=sample_manifest();m.permissions=vec![permission.into()];m.tools[0].permissions=m.permissions.clone();
            assert!(validate_manifest(&m).is_err());m.tools[0].effect="write".into();assert!(validate_manifest(&m).is_ok());
        }
    }
    #[test]
    fn archives_never_extract_untrusted_paths() {
        use std::io::Write;
        for name in ["../escape", "C:/bad", "a\\b", "CON.txt"] {
            let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"bad").unwrap();
            assert!(read_package(&zip.finish().unwrap().into_inner()).is_err());
        }
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("plugin.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&sample_manifest()).unwrap())
            .unwrap();
        zip.start_file("main.js", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"Shannon.register({hello:()=>42});").unwrap();
        zip.start_file("README.md",zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all("---\nid: sample\nname: 示例\nauthor: 测试\nversion: 1.0.0\ndescription: 测试\n---\n# 示例\n测试工具".as_bytes()).unwrap();
        let package = read_package(&zip.finish().unwrap().into_inner()).unwrap();
        assert_eq!(package.manifest.id, "sample");
        assert_eq!(package.sha256.len(), 64);
    }
    #[test]
    fn shipped_example_manifest_and_bundle_validate() {
        use std::io::Write;
        let manifest = include_bytes!("../../examples/plugins/quick-stats/plugin.json");
        let code = include_bytes!("../../examples/plugins/quick-stats/main.js");
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            ("plugin.json", manifest.as_slice()),
            ("main.js", code.as_slice()),
            ("README.md", include_bytes!("../../examples/plugins/quick-stats/README.md").as_slice()),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        let package = read_package(&zip.finish().unwrap().into_inner()).unwrap();
        assert_eq!(
            tool_name(&package.manifest.id, "summarize"),
            "plg__quick_stats__summarize"
        );
        crate::plugin_schema::validate(
            &package.manifest.tools[0].parameters,
            &serde_json::json!({"values":[2,4,6]}),
        )
        .unwrap();
        assert!(crate::plugin_schema::validate(
            &package.manifest.tools[0].parameters,
            &serde_json::json!({"values":[]})
        )
        .is_err());
    }
}
