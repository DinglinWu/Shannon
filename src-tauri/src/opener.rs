// /o 命令：把一个名字或路径解析成可以交给系统打开的东西。
//
// 解析顺序（用户别名、内置别名在 main.rs 里先查，这里只管普通匹配）：
//   1. 网址 / 协议（https://、www.、ms-settings: 之类）
//   2. 绝对路径，支持 ~ 和 %APPDATA% 这类环境变量，外面带引号也行
//   3. 开始菜单和桌面上的快捷方式，名字完全相同或以它开头。
//      名字除了原样，还认全拼、拼音首字母、英文单词首字母（见 name_keys）
//   4. PATH 里的可执行文件（notepad、calc、code 这种）
//   5. 快捷方式名字里包含它
// 3 放在 4 前面：输入 code 时想要的是 VS Code 本体，而不是 PATH 里那个会闪黑框的 code.cmd。
// 5 放最后：包含匹配太宽，只在别的都落空时兜底

use std::path::{Path, PathBuf};

// 从资源管理器"复制文件地址"得到的路径两边带双引号
pub fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(s)
        .trim()
}

// ~ 和 %VAR% 展开。找不到的变量原样留着，最后判断路径不存在时自然报错
pub fn expand_env(s: &str) -> String {
    let mut s = s.to_string();
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            s = format!("{}{}", home, &s[1..]);
        }
    }

    let mut out = String::new();
    let mut rest = s.as_str();
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) if j > 0 => {
                let name = &after[..j];
                match std::env::var(name) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[j + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

// 一个字母加冒号是盘符（C:），不能当成协议
pub fn looks_like_url(s: &str) -> bool {
    let lower = s.to_lowercase();
    if lower.starts_with("www.") {
        return true;
    }
    let Some(colon) = s.find(':') else {
        return false;
    };
    let scheme = &s[..colon];
    scheme.len() >= 2
        && scheme.chars().next().map_or(false, |c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

fn normalize_url(s: &str) -> String {
    if s.to_lowercase().starts_with("www.") {
        format!("https://{}", s)
    } else {
        s.to_string()
    }
}

fn shortcut_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut push = |var: &str, sub: &str| {
        if let Ok(base) = std::env::var(var) {
            dirs.push(Path::new(&base).join(sub));
        }
    };
    push("APPDATA", r"Microsoft\Windows\Start Menu\Programs");
    push("PROGRAMDATA", r"Microsoft\Windows\Start Menu\Programs");
    push("USERPROFILE", "Desktop");
    push("PUBLIC", "Desktop");
    dirs
}

const LINK_EXTS: [&str; 3] = ["lnk", "url", "appref-ms"];

fn collect_links(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            // 开始菜单一般就两三层，限个深度防止碰到奇怪的软链接绕圈
            if depth < 4 {
                collect_links(&p, depth + 1, out);
            }
        } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            if LINK_EXTS.contains(&ext.to_lowercase().as_str()) {
                out.push(p);
            }
        }
    }
}

// ---------- 名字的几种写法 ----------
//
// 学 uTools 的做法：一个应用名除了原样，还能用全拼、拼音首字母、英文单词首字母来叫。
//   微信               → weixin / wx
//   网易云音乐         → wangyiyunyinyue / wyyyy
//   QQ音乐             → qqyinyue / qyy / qqyy（英文词整体保留的那种首字母）
//   Visual Studio Code → visualstudiocode / vsc

enum Tok {
    Ascii(String),
    Han(Vec<String>), // 这个汉字的所有读音（不带声调）。多音字不止一个：乐 → le / yue
}

fn tokenize(name: &str) -> Vec<Tok> {
    use pinyin::ToPinyinMulti;
    let mut toks = Vec::new();
    let mut word = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            word.push(c.to_ascii_lowercase());
            continue;
        }
        if !word.is_empty() {
            toks.push(Tok::Ascii(std::mem::take(&mut word)));
        }
        // 不认识的字符（标点、空格、日文假名）当分隔符
        if let Some(multi) = c.to_pinyin_multi() {
            let mut readings: Vec<String> = Vec::new();
            for py in multi {
                let r = py.plain().to_string();
                if !readings.contains(&r) {
                    readings.push(r);
                }
            }
            toks.push(Tok::Han(readings));
        }
    }
    if !word.is_empty() {
        toks.push(Tok::Ascii(word));
    }
    toks
}

// 多音字会让读音组合成倍增长，应用名一般很短，但还是封个顶
const MAX_READINGS: usize = 16;

// 返回：[原名小写, 每种读音组合下的 全拼 / 首字母 / 英文词整体+汉字首字母]，去重去空
pub fn name_keys(name: &str) -> Vec<String> {
    let mut keys = vec![name.to_lowercase()];
    let toks = tokenize(name);
    if toks.len() <= 1 && !toks.iter().any(|t| matches!(t, Tok::Han(_))) {
        return keys;
    }

    // 每个组合是 (全拼, 首字母, 混合首字母)，逐个词往后接
    let mut combos: Vec<(String, String, String)> = vec![Default::default()];
    for t in &toks {
        match t {
            Tok::Ascii(w) => {
                for c in &mut combos {
                    c.0.push_str(w);
                    c.1.push_str(&w[..1]);
                    c.2.push_str(w);
                }
            }
            Tok::Han(readings) => {
                let mut next = Vec::new();
                for c in &combos {
                    for py in readings {
                        if next.len() >= MAX_READINGS {
                            break;
                        }
                        let f = &py[..1];
                        next.push((
                            format!("{}{}", c.0, py),
                            format!("{}{}", c.1, f),
                            format!("{}{}", c.2, f),
                        ));
                    }
                }
                combos = next;
            }
        }
    }
    for (full, ini, mixed) in combos {
        for k in [full, ini, mixed] {
            if !k.is_empty() && !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    keys
}

// ---------- 快捷方式索引 ----------

struct Entry {
    path: PathBuf,
    stem: String,      // 小写原名
    keys: Vec<String>, // name_keys 的结果，第 0 个就是 stem
}

// 每次 /o 扫一遍开始菜单和桌面，建好索引后解析内置别名和普通匹配都用它，不重复扫
pub struct Index {
    entries: Vec<Entry>,
}

impl Index {
    pub fn scan() -> Self {
        let mut links = Vec::new();
        for d in shortcut_dirs() {
            collect_links(&d, 0, &mut links);
        }
        Self::from_paths(links)
    }

    pub fn from_paths(links: Vec<PathBuf>) -> Self {
        let entries = links
            .into_iter()
            .filter_map(|path| {
                let name = path.file_stem()?.to_str()?.to_string();
                let keys = name_keys(&name);
                Some(Entry {
                    stem: keys[0].clone(),
                    keys,
                    path,
                })
            })
            .collect();
        Index { entries }
    }

    // 模糊匹配：取各种写法里分数最高的。同分取名字短的（更接近用户打的那个词）
    pub fn best(&self, query: &str) -> Option<(u8, PathBuf)> {
        let q = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| !is_noise(&e.stem, &q))
            .filter_map(|e| {
                let mut s = score(&e.stem, &q);
                for k in &e.keys[1..] {
                    s = s.max(score_key(k, &q));
                }
                (s > 0).then(|| (s, e.stem.chars().count(), e.path.clone()))
            })
            .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
            .map(|(s, _, p)| (s, p))
    }

    // 内置别名用的严格匹配：只认原名完全相同或以它开头（"Adobe Photoshop 2024" 以
    // "adobe photoshop" 开头），不走拼音，免得 ps 这种短词误中别的东西
    fn by_name(&self, name: &str) -> Option<PathBuf> {
        let n = name.to_lowercase();
        self.entries
            .iter()
            .filter(|e| !is_noise(&e.stem, &n))
            .filter_map(|e| {
                let s = if e.stem == n {
                    2
                } else if e.stem.starts_with(&n) {
                    1
                } else {
                    return None;
                };
                Some((s, e.stem.chars().count(), e.path.clone()))
            })
            .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
            .map(|(_, _, p)| p)
    }
}

// 卸载程序、帮助文档这类快捷方式和本体名字很像，除非用户点名要，否则不参与匹配
fn is_noise(stem: &str, query: &str) -> bool {
    const NOISE: [&str; 6] = ["uninstall", "卸载", "readme", "help", "帮助", "说明"];
    NOISE.iter().any(|w| stem.contains(w) && !query.contains(w))
}

// 原名：3 = 完全相同，2 = 以它开头或其中某个词就是它，1 = 包含，0 = 不匹配。
// "某个词就是它"是为了 code → Visual Studio Code 这种英文名
fn score(stem: &str, query: &str) -> u8 {
    if stem == query {
        3
    } else if stem.starts_with(query) || stem.split_whitespace().any(|w| w == query) {
        2
    } else if stem.contains(query) {
        1
    } else {
        0
    }
}

// 拼音 / 首字母这几种写法：完全相同 3，开头 2（wyy → 网易云音乐），
// 包含只给全拼算（yinyue → QQ音乐），首字母串太短，算包含会乱中
fn score_key(key: &str, query: &str) -> u8 {
    if key == query {
        3
    } else if key.starts_with(query) {
        2
    } else if query.len() >= 4 && key.contains(query) {
        1
    } else {
        0
    }
}

pub fn find_in_path(query: &str) -> Option<PathBuf> {
    // 带路径分隔符的就不是命令名了
    if query.contains('\\') || query.contains('/') {
        return None;
    }
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter(|e| !e.is_empty())
        .map(|e| e.to_lowercase())
        .collect();

    for dir in std::env::split_paths(&path) {
        // 自带扩展名的（notepad.exe、devmgmt.msc）直接找这个文件
        if query.contains('.') {
            let p = dir.join(query);
            if p.is_file() {
                return Some(p);
            }
        }
        for e in &exts {
            let p = dir.join(format!("{}{}", query, e));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

// ---------- 内置别名 ----------
//
// 国内用户约定俗成的叫法。左边是别名（都小写），右边是按顺序尝试的目标：
//   - 快捷方式名：原名完全相同或以它开头就算（中英文版 Windows / 新旧版本名字不同，所以列好几个）
//   - xxx.exe / xxx.msc：在 PATH 里找
//   - 带冒号的（ms-settings:、shell:Downloads）：直接交给系统
// 没装的应用自然解析不出来，/o 会接着走普通匹配，/rmbr 列表里也不显示。
// 能被拼音 / 首字母自动匹配到的（比如 微信 → wx）这里照样写上，
// 因为英文版系统上快捷方式可能叫 WeChat，拼音就对不上了

pub const BUILTIN: &[(&[&str], &[&str])] = &[
    // 聊天 / 办公
    (&["wx", "weixin", "wechat"], &["微信", "WeChat", "Weixin"]),
    (&["qq"], &["QQ", "腾讯QQ", "QQNT"]),
    (&["tim"], &["TIM"]),
    (&["qywx", "wework", "wxwork"], &["企业微信", "WXWork", "WeCom"]),
    (&["dd", "dingding", "dingtalk"], &["钉钉", "DingTalk"]),
    (&["fs", "feishu", "lark"], &["飞书", "Feishu", "Lark"]),
    (&["txhy", "tmeet", "voov"], &["腾讯会议", "VooV Meeting", "TencentMeeting"]),
    (&["zoom"], &["Zoom Workplace", "Zoom"]),
    (&["wps"], &["WPS Office", "WPS"]),
    (&["word"], &["Word", "Microsoft Word"]),
    (&["excel", "xls"], &["Excel", "Microsoft Excel"]),
    (&["ppt", "powerpoint"], &["PowerPoint", "Microsoft PowerPoint"]),
    (&["outlook"], &["Outlook", "Microsoft Outlook"]),
    (&["teams"], &["Microsoft Teams", "Teams"]),
    // 浏览器
    (&["chrome", "gg", "谷歌"], &["Google Chrome", "Chrome"]),
    (&["edge"], &["Microsoft Edge"]),
    (&["ff", "firefox", "火狐"], &["Firefox", "Mozilla Firefox"]),
    // 开发
    (&["vsc", "vscode", "code"], &["Visual Studio Code"]),
    (&["vs"], &["Visual Studio 2022", "Visual Studio 2019", "Visual Studio"]),
    (&["idea"], &["IntelliJ IDEA"]),
    (&["pycharm", "pyc"], &["PyCharm"]),
    (&["cursor"], &["Cursor"]),
    (&["trae"], &["Trae"]),
    (&["gitbash"], &["Git Bash"]),
    (&["xshell"], &["Xshell"]),
    (&["moba"], &["MobaXterm"]),
    (&["navicat"], &["Navicat Premium", "Navicat"]),
    (&["docker"], &["Docker Desktop"]),
    (&["jupyter", "jp"], &["Jupyter Notebook", "Jupyter Lab"]),
    (&["conda", "anaconda"], &["Anaconda Prompt", "Anaconda Navigator"]),
    (&["matlab"], &["MATLAB"]),
    (&["wt", "终端", "terminal"], &["wt.exe", "Windows Terminal", "终端"]),
    (&["cmd", "命令行"], &["cmd.exe"]),
    (&["pwsh", "powershell"], &["pwsh.exe", "powershell.exe"]),
    // 设计 / 工程
    (&["ps", "photoshop"], &["Adobe Photoshop", "Photoshop"]),
    (&["pr", "premiere"], &["Adobe Premiere Pro", "Premiere Pro"]),
    (&["ae"], &["Adobe After Effects", "After Effects"]),
    (&["ai", "illustrator"], &["Adobe Illustrator", "Illustrator"]),
    (&["lr", "lightroom"], &["Adobe Lightroom Classic", "Adobe Lightroom"]),
    (&["acrobat"], &["Adobe Acrobat", "Acrobat Reader"]),
    (&["cad"], &["AutoCAD"]),
    (&["sw", "solidworks"], &["SOLIDWORKS"]),
    (&["jy", "jianying", "剪映"], &["剪映专业版", "剪映", "JianyingPro", "CapCut"]),
    (&["xmind"], &["Xmind", "XMind"]),
    // 影音
    (&["wyy", "wyyyy", "网易云", "cloudmusic"], &["网易云音乐", "NetEase Cloud Music", "CloudMusic"]),
    (&["qqyy", "qqmusic"], &["QQ音乐", "QQMusic"]),
    (&["kg", "kugou"], &["酷狗音乐", "KuGou"]),
    (&["kw", "kuwo"], &["酷我音乐", "Kuwo"]),
    (&["pot", "potplayer"], &["PotPlayer", "Daum PotPlayer"]),
    (&["bili", "blbl", "b站"], &["哔哩哔哩", "bilibili"]),
    (&["dy", "douyin"], &["抖音"]),
    (&["obs"], &["OBS Studio"]),
    // 工具
    (&["bdwp", "bdy", "baidunetdisk"], &["百度网盘", "BaiduNetdisk"]),
    (&["kk", "quark"], &["夸克", "夸克网盘", "Quark"]),
    (&["xl", "xunlei", "thunder"], &["迅雷", "Thunder"]),
    (&["ev", "everything"], &["Everything"]),
    (&["snipaste"], &["Snipaste"]),
    (&["xrk", "sunlogin"], &["向日葵远程控制", "向日葵", "SunloginClient"]),
    (&["todesk"], &["ToDesk"]),
    (&["7z"], &["7-Zip File Manager", "7-Zip"]),
    (&["bz", "bandizip"], &["Bandizip"]),
    (&["yd", "youdao"], &["网易有道翻译", "有道翻译", "网易有道词典", "有道词典"]),
    (&["wegame"], &["WeGame"]),
    (&["mumu"], &["MuMu模拟器12", "MuMu模拟器", "MuMu Player 12", "MuMuPlayer"]),
    // 科研 / 笔记
    (&["zotero", "zt"], &["Zotero"]),
    (&["endnote"], &["EndNote"]),
    (&["caj", "知网"], &["CAJViewer"]),
    (&["ob", "obsidian"], &["Obsidian"]),
    (&["typora"], &["Typora"]),
    (&["itk", "itksnap"], &["ITK-SNAP"]),
    (&["slicer", "3dslicer"], &["3D Slicer", "Slicer"]),
    (&["mricron", "mricrogl"], &["MRIcroGL", "MRIcron"]),
    // 系统（中文、拼音首字母、英文都能叫）
    (&["jsb", "记事本", "notepad"], &["notepad.exe"]),
    (&["jsq", "计算器", "calc"], &["calc.exe"]),
    (&["ht", "画图", "mspaint"], &["mspaint.exe"]),
    (&["rwglq", "任务管理器", "taskmgr"], &["taskmgr.exe"]),
    (&["kzmb", "控制面板", "control"], &["control.exe"]),
    (&["sz", "设置", "settings"], &["ms-settings:"]),
    (&["zyglq", "资源管理器", "wjglq", "文件管理器", "explorer"], &["explorer.exe"]),
    (&["zcb", "注册表", "regedit"], &["regedit.exe"]),
    (&["sbglq", "设备管理器"], &["devmgmt.msc"]),
    (&["fw", "服务", "services"], &["services.msc"]),
    (&["jt", "截图"], &["ms-screenclip:"]),
    (&["hsz", "回收站"], &["shell:RecycleBinFolder"]),
    (&["xz", "下载", "downloads"], &["shell:Downloads"]),
    (&["zm", "桌面", "desktop"], &["shell:Desktop"]),
    (&["文档", "documents"], &["shell:Personal"]),
];

fn builtin_entry(alias: &str) -> Option<&'static (&'static [&'static str], &'static [&'static str])> {
    let a = alias.to_lowercase();
    BUILTIN.iter().find(|(names, _)| names.iter().any(|n| *n == a))
}

pub fn is_builtin(alias: &str) -> bool {
    builtin_entry(alias).is_some()
}

fn resolve_target(t: &str, idx: &Index) -> Option<String> {
    if looks_like_url(t) {
        return Some(t.to_string());
    }
    let lower = t.to_lowercase();
    if lower.ends_with(".exe") || lower.ends_with(".msc") {
        return find_in_path(t).map(|p| p.to_string_lossy().into_owned());
    }
    idx.by_name(t).map(|p| p.to_string_lossy().into_owned())
}

// 这个内置别名在本机指向什么；没装就是 None
pub fn resolve_builtin(alias: &str, idx: &Index) -> Option<String> {
    let (_, targets) = builtin_entry(alias)?;
    targets.iter().find_map(|t| resolve_target(t, idx))
}

// 本机装了的内置别名，给 /rmbr 列表用。同一个应用的几个别名合成一行
pub fn installed_builtins(idx: &Index, disabled: &dyn Fn(&str) -> bool) -> Vec<(Vec<String>, String)> {
    BUILTIN
        .iter()
        .filter_map(|(names, targets)| {
            let names: Vec<String> = names
                .iter()
                .filter(|n| !disabled(n))
                .map(|n| n.to_string())
                .collect();
            if names.is_empty() {
                return None;
            }
            let target = targets.iter().find_map(|t| resolve_target(t, idx))?;
            Some((names, target))
        })
        .collect()
}

// ---------- 普通解析 ----------

// 解析成最终要打开的字符串（路径或网址）。不查别名
pub fn resolve(query: &str, idx: &Index) -> Result<String, String> {
    let raw = strip_quotes(query);
    if raw.is_empty() {
        return Err("请输入要打开的应用、文件夹、文件或网址".into());
    }

    if looks_like_url(raw) {
        return Ok(normalize_url(raw));
    }

    let expanded = expand_env(raw);
    let p = Path::new(&expanded);
    if p.is_absolute() {
        return if p.exists() {
            Ok(expanded)
        } else {
            Err(format!("路径不存在：{}", expanded))
        };
    }

    let link = idx.best(raw);

    if let Some((s, p)) = &link {
        if *s >= 2 {
            return Ok(p.to_string_lossy().into_owned());
        }
    }
    if let Some(p) = find_in_path(raw) {
        return Ok(p.to_string_lossy().into_owned());
    }
    if let Some((_, p)) = link {
        return Ok(p.to_string_lossy().into_owned());
    }

    Err(format!(
        "未找到「{}」。请使用完整路径，或通过 /rmbr 添加别名",
        raw
    ))
}

pub fn open(target: &str) -> Result<(), String> {
    open::that_detached(target).map_err(|e| format!("无法打开：{}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_and_urls() {
        assert_eq!(strip_quotes("  \"C:\\a b\\c\"  "), "C:\\a b\\c");
        assert!(looks_like_url("https://x.com"));
        assert!(looks_like_url("ms-settings:display"));
        assert!(looks_like_url("www.baidu.com"));
        assert!(!looks_like_url("C:\\Windows"));
        assert!(!looks_like_url("微信"));
        assert_eq!(normalize_url("www.a.com"), "https://www.a.com");
    }

    #[test]
    fn env_expansion() {
        std::env::set_var("FOO_T", "bar");
        assert_eq!(expand_env("%FOO_T%\\x"), "bar\\x");
        assert_eq!(expand_env("%NOPE_T%\\x"), "%NOPE_T%\\x");
        assert_eq!(expand_env("100%"), "100%");
        std::env::set_var("USERPROFILE", "C:\\Users\\b");
        assert_eq!(expand_env("~\\Downloads"), "C:\\Users\\b\\Downloads");
    }

    fn idx(names: &[&str]) -> Index {
        Index::from_paths(names.iter().map(|n| PathBuf::from(format!("a/{}.lnk", n))).collect())
    }
    fn pick(i: &Index, q: &str) -> Option<String> {
        i.best(q).map(|(_, p)| p.file_stem().unwrap().to_string_lossy().into_owned())
    }

    #[test]
    fn keys() {
        let k = name_keys("微信");
        assert_eq!(&k[..3], &["微信", "weixin", "wx"]); // 常用读音排在前面
        assert!(name_keys("重庆银行").contains(&"chongqingyinhang".to_string())); // 重 → chong
        assert!(name_keys("银行").contains(&"yh".to_string()));
        let k = name_keys("QQ音乐"); // 乐 是多音字，le / yue 两套都要有
        for want in ["qqyinyue", "qqyinle", "qyy", "qqyy", "qyl"] {
            assert!(k.contains(&want.to_string()), "{} 不在 {:?}", want, k);
        }
        assert_eq!(name_keys("Visual Studio Code"), vec!["visual studio code", "visualstudiocode", "vsc"]);
        assert_eq!(name_keys("Zotero"), vec!["zotero"]);
        let k = name_keys("网易云音乐");
        assert!(k.contains(&"wangyiyunyinyue".to_string()) && k.contains(&"wyyyy".to_string()));
    }

    #[test]
    fn link_ranking() {
        let i = idx(&[
            "Visual Studio Code", "微信", "卸载微信", "WeChat Files", "Chrome",
            "Chrome Remote Desktop", "网易云音乐", "QQ音乐", "QQ", "腾讯会议", "记事本",
        ]);
        assert_eq!(pick(&i, "微信").as_deref(), Some("微信"));
        assert_eq!(pick(&i, "wx").as_deref(), Some("微信"));
        assert_eq!(pick(&i, "weixin").as_deref(), Some("微信"));
        assert_eq!(pick(&i, "chrome").as_deref(), Some("Chrome"));
        assert_eq!(pick(&i, "code").as_deref(), Some("Visual Studio Code"));
        assert_eq!(pick(&i, "vsc").as_deref(), Some("Visual Studio Code"));
        assert_eq!(pick(&i, "wyy").as_deref(), Some("网易云音乐"));
        assert_eq!(pick(&i, "wangyiyun").as_deref(), Some("网易云音乐"));
        assert_eq!(pick(&i, "qqyy").as_deref(), Some("QQ音乐"));
        assert_eq!(pick(&i, "qq").as_deref(), Some("QQ"));
        assert_eq!(pick(&i, "yinyue").as_deref(), Some("QQ音乐"));
        assert_eq!(pick(&i, "txhy").as_deref(), Some("腾讯会议"));
        assert_eq!(pick(&i, "jsb").as_deref(), Some("记事本"));
        assert_eq!(pick(&i, "卸载").as_deref(), Some("卸载微信"));
        assert_eq!(pick(&i, "xyz"), None);
    }

    #[test]
    fn builtins() {
        let i = idx(&["Adobe Photoshop 2024", "Google Chrome", "微信", "Photos"]);
        assert!(resolve_builtin("ps", &i).unwrap().ends_with("Adobe Photoshop 2024.lnk"));
        assert!(resolve_builtin("GG", &i).unwrap().ends_with("Google Chrome.lnk"));
        assert!(resolve_builtin("wx", &i).unwrap().ends_with("微信.lnk"));
        assert_eq!(resolve_builtin("sz", &i).as_deref(), Some("ms-settings:"));
        assert_eq!(resolve_builtin("pr", &i), None); // 没装
        assert_eq!(resolve_builtin("nope", &i), None);
        let list = installed_builtins(&i, &|n| n == "gg");
        let chrome = list.iter().find(|(_, t)| t.ends_with("Google Chrome.lnk")).unwrap();
        assert_eq!(chrome.0, vec!["chrome", "谷歌"]);
        // 别名不能有重复
        let mut seen = std::collections::HashSet::new();
        for (names, _) in BUILTIN {
            for n in *names {
                assert!(seen.insert(*n), "重复的内置别名 {}", n);
                assert_eq!(*n, n.to_lowercase());
            }
        }
    }
}
