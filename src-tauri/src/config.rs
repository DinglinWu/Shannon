// 配置文件 config.toml：闪念的全部设置都在这一个带注释的文件里，/config 打开它。
//
// 读：用 toml 反序列化成 Config，缺的项取默认值。文件改坏了（语法错、类型错）不崩，
//     继续用上一次读对的设置，错误原因留着给界面提示。
// 写：界面上的开关、/rmbr、/theme 要改配置时，用 toml_edit 只改那一个值，
//     用户写的注释、空行、顺序原样保留。文件正处在改坏的状态时拒绝写入，免得冲掉用户正在改的内容。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;
use toml_edit::{DocumentMut, Item, Value};

pub const TEMPLATE: &str = include_str!("config_template.toml");
pub const FILE_NAME: &str = "config.toml";
// 老版本的 JSON 配置。第一次启动新版时读出来并入 config.toml，然后改名留作备份
pub const LEGACY_FILE_NAME: &str = "config.json";

// ---------- 结构 ----------
// 每个字段都有默认值：用户删掉某一行，就回到默认

// 代码里各项的默认值和 config_template.toml 里写的必须一致，测试会比对
#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub ai: Ai,
    pub search: Search,
    pub history: History,
    pub selection: Selection,
    pub window: Window,
    pub update: Update,
    pub theme: Theme,
    pub aliases: BTreeMap<String, String>,
    pub builtin_aliases: BuiltinAliases,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct General {
    pub hotkey: String,
    pub autostart: bool,
    pub editor: String,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Ai {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub deep_thinking: bool,
    pub web_search: bool,
    pub use_system_proxy: bool,
    pub context_messages: u32,
    pub connect_timeout_secs: u64,
    // 选用 prompts.toml 里的第几条系统提示词，0 表示不用
    pub prompt: u32,
    // 上一版直接写在这里的提示词。只在第一次生成 prompts.toml 时读一次，搬过去后删掉
    #[serde(skip_serializing)]
    pub system_prompt: String,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Search {
    pub max_tokens: u32,
    pub max_searches: u32,
    pub thinking_budget: u32,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct History {
    pub retention_days: u64,
    pub input_history: u32,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Selection {
    pub enabled: bool,
    pub wait_ms: u64,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Window {
    pub width: u32,
    pub max_height: u32,
    pub expanded_width: u32,
    pub expanded_height: u32,
    pub placeholder: String,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Update {
    pub auto_check: bool,
    pub check_interval_hours: u64,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Theme {
    pub mode: String,
    pub online: bool,
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct BuiltinAliases {
    pub disabled: BTreeSet<String>,
}

impl Default for General {
    fn default() -> Self {
        Self { hotkey: "Alt+Space".into(), autostart: false, editor: "notepad".into() }
    }
}

impl Default for Ai {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.deepseek.com".into(),
            model: "deepseek-flash".into(),
            deep_thinking: false,
            web_search: false,
            use_system_proxy: false,
            context_messages: 20,
            connect_timeout_secs: 10,
            prompt: 1,
            system_prompt: String::new(),
        }
    }
}

impl Default for Search {
    fn default() -> Self {
        Self { max_tokens: 8192, max_searches: 5, thinking_budget: 4096 }
    }
}

impl Default for History {
    fn default() -> Self {
        Self { retention_days: 30, input_history: 100 }
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self { enabled: true, wait_ms: 200 }
    }
}

impl Default for Window {
    fn default() -> Self {
        Self {
            width: 640,
            max_height: 460,
            expanded_width: 860,
            expanded_height: 620,
            placeholder: "For you, always.".into(),
        }
    }
}

impl Default for Update {
    fn default() -> Self {
        Self { auto_check: true, check_interval_hours: 6 }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self { mode: "auto".into(), online: true }
    }
}

// 用户手写的值可能离谱，用的时候一律夹到合理范围
impl Config {
    pub fn context_messages(&self) -> usize {
        self.ai.context_messages.clamp(2, 200) as usize
    }
    pub fn connect_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.ai.connect_timeout_secs.clamp(2, 120))
    }
    pub fn base_url(&self) -> String {
        let b = self.ai.base_url.trim().trim_end_matches('/');
        if b.is_empty() { "https://api.deepseek.com".into() } else { b.to_string() }
    }
    pub fn model(&self) -> String {
        let m = self.ai.model.trim();
        if m.is_empty() { "deepseek-flash".into() } else { m.to_string() }
    }
    pub fn retention_days(&self) -> u64 {
        self.history.retention_days.clamp(1, 3650)
    }
    pub fn selection_wait_ms(&self) -> u64 {
        self.selection.wait_ms.clamp(50, 1000)
    }
    pub fn search_limits(&self) -> (u32, u32, u32) {
        let max_tokens = self.search.max_tokens.clamp(1024, 32768);
        // 思考预算必须小于 max_tokens，不然接口直接 400
        let budget = self.search.thinking_budget.clamp(1024, max_tokens - 512);
        (max_tokens, self.search.max_searches.clamp(1, 20), budget)
    }
    pub fn update_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.update.check_interval_hours.clamp(1, 168) * 3600)
    }
    pub fn builtin_disabled(&self, key: &str) -> bool {
        self.builtin_aliases.disabled.contains(key)
    }
}

// ---------- 读取（带缓存和"沿用上次"） ----------

struct Cache {
    path: std::path::PathBuf,
    cfg: Config,
    mtime: Option<SystemTime>,
    error: Option<String>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

// 记事本存 UTF-8 时可能带 BOM，toml 不认
fn strip_bom(s: &str) -> &str {
    s.trim_start_matches('\u{feff}')
}

pub fn parse(text: &str) -> Result<Config, String> {
    toml::from_str::<Config>(strip_bom(text)).map_err(|e| describe(&e.to_string()))
}

// toml 的报错是英文的，把最常见的几句换成人话，行号和出错位置原样保留
pub fn describe(msg: &str) -> String {
    msg.replace(", expected", "；expected")
        .replace("TOML parse error at line", "第")
        .replace(", column", " 行，第")
        .replacen('\n', " 列\n", 1)
        .replace("invalid type: string", "类型错误：此处不应为文本（请检查是否多加了引号），当前值为")
        .replace("invalid type: integer", "类型错误：此处不应为数字，当前值为")
        .replace("invalid type: boolean", "类型错误：此处不应为 true / false，当前值为")
        .replace("expected a boolean", "应为 true 或 false")
        .replace("expected a string", "应为带引号的文本")
        .replace("expected u32", "应为数字").replace("expected u64", "应为数字")
        .replace("invalid string", "字符串格式无效（Windows 路径请使用单引号）")
        .replace("duplicate key", "键重复")
}

pub fn load(path: &Path) -> Config {
    let mt = mtime(path);
    let mut cache = CACHE.lock().unwrap();
    if let Some(c) = cache.as_ref() {
        if mt.is_some() && c.mtime == mt && c.path == path {
            return c.cfg.clone();
        }
    }
    let last_good = cache.as_ref().filter(|c| c.path == path).map(|c| c.cfg.clone());
    let (cfg, error) = match fs::read_to_string(path) {
        // 文件被删掉了：先用默认值顶着，下次 ensure() 会重新生成
        Err(_) => (last_good.unwrap_or_default(), None),
        Ok(text) => match parse(&text) {
            Ok(cfg) => (cfg, None),
            Err(e) => (last_good.unwrap_or_default(), Some(e)),
        },
    };
    *cache = Some(Cache { path: path.to_path_buf(), cfg: cfg.clone(), mtime: mt, error });
    cfg
}

// 当前配置文件的错误（没有错误返回 None）。先 load 一次保证是最新的
pub fn error(path: &Path) -> Option<String> {
    load(path);
    CACHE.lock().unwrap().as_ref().and_then(|c| c.error.clone())
}

// ---------- 写入（保留注释） ----------

// 路径、别名这类文字尽量写成单引号字面量，反斜杠不用转义，用户自己看着也顺眼
pub fn str_value(s: &str) -> Item {
    if !s.contains('\'') && !s.contains('\n') && !s.contains('\r') {
        if let Ok(v) = format!("'{}'", s).parse::<Value>() {
            return Item::Value(v);
        }
    }
    toml_edit::value(s)
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    // 先写临时文件再改名：写到一半被读到、或者断电，都不会留下半个文件
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn edit(path: &Path, f: impl FnOnce(&mut DocumentMut)) -> Result<(), String> {
    let text = fs::read_to_string(path).unwrap_or_else(|_| TEMPLATE.to_string());
    let mut doc: DocumentMut = strip_bom(&text)
        .parse()
        .map_err(|e: toml_edit::TomlError| format!("配置文件存在语法错误，请通过 /config 修正后重试：\n{}", describe(&e.to_string())))?;
    // 类型错误也拦住：比如用户把 deep_thinking 写成了 "yes"，这时不能再往里写东西
    parse(&doc.to_string()).map_err(|e| format!("配置文件有误，请通过 /config 修正后重试：\n{}", e))?;
    f(&mut doc);
    let out = doc.to_string();
    parse(&out).map_err(|e| format!("写入后的配置无法解析，已取消写入：{}", e))?;
    write_atomic(path, &out)
}

pub fn set_disabled_builtins(doc: &mut DocumentMut, set: &BTreeSet<String>) {
    let mut arr = toml_edit::Array::new();
    for s in set {
        arr.push(s.as_str());
    }
    doc["builtin_aliases"]["disabled"] = toml_edit::value(arr);
}

// ---------- 首次生成与迁移 ----------

const PROMPT_COMMENT: &str = "\n# 当前使用的系统提示词编号（系统提示词用于设定回答风格）。\n\
# 0 为不使用，1 为默认，其余编号见 prompts.toml。可通过 /spt 编号 切换，/spt edit 编辑。\n";

// 上一版的 config.toml 里是 system_prompt = "…"，这一版改成 prompt = 编号、内容放 prompts.toml。
// 删掉旧键（连同它上面那段注释），补上新键和说明；legacy_id 是旧内容搬过去后的编号
pub fn upgrade_prompt_key(path: &Path, legacy_id: Option<u32>) -> Result<(), String> {
    edit(path, |doc| {
        let Some(ai) = doc["ai"].as_table_mut() else {
            return;
        };
        ai.remove("system_prompt");
        if !ai.contains_key("prompt") {
            ai.insert("prompt", toml_edit::value(1));
            if let Some(mut k) = ai.key_mut("prompt") {
                k.leaf_decor_mut().set_prefix(PROMPT_COMMENT);
            }
        }
        if let Some(id) = legacy_id {
            ai["prompt"] = toml_edit::value(id as i64);
        }
    })
}

// 没有 config.toml 就从模板生成；有老的 config.json 就把里面的值搬过来。
// autostart 传入注册表里的现状，这样迁移前后开机自启的状态不变
pub fn ensure(dir: &Path, autostart_now: bool) -> Result<(), String> {
    let path = dir.join(FILE_NAME);
    if path.exists() {
        return Ok(());
    }
    let mut doc: DocumentMut = TEMPLATE.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
    doc["general"]["autostart"] = toml_edit::value(autostart_now);

    let legacy = dir.join(LEGACY_FILE_NAME);
    if let Some(old) = fs::read_to_string(&legacy)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s.trim_start_matches('\u{feff}')).ok())
    {
        migrate(&mut doc, &old);
    }

    let out = doc.to_string();
    parse(&out)?;
    write_atomic(&path, &out)?;
    if legacy.exists() {
        let _ = fs::rename(&legacy, dir.join("config.json.bak"));
    }
    Ok(())
}

fn migrate(doc: &mut DocumentMut, old: &serde_json::Value) {
    if let Some(k) = old["api_key"].as_str() {
        doc["ai"]["api_key"] = str_value(k);
    }
    for (from, to) in [
        ("use_system_proxy", "use_system_proxy"),
        ("deep_thinking", "deep_thinking"),
        ("web_search", "web_search"),
    ] {
        if let Some(b) = old[from].as_bool() {
            doc["ai"][to] = toml_edit::value(b);
        }
    }
    if let Some(map) = old["aliases"].as_object() {
        for (k, v) in map {
            if let Some(v) = v.as_str() {
                doc["aliases"][k.as_str()] = str_value(v);
            }
        }
    }
    if let Some(list) = old["disabled_builtins"].as_array() {
        let set: BTreeSet<String> = list.iter().filter_map(|x| x.as_str().map(String::from)).collect();
        set_disabled_builtins(doc, &set);
    }
}

// ---------- 用编辑器打开 ----------

#[cfg(windows)]
pub fn open_in_editor(editor: &str, path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let ed = editor.trim().trim_matches('"').trim_matches('\'');
    let ed = if ed.is_empty() { "notepad" } else { ed };
    // 交给 cmd 的 start 去找程序：code 这类其实是 code.cmd，直接 Command::new 找不到。
    // CREATE_NO_WINDOW 不让 cmd 自己闪出一个黑框
    std::process::Command::new("cmd")
        .raw_arg(format!("/C start \"\" \"{}\" \"{}\"", ed, path.display()))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub fn open_in_editor(_editor: &str, path: &Path) -> Result<(), String> {
    open::that(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("shannon-cfg-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn template_parses_and_matches_code_defaults() {
        let t: Config = toml::from_str(TEMPLATE).unwrap();
        let empty: Config = toml::from_str("").unwrap();
        // 模板里写的默认值和代码里的默认值必须一致，不然"删掉一行恢复默认"会变成另一个值
        assert_eq!(serde_json::to_value(&t).unwrap(), serde_json::to_value(&empty).unwrap());
    }

    #[test]
    fn edit_keeps_comments_and_updates_value() {
        let d = tmpdir("edit");
        let p = d.join(FILE_NAME);
        fs::write(&p, TEMPLATE).unwrap();
        edit(&p, |doc| doc["ai"]["deep_thinking"] = toml_edit::value(true)).unwrap();
        edit(&p, |doc| doc["aliases"]["文档"] = str_value(r"D:\文档\项目")).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.contains("# 深度思考：先推理后作答"));
        assert!(text.contains("deep_thinking = true"));
        assert!(text.contains(r#""文档" = 'D:\文档\项目'"#));
        let cfg = parse(&text).unwrap();
        assert!(cfg.ai.deep_thinking);
        assert_eq!(cfg.aliases["文档"], r"D:\文档\项目");
        // 新别名紧跟在 [aliases] 后面，说明文字还在节头上方
        let a = text.find("[aliases]").unwrap();
        assert!(text[a..].trim_start_matches("[aliases]").trim_start().starts_with("\"文档\""));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn broken_file_keeps_last_good_and_refuses_writes() {
        let d = tmpdir("broken");
        let p = d.join(FILE_NAME);
        fs::write(&p, TEMPLATE.replace("web_search = false", "web_search = true")).unwrap();
        assert!(load(&p).ai.web_search);
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&p, TEMPLATE.replace("web_search = false", "web_search = yes")).unwrap();
        let cfg = load(&p);
        assert!(cfg.ai.web_search, "配置有误时应沿用上次有效的设置");
        assert!(error(&p).is_some());
        assert!(edit(&p, |doc| doc["ai"]["deep_thinking"] = toml_edit::value(true)).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn type_error_is_reported_in_chinese() {
        let Err(e) = parse(&TEMPLATE.replace("deep_thinking = false", "deep_thinking = \"yes\"")) else {
            panic!("应该报错");
        };
        println!("{}", e);
        assert!(e.contains("行"), "{}", e);
    }

    #[test]
    fn upgrades_system_prompt_key() {
        let d = tmpdir("upgrade");
        let p = d.join(FILE_NAME);
        let old = TEMPLATE.replace(
            PROMPT_COMMENT.trim_start_matches('\n'),
            "# 系统提示词：旧版说明\nsystem_prompt = \"只用英文\"\n",
        ).replace("prompt = 1\n", "");
        assert!(old.contains("system_prompt"), "构造旧文件失败");
        fs::write(&p, &old).unwrap();
        upgrade_prompt_key(&p, Some(4)).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert!(!text.contains("system_prompt") && !text.contains("旧版说明"));
        assert!(text.contains("prompt = 4") && text.contains("/spt edit 编辑。"));
        assert_eq!(parse(&text).unwrap().ai.prompt, 4);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn migrates_legacy_json() {
        let d = tmpdir("migrate");
        fs::write(
            d.join(LEGACY_FILE_NAME),
            r#"{"api_key":"sk-abc","use_system_proxy":true,"deep_thinking":false,"web_search":true,
                "aliases":{"文档":"D:\\文档","wx":"C:\\Program Files\\Tencent\\WeChat\\WeChat.exe"},
                "disabled_builtins":["ps"]}"#,
        )
        .unwrap();
        ensure(&d, true).unwrap();
        let text = fs::read_to_string(d.join(FILE_NAME)).unwrap();
        let cfg = parse(&text).unwrap();
        assert_eq!(cfg.ai.api_key, "sk-abc");
        assert!(cfg.ai.use_system_proxy && cfg.ai.web_search && !cfg.ai.deep_thinking);
        assert!(cfg.general.autostart);
        assert_eq!(cfg.aliases["文档"], r"D:\文档");
        assert!(cfg.builtin_disabled("ps"));
        assert!(text.contains("# 唤起或隐藏窗口的全局快捷键"));
        assert!(!d.join(LEGACY_FILE_NAME).exists() && d.join("config.json.bak").exists());
        let _ = fs::remove_dir_all(&d);
    }
}
