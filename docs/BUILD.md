# 打包说明

所有命令从项目根目录执行。需要 Node.js、Python 3.11+、Rust stable/MSVC、Windows SDK 和 WebView2；先运行 `npm ci`。

## 主程序入口

```powershell
python build_app.py
python build_app.py --unsigned --exe-only --mirror
python build_app.py --reuse-plugins --offline --mirror
```

默认先调用插件入口，同步 `src-tauri/official-tools.json` 和 `official-plugins.json`，随后构建核心前端、校验插件包、构建安装包。主程序不携带原生插件实现。`release/app/` 保留构建原件；签名成功后生成 `release/github/`、`release/gitee/` 两套发行目录，每套都包含同一份安装包、`.sig`、八个官方插件 ZIP、对应下载地址的 `latest.json` 和 `release.json` 附件清单。无需分别编译两次，也无需两套私钥。

根目录 `latest.json` 指向 Gitee 安装包，`latest-github.json` 指向 GitHub 安装包。程序按配置顺序检查两站；超时、连接失败、JSON 无效都会尝试下一站，一站未发现更新也会继续检查另一站。下载失败时切换同一版本和文件名的镜像，签名、版本、插件哈希校验保持有效；安装程序执行失败不会重复启动安装。两站须发布完全相同的附件及标签。`release.json` 是上传清单，不含认证令牌，不会自动推送远端。具体步骤见 [正式发布](release/RELEASE.md)。

`--unsigned --exe-only` 只构建本地测试 EXE，输出 `release/dev/`；`--reuse-plugins` 跳过插件重建，但仍核对目录、ZIP 和 README；`--offline` 禁止 Cargo 下载；项目内 `.cargo/config.toml` 默认使用 HTTPS Rust 镜像，`--mirror` 保留兼容。正式安装包默认要求更新签名私钥，并生成 `.sig` 和 `latest.json`。缺少私钥或签名校验失败时停止，不回退成无签名发布。首次制作安装包仍需联网获取 NSIS/WebView2 打包工具；`--offline` 仅约束 Cargo，工具缓存位于 `src-tauri/target/.tauri/`。

Windows 发布者签名需要单独的受信任证书；配置证书指纹后会对原生插件、主程序和安装包签名。使用 `--require-windows-signature` 强制要求此证书。没有证书时会明确提示仅有更新签名。

首次密钥配置、两种签名的区别及证书接入见 [签名说明](release/SIGNING.md)。`--signed` 仅作为兼容选项；本地测试需显式使用 `--unsigned`，不会更新发布目录。

## 插件入口

```powershell
python build_plugins.py --mirror
python build_plugins.py examples/plugins/quick-stats
python build_plugins.py E:\MyPlugin --output E:\Packages\my-plugin.zip
```

无目录参数时构建八个官方 ZIP 和快速统计示例包；传目录时只打包该 JS 插件，不启动 Rust 编译。官方说明位于 `plugins/official/<ID>/README.md`；其元信息需与官方目录中的 ID、名称、开发者、版本、描述一致。

官方产物在 `release/plugins/`，`catalog.json` 指定本次确切文件名、版本、README 和哈希。正式上传仅使用该目录列出的包。旧包保留在 `release/archive/plugins/`，不混入当前输出。构建官方插件后需要重建主程序，因为主程序固定校验所支持的插件包。

开发调试包可加 `--debug`，不能作为正式发行附件。

## README 格式

ZIP 根目录固定使用 `README.md`，UTF-8、最多 64 KiB。开头是简单的 `key: value` 元信息区，不是任意 YAML；五个字段必须各出现一次，正文不可为空。

```markdown
---
id: my-plugin
name: 我的插件
author: 开发者姓名
version: 1.0.0
description: 插件的简短功能说明。
---

# 我的插件

## 功能说明

说明工具用途、输入、输出和实际效果。

## 使用方式

说明调用示例、依赖和限制。
```

元信息必须与 `plugin.json` 一致。官方包还核对固定目录中的 README 原文及哈希。README 提供信息完整性，不证明作者身份；权限仍以应用实际校验为准。已有安装记录可继续读取，但重新安装 ZIP 必须符合新规范。

## 内部构建与测试

`scripts/build/` 保存两入口调用的内部实现。可选组件使用 `npm run build:components`，输出 `release/components/`，不属于主程序或插件入口的重复实现。

```powershell
npm test
python -m unittest discover -s tests -p test_official_plugin_build.py
cargo test --manifest-path src-tauri/Cargo.toml --bin shannon
```

原生插件测试需启用 `native-runtime,custom-protocol` 并先构建运行界面。开发辅助脚本 `scripts/smoke_plugin_runtime.py` 只接受隔离调试包，避免触碰正式用户数据。
