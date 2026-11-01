# 闪念 Shannon

Windows 桌面 AI 搜索框，使用 Tauri 2 和 WebView2。支持对话、便签、计时器，以及按需安装的工具插件。

## 两个打包入口

先安装 Node.js、Python 3.11+、Rust/MSVC 构建工具，并运行 `npm ci`。在项目目录直接运行 `python build_app.py`，即可完成插件构建、主程序签名打包和发行 JSON 生成。

| 目的 | 命令 | 输出 |
| --- | --- | --- |
| 打包主程序安装包 | `python build_app.py` | `release/app/` |
| 打包全部官方插件和示例插件 | `python build_plugins.py` | `release/plugins/` |
| 仅构建主程序 EXE | `python build_app.py --unsigned --exe-only` | `release/dev/Shannon.exe` |
| 打包自己的 JS 插件 | `python build_plugins.py 插件目录` | `release/plugins/插件ID-版本.zip` |

主程序默认先构建插件，以同步它内置的包哈希和工具目录；已有匹配的插件包时可加 `--reuse-plugins`。Rust 默认使用项目内配置的 HTTPS 镜像，直接运行即可；`--mirror` 保留兼容，依赖已缓存时可加 `--offline`。正式打包默认要求更新签名私钥，缺少密钥就停止。Windows 发布者签名需要单独的受信任证书，可用 `--require-windows-signature` 要求双重签名；首次配置见 [签名说明](docs/release/SIGNING.md)。

签名构建成功后，`release/app/latest.json` 用于自动更新，`release/app/release.json` 提供发行标签、说明和附件路径/大小/SHA-256 清单，便于上传发布。根目录 `latest.json` 同步更新；先上传附件，再推送它。

也可使用 `npm run build:app`、`npm run build:plugins`。完整选项与构建顺序见 [打包说明](docs/BUILD.md)。`scripts/build/` 是内部实现，不是另一套入口。

## 插件 README 要求

每个新插件 ZIP 根目录必须包含 UTF-8 的 `README.md`，提供插件 ID、名称、开发者、版本和功能说明。安装器读取原文并与插件清单核对，缺失或不一致时拒绝安装；确认窗口按纯文本显示，避免执行 README 中的 HTML。

模板与约定见 [插件开发规范](docs/development/PLUGIN_SPEC.md)，可直接参考 [快速统计插件](examples/plugins/quick-stats/README.md)。官方插件说明源文件在 `plugins/official/<ID>/README.md`。

## 项目目录

| 路径 | 内容 |
| --- | --- |
| `build_app.py` / `build_plugins.py` | 主程序 / 插件打包入口 |
| `src/` / `src-tauri/` | 前端 / Rust 后端 |
| 根目录 `*.html` | Vite 多页面入口，保留路径以兼容原生窗口 |
| `plugins/official/` | 各官方插件的可维护说明 |
| `examples/plugins/` | 第三方 JS 插件示例 |
| `scripts/build/` | 打包、校验与签名发布实现 |
| `scripts/` | 图标、组件准备、主题及开发辅助脚本 |
| `docs/` | 使用指南、开发规范、发布文档 |
| `tests/` | 自动测试 |
| `third_party/` | 第三方许可和署名 |
| `release/app/` / `release/plugins/` / `release/components/` | 主程序 / 插件 / 可选组件产物 |
| `dist-core/` / `dist-runtime/` / `node_modules/` | 自动生成的前端产物与依赖 |

使用方法见 [用户指南](docs/USER_GUIDE.md)，其他文档见 [文档索引](docs/INDEX.md)。

## 开发与验证

`npm run tauri dev` 启动开发环境；`npm test` 运行前端测试。原生测试、发布签名和组件构建见 [打包说明](docs/BUILD.md)。
