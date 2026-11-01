// 系统提示词 prompts.toml：每条一个带编号的小节 [编号]，/spt 编号 切换。
// 0 固定表示不用提示词，文件里从 1 开始。当前选哪一条记在 config.toml 的 [ai] prompt。
//
// 和 config.toml 一样：文件改坏了不崩，继续用上一次读对的内容，错误留给 /spt 列表显示。

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

pub const TEMPLATE: &str = include_str!("prompts_template.toml");
pub const FILE_NAME: &str = "prompts.toml";

// 单条提示词上限：再长就是贴错东西了，也会让每次提问都很贵
const MAX_TEXT_CHARS: usize = 8000;

#[derive(Deserialize, Clone, Debug)]
pub struct Prompt {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub text: String,
}

pub type Prompts = BTreeMap<u32, Prompt>;

pub fn parse(text: &str) -> Result<Prompts, String> {
    let raw: BTreeMap<String, Prompt> = toml::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| crate::config::describe(&e.to_string()))?;
    let mut out = BTreeMap::new();
    for (k, mut p) in raw {
        let id: u32 = k
            .trim()
            .parse()
            .map_err(|_| format!("[{}] 不是有效编号，小节名须为正整数，例如 [4]", k))?;
        if id == 0 {
            return Err("[0] 为保留编号（表示不使用提示词），请从 1 开始编号".into());
        }
        if p.text.chars().count() > MAX_TEXT_CHARS {
            return Err(format!("[{}] 的内容超过 {} 字上限", id, MAX_TEXT_CHARS));
        }
        p.name = p.name.trim().to_string();
        if p.name.is_empty() {
            p.name = format!("提示词 {}", id);
        }
        out.insert(id, p);
    }
    Ok(out)
}

// ---------- 读取（带缓存和"沿用上次"） ----------

struct Cache {
    path: PathBuf,
    mtime: Option<SystemTime>,
    prompts: Prompts,
    error: Option<String>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

// 返回当前可用的提示词，以及文件现在的错误（没有错误为 None）
pub fn load(path: &Path) -> (Prompts, Option<String>) {
    let mt = fs::metadata(path).and_then(|m| m.modified()).ok();
    let mut cache = CACHE.lock().unwrap();
    if let Some(c) = cache.as_ref() {
        if mt.is_some() && c.mtime == mt && c.path == path {
            return (c.prompts.clone(), c.error.clone());
        }
    }
    let last_good = cache.as_ref().filter(|c| c.path == path).map(|c| c.prompts.clone());
    let (prompts, error) = match fs::read_to_string(path) {
        Err(_) => (last_good.unwrap_or_else(|| parse(TEMPLATE).unwrap_or_default()), None),
        Ok(text) => match parse(&text) {
            Ok(p) => (p, None),
            Err(e) => (last_good.unwrap_or_else(|| parse(TEMPLATE).unwrap_or_default()), Some(e)),
        },
    };
    *cache = Some(Cache { path: path.to_path_buf(), mtime: mt, prompts: prompts.clone(), error: error.clone() });
    (prompts, error)
}

// 把 {date}、{weekday} 换成当天的。日期由前端按本地时区算好传进来
pub fn render(text: &str, date: &str, weekday: &str) -> String {
    text.trim().replace("{date}", date).replace("{weekday}", weekday)
}

// 提示词列表里显示的一行预览
pub fn preview(text: &str) -> String {
    let line = text.trim().lines().next().unwrap_or("").trim();
    let mut s: String = line.chars().take(36).collect();
    if line.chars().count() > 36 || text.trim().lines().count() > 1 {
        s.push('…');
    }
    s
}

// ---------- 首次生成 ----------

// 没有 prompts.toml 就从模板生成。legacy 是上一版 config.toml 里 system_prompt 的内容：
// 用户填过的话，放到下一个空编号里，返回这个编号，调用方把 config 的 prompt 指过去
pub fn ensure(dir: &Path, legacy: &str) -> Result<Option<u32>, String> {
    let path = dir.join(FILE_NAME);
    if path.exists() {
        return Ok(None);
    }
    let legacy = legacy.trim();
    let (text, id) = if legacy.is_empty() {
        (TEMPLATE.to_string(), None)
    } else {
        let next = parse(TEMPLATE)?.keys().max().copied().unwrap_or(0) + 1;
        let mut doc: toml_edit::DocumentMut =
            TEMPLATE.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
        let mut t = toml_edit::Table::new();
        t["name"] = toml_edit::value("迁移的提示词");
        t["text"] = multiline(legacy);
        t.decor_mut().set_prefix("\n\n# 由旧版 config.toml 中的 system_prompt 迁移\n");
        doc[&next.to_string()] = toml_edit::Item::Table(t);
        (doc.to_string(), Some(next))
    };
    parse(&text)?;
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(id)
}

// 能写成 '''…''' 就写成它，读起来和手写的一样；内容里恰好有 ''' 时退回普通字符串
fn multiline(s: &str) -> toml_edit::Item {
    if !s.contains("'''") {
        if let Ok(v) = format!("'''\n{}\n'''", s).parse::<toml_edit::Value>() {
            return toml_edit::Item::Value(v);
        }
    }
    toml_edit::value(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses() {
        let p = parse(TEMPLATE).unwrap();
        assert_eq!(p[&1].name, "默认");
        assert!(p[&1].text.contains("{date}"));
        assert!(p.len() >= 2);
    }

    #[test]
    fn rejects_zero_and_non_numbers() {
        assert!(parse("[0]\nname='x'\ntext='y'").unwrap_err().contains("0"));
        assert!(parse("[abc]\nname='x'\ntext='y'").unwrap_err().contains("编号"));
    }

    #[test]
    fn renders_placeholders() {
        let s = render("今天是 {date}，{weekday}", "2026-10-01", "星期四");
        assert_eq!(s, "今天是 2026-10-01，星期四");
    }

    #[test]
    fn migrates_legacy_prompt() {
        let d = std::env::temp_dir().join(format!("shannon-prompts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let id = ensure(&d, "只用英文回答。\n别用 'markdown'。").unwrap().unwrap();
        let text = fs::read_to_string(d.join(FILE_NAME)).unwrap();
        let p = parse(&text).unwrap();
        assert_eq!(p[&id].name, "迁移的提示词");
        assert!(p[&id].text.contains("只用英文回答"));
        assert!(text.contains("系统提示词在每次提问前发送给 AI"));
        // 已经有文件就不动
        assert_eq!(ensure(&d, "别的").unwrap(), None);
        let _ = fs::remove_dir_all(&d);
    }
}
