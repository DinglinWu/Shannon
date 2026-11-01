// 主题引擎：读主题包 → 按日期挑一个 → 把配色和槽位应用到界面 → 按频率放唤起特效。
// 主题包的格式见项目根目录的 THEME_SPEC.md，改这里的行为要同步改那份文档。
//
// 主题包只有数据（JSON + 图片），没有脚本。图片一律经 <img> 或 CSS 背景显示，
// SVG 在这两种用法下都不会执行里面的脚本；配色值也过一遍过滤，不许带 url()。
// 在线主题（从 Gitee 仓库同步、经签名校验的缓存，见 online_theme.rs）走同一套校验，
// 即使发布渠道被人动了手脚，也碰不到用户电脑。

import { invoke } from "./invoke.js";
import { listen } from "@tauri-apps/api/event";
import { runFx, EFFECT_NAMES } from "./fx.js";
import { themeActivationKey } from "./theme-activation.js";

// ---------- 可调参数 ----------
const SPEC_VERSION = 1;                    // 支持的 theme.json 规范版本
const BUILTIN_INDEX = "/themes/index.json"; // 内置主题清单（public/themes 下）
const FX_KEY = "shannon.fx.";              // + 主题 id → 上次放唤起特效的日期（或 "done"）
const ACTIVE_THEME_KEY = "shannon.theme.active";
const RELOAD_MS = 60 * 1000;               // 唤起时最多隔这么久重新扫一次主题目录
const MAX_TOKENS = 40;
const MAX_MESSAGE = 24;                    // 唤起寄语的字数上限
const MESSAGE_STEP = 70;                   // 寄语逐字浮现的间隔（毫秒）
const MESSAGE_HOLD = 2000;                 // 寄语全部浮现后至少停留的时间（毫秒）

const ID_RE = /^[a-z0-9][a-z0-9-]{0,47}$/;
const FILE_RE = /^[\w-][\w.-]{0,63}\.(svg|png|jpe?g|webp|gif)$/i;
const DATE_RE = /^(\d{4}-)?\d{2}-\d{2}$/;
const TOKEN_NAME_RE = /^--[a-z][a-z0-9-]{0,40}$/;
// 值里出现这些就整条丢掉：url() 能让主题去外网拉东西，分号花括号能拼出别的样式
const TOKEN_BAD_RE = /url\s*\(|expression|javascript:|image-set|[;{}<>\\]/i;

// ---------- 状态 ----------
let els = null;            // { appEl, topRowEl, queryInput, anchorEl }
let themes = [];           // 已校验过的主题，同 id 时 自定义 > 在线 > 内置
let problems = [];         // 读不进来的主题和原因，/theme 里列出来给写主题的人看
let loadedAt = 0;
let current = null;        // 当前应用中的主题（null = 默认外观）
let appliedTokens = [];
let defaultPlaceholder = "";
let mode = "auto";         // config.toml 的 [theme] mode："auto" 按日期；"off" 关闭；否则是主题 id
let badgeEl = null;
let patternEl = null;
let applySeq = 0;
let greeting = null;       // 上方寄语的本地收起监听 { timer }
let activationSeq = 0;
let pendingActivation = null;
let appliedActivationKey = null;
const assetCache = new Map();

// ---------- 小工具 ----------

function today() {
  const d = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

function store(key, value) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch (_) {}
}

function load(key) {
  try {
    return localStorage.getItem(key);
  } catch (_) {
    return null;
  }
}

function hexToRgb(hex) {
  let h = hex.replace("#", "");
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  if (!/^[0-9a-f]{6}$/i.test(h)) return null;
  const n = parseInt(h, 16);
  return `${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}`;
}

const num = (v, lo, hi, def) => {
  const n = Number(v);
  return Number.isFinite(n) ? Math.min(hi, Math.max(lo, n)) : def;
};

// when: { from, to }。带年份是一次性的，只写 MM-DD 就每年都生效，可以跨年（12-31 → 01-02）
function isActive(when, day) {
  if (!when) return false;
  const { from, to } = when;
  if (from.length === 10) return from <= day && day <= to;
  const md = day.slice(5);
  return from <= to ? from <= md && md <= to : md >= from || md <= to;
}

// ---------- 读取与校验 ----------

// 把 theme.json 规整成引擎用的结构。有问题返回字符串（原因），没问题返回对象
function normalize(raw, id, source) {
  if (!raw || typeof raw !== "object") return "theme.json 的内容必须是对象";
  if (raw.spec !== SPEC_VERSION) return `spec 应为 ${SPEC_VERSION}，当前为 ${JSON.stringify(raw.spec)}`;
  if (!ID_RE.test(id)) return "文件夹名仅可包含小写字母、数字和连字符";
  if (raw.id !== undefined && raw.id !== id) return `id "${raw.id}" 与文件夹名 "${id}" 不一致`;

  const t = {
    id,
    source,
    name: typeof raw.name === "string" && raw.name.trim() ? raw.name.trim().slice(0, 24) : id,
    author: typeof raw.author === "string" ? raw.author.slice(0, 40) : "",
    priority: num(raw.priority, -100, 100, 0),
    when: null,
    tokens: {},
    placeholder: null,
    badge: null,
    pattern: null,
    enter: null,
    warnings: [],
  };

  if (raw.when !== undefined) {
    const { from, to } = raw.when || {};
    if (!DATE_RE.test(from || "") || !DATE_RE.test(to || "") || from.length !== to.length) {
      return 'when 格式应为 {"from": "10-01", "to": "10-07"} 或含年份的 "2026-10-01"，起止格式需一致';
    }
    t.when = { from, to };
  }

  if (raw.tokens && typeof raw.tokens === "object") {
    for (const [k, v] of Object.entries(raw.tokens).slice(0, MAX_TOKENS)) {
      if (!TOKEN_NAME_RE.test(k) || typeof v !== "string" || v.length > 120 || TOKEN_BAD_RE.test(v)) {
        t.warnings.push(`已忽略无效配色 ${k}`);
        continue;
      }
      t.tokens[k] = v.trim();
    }
    // 只给了 --accent 的话，荧光、命令高亮要用的 --accent-rgb 自动算出来
    if (t.tokens["--accent"] && !t.tokens["--accent-rgb"]) {
      const rgb = hexToRgb(t.tokens["--accent"]);
      if (rgb) t.tokens["--accent-rgb"] = rgb;
    }
  }

  const s = raw.slots || {};
  if (typeof s.placeholder === "string" && s.placeholder.trim()) {
    t.placeholder = s.placeholder.trim().slice(0, 40);
  }
  if (s.badge) {
    if (FILE_RE.test(s.badge.image || "")) {
      t.badge = {
        image: s.badge.image,
        height: num(s.badge.height, 16, 40, 28),
        label: typeof s.badge.label === "string" ? s.badge.label.slice(0, 40) : t.name,
      };
    } else t.warnings.push("badge.image 文件名无效，已忽略");
  }
  if (s.pattern) {
    if (FILE_RE.test(s.pattern.image || "")) {
      t.pattern = {
        image: s.pattern.image,
        opacity: num(s.pattern.opacity, 0.02, 0.3, 0.08),
        size: num(s.pattern.size, 24, 400, 120),
      };
    } else t.warnings.push("pattern.image 文件名无效，已忽略");
  }

  if (raw.enter) {
    const e = raw.enter;
    if (!EFFECT_NAMES.includes(e.effect)) {
      t.warnings.push(`特效 "${e.effect}" 不存在，可选：${EFFECT_NAMES.join("、")}`);
    } else {
      const params = e.params && typeof e.params === "object" ? { ...e.params } : {};
      let message = null;
      if (typeof e.message === "string" && e.message.trim()) {
        message = [...e.message.trim().replace(/\s+/g, " ")].slice(0, MAX_MESSAGE).join("");
        if ([...e.message.trim()].length > MAX_MESSAGE) t.warnings.push(`message 超过 ${MAX_MESSAGE} 字，已截断`);
      }
      const images = Array.isArray(params.images) ? params.images.filter((f) => FILE_RE.test(f)).slice(0, 8) : [];
      delete params.images;
      t.enter = {
        effect: e.effect,
        scope: e.scope === "screen" ? "screen" : "window",
        frequency: ["always", "daily", "once"].includes(e.frequency) ? e.frequency : "daily",
        duration: num(e.duration, 500, 8000, 2800),
        params,
        images,
        message,
      };
    }
  }
  return t;
}

async function loadBuiltin() {
  const out = [];
  let ids = [];
  try {
    const idx = await (await fetch(BUILTIN_INDEX, { cache: "no-store" })).json();
    ids = Array.isArray(idx.themes) ? idx.themes : [];
  } catch (_) {
    return out;
  }
  for (const id of ids) {
    try {
      const raw = await (await fetch(`/themes/${id}/theme.json`, { cache: "no-store" })).json();
      out.push([id, raw, null]);
    } catch (err) {
      out.push([id, null, "theme.json 无法读取或格式有误"]);
    }
  }
  return out.map((x) => [...x, "builtin"]);
}

async function loadDisk(command, source) {
  try {
    const list = await invoke(command);
    return list.map((x) => [x.id, x.manifest, x.error, source]);
  } catch (_) {
    return [];
  }
}

async function reload() {
  const all = [
    ...(await loadBuiltin()),
    ...(await loadDisk("list_online_themes", "online")),
    ...(await loadDisk("list_user_themes", "user")),
  ];
  const map = new Map();
  problems = [];
  for (const [id, raw, error, source] of all) {
    const t = error ? error : normalize(raw, id, source);
    if (typeof t === "string") {
      problems.push({ id, source, reason: t });
      continue;
    }
    map.set(id, t); // 后读的覆盖先读的：自定义 > 在线 > 内置
  }
  themes = [...map.values()];
  loadedAt = Date.now();
}

async function assetUrl(theme, file) {
  const key = `${theme.source}/${theme.id}/${file}`;
  if (assetCache.has(key)) return assetCache.get(key);
  let url;
  if (theme.source === "builtin") {
    url = `/themes/${theme.id}/${file}`;
  } else {
    // 自定义主题与在线主题缓存在 AppData 里，网页读不到磁盘，由 Rust 读出来转成 data URL
    url = await invoke("theme_asset", { id: theme.id, file, source: theme.source });
  }
  assetCache.set(key, url);
  return url;
}

// ---------- 选择 ----------

const RANK = { user: 2, online: 1, builtin: 0 };
const SOURCE_NAME = { user: "自定义", online: "在线", builtin: "内置" };

function autoPick() {
  const day = today();
  const on = themes.filter((t) => isActive(t.when, day));
  // 优先级高的先；一样高时 自定义 > 在线 > 内置
  on.sort((a, b) => b.priority - a.priority || RANK[b.source] - RANK[a.source]);
  return on[0] || null;
}

function resolveCurrent() {
  if (mode === "off") return null;
  if (mode && mode !== "auto") {
    const t = themes.find((x) => x.id === mode);
    if (t) return t;
  }
  return autoPick();
}

// ---------- 应用 ----------

async function apply(theme, keepGreeting = false) {
  const seq = ++applySeq;
  const root = document.documentElement;

  // 先把所有资源解析好再一次性换上，不然会看到配色先变、图片后到
  let badgeUrl = null;
  let patternUrl = null;
  try {
    if (theme && theme.badge) badgeUrl = await assetUrl(theme, theme.badge.image);
    if (theme && theme.pattern) patternUrl = await assetUrl(theme, theme.pattern.image);
  } catch (_) {
    // 图片读不到就不显示图片，配色照常换
  }
  if (seq !== applySeq) return; // 期间又切了别的主题

  for (const k of appliedTokens) root.style.removeProperty(k);
  appliedTokens = [];
  if (theme) {
    for (const [k, v] of Object.entries(theme.tokens)) {
      root.style.setProperty(k, v);
      appliedTokens.push(k);
    }
  }

  els.queryInput.placeholder = (theme && theme.placeholder) || defaultPlaceholder;

  if (badgeUrl) {
    badgeEl.src = badgeUrl;
    badgeEl.alt = theme.badge.label;
    badgeEl.style.height = theme.badge.height + "px";
    badgeEl.classList.remove("hidden");
    els.anchorEl.classList.add("has-badge");
  } else {
    badgeEl.classList.add("hidden");
    badgeEl.removeAttribute("src");
    els.anchorEl.classList.remove("has-badge");
  }

  if (patternUrl) {
    patternEl.style.backgroundImage = `url("${patternUrl}")`;
    patternEl.style.backgroundSize = `${theme.pattern.size}px`;
    patternEl.style.opacity = theme.pattern.opacity;
    patternEl.classList.remove("hidden");
  } else {
    patternEl.classList.add("hidden");
    patternEl.style.backgroundImage = "";
  }

  if (theme) document.body.dataset.theme = theme.id;
  else delete document.body.dataset.theme;
  if (!keepGreeting) hideGreeting(false);
  current = theme;
  appliedActivationKey = themeActivationKey(theme, today());
  store(ACTIVE_THEME_KEY, appliedActivationKey);
}

// 首次进入节日：先让寄语/装饰出现，再过渡搜索框；同一节日期间保持配色。
async function activateTheme(theme, preview = false) {
  const key = themeActivationKey(theme, today());
  if (pendingActivation?.key === key) return pendingActivation.promise;
  if (pendingActivation) {
    ++activationSeq;
    ++applySeq;
    pendingActivation = null;
    document.documentElement.classList.remove("theme-changing");
    hideGreeting(false);
  }
  if (key === appliedActivationKey) {
    store(ACTIVE_THEME_KEY, key);
    if (preview) return playEnter(true, theme);
    return;
  }
  const seq = ++activationSeq;
  ++applySeq; // 取消先前尚未完成的资源加载。
  document.documentElement.classList.remove("theme-changing");
  hideGreeting(false);
  const promise = (async () => {
    const played = theme?.enter ? await playEnter(true, theme) : false;
    if (played) {
      const delay = theme.enter.message ? 160 + [...theme.enter.message].length * MESSAGE_STEP + 480 : 720;
      await new Promise((resolve) => setTimeout(resolve, delay));
    }
    if (seq !== activationSeq) return;
    const root = document.documentElement;
    if (played) {
      root.classList.add("theme-changing");
      void els.appEl.offsetWidth;
    }
    await apply(theme, played);
    if (played) await new Promise((resolve) => setTimeout(resolve, 540));
    if (seq === activationSeq) root.classList.remove("theme-changing");
  })();
  pendingActivation = { key, promise };
  try { await promise; }
  finally {
    if (pendingActivation?.promise === promise) pendingActivation = null;
  }
}

async function requestTheme(theme, preview = false) {
  if (!preview && document.documentElement.classList.contains("window-hidden")) {
    ++activationSeq;
    ++applySeq;
    pendingActivation = null;
    document.documentElement.classList.remove("theme-changing");
    // 后续几天重启也直接恢复节日外观；首次进入则等窗口唤起后再播开场。
    if (!theme?.enter || load(ACTIVE_THEME_KEY) === themeActivationKey(theme, today())) await apply(theme);
    return;
  }
  return activateTheme(theme, preview);
}

// ---------- 唤起特效 ----------

async function playEnter(force, t = current) {
  if (!t || !t.enter || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const e = t.enter;
  const key = FX_KEY + t.id;
  const day = today();

  if (!force) {
    const last = load(key);
    if (e.frequency === "once" && last) return;
    if (e.frequency === "daily" && last === day) return;
  }
  store(key, e.frequency === "once" ? "done" : day);

  let images = [];
  try {
    images = await Promise.all(e.images.map((f) => assetUrl(t, f)));
  } catch (_) {}
  const spec = { effect: e.effect, duration: e.duration, params: e.params };
  let greetingSpec = null;
  if (e.message) {
    let badge = null;
    let pattern = null;
    try {
      [badge, pattern] = await Promise.all([
        t.badge ? assetUrl(t, t.badge.image) : null,
        t.pattern ? assetUrl(t, t.pattern.image) : null,
      ]);
    } catch (_) {}
    const rect = els.appEl.getBoundingClientRect();
    const cs = getComputedStyle(document.documentElement);
    greetingSpec = {
      text: e.message, duration: e.duration,
      accent: t.tokens["--accent"] || cs.getPropertyValue("--accent").trim() || "#99601c",
      background: t.tokens["--bg"] || cs.getPropertyValue("--bg").trim() || "#fcf8ef",
      badge, pattern, ornaments: images,
      anchor: { left: rect.left, top: rect.top, width: rect.width },
    };
    showGreeting(e.message, e.duration);
  }

  if (e.scope === "screen" || greetingSpec) {
    try {
      await invoke("play_screen_fx", { payload: {
        ...spec, effect: e.scope === "screen" ? e.effect : null,
        images, greeting: greetingSpec,
      } });
      if (e.scope === "screen") return true;
    } catch (_) {
      // 覆盖窗口失败时仍可使用框内粒子，但不再用寄语遮盖输入框。
      hideGreeting(true);
    }
  }

  const canvas = document.createElement("canvas");
  canvas.className = "theme-fx";
  els.appEl.appendChild(canvas);
  runFx(canvas, spec, images, "window").finally(() => canvas.remove());
  return true;
}

// ---------- 唤起寄语 ----------
//
// 寄语由全屏透明层绘制在搜索框上方；主窗口只负责输入时提前收起。
// 不修改输入框文字、透明度或位置。
function showGreeting(text, duration) {
  if (greeting) clearTimeout(greeting.timer);
  const lifetime = Math.max(duration, 160 + [...text].length * MESSAGE_STEP + 480 + MESSAGE_HOLD);
  const timer = setTimeout(() => hideGreeting(false), lifetime + 500);
  greeting = { timer };
  window.addEventListener("keydown", dismissGreeting, true);
  window.addEventListener("pointerdown", dismissGreeting, true);
}

function dismissGreeting() {
  hideGreeting(false);
}

function hideGreeting(instant) {
  window.removeEventListener("keydown", dismissGreeting, true);
  window.removeEventListener("pointerdown", dismissGreeting, true);
  if (!greeting) return;
  clearTimeout(greeting.timer);
  greeting = null;
  invoke("dismiss_theme_greeting", { instant: Boolean(instant) }).catch(() => {});
}

// ---------- 对外接口 ----------

// main.js 启动时调一次，把要改的几个元素传进来
export async function initThemes(elements) {
  els = elements;
  if (!defaultPlaceholder) defaultPlaceholder = els.queryInput.placeholder;

  // 徽标：替代展开按钮里的箭头显示，类似 Google 的节日涂鸦。
  // 放在按钮内部，点击照常展开 / 收起；悬停时换回箭头，提示按钮的作用
  badgeEl = document.createElement("img");
  badgeEl.id = "theme-badge";
  badgeEl.className = "hidden";
  badgeEl.draggable = false;
  badgeEl.setAttribute("aria-hidden", "true");
  els.anchorEl.appendChild(badgeEl);

  // 底纹：铺在整个搜索框背后、内容下面
  patternEl = document.createElement("div");
  patternEl.id = "theme-pattern";
  patternEl.className = "hidden";
  els.appEl.insertBefore(patternEl, els.appEl.firstChild);

  await reload();
  await requestTheme(resolveCurrent());

  // 后台同步到新的在线主题：丢掉旧图片缓存，重新读取并按当前模式应用
  listen("themes-changed", refreshOnline);
  listen("window-hiding", dismissGreeting);
  listen("window-hidden", dismissGreeting);
}

async function refreshOnline() {
  for (const k of [...assetCache.keys()]) if (k.startsWith("online/")) assetCache.delete(k);
  await reload();
  const next = resolveCurrent();
  if (next?.id === current?.id && !pendingActivation) await apply(next);
  else await requestTheme(next);
}

// 配置文件里的主题模式变了（手改 config.toml，或 /theme 写进去之后回推过来）
export function setThemeMode(m) {
  if (m === mode) return;
  mode = m;
  if (els) requestTheme(resolveCurrent());
}

// 配置文件里的默认占位语。主题自带占位语时以主题为准
export function setDefaultPlaceholder(p) {
  defaultPlaceholder = p;
  if (!(current && current.placeholder)) {
    const input = els ? els.queryInput : document.getElementById("query-input");
    if (input) input.placeholder = p;
  }
}

// 给命令提示用：当前模式和所有主题
export function listThemes() {
  return { mode, themes: themes.map((t) => ({ id: t.id, name: t.name, when: t.when, source: t.source })) };
}

// /theme 改模式：写进 config.toml，成功了再换
async function saveMode(m) {
  await invoke("set_theme_mode", { mode: m });
  mode = m;
}

// 每次窗口唤起时调：日期可能已经变了（程序常驻，跨过午夜），主题目录也可能加了新东西
export async function onWindowShown() {
  if (!els) return;
  if (Date.now() - loadedAt > RELOAD_MS) await reload();
  const t = resolveCurrent();
  if (pendingActivation || themeActivationKey(t, today()) !== appliedActivationKey) {
    await activateTheme(t);
  } else {
    playEnter(false);
  }
}

// /theme sync：立即同步在线主题（后台也会定期同步）
async function syncCommand() {
  let r;
  try {
    r = await invoke("sync_themes");
  } catch (err) {
    return `在线主题同步失败：${err}`;
  }
  // 有变化时 Rust 会发 themes-changed，这里再刷新一次，确保返回的列表是最新的
  await refreshOnline();
  const online = themes.filter((t) => t.source === "online");
  let md = r.changed ? `在线主题已更新，共 ${r.count} 个。` : `在线主题已是最新，共 ${r.count} 个。`;
  if (online.length) md += "\n\n" + online.map(describe).join("\n");
  if (r.failed.length) md += "\n\n**未能同步的主题**（将在下次同步时重试）\n\n" + r.failed.map((f) => `- ${f}`).join("\n");
  if (current) md += `\n\n当前主题：「${current.name}」`;
  return md;
}

function describe(t) {
  const when = t.when ? `${t.when.from} ~ ${t.when.to}` : "手动启用";
  const src = SOURCE_NAME[t.source] || t.source;
  const fx = t.enter ? ` · ${{ fireworks: "烟花", particles: "粒子" }[t.enter.effect] || t.enter.effect}（${t.enter.scope === "screen" ? "全屏" : "窗口内"}）` : "";
  return `- \`${t.id}\` ${t.name} · ${src} · ${when}${fx}`;
}

// /theme 命令，返回要显示的 Markdown
export async function themeCommand(arg) {
  const a = arg.trim().toLowerCase();
  if (a === "sync") return syncCommand();
  await reload();

  try {
    if (a === "auto" || a === "off") await saveMode(a);
  } catch (err) {
    return `切换失败：${err}`;
  }
  if (a === "auto") {
    await requestTheme(resolveCurrent());
    return current ? `已恢复按日期自动切换，今日主题：「${current.name}」。` : "已恢复按日期自动切换，今日无节日主题。";
  }
  if (a === "off") {
    await activateTheme(null);
    return "已关闭主题，使用默认外观。使用 `/theme auto` 恢复按日期自动切换。";
  }
  if (a) {
    const t = themes.find((x) => x.id === a || x.name.toLowerCase() === a);
    if (!t) return `未找到主题 \`${arg.trim()}\`，使用 \`/theme\` 查看全部主题`;
    try {
      await saveMode(t.id);
    } catch (err) {
      return `切换失败：${err}`;
    }
    await activateTheme(t, true);
    const warn = t.warnings.length ? "\n\n" + t.warnings.map((w) => `- ${w}`).join("\n") : "";
    return `已切换至主题「${t.name}」，将持续生效。使用 \`/theme auto\` 恢复按日期自动切换。${warn}`;
  }

  const modeText = mode === "off" ? "已关闭" : mode && mode !== "auto" ? "手动指定" : "按日期自动";
  let dir = "";
  try {
    dir = await invoke("themes_dir");
  } catch (_) {}

  let md = `**主题** · ${modeText} · 当前：${current ? `「${current.name}」` : "默认外观"}\n\n`;
  md += themes.length ? themes.map(describe).join("\n") : "暂无主题。";
  if (problems.length) {
    md += "\n\n**加载失败的主题**\n\n" + problems.map((p) => `- \`${p.id}\`：${p.reason}`).join("\n");
  }
  md += "\n\n`/theme 名称` 切换并预览，`/theme auto` 按日期自动切换，`/theme off` 关闭主题，`/theme sync` 同步在线主题。当前设置保存在配置文件的 `[theme] mode` 中";
  if (dir) md += `\n\n自定义主题目录：\`${dir}\`（每个主题一个文件夹，格式参见 THEME_SPEC.md）`;
  return md;
}
