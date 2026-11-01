import { gestureHelp, parseGestureCommand, formatGestureResult } from "./gesture.js";
import { invoke } from "./invoke.js";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { renderMarkdown } from "./markdown.js";
import "katex/dist/katex.min.css";
import hljs from "highlight.js/lib/core";
import javascript from "highlight.js/lib/languages/javascript";
import typescript from "highlight.js/lib/languages/typescript";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import bash from "highlight.js/lib/languages/bash";
import json from "highlight.js/lib/languages/json";
import xml from "highlight.js/lib/languages/xml";
import css from "highlight.js/lib/languages/css";
import sql from "highlight.js/lib/languages/sql";
import yaml from "highlight.js/lib/languages/yaml";
import cpp from "highlight.js/lib/languages/cpp";
import java from "highlight.js/lib/languages/java";
import go from "highlight.js/lib/languages/go";
import "highlight.js/styles/github.css";
import {
  initThemes,
  onWindowShown as themeOnShown,
  themeCommand,
  setThemeMode,
  setDefaultPlaceholder,
} from "./theme.js";
import { initCmdHint, updateCmdHint, ghostText, invalidatePrompts, setPluginCommands, setBuiltinAvailability, setMentionTools } from "./cmdhint.js";
import { parseDuration } from "./clock-model.js";
import { initVoiceInput } from "./voice-input.js";
import { calendarFold, prepareCalendarFront, settleCalendarFold } from "./calendar-fold.js";
import { parseComponentCommand, componentHelp, componentProgress, formatComponents } from "./components.js";
import { parsePluginCommand, pluginHelp, pluginValue, formatPluginList, formatPluginInfo, formatBuiltinResult } from "./plugins.js";
import { initPluginRuntime } from "./plugins-runtime.js";
import { parsePluginMentions, selectedMentionRanges, removeMention, spaceMentionControls } from "./plugin-mentions.js";

document.querySelector('#plugins-manager-btn').onclick=()=>invoke('plugin_manager_open').catch(error=>showLocal(String(error)));
const pluginRuntimeReady = initPluginRuntime({invoke, listen});
let mentionPlugins = [];
let voiceFeatureEnabled=true;
pluginRuntimeReady.catch(error => console.error('插件运行环境未就绪', error));
async function refreshPluginCommands() {
  const data = await invoke('mention_catalog');
  mentionPlugins = data.items || [];
  setPluginCommands(mentionPlugins.filter(p=>!p.builtin));
  const catalog=await invoke('plugin_manager_catalog');
  setBuiltinAvailability(catalog.builtins);
  voiceFeatureEnabled=catalog.builtins.find(p=>p.id==='voice')?.status==='enabled';
  setMentionTools(mentionPlugins);
  updateCmdHint();
}
refreshPluginCommands().catch(() => {});
let pluginPanelActive = false;
let pluginCommandEpoch = 0;
listen('plugin-changed', ({payload}) => {
  refreshPluginCommands().catch(() => {});
  if (!pluginPanelActive) return;
  showLocal(payload.cancelled ? '已取消插件操作。' : payload.error
    ? '插件操作失败：\n\n' + pluginValue(payload.error)
    : '插件操作已完成：\n\n' + pluginValue(payload));
});

let componentPanelActive = false;
let lastComponentProgress = '';
let componentProgressRow = null;
listen('component-progress', ({ payload }) => {
  if (!componentPanelActive) return;
  const tick = `${payload.name}:${payload.phase}:${Math.floor((payload.total ? payload.received / payload.total : 0) * 20)}`;
  if (tick === lastComponentProgress) return;
  lastComponentProgress = tick;
  const md = componentProgress(payload);
  // Update one status row; never clear text the user is currently typing.
  if (expanded) {
    if (!componentProgressRow || !messages.includes(componentProgressRow)) {
      componentProgressRow = { role: 'help', content: md };
      messages.push(componentProgressRow);
    } else componentProgressRow.content = md;
    renderChat();
  } else {
    fullText = md;
    renderResult(md);
    resultEl.classList.remove('hidden');
    scheduleResize();
  }
});

// 按需注册语言。highlight.js 全量包一兆多，只挑常用的能砍到十分之一。
// 每个语言模块自带别名（js / py / sh / html / c++ 等），不用另外登记
[
  ["javascript", javascript],
  ["typescript", typescript],
  ["python", python],
  ["rust", rust],
  ["bash", bash],
  ["json", json],
  ["xml", xml],
  ["css", css],
  ["sql", sql],
  ["yaml", yaml],
  ["cpp", cpp],
  ["java", java],
  ["go", go],
].forEach(([name, lang]) => hljs.registerLanguage(name, lang));

const appWindow = getCurrentWindow();

const appEl = document.getElementById("app");
const queryInput = document.getElementById("query-input");
initVoiceInput(queryInput, invoke, listen,()=>voiceFeatureEnabled);
const settingsBtn = document.getElementById("settings-btn");
const settingsPanel = document.getElementById("settings-panel");
const apiKeyInput = document.getElementById("api-key-input");
const proxyCheckbox = document.getElementById("proxy-checkbox");
const keyEye = document.getElementById("key-eye");
const autostartCheckbox = document.getElementById("autostart-checkbox");
const deepCheckbox = document.getElementById("deep-checkbox");
const modelSelect = document.getElementById("model-select");
const modelStatus = document.getElementById("model-status");
let savedModel = "deepseek-flash";
function applyModelSelection(model) {
  savedModel = model || "deepseek-flash";
  modelSelect.querySelector('[data-custom]')?.remove();
  const display = savedModel === "deepseek-v4-flash" ? "deepseek-flash" : savedModel;
  if (!["deepseek-flash", "deepseek-v4-pro"].includes(display)) {
    const option = new Option(`自定义 · ${display}`, display);
    option.dataset.custom = "true";
    modelSelect.add(option);
  }
  modelSelect.value = display;
}
modelSelect.addEventListener("change", async () => {
  const previous = savedModel;
  const value = modelSelect.value;
  if (!["deepseek-flash", "deepseek-v4-pro"].includes(value)) return;
  modelSelect.disabled = true;
  modelStatus.hidden = true;
  try {
    await invoke("set_ai_model", { value });
    applyModelSelection(value);
  } catch (err) {
    applyModelSelection(previous);
    modelStatus.textContent = `模型切换失败：${String(err)}`;
    modelStatus.hidden = false;
  } finally {
    modelSelect.disabled = false;
  }
});
const searchCheckbox = document.getElementById("search-checkbox");
const topRowEl = document.getElementById("top-row");
const dockEl = document.getElementById("dock");
const rippleEl = document.getElementById("drop-ripple");
const resultEl = document.getElementById("result");
const expandBtn = document.getElementById("expand-btn");
const sessionListEl = document.getElementById("session-list");
const chatViewEl = document.getElementById("chat-view");
const gripEl = document.getElementById("resize-grip");
const moveGripEl = document.getElementById("move-grip");
const oBorderL = document.getElementById("o-border-l");
const oBorderR = document.getElementById("o-border-r");

// 输入框的镜像层，用来把构成命令的整行染成强调色，见 style.css 里 #input-mirror
const mirrorEl = document.createElement("div");
mirrorEl.id = "input-mirror";
mirrorEl.className = "hidden";
mirrorEl.setAttribute("aria-hidden", "true");
queryInput.parentNode.insertBefore(mirrorEl, queryInput);
const mentionDismiss=document.createElement('div');
mentionDismiss.id='mention-dismiss';
queryInput.parentNode.append(mentionDismiss);
function placeMentionDismiss(){
  const parent=queryInput.parentNode.getBoundingClientRect();
  const viewport=mirrorEl.getBoundingClientRect();
  const labels=[...mirrorEl.querySelectorAll('.mention-text')];
  [...mentionDismiss.children].forEach((button,i)=>{
    const rects=labels[i]?.getClientRects(),rect=rects?.[rects.length-1];
    button.hidden=button.dataset.ready==='false'||!rect||rect.bottom<=viewport.top||rect.top>=viewport.bottom||rect.right>viewport.right;
    if(!button.hidden){button.style.left=(rect.right-parent.left-19)+'px';button.style.top=(rect.top+(rect.height-18)/2-parent.top)+'px';}
  });
}



// 能识别的内部命令。只有斜杠跟后面的字母拼成其中一个时整行才变色，
// 拼不成（/op、/rm、单独一个 /）立刻变回来
const COMMAND_RE = /^[\/／](note|便签|clock|o|rmbr|spt|theme|update|config|help|version|v|\?|？)(?=\s|$)/i;

// 透明窗口四周那圈边的宽度，跟 style.css 里的 --pad / --pad-top 一致。
// 上边宽一些，放移动手柄。下面所有尺寸都指搜索框本身，真正设给窗口时再加上这圈边
const PAD = 12;
const PAD_TOP = 24;
// 下面四个尺寸来自 config.toml 的 [window]，启动时和配置文件变动时由 applyUiConfig 覆盖
let COLLAPSED_WIDTH = 640;
let MAX_COLLAPSED_HEIGHT = 460; // 折叠态窗口高度上限，超过就在结果区内部滚动
let EXPANDED_WIDTH = 860;
let EXPANDED_HEIGHT = 620;
const SIDEBAR_WIDTH = 210; // 跟 style.css 里 #session-list 的宽度一致

let fullText = "";      // 折叠态当前这一条回答
let settingsOpen = false;
let expanded = false;
let pinned = false;
let messages = [];      // 当前会话已落盘的消息
let streaming = null;   // 正在生成中的回答，null 表示当前没有在生成
let userSized = false;  // 用户手动拖过窗口尺寸后，就别再自动改它了
let helpShown = false;  // /? 的说明正在显示，Esc 先关它
let reasoningText = ""; // 深度思考的推理过程，只在界面上显示，不入库
let oMode = false;      // 单按 Alt 进入的"打开模式"：回车直接当 /o 处理

// 自己记着当前逻辑尺寸，动画要有起点。
// 不去问 appWindow.innerSize()：那是一次 IPC 往返，还多依赖一条权限
let curW = 640;
let curH = 90;   // 搜索框高度；tauri.conf.json 里的窗口初始高度是它加上 PAD_TOP + PAD
let lastCollapsedH = 90;

// ---------- 窗口尺寸 ----------

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// 内部命令 /? 的内容。本地渲染，不发给 API、不写进会话历史
const HELP_MD = `**闪念 Shannon** · Dinglin

**快捷键**

- \`Alt\`+\`Space\` 唤起或隐藏窗口，可在配置文件中修改
- \`Esc\` 隐藏窗口；展开状态下先收起
- \`Ctrl\`+\`E\` 展开或收起历史对话
- \`Ctrl\`+\`L\` 新建对话
- \`Shift\`+\`Enter\` 换行
- \`↑\` \`↓\` 浏览输入历史
- 鼠标中键 置顶窗口，置顶时显示红色边框

**输入**

- 唤起时自动填入其他程序中选中的文字，并保持全选状态
- 回答生成过程中向上滚动将暂停自动滚动，滚动至底部后恢复

**命令**

- \`/plugin\` 打开插件管理，支持内置功能和第三方插件；\`/plugin install ZIP路径或HTTPS地址\` 安装，\`/plugin help\` 查看管理用法；启用后可从 / 提示选择插件工具，也可让 AI 调用

- 输入 \`/\` 显示命令列表，\`↑\` \`↓\` 选择，\`Tab\` 补全，\`Enter\` 确认
- 识别到命令后显示用法并标示当前参数，输入框内的灰色文字为待填参数
- 在输入框内长按鼠标左键开始本地语音识别，停顿时分句出字，背景涟漪表示正在听；松开左键或 Esc 结束并完成最后一句，文字不会自动发送
- 便签箭头暂时收起，× 退出；也可用 \`/note --close\` 或让 AI 退出便签，内容保留
- 便签也支持 \`@note 明天写报告\`、\`@note 删除买牛奶\`、\`@note 把写报告放第一项\`；日期仅为安排，不触发通知
- 也可直接说“帮我记一条便签：买牛奶”或“打开五分钟倒计时设置”，AI 会调用对应工具（需要接口支持工具调用）
- \`/note 便签内容\` 创建便签（也可用 \`/便签\`）；\`/note\` 打开便签。点击图标展开、按住拖动贴边，\`Esc\` 收起；点击文字划去或恢复，点击日期文字修改安排，垃圾桶删除，拖动左侧把手排序，内容自动保存在本地
- \`/config\` 打开配置文件，保存后立即生效
- \`/clock\` 创建计时器；\`/clock up\` 正计时，\`/clock 5m\` 倒计时。点击开始后贴在屏幕顶部，悬停可暂停或关闭
- 已有计时器时，\`/clock restart\` 重新打开设置；左右键切换计时方式，编辑时在光标边界再按一次切换，\`Enter\` 开始，\`Esc\` 退出设置
- \`/spt 序号\` 切换系统提示词，\`0\` 为不使用，\`1\` 为默认；\`/spt\` 查看列表，\`/spt edit\` 编辑提示词文件
- \`/theme\` 查看与切换主题，节日主题按日期自动启用；\`/theme sync\` 立即同步在线主题
- \`/update\` 检查新版本，\`/update install\` 下载并安装
- \`/?\` 使用说明，\`/version\` 版本信息

- \`@下载\` 打开 Gopeed 下载试用窗口；\`@下载 https://example.com/file.zip\` 准备下载，确认后开始。首次需在窗口安装组件，支持暂停、继续与取消（保留文件）。\n- \`@story\` 打开故事舞台：设定人物、推进叙事、查看事件和角色微信；支持 AI 直调。\n- \`@wechat 姓名\` 打开微信风格 AI 模拟聊天；需填写对方姓名，不连接真实微信。\n\n**打开应用与文件夹**

- \`/o 名称\` 打开应用、文件夹、文件或网址，例如 \`/o 微信\`、\`/o D:\\Documents\`、\`/o ~/Downloads\`
- \`/rmbr 别名 目标\` 添加别名，之后可通过 \`/o 别名\` 打开
- \`/rmbr\` 或 \`/o\` 查看全部别名，\`/rmbr -d 别名\` 删除别名
- 应用名称支持全拼、拼音首字母与英文缩写，例如 \`/o wx\`、\`/o wangyiyun\`、\`/o vsc\`
- 内置常用别名，例如 \`ps\`（Photoshop）、\`gg\`（Chrome）、\`jsb\`（记事本）、\`xz\`（下载文件夹）
- 按开始菜单与桌面快捷方式名称匹配应用，无法匹配时请使用完整路径

**打开模式**

- 窗口显示时单按右侧 \`Alt\` 进入，窗口边缘显示高亮，输入路径或别名后按 \`Enter\` 打开
- 再次单按右侧 \`Alt\` 退出，\`Esc\` 不会退出打开模式
- 打开模式下无法展开历史对话

**鼠标操作**

- 窗口上边缘：拖动以移动窗口
- 窗口左侧：悬停显示设置入口，展开状态下固定于左下角
- 窗口右下角：拖动以调整大小
- 代码块右上角：复制代码

**其他**

- 设置面板提供深度思考、联网搜索、开机自启动与系统代理开关
- 对话记录保存在本地，默认保留 30 天；每次提问默认携带最近 20 条消息。以上均可在配置文件中修改`;

// 版本号由 Rust 在编译期从 Cargo.toml 取，这里只拼文案，不再另写一份号
function versionMd(v) {
  return `**闪念 Shannon v${v}** · Dinglin

更新说明整理日期：2026-11-01

### v3.0.1

- 新增统一插件管理，官方插件独立下载，主安装包不带插件实现。
- 故事舞台支持紧凑人设、撤回重做、输入重写和删除恢复。
- 修复人设初始化与引用更新流程，减少重复追问。
- 新增下载管理并优化工具交互。

### v3.0.0

**全新名称**

- 闪问正式更名为“闪念”，开启 3.0。
- 统一更新窗口、托盘、帮助信息与主题寄语中的中文品牌；英文名称 Shannon 保留。
- 延续原有配置、对话、便签与插件数据，保留旧插件接口和更新渠道兼容。

**插件与 AI 工具**

- 设置中新增 V4 Flash / V4 Pro 模型选择，切换后从下一次请求生效，普通对话、角色聊天和故事舞台共用。
- 支持安装闪念第三方工具插件，提供来源、权限及安装确认。
- 支持通过 @ 选择插件，便签、计时器也可直接使用 @ 调用。
- 更新工具标签样式，增加独立字体、关闭按钮和标签间距。
- 支持一次批量删除多条或全部便签，避免逐条调用触发工具数量限制，删除后可恢复。

**微信风格角色聊天**

- 新增手机样式的模拟聊天窗口，指定对方姓名即可开始；提供最近聊天、通讯录、发现与朋友圈。
- 支持人物设定、性格、当前状态、故事报告与独立聊天记录；聊天使用专属提示词，支持以方括号表达动作、旁白与文件描述。
- 支持分条回复、选择暂不回复和主动联系，结合角色性格、关系与近期话题决定交流方式；生成时显示“对方正在输入…”。
- 发送后可返回列表、切换角色或关闭聊天窗口，回复继续在应用后台生成；退出整个应用会中止尚未完成的任务。
- 支持发布朋友圈、点赞、评论与回复评论；角色可根据日常状态或聊天中的情绪变化决定是否发布动态和互动。
- 提供头像库、自定义头像上传与裁剪；支持删除好友，并可选择同时删除聊天记录。
- 新增插件专属托盘图标与入口；模拟聊天、朋友圈均在闪念内运行，不连接真实微信。

**故事舞台与长期记忆**

- 新增独立故事舞台，集中展示叙事、场景、角色状态、关键事件与人物变化；支持通过 @story 或 AI 工具创建、查看和推进故事。
- 角色可随剧情登场、离场和回归；角色卡支持外貌描述、性格、心理活动、目标与关系，并记录人物变化的原因。
- 将模拟微信作为故事角色的通信工具，支持角色间消息、公开动态，以及用户以故事主控角色身份发送消息。
- 新增长期记忆，持续保留未完成的约定、伏笔和重要事实，并从全部已保存的剧情与通信记录中查找相关旧事，减少人物遗忘。
- 区分亲历、听闻与推测，保留原文出处；新增“记忆”页，可搜索、修订、固定记忆，并将约定标记为完成或作废。
- 长期记忆用于辅助模型保持连续性，仍需模型正确使用；故事人物与原有独立聊天记录分别保存。

**局域网广播**

- 新增局域网广播工具，同一局域网中的其他设备可通过浏览器查看发布内容，无需安装闪念。
- 开启广播、发布和撤回内容通过应用内确认完成，支持手动操作与 AI 工具调用。

**便签全面升级**

- 收起显示小图标，离开边缘呈浮球，支持按住拖动与四边磁吸；展开后可拖动标题区域。
- 顶部显示当天日期和星期；默认鼠标移开后自动收起，图钉可固定展开。
- 支持今天、以后、全部日程视图，未来待办不混入今天，未完成的逾期待办继续保留。
- 点击日期标签即可修改安排，提供今天、明天、后天及后续周一至周五日期快捷选择。
- 支持单条删除、撤销与最近删除恢复，支持拖拽和键盘排序。
- AI 可创建待办、安排日期、调整顺序及删除恢复；安排日期仅作日程标记，不触发提醒。

**动画与交互优化**

- 调整主搜索框收起的翻折轴至输入框上边缘。
- 优化动画结束与原生窗口尺寸切换的衔接，处理文字跳动和滚动滑块短暂出现的问题。
- 调整便签收起流程，减少原生窗口缩放引起的闪帧。
- 优化拖动光标、方向箭头、按钮间距和低饱和度日期配色。

### v2.5.0

**新增**

- 添加纸笺便签，支持点击划去、贴边收起和悬停展开
- 添加本地语音输入与手势控制，支持按需安装组件
- 添加 AI 直调工具能力，支持管理便签、打开计时设置等操作
- 添加数学公式渲染与代码块语言标识
- 添加重阳节主题与搜索框上方的节日寄语

**优化**

- 优化窗口唤起、隐藏与挂历翻折动画
- 优化便签排版、计时器交互与键盘操作
- 语音与手势组件独立安装，减小默认安装包体积

**修复**

- 修复输入时视图回到开头的问题
- 修复若干显示与交互问题

### v2.4.0

- 添加计时器功能

### v2.3.1

- 修复若干问题和优化动画逻辑

### v2.3.0

**在线主题**

- 新增在线主题：节日主题可由发布者在线推送，无需更新版本
- 后台定期同步并提前下载至本地，生效当日无需联网即可切换
- 主题清单经数字签名校验，每个文件均核对完整性，校验未通过的内容不会被使用
- 新增 \`/theme sync\` 立即同步；\`/theme\` 列表中标注主题来源（内置、在线、自定义）
- 可在配置文件 \`[theme]\` 中设置 \`online = false\` 关闭在线主题

**节日主题**

- 新增唤起寄语：特效播放时，寄语在输入框中逐字浮现；按任意键即可收起
- 国庆主题新增寄语「闪念祝福祖国繁荣昌盛」

### v2.2.0

**配置文件**

- 新增配置文件 \`config.toml\`，集中管理全部设置，每项均附说明；通过 \`/config\` 打开，保存后立即生效，无需重启
- 新增可配置项：全局快捷键、接口地址与模型、上下文条数、对话保留天数、窗口尺寸、输入框提示文字、选中文字等待时间等
- 配置有误时沿用上次有效的设置，并提示错误所在的行、列及原因
- 通过设置面板、\`/rmbr\`、\`/theme\` 修改的设置会写入配置文件，文件中的注释保持不变
- 首次启动时自动迁移旧版设置，原文件保留为 \`config.json.bak\`

**系统提示词**

- 新增提示词文件 \`prompts.toml\`，支持多条提示词，内置「默认」「翻译」「润色」
- 新增 \`/spt 序号\` 切换提示词：\`0\` 为不使用，\`1\` 为默认；\`/spt\` 查看列表，\`/spt edit\` 编辑提示词文件
- 提示词支持 \`{date}\`、\`{weekday}\` 变量，发送时替换为当天日期与星期
- 默认启用提示词 1，回答更加简洁；如需恢复旧版风格，请使用 \`/spt 0\`

**命令提示**

- 输入 \`/\` 时显示命令列表及说明，支持 \`↑\` \`↓\` 选择、\`Tab\` 补全、\`Enter\` 确认
- 命令名称未输入完整时，按 \`Enter\` 将自动补全，避免误发送
- 输入参数时显示命令用法并标示当前参数，输入框内以灰色文字提示待填参数
- \`/spt\`、\`/theme\` 列出全部可选项并标注当前项，选中后按 \`Enter\` 即可切换

**节日主题**

- 新增主题功能，按日期自动启用节日外观，包括配色、徽标、底纹与提示文字
- 内置国庆主题（每年 10 月 1 日至 7 日），每天首次唤起时播放全屏烟花
- 新增 \`/theme\` 查看与切换主题，\`/theme auto\` 按日期自动切换，\`/theme off\` 关闭主题
- 支持自定义主题，放入主题目录即可生效，格式参见 \`THEME_SPEC.md\`

**输入**

- 唤起时自动填入其他程序中选中的文字，并保持全选状态
- 支持 \`↑\` \`↓\` 浏览输入历史，默认保留最近 100 条

**对话**

- 回答生成过程中向上滚动将暂停自动滚动，滚动至底部后恢复

**自动更新**

- 新增在线更新：后台定期检查新版本，发现新版本时在输入框下方提示
- 新增 \`/update\` 查看新版本说明，\`/update install\` 下载并安装，安装完成后自动重新启动
- 安装包经数字签名校验，校验未通过时停止更新
- 可在配置文件 \`[update]\` 中关闭自动检查或调整检查间隔

**界面**

- 优化展开箭头的绘制，消除尖端叠色
- 窗口右下角缩放手柄简化为单道弧线
- 命令列表的选中高亮与参数下划线加入弹性动画

### v2.1.0

**打开模式**

- 改为通过右侧 \`Alt\` 切换，左侧 \`Alt\` 保留给 \`Alt\`+\`Space\` 等系统快捷键
- \`Esc\` 不再退出打开模式，仅再次单按右侧 \`Alt\` 退出；窗口隐藏后再次唤起时保持打开模式
- 打开模式下禁用历史对话展开，展开按钮自动隐藏
- 打开成功后自动隐藏窗口；窗口置顶时保持显示，便于连续打开

**设置**

- 展开状态下设置入口固定于左下角
- 展开状态下点击设置面板外部、点击「设置」标题或按 \`Esc\` 均可收起设置

**窗口**

- 搜索框上边缘整体可用于拖动窗口
- 展开历史对话后直接定位到最新消息

**界面与性能**

- 界面视觉全面改版：拨动开关、统一强调色、同心圆角
- 优化展开、收起及长对话场景下的动画性能
- 修复收起时闪白、结尾跳动及搜索栏分层线问题

### v2.0.0

**打开应用与文件夹**

- 新增 \`/o 名称\`：打开应用、文件夹、文件或网址，例如 \`/o 微信\`、\`/o D:\\Documents\`、\`/o www.baidu.com\`
- 路径支持 \`~\` 与 \`%APPDATA%\` 等环境变量，支持从资源管理器复制的带引号路径
- 按开始菜单与桌面快捷方式名称查找应用，并支持系统命令（记事本、计算器等）与 \`ms-settings:\` 等系统链接
- 应用名称支持全拼、拼音首字母与英文缩写，兼容多音字，例如 \`/o wx\`（微信）、\`/o wangyiyun\`（网易云音乐）、\`/o vsc\`（Visual Studio Code）
- 无法打开时提示原因，并保留输入内容以便修改

**别名**

- 新增 \`/rmbr 别名 目标\`：为应用、文件夹或网址添加别名，之后可通过 \`/o 别名\` 打开；添加时显示实际匹配的目标
- \`/rmbr\` 或 \`/o\` 查看全部别名，\`/rmbr -d 别名\` 删除别名
- 内置约 90 组常用别名，仅对本机已安装的应用生效，例如 \`ps\`（Photoshop）、\`gg\`（Chrome）、\`fs\`（飞书）、\`zt\`（Zotero）、\`jsb\`（记事本）、\`sz\`（系统设置）、\`xz\`（下载文件夹）
- 自定义别名优先于内置别名；内置别名可通过 \`/rmbr -d\` 停用

**打开模式**

- 窗口显示时单按 \`Alt\` 进入打开模式，窗口边缘显示高亮，输入路径或别名后按 \`Enter\` 直接打开

**其他**

- 输入内容构成命令（\`/o\`、\`/rmbr\`、\`/?\`、\`/version\`）时整行高亮
- 窗口采用透明边距设计，圆角与阴影由应用自行绘制

### v1.1.0

- 支持拖动与缩放窗口；鼠标中键置顶窗口，置顶时显示红色边框
- 对话记录保存在本地，保留 30 天；\`Ctrl\`+\`E\` 展开历史对话，可查看、切换、删除并继续对话
- 设置面板新增深度思考、联网搜索、开机自启动与系统代理开关
- 代码块支持语法高亮与一键复制
- 输入框支持多行输入，\`Shift\`+\`Enter\` 换行
- 新增 \`/?\` 使用说明与 \`/version\` 版本信息
- 修复开启系统代理时请求失败的问题；网络错误提示更加详细

### v1.0.0

- 首个版本：通过 \`Alt\`+\`Space\` 唤起对话窗口，接入 DeepSeek，支持流式输出与 Markdown 渲染
- 常驻系统托盘，按 \`Esc\` 或窗口失去焦点时自动隐藏

### 已知问题

- 窗口四周的透明边距（上方 24 px，其余 12 px）会拦截下层窗口的鼠标点击
- 选中文字填入通过模拟复制实现：部分程序无法获取；VS Code 等编辑器在未选中文字时会填入光标所在行
- 填入选中文字时会临时占用剪贴板并随后恢复，Windows 剪贴板历史（\`Win\`+\`V\`）中会新增一条记录
- 通过应用商店安装且无开始菜单快捷方式的应用无法通过 \`/o\` 找到，请使用完整路径或添加别名
- 联网搜索使用 DeepSeek 的 Anthropic 兼容接口，如遇异常请反馈`;
}

// 本地文档统一走这一条，/? 和 /version 共用显示与关闭逻辑
function showLocal(md) {
  queryInput.value = "";
  autoGrowInput();
  streaming = null;
  helpShown = true;

  if (expanded) {
    messages.push({ role: "help", content: md });
    renderChat();
  } else {
    fullText = md; // 存一份，展开/收起来回切时不会丢
    renderResult(md);
    resultEl.classList.remove("hidden");
    followContent(160);
  }
}

async function showVersion() {
  let v = "2.3.1";
  try {
    v = await invoke("app_version");
  } catch (_) {
    // 拿不到就用兜底的号，不该因此不显示
  }
  showLocal(versionMd(v));
}

function showHelp() {
  showLocal(HELP_MD);
}

// ---------- /o 与 /rmbr ----------

// 别名、路径里可能有反引号以外的任何东西，放进代码段里显示最省事
const code = (s) => "`" + String(s).replace(/`/g, "'") + "`";

// 内置别名的目标只显示个名字：快捷方式去掉路径和 .lnk，exe 只留文件名
function shortTarget(t) {
  const base = t.split(/[\\/]/).pop();
  return /\.(lnk|url|appref-ms)$/i.test(base) ? base.replace(/\.[^.]+$/, "") : base;
}

async function showAliases() {
  let list;
  try {
    list = await invoke("list_aliases");
  } catch (err) {
    showLocal("无法读取别名：" + err);
    return;
  }
  const { user, builtin } = list;
  const tip = "\n\n\`/rmbr 别名 目标\` 添加别名，\`/rmbr -d 别名\` 删除别名，\`/o 别名\` 打开。" +
    "应用名称同样支持全拼与拼音首字母，例如 \`/o txhy\`（腾讯会议）。";
  let md = user.length
    ? "**自定义别名**\n\n" + user.map(([n, t]) => `- ${code(n)} → ${code(t)}`).join("\n")
    : "暂无自定义别名。";
  if (builtin.length) {
    md += "\n\n**内置别名**（仅显示本机已安装的应用）\n\n" +
      builtin.map(([ns, t]) => `- ${ns.map(code).join(" ")} → ${shortTarget(t)}`).join("\n");
  }
  showLocal(md + tip);
}

// 出错时把输入留在框里，改个字就能重试
function showLocalKeep(md, text) {
  showLocal(md);
  queryInput.value = text;
  autoGrowInput();
  scheduleResize();
}

async function openCommand(arg, raw) {
  if (!arg) return showAliases();
  try {
    await invoke("open_target", { query: arg });
    if (helpShown) closeHelp();
    queryInput.value = "";
    autoGrowInput();
    scheduleResize();
    // 固定时说明用户还想留着窗口，就不藏了。
    // 打开模式只认 Alt 退出，这里不动它，下次唤起还在打开模式里
    if (!pinned) await invoke("hide_window");
  } catch (err) {
    showLocalKeep(`**无法打开** ${code(arg)}\n\n${err}`, raw);
  }
}

async function rmbrCommand(arg, raw) {
  if (!arg) return showAliases();

  const del = arg.match(/^-d\s+(.+)$/);
  if (del) {
    try {
      const old = await invoke("remove_alias", { name: del[1] });
      const n = code(del[1].trim().toLowerCase());
      // 删内置别名后端返回 null：它写在程序里，只是以后不再用它
      showLocal(
        old === null
          ? `已停用内置别名 ${n}。如需指向其他目标，请使用 ${code("/rmbr " + del[1].trim() + " 目标")}`
          : `已删除别名 ${n}（原目标：${code(old)}）`
      );
    } catch (err) {
      showLocalKeep(String(err), raw);
    }
    return;
  }

  // 别名不含空格，第一个空白之后的全部算目标，路径里有空格也没关系
  const m = arg.match(/^(\S+)\s+(.+)$/);
  if (!m) {
    showLocalKeep(
      `格式：${code("/rmbr 别名 目标")}，例如 ${code("/rmbr 文档 D:\\Documents")}`,
      raw
    );
    return;
  }

  const [, name, target] = m;
  try {
    const resolved = await invoke("set_alias", { name, target });
    showLocal(`已添加别名：${code(name.toLowerCase())} → ${code(resolved)}\n\n输入 ${code("/o " + name)} 即可打开`);
  } catch (err) {
    showLocalKeep(`**无法添加别名** ${code(name)}\n\n${err}`, raw);
  }
}

function closeHelp() {
  helpShown = false;
  if (expanded) {
    // 说明只存在于内存里，没进会话历史，直接滤掉即可
    messages = messages.filter((m) => m.role !== "help");
    renderChat();
  } else {
    fullText = "";
    resultEl.innerHTML = "";
    resultEl.classList.add("hidden");
    followContent(160);
  }
}

// textarea 高度跟着内容走：先塌回 auto 量出真实 scrollHeight，再设回去
const MAX_INPUT_HEIGHT = 132;
function autoGrowInput() {
  // 输入事件发生前浏览器已经把光标滚进视野。临时塌缩高度和切换
  // overflow 会重置这个位置，测量结束后恢复，避免每输入一个字都跳回开头。
  const scrollTop = queryInput.scrollTop;
  const scrollLeft = queryInput.scrollLeft;
  // 量高度期间一律先关掉滚动条：它占宽度，会改变换行结果，量出来的值就不准
  queryInput.style.overflowY = "hidden";

  // 先按大字号量一次，判断是不是真的超过一行
  queryInput.classList.remove("multiline");
  queryInput.style.height = "auto";

  const cs = getComputedStyle(queryInput);
  const pad = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
  const lineH = parseFloat(cs.lineHeight) || 26;

  if (queryInput.scrollHeight - pad > lineH * 1.5) {
    // 降字号后行高变了，得重新塌一次再量
    queryInput.classList.add("multiline");
    queryInput.style.height = "auto";
  }

  const h = Math.min(queryInput.scrollHeight, MAX_INPUT_HEIGHT);
  queryInput.style.height = h + "px";
  // 只有真的顶到上限才给滚动条，否则内容删空后那条灰杠会一直留着
  queryInput.style.overflowY = h >= MAX_INPUT_HEIGHT ? "auto" : "hidden";
  queryInput.scrollTop = scrollTop;
  queryInput.scrollLeft = scrollLeft;
  syncMirror();
  updateCmdHint();
}

// 光标移动也会改变该不该显示灰字参数提示
document.addEventListener("selectionchange", () => {
  if (document.activeElement === queryInput) syncMirror();
});

// ---------- 命令高亮 ----------

function escapeText(s) {
  return s.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));
}

let mentionComposing=false;
queryInput.addEventListener('compositionstart',()=>{mentionComposing=true;});
queryInput.addEventListener('compositionend',()=>{mentionComposing=false;syncMirror();});
function syncMirror() {
  let v = queryInput.value;
  let mentions = selectedMentionRanges(v,mentionPlugins);
  if(mentions.length&&!queryInput.matches(':disabled')&&!COMMAND_RE.test(v)&&!mentionComposing){
    const spaced=spaceMentionControls(v,mentions,queryInput.selectionStart,queryInput.selectionEnd);
    if(spaced.text!==v){const top=queryInput.scrollTop,left=queryInput.scrollLeft;queryInput.value=spaced.text;queryInput.setSelectionRange(spaced.start,spaced.end);queryInput.scrollTop=top;queryInput.scrollLeft=left;v=spaced.text;mentions=selectedMentionRanges(v,mentionPlugins);queueMicrotask(()=>{autoGrowInput();scheduleResize();});}
  }
  const isCommand = COMMAND_RE.test(v);
  const on = isCommand || mentions.length > 0;
  queryInput.classList.toggle("cmd", on);
  mirrorEl.classList.toggle("hidden", !on);
  mentionDismiss.replaceChildren();
  if (!on) return;

  // 字号会随单行/多行切换，每次都从 textarea 现场抄，不在 CSS 里写死
  const cs = getComputedStyle(queryInput);
  const st = mirrorEl.style;
  st.left = queryInput.offsetLeft + "px";
  st.top = queryInput.offsetTop + "px";
  // clientWidth 不含滚动条，折行位置才和 textarea 一致
  st.width = queryInput.clientWidth + "px";
  st.height = queryInput.clientHeight + "px";
  st.font = cs.font;
  st.lineHeight = cs.lineHeight;
  st.letterSpacing = cs.letterSpacing;
  st.padding = cs.padding;

  // 结尾是换行时 pre-wrap 不会给空的最后一行留高度，补一个零宽字符
  // 光标在最后时，在后面用灰字标出还差哪些参数（像 Excel 输公式时的提示）
  const atEnd = queryInput.selectionStart === v.length && queryInput.selectionEnd === v.length;
  const whole = queryInput.selectionStart === 0 && queryInput.selectionEnd === v.length; // 刚唤起时的全选
  const ghost = isCommand && (atEnd || whole) ? ghostText(v) : "";
  let mentionHtml = '', end = 0;
  for (const range of mentions) {
    const reserved=Math.min(7,(v.slice(range.end).match(/^ */)||[''])[0].length);
    mentionHtml += `<span class="query-plain">${escapeText(v.slice(end,range.start))}</span><span class="mention-text"><span class="mention-metrics">${escapeText(v.slice(range.start,range.end))}${' '.repeat(reserved)}</span><span class="mention-face"><span class="mention-at">${escapeText(v.slice(range.start,range.start+1))}</span>${escapeText(v.slice(range.start+1,range.end))}</span></span>`;
    end = range.end+reserved;
  }
  mentionHtml += `<span class="query-plain">${escapeText(v.slice(end))}</span>`;
  mirrorEl.innerHTML =
    // 一旦构成命令，整行（命令本身加后面的参数）都染色
    (isCommand ? `<span class="cmd-text">${escapeText(v)}</span>` : mentionHtml) +
    (ghost ? `<span class="cmd-ghost">${escapeText(ghost)}</span>` : "") +
    (v.endsWith("\n") ? "\u200b" : "");
  mirrorEl.scrollTop = queryInput.scrollTop;
  if(!isCommand)for(const range of mentions){
    const button=document.createElement('button');button.type='button';button.className='mention-remove';button.dataset.ready=String(v.slice(range.end,range.end+3)==='   ');button.textContent='×';button.setAttribute('aria-label',`移除工具 ${v.slice(range.start,range.end)}`);button.title='移除此工具';
    button.onpointerdown=event=>{event.preventDefault();event.stopPropagation();};
    button.onclick=()=>{
      if(queryInput.value!==v){syncMirror();return;}
      const updated=removeMention(v,range,queryInput.selectionStart,queryInput.selectionEnd);
      queryInput.value=updated.text;queryInput.focus();queryInput.setSelectionRange(updated.start,updated.end);
      queryInput.dispatchEvent(new Event('input',{bubbles:true}));
    };
    mentionDismiss.append(button);
  }
  placeMentionDismiss();
}

// 输入框宽度变了（箭头按钮收走 / 长回来、窗口缩放），折行位置跟着变，
// 镜像层和输入框高度都要重算。只看宽度，高度变化是 autoGrowInput 自己引起的，
// 也响应的话会互相触发
new ResizeObserver(() => {
  // 展开/收起动画期间窗口每帧都在变宽变窄，这里每帧都会触发。
  // 那时候什么都别做，动画结束时 animateSize 会补一次
  if (sizeLocked()) return;
  const w = queryInput.clientWidth;
  if (w === lastInputW) return;
  lastInputW = w;
  autoGrowInput();
  scheduleResize();
}).observe(queryInput);

queryInput.addEventListener("scroll", () => {
  mirrorEl.scrollTop = queryInput.scrollTop;
  placeMentionDismiss();
});
window.addEventListener("resize", () => {
  syncMirror();
  if (oMode) layoutOBorder();
  // 展开过程中窗口每帧都在变高，对话区也跟着变高；每帧都贴住底部，
  // 这样看到的始终是最新的消息，不会先露出最早的、结束时再跳下去
  // 收起时也一样：窗口从下往上收，贴底才能让最新的消息留到最后
  if (morphing && !chatViewEl.classList.contains("hidden")) {
    chatViewEl.scrollTop = chatViewEl.scrollHeight;
  }
});

// ---------- 打开模式 ----------

// 两条半周长的路径，都从搜索框下边缘中点出发，一条向左一条向右，
// 沿着圆角边框往上走，终点都是上边缘中点，于是画完正好在那里合拢。
// 路径正好压在搜索框边缘上，里面一半被搜索框挡住，只露出朝外的光
function layoutOBorder() {
  const b = appEl.getBoundingClientRect();
  const x0 = b.left, y0 = b.top, x1 = b.right, y1 = b.bottom;
  // 跟 #app 的圆角一致，从 style.css 的 --r-window 读，改 token 这里自动跟着变
  const r = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--r-window")) || 12;
  const mx = (x0 + x1) / 2;
  oBorderL.setAttribute(
    "d",
    `M${mx} ${y1} H${x0 + r} A${r} ${r} 0 0 1 ${x0} ${y1 - r} ` +
      `V${y0 + r} A${r} ${r} 0 0 1 ${x0 + r} ${y0} H${mx}`
  );
  oBorderR.setAttribute(
    "d",
    `M${mx} ${y1} H${x1 - r} A${r} ${r} 0 0 0 ${x1} ${y1 - r} ` +
      `V${y0 + r} A${r} ${r} 0 0 0 ${x1 - r} ${y0} H${mx}`
  );
}

// 折叠态下搜索框高度会随输入、回答变化，窗口不一定跟着触发 resize，
// 所以直接盯着 #app 本身的尺寸
new ResizeObserver(() => {
  if (oMode) layoutOBorder();
  // 展开 / 收起时是 #app 自己在变大变小（系统窗口不逐帧变了），
  // 对话区跟着变高变矮，每帧贴住底部，看到的始终是最新的消息
  if (morphing && !chatViewEl.classList.contains("hidden")) {
    chatViewEl.scrollTop = chatViewEl.scrollHeight;
  }
}).observe(appEl);

function setOMode(on, animate = true) {
  if (on === oMode) return;
  oMode = on;
  if (on) layoutOBorder();
  if (!animate) {
    // 瞬时切换：暂时拿掉过渡，改完强制回流再还回去
    for (const p of [oBorderL, oBorderR]) p.style.transition = "none";
    appEl.classList.toggle("o-mode", on);
    document.body.classList.toggle("o-mode", on); // #o-border 在 #app 外面，靠 body 上的类驱动
    void appEl.offsetWidth;
    for (const p of [oBorderL, oBorderR]) p.style.transition = "";
  } else {
    appEl.classList.toggle("o-mode", on);
    document.body.classList.toggle("o-mode", on); // #o-border 在 #app 外面，靠 body 上的类驱动
  }
  // 打开模式下不许展开：箭头按钮由 CSS 动画收走，Ctrl+E 在 setExpanded 里挡掉。
  // disabled 是为了收走期间和收走后键盘 Tab 也点不到它。
  // 进打开模式时如果正展开着，先收起来
  expandBtn.disabled = on;
  if (on && expanded) setExpanded(false);
  queryInput.focus();
}

// 单按右 Alt 切换打开模式。左 Alt 完全不管，留给 Alt+Space 这些系统组合键。
// 用 e.code 区分左右：左边是 AltLeft，右边是 AltRight（e.key 两边都是 "Alt"）。
// 按下和松开之间没有别的键、没有点鼠标才算，这样右 Alt+别的键不会误触发。
// 自动重复的 keydown 不算"按下"：按住右 Alt 不放时不会反复触发
let altArmed = false;
document.addEventListener(
  "keydown",
  (e) => {
    if (e.code === "AltRight") {
      if (!e.repeat) altArmed = true;
    } else if (e.code === "ControlLeft" && e.getModifierState("AltGraph")) {
      // 德语、法语等键盘布局下右 Alt 是 AltGr，Windows 会先塞一个假的左 Ctrl，别让它打断
    } else {
      altArmed = false;
    }
  },
  true
);
document.addEventListener("mousedown", () => (altArmed = false), true);
window.addEventListener("blur", () => (altArmed = false));
document.addEventListener("keyup", (e) => {
  if (e.key !== "Alt" && e.key !== "AltGraph") return;
  e.preventDefault(); // 挡掉 WebView 单按 Alt 激活菜单的默认行为（左右都挡）
  if (e.code !== "AltRight" || !altArmed) return;
  altArmed = false;
  if (settingsOpen) return; // 在填 Key 的时候别跳模式
  setOMode(!oMode);
});

queryInput.addEventListener("input", () => {
  histPos = -1; // 动手改了就算新的一条，下次 ↑ 从最近一条重新翻
  autoGrowInput();
  scheduleResize(); // 折叠态窗口高度跟着输入框一起长
});

function applySize(w, h) {
  // 尺寸没变就别再调系统窗口：每次调用都是一次跨进程的窗口操作，
  // 在 Windows 透明窗口上还可能让 WebView2 闪一下
  if (Math.round(w) === Math.round(curW) && Math.round(h) === Math.round(curH)) return;
  curW = w;
  curH = h;
  return appWindow.setSize(new LogicalSize(Math.round(w + 2 * PAD), Math.round(h + PAD_TOP + PAD)));
}

// 逐帧 setSize 做缓动。用 easeOutCubic：起步快、收尾慢，
// 视觉上比线性"稳得住"，也更像系统自己的窗口动画
let animId = 0;
// 尺寸动画进行中。期间别的地方（scheduleResize、followContent、输入框宽度监听）
// 一律不许碰窗口尺寸，否则每帧都会把动画拽回去——收起动画就是这么丢的
let sizeAnimating = false;
// 整个展开/收起流程（包括侧栏先滑走的那段停顿）都算"变形中"，同样不许别人碰窗口尺寸。
// 收起时侧栏先收、停 170ms 再缩窗口，这段停顿里输入框宽度会变，
// 以前会触发自动定高，把窗口和收起的目标高度一起改成展开时的高度，动画就没了
let morphing = false;
let collapseRenderPending = false;
let collapseStreamError = null;
const sizeLocked = () => sizeAnimating || morphing;
let lastInputW = 0; // 输入框上次处理过的宽度，见下面输入框的 ResizeObserver
// 展开 / 收起的尺寸变化。
//
// 以前是逐帧调系统窗口的尺寸（每帧一次 JS → Rust → Windows 改窗口 → WebView2 重排）。
// 透明窗口在 Windows 上这样做，WebView2 的画面会跟不上窗口，露出没画好的一帧，
// 看起来就是闪白；每帧一次跨进程调用本身也是卡顿的主要来源。
//
// 现在系统窗口只动一次：
//   变大：先一次性把窗口放到目标大小（多出来的部分是透明的，看不见），
//         里面的 #app 再用 CSS 过渡从旧尺寸长到新尺寸；
//   变小：先用 CSS 把 #app 缩到目标尺寸，缩完再一次性把窗口缩小（缩掉的只是透明区）。
// 动画全程由浏览器自己合成，圆角和阴影也一直是完整的
const morphMs = () =>
  parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--t")) * 1000 || 220;

function animateSize(targetW, targetH) {
  const id = ++animId;
  const fromW = curW;
  const fromH = curH;
  const growing = targetW > fromW || targetH > fromH;
  sizeAnimating = true;

  // 1. 先把 #app 钉在当前尺寸（不带过渡）
  appEl.classList.remove("morph-anim");
  appEl.style.width = fromW + "px";
  appEl.style.height = fromH + "px";
  // 2. 变大时系统窗口先一步到位
  if (growing) applySize(Math.max(fromW, targetW), Math.max(fromH, targetH));
  void appEl.offsetWidth; // 让钉住的尺寸先生效，下面的改动才会产生过渡
  // 3. CSS 过渡到目标尺寸
  appEl.classList.add("morph-anim");
  appEl.style.width = targetW + "px";
  appEl.style.height = targetH + "px";

  return new Promise((resolve) => {
    setTimeout(() => {
      if (id !== animId) return resolve(); // 被后一次动画接管了，收尾交给它
      // 4. 系统窗口定到目标大小（变小时就是这一步才真正缩窗口）
      if (curW !== targetW || curH !== targetH) applySize(targetW, targetH);
      // 解除钉住，交还给样式表（折叠态按内容定高、展开态铺满窗口），算出来和目标一致
      appEl.classList.remove("morph-anim");
      appEl.style.width = "";
      appEl.style.height = "";
      sizeAnimating = false;
      resolve();
    }, morphMs() + 30);
  });
}

// CSS 过渡期间内容高度是一路在变的，窗口必须每帧都重新量。
// 只在过渡开始时调一次 scheduleResize()，采到的是动画第一帧的高度，
// 结果就是面板展开了、窗口却没跟着长，把内容挡住
function followContent(duration = 320) {
  if (expanded || userSized || sizeLocked()) return;
  const t0 = performance.now();
  function step(now) {
    if (sizeLocked()) return; // 半路开始了展开/收起动画，让给它
    const h = Math.min(appEl.scrollHeight, MAX_COLLAPSED_HEIGHT);
    lastCollapsedH = h;
    applySize(COLLAPSED_WIDTH, h);
    if (now - t0 < duration) requestAnimationFrame(step);
  }
  requestAnimationFrame(step);
}

let resizePending = false;
function scheduleResize() {
  if (expanded || userSized || sizeLocked()) return; // 展开态、手动调过尺寸、动画中都不自动改
  if (resizePending) return;
  resizePending = true;
  requestAnimationFrame(() => {
    resizePending = false;
    if (expanded || userSized || sizeLocked()) return; // 排队这一帧里状态可能变了
    const h = Math.min(appEl.scrollHeight, MAX_COLLAPSED_HEIGHT);
    lastCollapsedH = h;
    // 流式输出时每个 chunk 都会走到这里，这里必须是瞬时的，
    // 加缓动会让窗口一直追在文字后面，反而显得卡
    applySize(COLLAPSED_WIDTH, h);
  });
}

// ---------- 渲染 ----------

const CODE_LANGUAGE_LABELS = {
  py: 'Python', python: 'Python', python3: 'Python',
  c: 'C', h: 'C', cpp: 'C++', 'c++': 'C++', cc: 'C++', cxx: 'C++',
  cs: 'C#', csharp: 'C#', 'c#': 'C#',
  js: 'JavaScript', javascript: 'JavaScript', jsx: 'JSX',
  ts: 'TypeScript', typescript: 'TypeScript', tsx: 'TSX',
  java: 'Java', rust: 'Rust', rs: 'Rust', go: 'Go', golang: 'Go',
  html: 'HTML', xml: 'XML', css: 'CSS', scss: 'SCSS',
  json: 'JSON', jsonc: 'JSONC', sql: 'SQL', yaml: 'YAML', yml: 'YAML',
  sh: 'Shell', shell: 'Shell', bash: 'Bash', zsh: 'Zsh',
  powershell: 'PowerShell', ps1: 'PowerShell', pwsh: 'PowerShell',
  bat: 'Batch', batch: 'Batch', cmd: 'Batch',
  md: 'Markdown', markdown: 'Markdown', tex: 'LaTeX', latex: 'LaTeX',
  swift: 'Swift', kotlin: 'Kotlin', kt: 'Kotlin', ruby: 'Ruby', rb: 'Ruby',
  php: 'PHP', r: 'R', dart: 'Dart', lua: 'Lua', toml: 'TOML',
  text: '纯文本', txt: '纯文本', plaintext: '纯文本', plain: '纯文本',
};

function decorateCodeBlocks(root, highlight = true) {
  root.querySelectorAll("pre").forEach((pre) => {
    if (pre.parentElement.classList.contains("code-block")) return;
    const wrap = document.createElement("div");
    wrap.className = "code-block";
    pre.parentNode.insertBefore(wrap, pre);
    wrap.appendChild(pre);

    const header = document.createElement("div");
    header.className = "code-header";
    const label = document.createElement("span");
    label.className = "code-language";
    // Use the model's explicit fence language, before syntax highlighting.
    // Auto-detection can mistake short C/Python snippets for other languages.
    const language = pre.querySelector("code")?.className.match(/(?:^|\s)language-(\S+)/)?.[1] || "";
    label.textContent = Object.hasOwn(CODE_LANGUAGE_LABELS, language.toLowerCase())
      ? CODE_LANGUAGE_LABELS[language.toLowerCase()] : language || "代码";
    label.title = language || "未注明语言";
    header.appendChild(label);
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "copy-btn";
    btn.textContent = "复制";
    header.appendChild(btn);
    wrap.prepend(header);
  });

  // 流式输出期间不上色：每来一个 chunk 整块都会重绘，
  // 对每个代码块重跑一遍词法分析太费；而且没闭合的代码本来也高亮不准。
  // stream-done 之后会再渲染一次，那时候才真正着色
  if (streaming !== null || !highlight) return;
  root.querySelectorAll("pre code").forEach((el) => {
    try {
      if (el.dataset.highlighted !== "yes") hljs.highlightElement(el);
    } catch (_) {
      // 识别不了的语言就保持纯文本，不该因此中断渲染
    }
  });
}

function reasoningBlock(r) {
  return r ? `<div class="reasoning">${renderMarkdown(r)}</div>` : "";
}

// The first formula may load a bundled math font after the initial resize.
// Recheck height when fonts settle so the answer is not clipped at the bottom.
document.fonts?.addEventListener("loadingdone", () => scheduleResize());

function renderResult(md, highlight = true) {
  resultEl.innerHTML = reasoningBlock(reasoningText) + renderMarkdown(md);
  decorateCodeBlocks(resultEl, highlight);
}

function escapeHtml(s) {
  return s
    .replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]))
    .replace(/\n/g, "<br>");
}

function renderChat() {
  // 整个对话区是重建的，重建前记下滚动位置，用户往上翻着看的时候还给他
  const prevTop = chatViewEl.scrollTop;
  const all = streaming === null
    ? messages
    : messages.concat([{ role: "assistant", content: streaming, reasoning: reasoningText }]);

  chatViewEl.innerHTML = "";
  if (all.length === 0) {
    // 空对话就空着，不放"在这里输入"之类的提示语
    return;
  }

  for (const m of all) {
    const row = document.createElement("div");
    row.className = "msg msg-" + m.role;
    const body = document.createElement("div");
    body.className = "msg-body";
    // 用户输入按纯文本处理，不走 markdown，避免自己打的尖括号被吃掉
    body.innerHTML =
      m.role === "user"
        ? escapeHtml(m.content)
        : reasoningBlock(m.reasoning) + renderMarkdown(m.content);
    row.appendChild(body);
    chatViewEl.appendChild(row);
  }
  decorateCodeBlocks(chatViewEl);
  if (followBottom) chatViewEl.scrollTop = chatViewEl.scrollHeight;
  else chatViewEl.scrollTop = prevTop;
}

// 事件委托挂在 #app 上，折叠态和展开态的代码块共用一套复制逻辑
appEl.addEventListener("click", async (e) => {
  const btn = e.target.closest(".copy-btn");
  if (!btn) return;

  const code = btn.closest(".code-block")?.querySelector("pre code");
  if (!code) return;

  try {
    await navigator.clipboard.writeText(code.innerText);
  } catch (_) {
    const ta = document.createElement("textarea");
    ta.value = code.innerText;
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  }

  btn.textContent = "已复制";
  btn.classList.add("copied");
  clearTimeout(btn._t);
  btn._t = setTimeout(() => {
    btn.textContent = "复制";
    btn.classList.remove("copied");
  }, 1500);
});

// ---------- 会话列表 ----------

async function refreshSessions() {
  const [list, cur] = await Promise.all([
    invoke("list_sessions"),
    invoke("current_session_id"),
  ]);

  sessionListEl.innerHTML = "";

  const newBtn = document.createElement("button");
  newBtn.type = "button";
  newBtn.className = "new-session-btn";
  newBtn.textContent = "＋ 新建对话";
  newBtn.addEventListener("click", startNewSession);
  sessionListEl.appendChild(newBtn);

  for (const s of list) {
    const item = document.createElement("div");
    item.className = "session-item" + (s.id === cur ? " active" : "");

    const title = document.createElement("span");
    title.className = "session-title";
    title.textContent = s.title;
    item.appendChild(title);

    const del = document.createElement("button");
    del.type = "button";
    del.className = "session-del";
    del.textContent = "×";
    del.setAttribute("aria-label", "删除对话");
    item.appendChild(del);

    item.addEventListener("click", async (e) => {
      if (e.target === del) {
        await invoke("delete_session", { id: s.id });
        if (s.id === cur) {
          messages = [];
          streaming = null;
          renderChat();
        }
        await refreshSessions();
        return;
      }
      messages = await invoke("switch_session", { id: s.id });
      streaming = null;
      fullText = ""; // 属于上一个会话的内容，别带到折叠态去
      helpShown = false;
      renderChat();
      await refreshSessions();
      queryInput.focus();
    });

    sessionListEl.appendChild(item);
  }
}

async function startNewSession() {
  helpShown = false;
  await invoke("new_session");
  messages = [];
  streaming = null;
  fullText = "";
  resultEl.innerHTML = "";
  resultEl.classList.add("hidden");
  queryInput.value = "";
  autoGrowInput();
  closeSettings();
  renderChat();
  if (expanded) await refreshSessions();
  scheduleResize();
  queryInput.focus();
}

// ---------- 展开 / 固定 ----------

// 展开态提问时 stream-chunk 也在往 fullText 里累积，但从没渲染进 #result。
// 只把 #result 取消隐藏的话会留下一个空的、只有内边距和上边框的条，
// 看着就是"没收干净"。这里要么把内容真的渲染出来，要么彻底隐藏
function showCollapsedResult(highlight = !morphing) {
  if (fullText || reasoningText) {
    renderResult(fullText, highlight);
    resultEl.classList.remove("hidden");
  } else {
    resultEl.innerHTML = "";
    resultEl.classList.add("hidden");
  }
}

// 量出收起后窗口该有多高：临时把布局切成折叠态（折叠宽度、输入框回到顶部、
// 对话区藏起来、结果区按收起后的样子摆好），读完高度立刻还原。
// 全在同一帧里同步完成，浏览器还没来得及画，所以看不到这个中间状态
function measureCollapsedHeight() {
  const prevWidth = appEl.style.width;
  const chatHidden = chatViewEl.classList.contains("hidden");
  const resultHidden = resultEl.classList.contains("hidden");

  appEl.style.width = COLLAPSED_WIDTH + "px"; // 输入框的折行跟宽度有关
  appEl.classList.remove("is-expanded");
  chatViewEl.classList.add("hidden");
  showCollapsedResult(true);
  keepBottom(resultEl);
  autoGrowInput();
  // 量主列本身，不用 appEl.scrollHeight：那个会把浮在 #app 里的按钮也算进去。
  // 这一刻设置按钮正从展开态的左下角往折叠态的位置过渡，还没到位，
  // 它的下沿会多出几像素，收起动画最后就会再跳一下
  const h = Math.min(document.getElementById("main-col").offsetHeight, MAX_COLLAPSED_HEIGHT);

  appEl.style.width = prevWidth;
  appEl.classList.add("is-expanded");
  chatViewEl.classList.toggle("hidden", chatHidden);
  resultEl.classList.toggle("hidden", resultHidden);
  autoGrowInput();
  return h;
}

// 固定 / 解除固定对话区的宽度。固定时它比主列宽也没关系，多出来的被窗口裁掉
// 同时开关 .lazy（变形期间跳过屏幕外的消息，见 style.css）
function freezeChatWidth(w) {
  const on = w != null;
  chatViewEl.style.width = on ? Math.round(w) + "px" : "";
  // 注意不能顺手设 flex-shrink:0：主列是纵向排的，它管的是高度。
  // 设了的话收起到最后对话区不肯让出高度，会把输入栏挤到下面只露半截
  // 靠右：侧栏在左边进出、窗口在右边伸缩时，它贴着窗口右边不动
  chatViewEl.style.alignSelf = on ? "flex-end" : "";
  chatViewEl.classList.toggle("lazy", on);
}

// 变形结束：解锁，并补上期间被跳过的输入框宽度处理（折行、高度、命令高亮层）
function endMorph() {
  morphing = false;
  freezeChatWidth(null);
  // 侧栏滑出来会让对话区变窄、折行变多，最后再贴一次底
  if (expanded) chatViewEl.scrollTop = chatViewEl.scrollHeight;
  lastInputW = queryInput.clientWidth;
  autoGrowInput();
  if (!expanded && collapseRenderPending) {
    collapseRenderPending = false;
    if (collapseStreamError !== null) {
      resultEl.textContent = "请求失败：" + collapseStreamError;
      resultEl.classList.remove("hidden");
      collapseStreamError = null;
    } else {
      showCollapsedResult();
      keepBottom(resultEl);
    }
  }
  if (!expanded) {
    // 代码高亮放到落稳后的空闲时间，避免长代码块阻塞翻折的首帧。
    const highlightWhenIdle = () => {
      if (!expanded && !sizeLocked() && streaming === null) decorateCodeBlocks(resultEl);
    };
    if (window.requestIdleCallback) window.requestIdleCallback(highlightWhenIdle);
    else setTimeout(highlightWhenIdle, 60);
  }
}

async function setExpanded(next) {
  if (morphing || next === expanded) return;
  if (next && oMode) return; // 打开模式下禁用展开
  stopShowMotion();
  morphing = true;
  // 展开/收起会把输入框和设置面板整个换位置。与其让标题在变形过程里
  // 逐帧去追一个移动的目标，不如记下状态、先收掉，变形完再开回来
  const reopenSettings = settingsOpen;
  if (settingsOpen) closeSettingsInstant();

  expanded = next;
  // 展开/收起是明确的尺寸意图，把手动尺寸重置掉
  userSized = false;
  appEl.classList.remove("is-sized");
  if (expanded) {
    expandBtn.setAttribute("aria-label", "收起");

    // 先把内容取回来渲染好，再放大窗口，避免"窗口先空着变大、内容后蹦出来"
    const id = await invoke("current_session_id");
    messages = await invoke("get_messages", { id });
    renderChat();
    await refreshSessions();

    resultEl.classList.add("hidden");
    // 对话区一开始就按最终宽度（去掉侧栏）排好，靠右放；窗口长大、侧栏滑进来，
    // 都只是把它露出来 / 挪到位，全程不再折行。
    // 不固定的话它跟着窗口、再跟着侧栏逐帧变宽变窄，几十条消息每帧都要重新折行，
    // 这是展开卡的主因。左边空出来的正好是侧栏要滑进来的位置
    freezeChatWidth(EXPANDED_WIDTH - SIDEBAR_WIDTH);
    chatViewEl.classList.remove("hidden");
    // renderChat 里那次滚到底发生在对话区还隐藏的时候，不生效；显示出来以后再滚一次，
    // 否则展开看到的是最早的消息而不是最新的
    chatViewEl.scrollTop = chatViewEl.scrollHeight;
    chatViewEl.classList.add("fade-start");
    // 左下角的设置按钮先藏在左边外面，等会话栏滑出来时一起进场
    settingsBtn.classList.add("corner-out");
    void chatViewEl.offsetHeight; // 强制回流，否则下一行的 class 变化不会产生过渡
    appEl.classList.add("is-expanded");
    chatViewEl.scrollTop = chatViewEl.scrollHeight; // 现在才是真正的滚动容器，贴底
    chatViewEl.classList.remove("fade-start"); // 淡入与窗口放大同时进行

    // 第一步：窗口长大、输入框沉底
    await animateSize(EXPANDED_WIDTH, EXPANDED_HEIGHT);

    // 窗口变形结束后箭头才开始转，不和尺寸动画抢注意力
    expandBtn.classList.add("flipped");

    // 第二步：会话栏从左侧滑出来
    sessionListEl.classList.remove("hidden");
    void sessionListEl.offsetHeight;
    sessionListEl.classList.add("open");
    // 如果变形完要把设置开回来，按钮就留在外面，由 openSettings 接着处理
    if (!reopenSettings) settingsBtn.classList.remove("corner-out");
    await sleep(240); // 等侧栏滑完（--t）再解除冻结，这期间对话区宽度不变
    endMorph();
  } else {
    expandBtn.setAttribute("aria-label", "展开历史对话");
    collapseRenderPending = false;
    collapseStreamError = null;

    // 收起一开始就按当前宽度冻结对话区（靠右），侧栏滑走、窗口缩小都不再折行
    freezeChatWidth(chatViewEl.getBoundingClientRect().width);

    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const duration = reduceMotion ? 0 : Math.max(500, morphMs() * 2);

    // 第一步：侧栏完整退出，主面板此时保持展开布局。
    sessionListEl.style.transitionDuration = reduceMotion ? "0s" : "";
    sessionListEl.classList.remove("open");
    settingsBtn.classList.add("corner-out");
    await sleep(reduceMotion ? 0 : morphMs() + 20);
    sessionListEl.classList.add("hidden");
    sessionListEl.style.transitionDuration = "";

    // 第二步：对话与结果作为同一张纸的正反面，连续向前翻过下沿。
    // 输入栏保持正面可见，和翻页的转轴一起上移。
    // Capture before temporary measurement changes scroll layout. The static
    // front contains only this viewport, rather than the whole chat history.
    const front = prepareCalendarFront(chatViewEl);
    const targetH = measureCollapsedHeight();
    lastCollapsedH = targetH;
    const dockRect = dockEl.getBoundingClientRect();
    const mainRect = document.getElementById("main-col").getBoundingClientRect();
    const chatHeight = chatViewEl.clientHeight;
    const sourceW = mainRect.width;
    const chatOffsetX = curW - COLLAPSED_WIDTH;
    appEl.style.setProperty("--collapse-dock-height", dockRect.height + "px");
    // 动画期间撤掉整块窗口的底色与裁切，让独立面板自己形成轮廓。
    // 系统窗口保持原尺寸，等下方输出面板完全展开后再缩小。
    // 最终宽度只排版一次。之后用 transform 还原起始外观并动画归位，
    // 避免 width 动画每帧触发输入栏、命令镜像和 ResizeObserver 的重排。
    appEl.style.width = COLLAPSED_WIDTH + "px";
    appEl.style.height = curH + "px";
    appEl.classList.add("collapsing");
    chatViewEl.style.height = chatHeight + "px";
    // measureCollapsedHeight 已渲染好结果，避免重复解析 Markdown 和代码高亮。
    autoGrowInput();
    appEl.style.setProperty("--collapse-dock-height", dockEl.offsetHeight + "px");
    resultEl.style.visibility = "hidden";
    resultEl.classList.toggle("hidden", !fullText && !reasoningText);
    expandBtn.classList.remove("flipped");
    await calendarFold({
      main: document.getElementById("main-col"), chat: chatViewEl, front,
      result: resultEl, dock: dockEl, height: chatHeight, offsetX: chatOffsetX,
      dockY: dockRect.top - mainRect.top, sourceWidth: sourceW,
      width: COLLAPSED_WIDTH, duration,
    });

    await settleCalendarFold({
      commit() {
        // Commit the real surface in the same microtask as the temporary
        // page is removed. Do not let the native resize race this paint.
        appEl.classList.add("collapse-settling");
        chatViewEl.style.height = "";
        freezeChatWidth(null);
        chatViewEl.classList.remove("fade-start");
        appEl.classList.remove("is-expanded", "collapsing");
        appEl.style.removeProperty("--collapse-dock-height");
        settingsBtn.classList.remove("corner-out");
        // The WebView still has its expanded viewport here. Keep the compact
        // surface pinned until the native resize has actually repainted.
        appEl.style.width = COLLAPSED_WIDTH + "px";
        appEl.style.height = targetH + "px";
      },
      resize: () => applySize(COLLAPSED_WIDTH, targetH),
      release() {
        appEl.style.width = "";
        appEl.style.height = "";
        appEl.classList.remove("collapse-settling");
        endMorph();
        scheduleResize();
      },
    });
  }
  queryInput.focus();

  // 变形全部结束后再把设置开回来，此时布局已经稳定
  if (reopenSettings) openSettings();
}

async function togglePin() {
  pinned = await invoke("toggle_pin");
  // 固定状态没有按钮，靠整圈红色内边框提示
  appEl.classList.toggle("pinned", pinned);
}

// 鼠标中键：窗口任意位置按下都切换固定状态
document.addEventListener("mousedown", (e) => {
  if (e.button !== 1) return;
  e.preventDefault(); // 挡掉 WebView 的中键自动滚动
  togglePin();
});

expandBtn.addEventListener("click", () => setExpanded(!expanded));

// 右下角手柄：交给系统做真正的窗口缩放，比自己算鼠标位移跟手得多。
// 方向参数直接传字符串：@tauri-apps/api 没有导出 ResizeDirection 枚举，
// 而不同版本后端接受的大小写不一样，所以两种都试一遍
async function startCornerResize() {
  for (const dir of ["southEast", "SouthEast"]) {
    try {
      await appWindow.startResizeDragging(dir);
      return;
    } catch (_) {
      // 换下一种写法再试
    }
  }
}

// 鼠标靠近窗口顶部时浮出移动手柄。
// 用 mousemove 判高度而不是 CSS :hover——顶部那条区域在折叠态就是输入框，
// 挂一个撑满宽度的 hover 层会把输入框的点击全吃掉
const GRIP_ZONE = 26;
let moveGripShown = false;
let settingsShown = false;

document.addEventListener("mousemove", (e) => {
  // clientX/Y 是整个透明窗口的坐标，减掉那圈边才是搜索框里的位置
  // 手柄在搜索框上方的透明边里：鼠标在那条边上、或者在搜索框最上面一截时浮出来
  const showMove = e.clientY - PAD_TOP <= GRIP_ZONE;
  if (showMove !== moveGripShown) {
    moveGripShown = showMove;
    if (moveGripEl) moveGripEl.classList.toggle("visible", showMove);
  }

  // 设置面板开着时强制留着，否则鼠标一走开入口就没了、面板却还开着
  const showSettings = e.clientX - PAD <= GRIP_ZONE || settingsOpen;
  if (showSettings !== settingsShown) {
    settingsShown = showSettings;
    settingsBtn.classList.toggle("visible", showSettings);
  }
});

document.addEventListener("mouseleave", () => {
  moveGripShown = false;
  if (moveGripEl) moveGripEl.classList.remove("visible");
  if (!settingsOpen) {
    settingsShown = false;
    settingsBtn.classList.remove("visible");
  }
});

if (gripEl) {
  gripEl.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    userSized = true;
    appEl.classList.add("is-sized");
    startCornerResize();
  });
}

// ---------- 设置面板 ----------

// 在指定位置打一圈涟漪
function splash(x, y) {
  if (!rippleEl) return;
  rippleEl.classList.remove("splash");
  rippleEl.style.left = x + "px";
  rippleEl.style.top = y + "px";
  void rippleEl.offsetWidth; // 不强制回流，animation 不会重新播
  rippleEl.classList.add("splash");
}

// 量出面板完全展开时的真实高度。
// 直接用 CSS 里那个 280px 的上限做动画是不行的：元素实际高度是
// min(max-height, 内容高度)，内容只有 150 左右，于是 max-height 走到 150 时
// 高度就到顶了——只占整条时间轴的前 20%，后面 200ms 画面完全静止，
// 看上去就是"啪"地弹出来，缓动曲线等于没生效
function panelFullHeight() {
  const prev = settingsPanel.style.maxHeight;
  settingsPanel.style.maxHeight = "none";
  const h = settingsPanel.scrollHeight;
  settingsPanel.style.maxHeight = prev;
  void settingsPanel.offsetHeight; // 把起点定回收起态，否则过渡从展开态起算
  return h;
}

function openPanel() {
  settingsPanel.style.maxHeight = panelFullHeight() + "px";
  settingsPanel.classList.remove("collapsed");
}

function collapsePanel() {
  settingsPanel.style.maxHeight = ""; // 交还给 CSS 的 38px / closing 的 0
  settingsPanel.classList.add("collapsed");
}

// 不带动画地把设置收掉并复位。用在窗口即将变形、
// 留着它只会让标题去追一个一直在动的目标的时候
function closeSettingsInstant() {
  settingsAnim++; // 让进行中的开关动画作废
  // 展开态推上去的输入栏瞬间落回原位
  dockEl.style.transition = "none";
  dockEl.style.transform = "";
  void dockEl.offsetWidth;
  dockEl.style.transition = "";
  settingsOpen = false;
  settingsShown = false;
  settingsBtn.classList.remove(
    "visible",
    "docked",
    "titled",
    "no-fly",
    "returning",
    "exit-left",
    "corner-out"
  );
  settingsBtn.style.left = "";
  settingsBtn.style.top = "";
  settingsPanel.style.maxHeight = "";
  settingsPanel.classList.add("hidden");
  settingsPanel.classList.remove("collapsed", "closing");
}

// 标题条是 position:fixed 的，布局一变它就会掉队。
// 锚点取面板自己的上边缘，而不是顶栏底边——展开态下面板在窗口底部、
// 而且窗口高度固定，面板一长高是往上顶的，按顶栏算会整个错位
function syncSettingsTitle(force = false) {
  if (!settingsOpen && !force) return;
  const r = settingsPanel.getBoundingClientRect();
  // 横向也得跟着面板：展开态左边 210px 是会话栏，面板起点在主列里，
  // 写死 left:14 会把标题压在会话栏上。
  // 9.5 = 16(下面各行的内边距) - 6.5(按钮自带的左内边距)，
  // 这样图标左缘和下面的输入框、勾选框是一条竖线
  // getBoundingClientRect 给的是窗口坐标，而 #app 里的 fixed 元素以 #app 为参照，减掉透明边
  settingsBtn.style.left = r.left - PAD + 9.5 + "px";
  settingsBtn.style.top = r.top - PAD_TOP + 10 + "px";
}

// 面板展开、窗口变形这些过程里上边缘一直在动，得逐帧跟住
// keepGoing 默认是"设置还开着"。展开态关闭时标题要跟着下落的面板一起走，
// 那时 settingsOpen 已经是 false，由调用方传自己的判断进来
function followSettingsTitle(duration = 460, keepGoing = () => settingsOpen) {
  const t0 = performance.now();
  function step(now) {
    if (!keepGoing()) return;
    syncSettingsTitle(true);
    if (now - t0 < duration) requestAnimationFrame(step);
  }
  requestAnimationFrame(step);
}

// 水滴：加速下落 → 落地 + 涟漪 → 化开成标题 → 设置项漫下来
let settingsAnim = 0;
async function openSettings() {
  const token = ++settingsAnim;
  settingsOpen = true;
  settingsShown = true;
  // 回填已存的 Key，方便核对和替换。每次打开都退回遮蔽状态，
  // 免得上次点开看过之后就一直明文摆在那儿
  apiKeyInput.type = "password";
  keyEye.classList.remove("on");
  invoke("get_api_key").then((k) => {
    apiKeyInput.value = k;
  });
  settingsBtn.classList.add("visible");

  // 面板先出现但只占标题那一条的高度
  settingsPanel.classList.add("collapsed");
  settingsPanel.classList.remove("hidden");
  void settingsPanel.offsetHeight; // 强制回流，让后面的过渡有起点

  // 展开态：左下角的按钮往左退出去，面板晚半拍从底部升起来，
  // 像是它退出去时把设置拉了上来。标题是面板自带的（#settings-heading），跟着面板一起升
  if (expanded) {
    settingsBtn.classList.add("corner-out");
    await sleep(70);
    if (token !== settingsAnim) return;
    // 面板挂在输入栏下沿外面，量出它的高度，把输入栏 + 面板整块推上来
    dockEl.style.transform = `translateY(${-settingsPanel.offsetHeight}px)`;
    settingsPanel.classList.remove("collapsed"); // 设置项错峰淡入
    // preventScroll：这一刻输入框还在窗口外面，不加的话浏览器会先把整个窗口内容滚过去找它
    apiKeyInput.focus({ preventScroll: true });
    return;
  }

  // 停靠点按面板自己的左上角算，折叠态和展开态都成立。
  // 折叠态下面板左边缘就是 0，算出来仍是 14，垂直下落不受影响
  const panelRect = settingsPanel.getBoundingClientRect();
  const dropLeft = panelRect.left - PAD + 9.5;
  const dropTop = panelRect.top - PAD_TOP + 10;

  // 第一拍：加速下落，途中被拉长
  settingsBtn.style.left = dropLeft + "px";
  settingsBtn.style.top = dropTop + "px";
  settingsBtn.classList.add("docked");
  followContent(220); // 跟住那 38px 标题留白长出来的过程
  await sleep(135);
  if (token !== settingsAnim) return; // 中途被关掉了

  // 落地：涟漪散开、化开成标题条、设置项漫下来，全在同一帧。
  // 不做落地压扁回弹那一下——多出来的那段等待不值当
  // 涟漪起点就是图标中心，从那里往右绽开
  splash(dropLeft + 15, dropTop + 15);
  settingsBtn.classList.add("titled");
  openPanel();
  followContent(320);
  followSettingsTitle(460); // 面板长高时上边缘会移动，标题得跟着
  apiKeyInput.focus();
}

async function closeSettings() {
  const token = ++settingsAnim;
  settingsOpen = false;

  // 展开态：跟打开反过来。面板连同标题一起往下落，按钮随后从左边滑回左下角
  if (expanded) {
    dockEl.style.transform = ""; // 整块落回去
    settingsPanel.classList.add("collapsed"); // 设置项同时淡出
    await sleep(110);
    if (token !== settingsAnim) return;
    settingsBtn.classList.remove("corner-out");
    await sleep(200);
    if (token !== settingsAnim) return;

    settingsShown = false;
    settingsPanel.classList.add("hidden");
    settingsPanel.classList.remove("collapsed", "closing");
    return;
  }

  collapsePanel();
  followContent(720); // 整个关闭流程约 650ms，窗口高度得一路跟到底

  // 第一步：收成圆气泡，并回到搜索栏那一行。
  // 必须带上顶栏自己的 top——展开态下搜索栏在窗口底部，只按高度算会飞到顶上去
  const rowRect = topRowEl.getBoundingClientRect();
  const restTop = rowRect.top - PAD_TOP + rowRect.height / 2 - 15;
  settingsBtn.classList.remove("titled");
  settingsBtn.classList.add("returning");
  settingsBtn.style.top = restTop + "px";
  await sleep(330);
  if (token !== settingsAnim) return;

  // 气泡已经离开面板区域，那 38px 的标题留白现在可以一起收掉
  settingsPanel.classList.add("closing");

  // 第二步：从左边缘滑出去
  settingsBtn.classList.add("exit-left");
  await sleep(320);
  if (token !== settingsAnim) return;

  // 趁已经看不见了做瞬时复位，下次浮现时不会带着上一次的位移
  settingsShown = false;
  settingsBtn.classList.remove("visible", "docked", "returning", "exit-left");
  settingsBtn.style.left = "";
  settingsBtn.style.top = "";
  void settingsBtn.offsetWidth;

  settingsPanel.classList.add("hidden");
  settingsPanel.classList.remove("collapsed", "closing");
  followContent(120);
}

searchCheckbox.addEventListener("change", async () => {
  try {
    await invoke("set_web_search", { value: searchCheckbox.checked });
  } catch (err) {
    searchCheckbox.checked = !searchCheckbox.checked;
  }
});

deepCheckbox.addEventListener("change", async () => {
  try {
    await invoke("set_deep_thinking", { value: deepCheckbox.checked });
  } catch (err) {
    deepCheckbox.checked = !deepCheckbox.checked;
  }
});

autostartCheckbox.addEventListener("change", async () => {
  try {
    await invoke("set_autostart", { value: autostartCheckbox.checked });
  } catch (err) {
    // 写注册表失败就把勾回退，免得界面和实际状态对不上
    autostartCheckbox.checked = !autostartCheckbox.checked;
  }
});

keyEye.addEventListener("click", (e) => {
  e.preventDefault(); // 它在 <label> 里，不拦的话点击会连带触发聚焦输入框
  const reveal = apiKeyInput.type === "password";
  apiKeyInput.type = reveal ? "text" : "password";
  keyEye.classList.toggle("on", reveal);
});

proxyCheckbox.addEventListener("change", async () => {
  try {
    await invoke("set_use_system_proxy", { value: proxyCheckbox.checked });
  } catch (err) {
    // 存盘失败就把勾回退，免得界面和实际配置对不上
    proxyCheckbox.checked = !proxyCheckbox.checked;
  }
});

// 展开态下设置按钮打开后就退出去了，没有东西可以再点一下关掉。
// 所以点面板以外的任何地方都算关闭（Esc 也照样能关）
document.addEventListener(
  "mousedown",
  (e) => {
    if (!expanded || !settingsOpen || e.button !== 0) return;
    if (settingsPanel.contains(e.target) || settingsBtn.contains(e.target)) return;
    closeSettings();
  },
  true
);

// 展开态面板上的标题：点一下收起设置，跟折叠态点标题一样
document.getElementById("settings-heading").addEventListener("click", () => {
  closeSettings();
  queryInput.focus();
});

settingsBtn.addEventListener("click", () => {
  if (settingsOpen) {
    closeSettings();
    queryInput.focus();
  } else {
    openSettings();
  }
});

apiKeyInput.addEventListener("keydown", async (e) => {
  if (e.key === "Enter" && apiKeyInput.value.trim()) {
    try {
      await invoke("save_api_key", { key: apiKeyInput.value.trim() });
    } catch (err) {
      // 多半是配置文件正处在改坏的状态，拒绝写入。留在面板里，把原因给出来
      apiKeyInput.value = "";
      apiKeyInput.placeholder = "保存失败：配置文件有误，请通过 /config 修正";
      return;
    }
    apiKeyInput.value = "";
    apiKeyInput.placeholder = "粘贴后按 Enter 保存";
    closeSettings();
    queryInput.focus();
  }
});

// ---------- /update 自动更新 ----------
//
// 后台发现新版本时（Rust 发 update-available），输入框下方显示一行提示；
// 查看过这个版本的说明后不再提示，直到有更新的版本

const UPDATE_SEEN_KEY = "shannon.update.seen";
let updateBar = null;
let updateTarget = null; // 正在下载的版本号

function showUpdateBar(info) {
  let seen = null;
  try {
    seen = localStorage.getItem(UPDATE_SEEN_KEY);
  } catch (_) {}
  if (!info || seen === info.version) {
    updateBar.classList.add("hidden");
  } else {
    updateBar.innerHTML =
      `<span class="upd-text">新版本 v${escapeText(info.version)} 可用</span>` +
      `<span class="upd-action">/update 查看</span>`;
    updateBar.classList.remove("hidden");
  }
  if (!expanded) scheduleResize();
}

function markUpdateSeen(version) {
  try {
    localStorage.setItem(UPDATE_SEEN_KEY, version);
  } catch (_) {}
  updateBar.classList.add("hidden");
  if (!expanded) scheduleResize();
}

async function updateCommand(arg) {
  if (arg === "install") {
    showLocal("正在检查更新…");
    try {
      const info = await invoke("check_update");
      if (!info) {
        showLocal(`当前已是最新版本（v${await invoke("app_version")}）。`);
        return;
      }
      updateTarget = info.version;
      markUpdateSeen(info.version);
      showLocal(`**正在下载 v${info.version}** <span id="update-progress">0%</span>`);
      // Windows 上安装程序启动后闪念会退出，这个调用不会返回
      await invoke("install_update");
    } catch (err) {
      showLocal("更新失败：" + err);
    }
    return;
  }
  if (arg) {
    showLocal("用法：`/update` 检查新版本，`/update install` 下载并安装");
    return;
  }

  showLocal("正在检查更新…");
  try {
    const info = await invoke("check_update");
    if (!info) {
      showLocal(`当前已是最新版本（v${await invoke("app_version")}）。`);
      showUpdateBar(null);
      return;
    }
    markUpdateSeen(info.version);
    const date = info.date ? `，发布于 ${info.date}` : "";
    const notes = info.notes.trim() ? `\n\n${info.notes.trim()}` : "";
    showLocal(
      `**发现新版本 v${info.version}**（当前 v${info.current}${date}）${notes}\n\n` +
        "使用 `/update install` 下载并安装，安装完成后闪念将自动重新启动。"
    );
  } catch (err) {
    showLocal("检查更新失败：" + err);
  }
}

listen("update-available", (event) => showUpdateBar(event.payload));

listen("update-progress", (event) => {
  const [received, total] = event.payload;
  const el = document.getElementById("update-progress");
  if (!el) return;
  el.textContent = total ? `${Math.floor((received / total) * 100)}%` : `${(received / 1048576).toFixed(1)} MB`;
});

listen("update-downloaded", () => {
  showLocal(`**v${updateTarget || ""} 下载完成**，正在启动安装程序。安装完成后闪念将自动重新启动。`);
});

// ---------- /spt 系统提示词 ----------

async function sptCommand(arg) {
  const a = arg.toLowerCase();
  if (a === "edit") {
    try {
      const path = await invoke("open_prompts");
      invalidatePrompts();
      return `已打开提示词文件，保存后下次提问生效。\n\n\`${path}\``;
    } catch (err) {
      return "无法打开提示词文件：" + err;
    }
  }
  if (/^\d+$/.test(a)) {
    try {
      const name = await invoke("set_prompt", { id: Number(a) });
      invalidatePrompts();
      return a === "0"
        ? "已停用系统提示词，下次提问生效。使用 `/spt 1` 恢复默认。"
        : `已切换至提示词 ${a}「${name}」，下次提问生效。`;
    } catch (err) {
      return String(err);
    }
  }
  if (a) return "用法：`/spt 序号` 切换提示词，`/spt edit` 编辑提示词文件，`/spt` 查看列表";

  const list = await invoke("list_prompts");
  const cur = list.active === 0 ? "不使用" : (list.items.find((p) => p.id === list.active) || {}).name;
  const mark = (id) => (id === list.active ? " · **当前**" : "");
  let md = `**系统提示词** · 当前：${cur ? `${list.active}「${cur}」` : `${list.active}（提示词文件中不存在，视为不使用）`}\n\n`;
  md += `- \`0\` 不使用${mark(0)}\n`;
  md += list.items.map((p) => `- \`${p.id}\` **${p.name}** · ${p.preview}${mark(p.id)}`).join("\n");
  if (list.error) md += `\n\n**提示词文件有误**，当前沿用上次有效的内容：\n\n\`\`\`\n${list.error}\n\`\`\``;
  md += "\n\n`/spt 序号` 切换，`/spt edit` 编辑提示词文件";
  return md;
}

// ---------- 输入历史：↑ / ↓ 翻之前发过的内容 ----------
//
// 问题和命令都记，像终端一样。存在本机 localStorage 里，只留最近 HISTORY_MAX 条

const HISTORY_KEY = "shannon.history";
let HISTORY_MAX = 100; // config.toml 的 [history] input_history，0 表示不记

let history = loadHistory(); // 旧 → 新
let histPos = -1;            // -1 表示没在翻历史，否则是 history 的下标
let histDraft = "";          // 开始翻之前框里的内容，翻回最底下时还给用户

function loadHistory() {
  try {
    const h = JSON.parse(localStorage.getItem(HISTORY_KEY) || "[]");
    // 这里先不按条数截：配置还没读到，截了就把多存的那些丢了
    return Array.isArray(h) ? h.filter((x) => typeof x === "string") : [];
  } catch (_) {
    return [];
  }
}

function saveHistory() {
  if (history.length > HISTORY_MAX) history.splice(0, history.length - HISTORY_MAX);
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(history));
  } catch (_) {}
}

function pushHistory(text) {
  histPos = -1;
  if (HISTORY_MAX <= 0) return;
  if (history[history.length - 1] === text) return; // 连着发同一条只记一次
  history.push(text);
  saveHistory();
}

function showHistoryItem(text) {
  queryInput.value = text;
  autoGrowInput();
  scheduleResize();
  queryInput.setSelectionRange(text.length, text.length);
}

queryInput.addEventListener("keydown", (e) => {
  if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
  if (e.isComposing || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;

  const v = queryInput.value;
  const { selectionStart: start, selectionEnd: end } = queryInput;

  if (e.key === "ArrowUp") {
    // 多行输入时，光标不在第一行就照常往上移一行，不翻历史
    if (v.slice(0, start).includes("\n")) return;
    if (histPos === -1) histDraft = v;
    let i = histPos === -1 ? history.length : histPos;
    // 跳过和框里一模一样的：折叠态发完问题不清空输入框，第一下 ↑ 不该原地不动
    do i--; while (i >= 0 && history[i] === v);
    if (i < 0) return; // 已经是最早的一条
    e.preventDefault();
    histPos = i;
    showHistoryItem(history[i]);
  } else {
    if (histPos === -1) return;
    if (v.slice(end).includes("\n")) return; // 光标不在最后一行
    e.preventDefault();
    let i = histPos;
    do i++; while (i < history.length && history[i] === v);
    if (i >= history.length) {
      histPos = -1;
      showHistoryItem(histDraft);
    } else {
      histPos = i;
      showHistoryItem(history[i]);
    }
  }
});

// ---------- 提问 ----------

let noteCommandBusy = false;
queryInput.addEventListener("keydown", async (e) => {
  // Shift+回车换行，单独回车才发送
  if (e.key !== "Enter" || e.shiftKey || e.isComposing) return;
  e.preventDefault(); // 不拦的话 textarea 会先插入一个换行
  if (!queryInput.value.trim()) return;

  const prompt = queryInput.value.trim();
  pushHistory(prompt);
  componentPanelActive = false;
  pluginPanelActive = false;
  const pluginEpoch = ++pluginCommandEpoch;
  let pluginTargets = [];

  try {
    const command = parsePluginCommand(prompt);
    const mentions = !command && !/^[\/／]/.test(prompt) ? parsePluginMentions(prompt,mentionPlugins) : null;
    pluginTargets = mentions?.targets || [];
    const plugin = command || mentions?.direct;
    if (plugin) {
      pluginPanelActive = plugin.action !== 'builtin';
      if (plugin.action === 'help') { showLocal(pluginHelp); return; }
      if(plugin.action==='manage'){await invoke('plugin_manager_open');return;}
      if (plugin.action !== 'builtin') await pluginRuntimeReady;
      let data;
      if (plugin.action === 'list') data = await invoke('plugin_list');
      else if (plugin.action === 'info') data = await invoke('plugin_info', {id:plugin.id});
      else if (plugin.action === 'install') {
        showLocal('正在读取插件包，随后将展示来源与权限供你确认…');
        data = await invoke('plugin_install', {source:plugin.source});
      } else if (plugin.action === 'builtin') {
        data = await invoke('builtin_mention_run', {id:plugin.id,tool:plugin.tool,args:plugin.args});
      } else if (plugin.action === 'run') {
        showLocal('正在执行插件工具…');
        data = await invoke('plugin_run', {id:plugin.id,tool:plugin.tool,args:plugin.args});
      } else data = await invoke('plugin_change', {id:plugin.id,action:plugin.action});
      if (pluginEpoch === pluginCommandEpoch) {
        showLocal(data.confirmation_opened ? '已打开插件确认窗口，确认后才会生效。'
          : plugin.action === 'list' ? formatPluginList(data)
          : plugin.action === 'info' ? formatPluginInfo(data)
          : plugin.action === 'builtin' ? formatBuiltinResult(plugin.id,plugin.tool,data)
          : pluginValue(data) + '\n\n输入 `/plugin help` 查看插件用法。');
      }
      return;
    }
  } catch(error) {
    if (pluginEpoch === pluginCommandEpoch) showLocalKeep('工具调用提示：\n\n' + pluginValue(String(error)),prompt);
    return;
  }

  try {
    const component = parseComponentCommand(prompt);
    if (component) {
      if (component.action === 'help') showLocal(componentHelp);
      else if (component.action === 'status') {
        componentPanelActive = true;
        showLocal(formatComponents(await invoke('component_status')));
      } else {
        componentPanelActive = true;
        showLocal('正在准备下载组件…');
        componentProgressRow = expanded ? messages.at(-1) : null;
        const result = await invoke('component_install', { name: component.name });
        if (result.installed) showLocal('该组件已安装，可直接使用。');
      }
      return;
    }
  } catch (error) { showLocal(String(error)); return; }

  // 内部命令在本地处理，不走网络
  const gestureMatch = prompt.match(/^[\/／](?:gesture|手势)(?:\s+([\s\S]*))?$/i);
  if (gestureMatch) {
    try {
      const command = parseGestureCommand(gestureMatch[1] || "");
      if (command.help) showLocal(gestureHelp);
      else {
        const data = await invoke("gesture_control", { action: command.action, payload: command.payload });
        showLocal(formatGestureResult(command.action, data));
      }
    } catch (err) {
      showLocal("手势鼠标：" + String(err));
    }
    return;
  }
  const noteCommand = prompt.match(/^[\/／](?:note|便签)(?:\s+([\s\S]*))?$/i);
  if (noteCommand) {
    if (noteCommandBusy) return;
    noteCommandBusy = true;
    try {
      if ((noteCommand[1] || "").trim() === "--close") await invoke("close_notes");
      else await invoke("open_notes", { text: noteCommand[1] || "" });
      if (queryInput.value.trim() === prompt) {
        queryInput.value = "";
        queryInput.dispatchEvent(new Event("input", { bubbles: true }));
      }
    } catch (err) { showLocal("无法打开便签：" + String(err)); }
    finally { noteCommandBusy = false; }
    return;
  }
  const clockCommand = prompt.match(/^[\/／]clock(?:\s+([\s\S]*))?$/i);
  if (clockCommand) {
    const arg = (clockCommand[1] || "").trim().toLowerCase();
    const restart = /^restart(?:\s|$)/.test(arg);
    const options = restart ? arg.replace(/^restart\s*/, "") : arg;
    const mode = options === "up" ? "up" : "down";
    const value = options.replace(/^down\s*/i, "").trim();
    const duration = mode === "down" && value ? parseDuration(value) : 0;
    if (duration === null) {
      showLocal("用法：`/clock` 打开计时器，`/clock up` 正计时，`/clock 5m` 倒计时。时长须大于 0 且不超过 24 小时。");
      return;
    }
    try {
      const opened = await invoke("open_clock", { mode, seconds: Math.round(duration / 1000), restart });
      if (!opened) {
        showLocal("已有计时器正在运行，可输入 `/clock restart` 重新开设一个新的计时器。");
        queryInput.value = "/clock restart";
        queryInput.focus();
        queryInput.setSelectionRange(queryInput.value.length, queryInput.value.length);
        queryInput.dispatchEvent(new Event("input", { bubbles: true }));
      }
    } catch (err) {
      showLocal("无法打开计时器：" + String(err));
    }
    return;
  }
  if (prompt === "/?" || prompt === "/？" || prompt === "/help") {
    showHelp();
    return;
  }
  if (prompt === "/version" || prompt === "/v") {
    showVersion();
    return;
  }
  if (/^[\/／]config$/i.test(prompt)) {
    try {
      const path = await invoke("open_config");
      showLocal(`已打开配置文件，保存后立即生效。\n\n\`${path}\``);
    } catch (err) {
      showLocal("无法打开配置文件：" + err);
    }
    return;
  }
  const upd = prompt.match(/^[\/／]update(?:\s+([\s\S]*))?$/i);
  if (upd) {
    updateCommand((upd[1] || "").trim().toLowerCase());
    return;
  }
  const spt = prompt.match(/^[\/／]spt(?:\s+([\s\S]*))?$/i);
  if (spt) {
    showLocal(await sptCommand((spt[1] || "").trim()));
    return;
  }
  const th = prompt.match(/^[\/／]theme(?:\s+([\s\S]*))?$/i);
  if (th) {
    showLocal(await themeCommand(th[1] || ""));
    return;
  }
  // 全角斜杠也认，中文输入法下常常打出来的是它
  const cmd = prompt.match(/^[\/／](o|rmbr)(?:\s+([\s\S]*))?$/i);
  if (cmd) {
    const arg = (cmd[2] || "").trim();
    if (cmd[1].toLowerCase() === "o") openCommand(arg, prompt);
    else rmbrCommand(arg, prompt);
    return;
  }
  // 打开模式：不是命令的输入一律当成 /o 的参数
  if (oMode) {
    if (!pluginTargets.length) { openCommand(prompt, prompt); return; }
  }

  if (streaming !== null) return;
  if (helpShown) closeHelp();
  fullText = "";
  reasoningText = "";
  streaming = "";
  followBottom = true;

  if (expanded) {
    queryInput.value = "";
    autoGrowInput();
    messages.push({ role: "user", content: prompt });
    renderChat();
  } else {
    resultEl.innerHTML = "";
    resultEl.classList.remove("hidden");
    scheduleResize();
  }

  try {
    const now = new Date();
    const pad = (n) => String(n).padStart(2, "0");
    await invoke("ask_deepseek", {
      prompt,
      pluginTargets,
      date: `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`,
      weekday: "星期" + "日一二三四五六"[now.getDay()],
    });
  } catch (err) {
    streaming = null;
    if (!expanded) {
      resultEl.textContent = "请求失败：" + err;
      scheduleResize();
    }
  }
});

// ---------- 全局快捷键 ----------

document.addEventListener("keydown", async (e) => {
  if (e.isComposing) return;

  if (e.key === "Escape") {
    e.preventDefault();
    if (settingsOpen) {
      closeSettings();
      queryInput.focus();
      return;
    }
    // /? 的说明先关掉
    if (helpShown) {
      closeHelp();
      queryInput.focus();
      return;
    }
    // 打开模式不归 Esc 管，只有单按 Alt 才退出；Esc 照常收起 / 隐藏窗口
    // 展开态先收起，再按一次才隐藏窗口
    if (expanded) {
      await setExpanded(false);
      return;
    }
    await invoke("hide_window");
    return;
  }

  if (e.ctrlKey && !e.altKey && e.key === "l") {
    e.preventDefault();
    await startNewSession();
    return;
  }

  if (e.ctrlKey && !e.altKey && e.key === "e") {
    e.preventDefault();
    await setExpanded(!expanded);
  }
});

// ---------- 流式输出时跟到底部 ----------
//
// 回答在长，默认一直贴着最底下。一旦用滚轮往上翻，就不再往下带，让人安心看；
// 自己滚回最底下时恢复跟随。每次新提问都重新开始跟随

let followBottom = true;

function nearBottom(el) {
  return el.scrollHeight - el.scrollTop - el.clientHeight < 24;
}

for (const el of [resultEl, chatViewEl]) {
  el.addEventListener(
    "wheel",
    (e) => {
      if (e.deltaY < 0) {
        followBottom = false;
      } else {
        // 等这一下滚完再看是不是到底了
        requestAnimationFrame(() => {
          if (nearBottom(el)) followBottom = true;
        });
      }
    },
    { passive: true }
  );
}

function keepBottom(el) {
  if (followBottom) el.scrollTop = el.scrollHeight;
}

// ---------- 后端事件 ----------

listen("tool-status", (event) => {
  if (streaming === null || event.payload.phase !== "running") return;
  reasoningText += `正在处理：${String(event.payload.description || "本地工具").split("。")[0]}…\n`;
  if (morphing && !expanded) { collapseRenderPending = true; return; }
  if (expanded) renderChat();
  else {
    resultEl.classList.remove("hidden");
    renderResult(fullText);
    scheduleResize();
  }
});

listen("stream-reasoning", (event) => {
  reasoningText += event.payload;
  if (morphing && !expanded) {
    collapseRenderPending = true;
    return;
  }
  if (expanded) {
    renderChat();
  } else {
    // 推理阶段还没有正文，也得让窗口长出来，不然看不到任何反馈
    resultEl.classList.remove("hidden");
    renderResult(fullText);
    keepBottom(resultEl);
    scheduleResize();
  }
});

listen("stream-chunk", (event) => {
  fullText += event.payload;
  streaming = (streaming === null ? "" : streaming) + event.payload;
  if (morphing && !expanded) {
    collapseRenderPending = true;
    return;
  }

  if (expanded) {
    renderChat();
  } else {
    const top = resultEl.scrollTop;
    renderResult(fullText);
    if (followBottom) keepBottom(resultEl);
    else resultEl.scrollTop = top;
    scheduleResize();
  }
});

listen("stream-done", async () => {
  streaming = null;
  // 直接从后端重新取一遍，保证界面和落盘的内容完全一致
  const id = await invoke("current_session_id");
  messages = await invoke("get_messages", { id });
  if (morphing && !expanded) {
    collapseRenderPending = true;
    followBottom = true;
    return;
  }
  if (expanded) {
    renderChat();
    await refreshSessions();
  } else if (fullText) {
    const top = resultEl.scrollTop;
    renderResult(fullText); // 重渲染一次，这回会带语法高亮
    resultEl.scrollTop = top; // 高亮后内容高度可能略变，停在原处
  }
  // 这一轮结束了，之后切会话、翻历史等都照常贴底
  followBottom = true;
});

listen("stream-error", (event) => {
  streaming = null;
  if (morphing && !expanded) {
    collapseRenderPending = true;
    collapseStreamError = event.payload;
    return;
  }
  if (expanded) {
    messages.push({ role: "error", content: "请求失败：" + event.payload });
    renderChat();
  } else {
    resultEl.textContent = "请求失败：" + event.payload;
    resultEl.classList.remove("hidden");
    scheduleResize();
  }
});

let showMotions = [];
let showMotionId = 0;
function stopShowMotion() {
  ++showMotionId;
  for (const motion of showMotions) motion.cancel();
  showMotions = [];
}

function animateWindowShown() {
  const fromOpacity = getComputedStyle(document.body).opacity;
  const fromTransform = getComputedStyle(appEl).transform;
  stopShowMotion();
  const id = showMotionId;
  document.documentElement.classList.remove("window-hidden", "window-hiding");
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const options = { duration: 290, easing: "cubic-bezier(.22,1,.36,1)", fill: "both" };
  showMotions.push(document.body.animate([{ opacity: fromOpacity }, { opacity: 1 }], options));
  // 展开/翻折尚未结束时只淡入，避免两段 transform 同时控制同一张面板。
  if (!sizeLocked()) {
    showMotions.push(appEl.animate([
      { transform: fromTransform === "none" ? "translateY(5px) scale(.985)" : fromTransform, transformOrigin: "50% 0" },
      { transform: "translateY(0) scale(1)", transformOrigin: "50% 0" },
    ], options));
  }
  Promise.all(showMotions.map((motion) => motion.finished)).then(() => {
    if (id === showMotionId) stopShowMotion();
  }).catch(() => {}); // 快速隐藏/再次唤起会正常取消上一段动画。
}

listen("window-hidden", () => {
  document.documentElement.classList.add("window-hidden");
  document.documentElement.classList.remove("window-hiding");
  stopShowMotion();
});

listen("window-hiding", async ({ payload: token }) => {
  const fromOpacity = getComputedStyle(document.body).opacity;
  const fromTransform = getComputedStyle(appEl).transform;
  stopShowMotion();
  const id = showMotionId;
  document.documentElement.classList.add("window-hiding");
  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const options = { duration: reduceMotion ? 0 : 220, easing: "cubic-bezier(.4,0,.6,1)", fill: "both" };
  showMotions.push(document.body.animate([{ opacity: fromOpacity }, { opacity: 0 }], options));
  if (!sizeLocked()) {
    showMotions.push(appEl.animate([
      { transform: fromTransform, transformOrigin: "50% 0" },
      { transform: "translateY(5px) scale(.985)", transformOrigin: "50% 0" },
    ], options));
  }
  try {
    await Promise.all(showMotions.map((motion) => motion.finished));
    if (id === showMotionId) await invoke("finish_hide_window", { token });
  } catch (_) {} // 再次唤起会取消退出动画；后端也会校验 token，防止迟到的隐藏。
});

// 每次热键/托盘重新唤起：聚焦并全选输入框，不清空上次的内容。
// payload 是唤起前在别的程序里选中的文字（没有就是 null）
listen("window-shown", (event) => {
  // 打开模式跨隐藏保留。窗口藏着时尺寸可能变过，重新贴一下荧光
  if (oMode) layoutOBorder();
  const selected = event.payload;
  if (selected) {
    // 选中的文字替换掉上次留下的内容，然后和平时唤起一样全选：
    // 直接打字就整段替换，按 → 或 End 取消选择、接着往后补
    queryInput.value = selected;
    autoGrowInput(); // 里面会顺带 syncMirror
    scheduleResize();
  } else {
    syncMirror();
  }
  queryInput.focus();
  queryInput.select();
  // 多行时从第一行看起
  queryInput.scrollTop = 0;
  animateWindowShown();
  // 节日主题：跨过午夜要换主题，当天第一次唤起放特效
  themeOnShown();
});

// ---------- 启动 ----------

// ---------- 配置文件（config.toml） ----------
//
// 所有设置都在 config.toml 里，Rust 负责读写和监视文件变化。
// 前端只管自己用得到的几项：窗口尺寸、占位语、输入历史条数、主题模式、设置面板里的开关

function applyUiConfig(c) {
  COLLAPSED_WIDTH = c.width;
  MAX_COLLAPSED_HEIGHT = c.max_height;
  EXPANDED_WIDTH = c.expanded_width;
  EXPANDED_HEIGHT = c.expanded_height;
  HISTORY_MAX = c.input_history;
  if (history.length > HISTORY_MAX) saveHistory();
  setDefaultPlaceholder(c.placeholder || "For you, always.");
  setThemeMode(c.theme_mode || "auto");
  deepCheckbox.checked = c.deep_thinking;
  applyModelSelection(c.model);
  searchCheckbox.checked = c.web_search;
  proxyCheckbox.checked = c.use_system_proxy;
  autostartCheckbox.checked = c.autostart;
  if (!expanded) scheduleResize(); // 宽度可能变了；展开态下次展开时才用新尺寸
}

// 配置文件改坏了：告诉用户哪里错了，程序继续用上一次正确的设置
let configErrorShown = false;
function showConfigError(msg) {
  configErrorShown = true;
  showLocal(
    "**配置文件有误**，修正前将沿用上次有效的设置。\n\n" +
      "```\n" + String(msg).replace(/```/g, "'''") + "\n```\n\n使用 `/config` 打开配置文件"
  );
}

listen("config-changed", (event) => {
  applyUiConfig(event.payload);
  // 之前显示着配置错误的话，改好了就收起来
  if (configErrorShown && helpShown) closeHelp();
  configErrorShown = false;
});

listen("config-error", (event) => showConfigError(event.payload));

// ---------- 启动 ----------

async function init() {
  autoGrowInput();
  let ui = await invoke("get_ui_config");
  // 老版本把 /theme 的选择存在浏览器存储里，搬进 config.toml
  let legacyTheme = null;
  try {
    legacyTheme = localStorage.getItem("shannon.theme");
    localStorage.removeItem("shannon.theme");
  } catch (_) {}
  if (legacyTheme && ui.theme_mode === "auto") {
    try {
      await invoke("set_theme_mode", { mode: legacyTheme });
      ui = { ...ui, theme_mode: legacyTheme };
    } catch (_) {}
  }
  applyUiConfig(ui);
  // 主题不挡启动：读不出来就是默认外观
  initThemes({ appEl, topRowEl, queryInput, anchorEl: expandBtn }).catch(() => {});
  // 命令提示挂在输入栏下面、设置面板上面；出现 / 消失时窗口跟着改高度
  initCmdHint({
    queryInput,
    dockEl,
    beforeEl: settingsPanel,
    onChange: () => {
      syncMirror();
      if (!expanded) scheduleResize();
    },
    // 正在 ↑↓ 翻输入历史时，下拉不接管方向键
    isBrowsingHistory: () => histPos !== -1,
  });
  // 新版本提示：挂在命令提示下面、设置面板上面，点一下填入 /update
  updateBar = document.createElement("div");
  updateBar.id = "update-bar";
  updateBar.className = "hidden";
  dockEl.insertBefore(updateBar, settingsPanel);
  updateBar.addEventListener("mousedown", (e) => {
    e.preventDefault();
    queryInput.value = "/update";
    queryInput.focus();
    queryInput.dispatchEvent(new Event("input", { bubbles: true }));
  });
  invoke("pending_update").then(showUpdateBar).catch(() => {});
  pinned = await invoke("is_pinned");
  appEl.classList.toggle("pinned", pinned);

  const id = await invoke("current_session_id");
  messages = await invoke("get_messages", { id });

  const hasKey = await invoke("has_api_key");
  if (!hasKey) {
    // 还没设置过 Key：自动展开一次，方便直接填
    openSettings();
  } else {
    scheduleResize();
    queryInput.focus();
  }

  // 启动时配置文件就有问题（或热键注册不上），等窗口第一次出来时能看到
  const err = await invoke("config_error");
  if (err) showConfigError(err);
}

init();
