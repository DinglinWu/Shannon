// 命令输入时的实时提示。
//
// 1. 下拉说明：输入框下面展开一块，输入 / 时列出能用的命令和作用，↑↓ 选、Tab 补全、回车确认；
//    命令认出来以后换成这条命令的用法，正在填的那个参数加粗，/spt、/theme 还会列出可选项。
// 2. 参数提示：像 Excel 输公式时那样，在输入框里光标后面用灰字标出还差哪些参数。
//    灰字画在命令高亮用的镜像层里（见 main.js 的 syncMirror），textarea 本身不动。
//
// 新加命令时在 COMMANDS 里登记一条，提示就自动有了。

import { invoke } from "./invoke.js";
import { listThemes } from "./theme.js";
import { spring } from "./spring.js";
import { mentionContext, mentionCandidates, replaceMention } from "./plugin-mentions.js";

// ---------- 命令表 ----------
// sig(已输入的参数) 返回参数列表：label 显示名；optional 可省略（显示成 [名称]）；
// rest 表示它吃掉后面所有内容（路径里有空格也算一个参数）

let clockActive = false;
let mentionPlugins = [];
export function setMentionTools(items) { mentionPlugins = items; }

export const COMMANDS = [
  {
    name: "plugin",
    aliases: ["plugins", "插件"],
    desc: "安装、管理第三方工具插件；启用后支持 AI 直调",
    sig: () => [{ label: "list | install ZIP路径/HTTPS地址 | info/enable/disable/uninstall ID | run ID/工具名 {JSON} | help", optional: true, rest: true }],
  },
  {
    name: "component",
    aliases: ["components", "组件"],
    desc: "按需安装语音或手势组件；默认安装包不包含模型与运行库",
    sig: () => [{ label: "status | install voice | install gesture | help", optional: true, rest: true }],
  },
  {
    name: "voice",
    aliases: ["语音"],
    desc: "install 下载离线语音组件；安装后在输入框内按住左键说话",
    sig: () => [{ label: "install" }],
  },
  {
    name: "note",
    aliases: ["便签"],
    desc: "记下便签，点击文字划去或恢复；不填文字时打开，--close 退出并保留内容",
    sig: () => [{ label: "便签内容", optional: true, rest: true }],
  },
  {
    name: "gesture",
    aliases: ["手势"],
    desc: "手势快捷操作/鼠标模式；install 安装可选组件，help 查看用法",
    sig: () => [{ label: "install | start | stop | status | pause | resume | preview | cameras | config | bind | restart | help", optional: true, rest: true }],
  },
  {
    name: "clock",
    desc: "创建正计时或倒计时；开始后吸附屏幕顶部，悬停可暂停、关闭",
    sig: (t) => t[0] === "restart"
      ? [{ label: "restart" }, { label: "up | 时长（如 5m）", optional: true }]
      : [{ label: `${clockActive ? "restart | " : ""}up | 时长（如 5m）`, optional: true }],
  },
  {
    name: "o",
    desc: "打开应用、文件夹、文件或网址；不带参数时列出全部别名",
    sig: () => [{ label: "名称或路径", optional: true, rest: true }],
  },
  {
    name: "rmbr",
    desc: "为应用、文件夹或网址添加别名，之后可通过 /o 别名 打开",
    sig: (t) =>
      t[0] === "-d"
        ? [{ label: "-d" }, { label: "要删除的别名" }]
        : [{ label: "别名" }, { label: "目标", rest: true }],
  },
  {
    name: "spt",
    desc: "切换系统提示词：0 为不使用，1 为默认，edit 编辑提示词文件",
    sig: () => [{ label: "序号 | edit", optional: true }],
  },
  {
    name: "theme",
    desc: "切换主题：主题名、auto（按日期自动）、off（关闭）、sync（同步在线主题）",
    sig: () => [{ label: "主题名 | auto | off | sync", optional: true }],
  },
  {
    name: "update",
    desc: "检查新版本；install 下载并安装",
    sig: () => [{ label: "install", optional: true }],
  },
  {
    name: "config",
    desc: "打开配置文件，保存后立即生效",
    sig: () => [],
  },
  {
    name: "?",
    aliases: ["？", "help"],
    desc: "使用说明",
    sig: () => [],
  },
  {
    name: "version",
    aliases: ["v"],
    desc: "版本信息与更新日志",
    sig: () => [],
  },
];

let disabledBuiltins=new Set();
export function setBuiltinAvailability(items){disabledBuiltins=new Set(items.filter(p=>p.status!=='enabled').map(p=>p.id));}
function available(c){return !disabledBuiltins.has(c.name==='component'?null:c.name);}
const findCommand = (n) => {
  const k = n.toLowerCase();
  return COMMANDS.find((c) => available(c)&&(c.name === k || (c.aliases || []).includes(k)));
};

export function setPluginCommands(plugins) {
  mentionPlugins = plugins;
  for (let i = COMMANDS.length - 1; i >= 0; i--) if (COMMANDS[i].pluginCommand) COMMANDS.splice(i, 1);
  for (const plugin of plugins.filter(p => p.enabled)) {
    for (const tool of plugin.manifest.tools) {
      COMMANDS.push({
        pluginCommand: true,
        name: `${plugin.manifest.id}/${tool.name}`,
        desc: `${plugin.manifest.name} · ${tool.description}`,
        sig: () => [{label:'JSON 参数',optional:!tool.parameters.required?.length,rest:true}],
      });
    }
  }
}

const label = (p) => (p.optional ? `[${p.label}]` : p.label);

const esc = (s) =>
  String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

// ---------- 解析输入 ----------

// 返回：{ kind: "name", partial } 还在打命令名；
//       { kind: "args", cmd, rest } 命令已认出，rest 是命令名后面的全部（以空白开头）；
//       null 不是命令
function parse(v) {
  const m = v.match(/^[\/／](\S*)([\s\S]*)$/);
  if (!m) return null;
  const [, partial, rest] = m;
  if (rest === "") return { kind: "name", partial };
  const cmd = findCommand(partial);
  return cmd ? { kind: "args", cmd, rest } : null;
}

// 现在正在填第几个参数
function argState(cmd, rest) {
  const body = rest.replace(/^\s+/, "");
  const tokens = body.split(/\s+/).filter(Boolean);
  const params = cmd.sig(tokens);
  let idx;
  let typing; // true：光标还在这个参数里；false：这个参数还一个字都没打
  if (body === "") {
    idx = 0;
    typing = false;
  } else if (/\s$/.test(rest)) {
    idx = tokens.length;
    typing = false;
  } else {
    idx = tokens.length - 1;
    typing = true;
  }
  const r = params.findIndex((p) => p.rest);
  if (r !== -1 && idx > r) {
    idx = r;
    typing = true;
  }
  return { params, idx, typing, tokens };
}

// 光标后面的灰字：还差哪些参数。不是命令、或者参数都齐了就返回空串
export function ghostText(v) {
  const p = parse(v);
  if (!p) return "";
  if (p.kind === "name") {
    const c = findCommand(p.partial);
    const params = c ? c.sig([]) : [];
    return params.length ? " " + params.map(label).join(" ") : "";
  }
  const { params, idx, typing } = argState(p.cmd, p.rest);
  const remaining = params.slice(typing ? idx + 1 : idx);
  if (!remaining.length) return "";
  return (typing ? " " : "") + remaining.map(label).join(" ");
}

// ---------- 下拉说明 ----------

let els = null;
let hintEl = null;
let bodyEl = null; // 每次重画的是它；高亮块在它外面，重画时不丢，才能从上一个位置滑过去
let pillEl = null;
let pill = null;   // 高亮块的上下两条边
let barEl = null;  // 用法那一行里，正在填的参数下面那道强调色横线，换参数时滑过去
let bar = null;    // 横线的左右两端

// ---------- 黏滞的移动 ----------
//
// 两端各用一个弹簧：往哪边走，哪边的那条边（前沿）先动、走得快，到位时带一点点回弹；
// 另一条边（后沿）先"粘"在原地一小会儿，再慢慢被拽过去。
// 于是移动途中整块被拉长，像一滴黏稠的东西被拖过去，最后才收拢成原来的大小。

const LEAD = { response: 0.12, bounce: 0.15 }; // 前沿：快，落地时轻轻一弹
const TRAIL = { response: 0.15, bounce: 0 };   // 后沿：只比前沿慢一点点，不弹
const TRAIL_DELAY = 0;                         // 后沿粘住不动的毫秒数（0：同时出发）

function stretchy(paint) {
  let a = 0; // 起始边（上 / 左）
  let b = 0; // 结束边（下 / 右）
  let ta = null; // 两条边当前的目标
  let tb = null;
  let timer = 0;
  const sa = spring({ ...TRAIL, onUpdate: (v) => ((a = v), paint(a, b)) });
  const sb = spring({ ...TRAIL, onUpdate: (v) => ((b = v), paint(a, b)) });
  return {
    to(na, nb, immediate = false) {
      clearTimeout(timer);
      if (immediate || ta === null) {
        ta = na;
        tb = nb;
        sa.to(na, true);
        sb.to(nb, true);
        return;
      }
      if (na === ta && nb === tb) return;
      const forward = na + nb > ta + tb; // 往下 / 往右走
      ta = na;
      tb = nb;
      const [lead, trail, nl, nt] = forward ? [sb, sa, nb, na] : [sa, sb, na, nb];
      lead.to(nl, false, LEAD);
      timer = setTimeout(() => trail.to(nt, false, TRAIL), TRAIL_DELAY);
    },
    reset() {
      clearTimeout(timer);
      ta = tb = null;
      sa.reset();
      sb.reset();
    },
  };
}
let candidates = []; // 打命令名时的候选
let active = 0;
let mode = null;      // "name" 在打命令名；"args" 命令已认出，在填参数
let sel = -1;         // 下拉里当前选中的行（-1 表示没选）
let navigated = false; // 用户按过 ↑↓ 挑过行
let renderSeq = 0;
let promptCache = null;
let promptAt = 0;

async function loadPrompts() {
  // 短时间内连续打字不要每个字都去读一次文件
  if (!promptCache || Date.now() - promptAt > 3000) {
    try {
      promptCache = await invoke("list_prompts");
    } catch (_) {
      promptCache = { active: 0, items: [], error: null };
    }
    promptAt = Date.now();
  }
  return promptCache;
}

// 刚用 /spt 切换过，下次显示要重新读
export function invalidatePrompts() {
  promptCache = null;
}

function row({ fill, name, args = "", desc = "", tag = "", on = false }) {
  return (
    `<div class="hint-row${on ? " active" : ""}" data-fill="${esc(fill)}">` +
    `<span class="hint-name">${esc(name)}</span>` +
    (args ? `<span class="hint-args">${esc(args)}</span>` : "") +
    `<span class="hint-desc">${esc(desc)}</span>` +
    (tag ? `<span class="hint-tag">${esc(tag)}</span>` : "") +
    `</div>`
  );
}

function signature(cmd, st) {
  const parts = st.params.map((p, i) => {
    const t = esc(label(p));
    return i === st.idx ? `<span class="cur">${t}</span>` : t;
  });
  return `<div class="hint-sig"><span class="cmd">/${esc(cmd.name)}</span> ${parts.join(" ")}</div>`;
}

async function extras(cmd, st) {
  const typed = (st.tokens[0] || "").toLowerCase();

  if (cmd.name === "clock") {
    return [
      { name: "restart", desc: "结束当前计时器，重新打开设置" },
      { name: "up", desc: "正计时" },
    ].filter((item) => (item.name !== "restart" || clockActive) && (!typed || item.name.startsWith(typed)))
      .map((item) => row({ fill: `/clock ${item.name}`, name: item.name, desc: item.desc, on: Boolean(typed) }))
      .join("");
  }

  if (cmd.name === "spt") {
    const list = await loadPrompts();
    let html = row({ fill: "/spt 0", name: "0", desc: "不使用", tag: list.active === 0 ? "当前" : "", on: typed === "0" });
    for (const p of list.items) {
      html += row({
        fill: `/spt ${p.id}`,
        name: String(p.id),
        args: p.name,
        desc: p.preview,
        tag: list.active === p.id ? "当前" : "",
        on: typed === String(p.id),
      });
    }
    html += row({ fill: "/spt edit", name: "edit", desc: "编辑提示词文件 prompts.toml", on: typed === "edit" });
    if (list.error) {
      html += `<div class="hint-err">提示词文件有误，当前沿用上次有效的内容：${esc(list.error.split("\n")[0])}</div>`;
    }
    return html;
  }

  if (cmd.name === "update") {
    let info = null;
    try {
      info = await invoke("pending_update");
    } catch (_) {}
    return row({
      fill: "/update install",
      name: "install",
      desc: "下载并安装新版本，完成后自动重新启动",
      tag: info ? `v${info.version} 可用` : "",
      on: typed === "install",
    });
  }

  if (cmd.name === "theme") {
    const { mode, themes } = listThemes();
    const tag = (m) => (mode === m ? "当前" : "");
    let html = row({ fill: "/theme auto", name: "auto", desc: "按日期自动启用节日主题", tag: tag("auto"), on: typed === "auto" });
    html += row({ fill: "/theme off", name: "off", desc: "关闭主题，使用默认外观", tag: tag("off"), on: typed === "off" });
    html += row({ fill: "/theme sync", name: "sync", desc: "立即同步在线主题", tag: "", on: typed === "sync" });
    for (const t of themes) {
      const when = t.when ? `${t.when.from} ~ ${t.when.to}` : "手动启用";
      const desc = t.source === "online" ? `${when} · 在线` : when;
      html += row({ fill: `/theme ${t.id}`, name: t.id, args: t.name, desc, tag: tag(t.id), on: typed === t.id });
    }
    return html;
  }

  return "";
}

function hide() {
  mode = null;
  sel = -1;
  navigated = false;
  if (!hintEl || hintEl.classList.contains("hidden")) return;
  hintEl.classList.add("hidden");
  bodyEl.innerHTML = "";
  candidates = [];
  movePill();
  moveBar();
  els.onChange();
}

const rowEls = () => [...bodyEl.querySelectorAll(".hint-row")];

let pillRow = { left: 0, width: 0, height: 0 };
let barLen = 0;

// 选中行的底色是一块单独的高亮块，换行时用弹簧滑过去（冲过头一点再落定）
function movePill(immediate = false) {
  const r = rowEls()[sel];
  if (!r) {
    pillEl.classList.add("off");
    pill.reset();
    return;
  }
  pillEl.classList.remove("off");
  pillRow = { left: r.offsetLeft, width: r.offsetWidth, height: r.offsetHeight };
  pill.to(r.offsetTop, r.offsetTop + r.offsetHeight, immediate);
}

// 参数下划线：跟着"正在填的参数"走，从上一个参数滑到下一个，宽度一起变
function moveBar(immediate = false) {
  const cur = bodyEl.querySelector(".hint-sig .cur");
  if (!cur) {
    barEl.classList.add("off");
    bar.reset();
    return;
  }
  barEl.classList.remove("off");
  barEl.style.top = cur.offsetTop + cur.offsetHeight + 1 + "px";
  barLen = cur.offsetWidth;
  bar.to(cur.offsetLeft, cur.offsetLeft + cur.offsetWidth, immediate);
}

function setSel(i) {
  const rows = rowEls();
  rows.forEach((r, k) => r.classList.toggle("active", k === i));
  sel = i;
  if (rows[i]) rows[i].scrollIntoView({ block: "nearest" });
  movePill();
}

export async function updateCmdHint() {
  if (!els) return;
  const seq = ++renderSeq;
  const v = els.queryInput.value;
  const q = els.queryInput;
  const mention = !/^[\/／]/.test(v) && q.selectionStart === q.selectionEnd ? mentionContext(v,q.selectionStart) : null;
  if (mention) {
    const items = mentionCandidates(mentionPlugins,mention.query);
    candidates = items;
    const wasHidden = hintEl.classList.contains("hidden");
    mode = "mention";
    navigated = false;
    bodyEl.innerHTML = items.length ? items.map((item,i) => row({
      fill:item.fill,name:item.name,args:item.label,desc:item.description,tag:item.tag,on:i===0,
    })).join('') + '<div class="hint-foot">↑↓ 选择 · Tab / Enter 插入 · 接着输入你的需求</div>'
      : '<div class="hint-desc-line">' + (mentionPlugins.some(p=>p.enabled)
        ? '没有匹配的工具' : '工具目录尚未就绪，请稍后重试') + '</div>';
    hintEl.classList.remove("hidden");
    sel = items.length ? 0 : -1;
    movePill(wasHidden);
    moveBar(wasHidden);
    els.onChange();
    return;
  }
  const p = v.includes("\n") ? null : parse(v);
  if (!p) return hide();

  if ((p.kind === "name" && "clock".startsWith(p.partial.toLowerCase())) ||
      (p.kind === "args" && p.cmd.name === "clock")) {
    let active = false;
    try { active = await invoke("clock_active"); } catch (_) {}
    if (seq !== renderSeq) return;
    clockActive = active;
  }

  let html = "";
  if (p.kind === "name") {
    const k = p.partial.toLowerCase();
    candidates = COMMANDS.filter(
      (c) => available(c)&&(c.name.startsWith(k) || (c.aliases || []).some((a) => a.startsWith(k)))
    );
    if (!candidates.length) return hide();
    // 完全对上的排第一个
    const exact = candidates.findIndex((c) => findCommand(k) === c);
    active = exact === -1 ? 0 : exact;
    html = candidates
      .map((c, i) =>
        row({
          fill: `/${c.name}${c.sig([]).length ? " " : ""}`,
          name: `/${c.name}`,
          args: c.sig([]).map(label).join(" "),
          desc: c.desc,
          on: i === active,
        })
      )
      .join("");
    html += `<div class="hint-foot">↑↓ 选择 · Tab 补全 · Enter 确认</div>`;
  } else {
    candidates = [];
    const st = argState(p.cmd, p.rest);
    html = signature(p.cmd, st) + `<div class="hint-desc-line">${esc(p.cmd.desc)}</div>`;
    const more = await extras(p.cmd, st);
    if (more) html += more + `<div class="hint-foot">↑↓ 选择 · Enter 确认</div>`;
  }

  if (seq !== renderSeq) return; // 等数据期间又打了字
  const wasHidden = hintEl.classList.contains("hidden");
  mode = p.kind;
  navigated = false;
  bodyEl.innerHTML = html;
  hintEl.classList.remove("hidden");
  // 渲染时标了 active 的行（候选里的第一个，或者和已输入内容对上的那一项）就是当前选中
  sel = rowEls().findIndex((r) => r.classList.contains("active"));
  // 刚冒出来时直接到位；已经开着、只是内容变了，就从原来的位置滑过去
  movePill(wasHidden);
  moveBar(wasHidden);
  els.onChange();
}

function fill(text) {
  const q = els.queryInput;
  const mention = mode === "mention" ? replaceMention(q.value,q.selectionStart,text) : null;
  q.value = mention ? mention.text : text;
  const caret = mention ? mention.caret : text.length;
  q.setSelectionRange(caret, caret);
  q.focus();
  // 走一遍 input 事件：输入框长高、命令高亮、提示刷新都在那里
  q.dispatchEvent(new Event("input", { bubbles: true }));
}

// elements：{ queryInput, dockEl, beforeEl, onChange }，onChange 在提示出现 / 消失 / 变高时调，
// 用来让窗口跟着改高度
export function initCmdHint(elements) {
  els = elements;
  hintEl = document.createElement("div");
  hintEl.id = "cmd-hint";
  hintEl.className = "hidden";
  pillEl = document.createElement("div");
  pillEl.className = "hint-pill off";
  barEl = document.createElement("div");
  barEl.className = "hint-bar off";
  bodyEl = document.createElement("div");
  hintEl.append(pillEl, bodyEl, barEl);
  els.dockEl.insertBefore(hintEl, els.beforeEl);

  // 高亮块：被拉长时两侧往里收一点（最多 3px），像拉长的液滴中间变细
  pill = stretchy((top, bottom) => {
    const h = Math.max(0, bottom - top);
    const neck = Math.min(1.5, Math.max(0, h - pillRow.height) * 0.05);
    pillEl.style.transform = `translateY(${top}px)`;
    pillEl.style.height = h + "px";
    pillEl.style.left = pillRow.left + neck + "px";
    pillEl.style.width = Math.max(0, pillRow.width - 2 * neck) + "px";
  });

  // 参数下划线：被拉长时稍微变细一点
  bar = stretchy((left, right) => {
    const w = Math.max(0, right - left);
    const thin = Math.min(0.8, Math.max(0, w - barLen) / 40);
    barEl.style.transform = `translateX(${left}px)`;
    barEl.style.width = w + "px";
    barEl.style.height = 2 - thin + "px";
  });

  // mousedown 而不是 click：不让输入框失焦
  hintEl.addEventListener("mousedown", (e) => {
    const r = e.target.closest(".hint-row");
    if (!r) return;
    e.preventDefault();
    fill(r.dataset.fill);
  });

  // 捕获阶段注册：先于 main.js 里的回车发送、↑↓ 翻历史处理
  els.queryInput.addEventListener("keydown", onKeyDown, true);
  els.queryInput.addEventListener("click", () => updateCmdHint());
  els.queryInput.addEventListener("keyup", e => {
    if (["ArrowLeft","ArrowRight","Home","End"].includes(e.key)) updateCmdHint();
  });
}

function onKeyDown(e) {
  if (e.isComposing || !mode || hintEl.classList.contains("hidden")) return;
  if (mode === "mention" && e.key === "Escape") {
    e.preventDefault(); e.stopImmediatePropagation(); hide(); return;
  }
  const rows = rowEls();
  if (!rows.length) return;
  const plain = !e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey;
  const stop = () => {
    e.preventDefault();
    e.stopImmediatePropagation();
  };

  // ↑↓ 在下拉里挑行，到头了绕回另一头。
  // 正在用 ↑↓ 翻输入历史时（翻出来的恰好是条命令）不接管，让历史接着往前翻
  if ((e.key === "ArrowDown" || e.key === "ArrowUp") && plain) {
    if (els.isBrowsingHistory && els.isBrowsingHistory()) return;
    stop();
    const n = rows.length;
    const i = e.key === "ArrowDown" ? (sel < 0 ? 0 : (sel + 1) % n) : sel < 0 ? n - 1 : (sel - 1 + n) % n;
    setSel(i);
    navigated = true;
    return;
  }

  if (sel < 0) return;
  const target = rows[sel].dataset.fill;

  if (e.key === "Tab" && plain) {
    stop();
    fill(target);
    return;
  }

  if (e.key !== "Enter" || e.shiftKey) return;

  if (mode === "mention") { stop(); fill(target); return; }

  if (mode === "name") {
    const typed = els.queryInput.value.replace(/^[\/／]/, "").trim().toLowerCase();
    const c = candidates[sel];
    // 已经完整打出了选中的这条命令：照常执行
    if (c && findCommand(typed) === c) return;
    if (c && c.sig([]).length) {
      // 要参数的命令：补全后停下来，等着填参数
      stop();
      fill(target);
    } else {
      // 不要参数的（/config、/?、/version）：补全后直接执行
      els.queryInput.value = target.trim();
    }
    return;
  }

  // 填参数时用 ↑↓ 挑了一项（比如 /spt 的某条提示词）：换成那一项，直接执行
  if (navigated) els.queryInput.value = target.trim();
}

