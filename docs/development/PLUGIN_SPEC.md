# 闪念工具插件规范 · API v1

闪念插件是 ZIP 包，通过搜索框安装，无需修改或重新编译主程序。插件用 JavaScript 实现计算/文本处理工具，并可申请一组受限的宿主能力。启用后，手动命令和普通对话/联网对话中的 AI 工具共用同一执行器。

## 安装与使用

便签和计时器也可通过 @ 调用，无需安装：`@note 记下买牛奶`、`@clock 设置5分钟倒计时`，支持中文 `@便签`、`@计时器`。单独发送 `@note` 展开便签，单独发送 `@clock` 打开计时设置；计时仍需用户确认开始。可选具体操作，例如 `@note/list`、`@note/add`、`@clock/status`、`@clock/open`。

内置与第三方同名时，`@note` / `@clock` 优先代表内置工具，第三方在列表中显示为 `@plugin/note` 等命名空间；插件清单 ID、斜杠命令和 AI 工具名无需迁移。

输入 `@` 可以按插件名称或工具名搜索，↑↓ 选择、Tab/Enter 插入，随后直接输入自然语言需求。例如 `@quick-stats 计算 2、4、6 的平均值`，或 `@quick-stats/summarize 求这些数字的中位数`。选择插件时向本轮 AI 提供该插件的全部工具；选择具体工具则只提供对应工具。多个 @ 可组合使用，最多8个目标。后端再次检查启用状态与本轮工具范围，不通过在提示词中拼接代码执行功能。

自然语言调用使用现有 AI 接口，需要配置可工具调用的模型；参数不足时由 AI 询问。也可输入 `@quick-stats {"values":[2,4,6]}` 或 `@quick-stats/summarize {"values":[2,4,6]}` 直接离线执行。仅指定插件并传 JSON 时，插件必须只有一个工具，否则需要选到具体工具。邮箱、代码及引用中的 @ 不选择工具。未找到或已停用的插件会明确提示，保留输入供修改。

```text
/plugin install E:\插件\quick-stats-1.0.0.zip
/plugin install https://example.com/quick-stats-1.0.0.zip
/plugin list
/plugin info quick-stats
/plugin run quick-stats/summarize {"values":[2,4,6]}
/quick-stats/summarize {"values":[2,4,6]}
/plugin disable quick-stats
/plugin enable quick-stats
/plugin uninstall quick-stats
```

路径可包含空格，也可以加双引号。安装、更新、启停和卸载都由独立应用窗口确认，显示插件名称、版本、作者、完整来源、包 SHA-256、权限及工具。安装确认针对已经读取到内存的确切包，源文件之后变化不会替换待批准的代码。同 ID 更新完整替换清单/代码/权限，每次重新确认，不静默增加权限。

包的 SHA-256 是内容标识，不代表开发者身份或可信签名。v1 第三方包尚无发布者签名认证；来源和发布者应由用户核对。安装仅保存代码和注册工具，不运行入口；入口只在实际工具调用时启动。

安装后可以对 AI 说“用快速统计插件计算 2、4、6 的平均值”。AI 会看到 `plg__quick_stats__summarize`，其参数与手动命令一致。确认安装返回 `confirmation_opened:true`，不代表已安装；批准后重新查询目录或发起下一次对话。正在进行的模型请求不会中途修改已经发送的工具目录。

停用立即阻止后续调用及宿主访问；卸载取消命令/AI 注册。插件数据和包缓存保留在本机，重新安装同 ID 后可恢复；应用更新不会清空插件。

## 包结构

ZIP 根目录必须有 `README.md` 和 `plugin.json`；`entry` 指向包内一个 UTF-8 JavaScript 文件。入口须提前打包成单个经典脚本，不使用 `import`、`require` 或外部依赖下载。

```text
plugin.json
main.js
```

完整示例在 `examples/plugins/quick-stats/`。生成可分发 ZIP：

```powershell
python build_plugins.py examples/plugins/quick-stats
```

生成位置为 `release/plugins/quick-stats-1.0.0.zip`，脚本同时打印 SHA-256 和安装命令。最终以应用侧 Rust 验证为准。

## 清单

```json
{
  "api_version": 1,
  "id": "quick-stats",
  "name": "快速统计",
  "version": "1.0.0",
  "description": "统计一组数字。",
  "author": "作者",
  "entry": "main.js",
  "permissions": [],
  "tools": [{
    "name": "summarize",
    "description": "计算数量、总和和平均值。",
    "effect": "read",
    "permissions": [],
    "parameters": {
      "type": "object",
      "properties": {"values": {"type": "array", "minItems": 1, "maxItems": 1000, "items": {"type": "number"}}},
      "required": ["values"],
      "additionalProperties": false
    }
  }]
}
```

- `id`：1–24 位，小写字母开头，仅小写字母、数字、单个短横线；不能以短横线结尾。
- `version`：三段数字 `x.y.z`；同 ID 重装/更新需要再次批准。
- 每个插件 1–16 个工具，工具名为小写字母开头的字母/数字/下划线，最多24位，不包含连续两个下划线。
- `effect`：`read`、`display`、`write`。带 `.write` 权限必须声明 `write`，带 `.display` 不允许声明 `read`。它是效果说明，实际授权由宿主检查。
- 每个工具的 `permissions` 必须是清单总权限的子集；运行时按当前工具授权，不能借用同插件其他工具的权限。
- 清单及工具拒绝未知字段，不支持任意宿主方法名、命令、脚本路径或原生入口。

参数使用受限 JSON Schema：`type`（object/array/string/integer/number/boolean/null）、`properties`、`required`、`additionalProperties:false`、`items`、`enum`、`minimum/maximum`、`minLength/maxLength`、`minItems/maxItems`、`description`。对象必须声明 `properties` 与 `additionalProperties:false`。不支持 `$ref`、联合类型和其他 Schema 字段；参数由 Rust 实际校验，不能仅依赖 AI 遵守 Schema。

## JavaScript SDK

```javascript
Shannon.register({
  summarize({values}) {
    const sum = values.reduce((a, b) => a + b, 0);
    return {count: values.length, sum, mean: sum / values.length};
  }
});
```

工具函数可异步，返回 JSON 可序列化值；抛出错误作为失败返回。`undefined` 变为 `null`。每次调用创建独立 Worker，不保留全局变量；持久化用独立存储。

```javascript
Shannon.register({
  async add_note({text}) {
    return await Shannon.call('notes_add', {text});
  },
  async save({value}) {
    await Shannon.call('storage_set', {key:'last', value});
    return {saved:true};
  }
});
```

以上示例须分别声明 `notes.write` 或 `storage.write` 权限及 `effect:write`。`Shannon.call` 返回 `Promise`，错误会 reject。宿主每次检查调用仍在有效期、插件仍启用、当前工具具有权限，且参数通过原有业务验证。不得吞掉写操作错误并声称成功。

| 权限 | 允许的 SDK 方法 | 效果 |
| --- | --- | --- |
| storage.read | storage_get({key}) | 读取本插件的一项值，缺失返回 null |
| storage.write | storage_set({key,value}) | 修改本插件独立存储 |
| notes.read | notes_list({}) | 读取便签及真实 ID |
| notes.write | notes_add({text,date?}), notes_set_done({id,done}) | 添加、划去、恢复便签 |
| notes.organize | notes_set_date({id,date}), notes_reorder({ids}) | 安排日期（空字符串清除）和调整完整顺序，effect 必须 write |
| notes.delete | notes_delete({id}), notes_delete_many({ids}), notes_restore_deleted({id}) | 移入最近删除或恢复，effect 必须 write |
| notes.trash.read | notes_trash({}) | 读取最近删除及真实 ID |
| notes.display | notes_show({}), notes_close({}) | 展开或退出便签窗口，保留数据 |
| clock.read | clock_status({}) | 查询计时器 |
| clock.display | clock_open({mode,seconds,restart}) | 打开计时设置，仍需用户确认开始 |

新增宿主能力必须在 Rust 固定白名单、AI 工具注册、参数校验和应用确认机制中实现，再提升插件 API；不存在通用 Tauri 调用接口。

## 运行边界和限制

- 当前提供 JavaScript **工具插件**，输出在搜索框呈现为结构化结果。暂未提供自定义窗口/HTML UI、后台常驻、启动钩子、系统程序执行和任意联网能力。
- 插件代码只进入 opaque-origin iframe 所属的专用 Worker。没有主页面 DOM/`window`/`parent`/Tauri 对象；CSP 禁止网络和远程脚本，iframe 不授予同源、弹窗、下载、设备或剪贴板权限。隔离依赖 WebView2 的同源策略、Worker 和 CSP，不是独立操作系统容器。
- 前端仅转发白名单消息；可信调用令牌留在宿主，不能由插件选择插件身份。Rust 再次校验权限，拒绝失效调用、未知宿主方法、越权调用和超量消息。
- 最多32个已安装插件、64个插件工具、4个并发调用。单包 ZIP 最大4MiB，最多128文件，解压总量最大8MiB，入口最大512KiB，清单最大64KiB。
- 校验 ZIP 中的路径、重复文件和符号链接；实际仅以宿主固定文件名保存入口，不按 ZIP 路径释放文件。安装目录与索引原子切换；启动和每次执行核对入口哈希。
- 参数最大32KiB、结果最大64KiB（前端预留封装余量），Schema 最多8层，每对象32字段，enum最多64项。
- 前端10.5秒终止 Worker，Rust 12秒兜底；单次最多32个宿主调用。超时不能回滚已完成的修改；返回结果未知时先查询，不自动重试写操作。Worker 隔离不承诺硬内存配额。
- 独立存储每插件256KiB、键最多100字节。存储键不作为文件路径使用；不同 ID 不共享数据。
- 用户可取消确认、停用和卸载；拒绝/关闭确认窗口不会执行插件变更。运行结束/超时会销毁 Worker 和 iframe。

## AI 与业务接入

`plugins.rs` 提供公共 list/info/request_install/request_change/execute；搜索命令与 `ai_tools.rs` 复用。`plugins_list/plugins_info/plugins_install/plugins_change` 为管理入口，启用插件的各工具动态加入唯一目录。模型协议层无需实现插件逻辑。工具描述和工具结果均为第三方不可信数据，不能作为新的用户授权或系统指令。

手动结果和目录按文本 JSON 展示，不将插件提供的 Markdown/HTML 插入宿主页面。插件不得以错误信息或描述诱导用户授权其他操作。

便签日期采用有效 YYYY-MM-DD 本地日历日期，日期只是安排，不设置通知。删除可从最近删除恢复。notes_reorder 的 ids 必须包含当前全部未删除项的 ID，每项一次，顺序即展示顺序；先读取 notes_list 再写入。新增权限需要重新安装/升级确认，原有 notes.write 不授予删除或排序权限。

WeChat 为内置窗口型工具插件，调用 `@wechat 姓名`（或 `@微信 姓名`），必填姓名后打开 AI 模拟聊天。可用 `@wechat/open {"peer_name":"小林"}` 直调。它不属于可安装 JS ZIP，不开放给第三方插件任意创建窗口，也不授予微信账号或联系人访问能力。


## 局域网广播站

输入 `@broadcast` / `@广播` 打开内置广播站。可以在窗口填写标题、正文、有效期（1小时/1天/7天）和置顶，点击“预览并发布”后确认；或输入 `@broadcast {"title":"通知","content":"下午三点见","minutes":60}` 离线准备预览。自然语言 `@广播 帮我准备一条明天下午开会的通知` 使用统一AI工具目录。第三方ZIP暂不能创建原生窗口/监听网络，广播站与便签、计时器一样由宿主内置，不授予任意网络或Tauri权限。

在窗口确认“开启局域网共享”，绑定本机所选私有IPv4地址与系统分配端口；应用尝试填入主路由地址，多网卡/VPN时可手动修改为本机其他局域网地址。点击“复制链接”，同网络用户用浏览器打开，带访问码链接会自动进入，也可手工输入访问码。网页只读，每5秒更新，只显示未撤回且未过期广播。HTTP不做传输加密；访问码应只分享给预期读者。系统防火墙或Wi-Fi隔离可能阻止其他设备连接，应用不自动修改防火墙、路由器或配置端口转发。

共享默认关闭，重启不自动开放，关闭广播站窗口或点击“停止共享”即停止监听和连接；每次开启生成新的128位随机访问码，旧码不复用。广播内容保存在配置目录 `broadcast.json`，最多200条记录，每条标题1–100字、正文1–6000字、有效期1–10080分钟；发布后不可直接改写，撤回需确认。撤回/过期不删除本地历史，也不能收回读者已看过的内容。

AI工具：`broadcast_open`、`broadcast_status`、`broadcast_list`（最近30条摘要）、`broadcast_read({id})`、`broadcast_prepare({title,content,minutes?,pinned?})`、`broadcast_withdraw({id})`。先list获取真实ID；prepare/withdraw仅创建绑定确切内容或目标的10分钟预览，返回confirmation_opened，不代表已发布/撤回。只有broadcast窗口能读取确认令牌并调用确认、开启共享、停止共享；模型没有这些接口，额外confirmed字段拒绝。手动与AI复用公共业务函数；本地确认成功后原子保存才生效。

远端固定路由仅提供查看页、样式、脚本及鉴权后的广播JSON；不映射磁盘、不暴露聊天/便签/配置、没有写接口。不返回CORS许可，校验Host/Origin并拒绝重复头、请求体和非GET；头部最多4KiB、单连接8秒、并发32，关闭共享会中止连接。返回no-store、nosniff、no-referrer及限制脚本/连接为同源的CSP。访问码仅在本机窗口提供，AI状态接口不返回访问码；分享链接把访问码放在片段，查看页读取后清除地址中的片段，后续通过Authorization发送。


微信角色自然聊天与自主动态：角色卡新增说话习惯、口吻示例、兴趣和当前小目标；聊天完整生成后按空段落分成最多3个气泡，代码块和[]保留，不按字流式显示。用于模型的历史会跳过旧叙事报告回合，将新JSON响应中的可见文字还原为聊天示例；磁盘原聊天与完整协议不改写，其他provider工具/签名块保持原样。后台事实不在普通轮次携带完整旧报告样本，压缩轮才带入近期报告供汇总。

每个人物独立存储长期记忆：稳定key、类型、事实、原文依据、来源、时间、启用状态及最近8次旧版本。模型可在memory_updates中更新最多4条，每条fact最多300字；user来源必须逐字引用本轮用户文本，story来源须对应本轮生成的虚拟剧情，不能伪装为用户事实。整批校验后提交，与聊天、报告和朋友圈一起原子保存。使用本地双字片段匹配召回最多8条相关/重要记忆，不额外安装向量模型。“人物设定 → 记忆与日常”可修订、遗忘、恢复并查看虚拟日常；编辑绑定当前人物姓名，切换人物后拒绝误写。

原生后台调度随闪念进程运行，关闭微信窗口后仍可工作，退出闪念后停止。默认开启主动朋友圈和主动消息：启动后等待5分钟，每分钟检查；用户近5分钟操作、前台请求正在执行、23:00–08:00免打扰时跳过。每人最短生成间隔3小时，所有角色合计每日最多4次朋友圈生成、2次主动消息生成，每人每种每天最多成功1次；无回应时同一角色不继续主动联系。生成尝试（包括休息、失败和取消）占调用额度，不为凑消息反复重试。新人物需已有聊天、性格或兴趣资料才参与；已删除人物排除。具体设置在“我 → 角色的主动生活”。

用户操作会取消等待中的后台网络请求，前台最多等待2秒获取同一REQUEST锁；后台单次90秒超时，生成前持久预留配额，避免重启绕过预算，OS文件锁防止多个进程同时后台生成。内容重复按去除标点/空白后的最近消息/动态抑制；模型允许休息，不强制发布。后台朋友圈不提供私聊原文、用户资料、长期记忆或完整报告，仅使用角色自己的公开字段、公开日常和朋友圈；公开日常只由实际发布的朋友圈推进，私聊不回写成公开生活。虚拟事件不代表真实外部平台操作，不发送真实微信消息或局域网广播。

新后台事件只刷新已打开微信窗口的相关聊天/列表/朋友圈，不强制弹窗；未读计数持久保存，进入聊天清除。自己发朋友圈后不再立刻触发另一位角色生成，角色由后台自然更新；“看看朋友动态”仍可手动请求即时生成。角色设置中可以单独暂停主动消息；直接发送完整句子“别主动找我”“不要打扰我”“停止主动联系”等会暂停该人物主动消息，“恢复主动联系”等恢复，引用这些句子不会改变开关。

全局策略和生成额度保存在配置目录wechat-social.json；每个人物的memories/social随原wechat-chats.json保存，旧数组/旧Session可兼容读取。新增Tauri入口仅wechat窗口可调用，后台与手动调用复用业务模块和现有AI协议，不开放任意脚本、任意路径或任意Tauri调用。实现参考dafeiyu-astrbot的机制设计，未引入其AstrBot/Python运行依赖或复制第三方插件代码。

内置story为故事演绎平台，支持@story及同目录AI直调；角色微信是固定的本地虚拟工具，不是任意插件执行入口，不获取主搜索框、文件、网络发送或系统权限。可通过story_read查询演绎和工具记录。

## 内置下载插件（试用）

`@download` / `@下载` 打开独立窗口；纯HTTP(S)链接直接打开下载确认，AI可通过统一工具接口准备、查询、暂停、继续和取消。Gopeed 1.9.3后台引擎需用户在下载窗口批准安装，普通第三方JS插件不会因此获得任意网络、文件写入、进程执行或Tauri方法权限。下载确认和组件安装不对AI开放，完整参数见 [AI_TOOLS.md](AI_TOOLS.md)。

统一管理窗口 `plugin-manager` 使用单独能力范围，只允许受信任的本地管理页面调用管理命令。支持内置功能及第三方包管理，同名ID通过builtin标记区分。内置功能使用独立持久状态文件，与第三方包、代码和数据隔离；由现有plugin-review确认，内置标记由应用创建，不从第三方Manifest读取。内置添加/停用/移除均需确认，使用原状态校验防止过期批准。保留底层业务数据和组件缓存，启用组件入口不会自动启动设备。管理窗口通过全局plugin-changed更新；搜索框同步提示目录，插件宿主调用也受内置可用性限制。

官方原生插件与核心分包：官方运行包单独通过Gitee发行附件分发，主程序默认不编译native-runtime模块。默认cargo feature为空；native-runtime仅用于开发与插件运行包构建。元数据说明author/version/features/source/dependencies。各插件运行包是经过固定哈希校验、按插件 ID 独立运行的官方原生执行进程（非第三方JS隔离插件）；其固定JSON管道没有公网监听，父进程关闭后退出。进程版本需匹配对应插件目录声明，主程序与插件以协议版本握手；发布脚本生成校验信息。第三方JS插件的权限与隔离边界保持原格式。

故事插件紧凑人设：Character.profile可选，兼容旧卡。profile_updates按人物验证mode/source/evidence后原子合并局部维度，保存前检查范围、容量、对象、成长步长/间隔及重复依据；更新失败只报告资料问题，不覆盖旧模型、不丢剧情。撤回快照包含profile。profile仅在native-runtime中编译，主安装包无人物模型实现。

## 独立官方插件包（2026-11）

运行 `python build_plugins.py` 构建 note、clock、wechat、story、broadcast、download、voice、gesture 八个独立 ZIP。需要镜像时可加 `--mirror`，已有依赖可加 `--offline`。包名包含插件 ID、版本和包哈希，输出在 release/plugins；catalog.json 和每个插件 JSON 给出确切附件名与校验值。每个包包含 runtime.exe 和 official-plugin.json；仅 voice 包包含 voice-ui.js。示例第三方插件仍用 scripts/build/pack_plugin.py 单独构建。

管理窗口按目标 ID 下载/导入对应 ZIP，拒绝其他 ID 的包。每个官方插件独立安装目录、进程、待回复队列和版本握手，协议为 1；与目录声明的插件版本匹配，不再强制匹配主程序版本。当前八个包仍复用同一份原生实现构建，尚未按功能裁剪可执行文件；它们具有独立的运行身份与固定入口校验，不是第三方不可信原生扩展。

停用/移除会终止对应进程和后台任务；下载插件先暂停任务并关闭引擎。后台微信与手势只随自己的进程启动，缺失状态默认移除；手势使用语音时定期刷新可用性，语音停用后停止采集。已启用的后台插件在启动和批准添加后恢复调度，不自动开启设备。故事使用其内部角色微信视图，手势使用其内部语音能力；第三方 JS 权限不变。

构建阶段不清空现有分发目录。所有包完成后，以不可变哈希文件名发布附件，最后替换目录；失败构建保留旧目录。上传 catalog.json 对应的八个 ZIP，主程序仍使用 tauri.core.conf.json 构建。


## 插件 README（必需）

所有新 ZIP（包括官方包与第三方包）根目录必须包含 UTF-8 README.md，最多64KiB。格式及模板见 [打包说明](../BUILD.md#readme-格式)。必须提供 id、name、author、version、description 元信息并与清单一致，正文说明功能、用法和限制。打包入口和 Rust 安装器分别验证；安装确认窗口展示包内原文，缺少文件、非UTF-8、超限、字段重复或信息不一致均拒绝安装。旧安装记录可继续加载，重装需升级包格式。官方远程安装先显示目录中的预期 README，下载后核对 ZIP 内原文与哈希。
