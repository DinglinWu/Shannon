# 正式发布

统一入口为项目根目录 `python build_app.py`，默认要求自动更新签名。首次生成密钥及 Windows 证书配置见 [签名说明](SIGNING.md)。内部 `scripts/build/release.py` 仅转发到同一入口。

1. 同步更新 package.json、src-tauri/Cargo.toml、src-tauri/tauri.conf.json 的版本，并更新各官方插件 README 的版本。新版本须高于根目录 latest.json。
2. 编辑 docs/release/RELEASE_NOTES.md。
3. 在本机交互终端运行 `python build_app.py`，输入更新密钥密码。配置 Windows 证书后推荐运行 `python build_app.py --require-windows-signature`，缺少任何所需签名即停止。
4. 检查 `release/github/release.json`、`release/gitee/release.json` 的标签、说明和附件哈希。每个目录包含安装包、`.sig`、八个官方 ZIP 和对应下载地址的 `latest.json`。`release/app/` 是构建原件；不要上传 `release/dev/` 或 `release/archive/`。
5. GitHub 仓库 `DinglinWu/Shannon` 的 Releases → Draft a new release，标签填 `v版本号`，目标分支选 `main`，标题填 `Shannon 版本号`，填写更新说明；上传 `release/github/release.json` 的 assets 列出的所有文件，等待完成后发布。
6. Gitee 仓库 `wu-dinglin/shannon` 的发行版中创建相同标签，上传 `release/gitee/release.json` 的 assets 列出的所有文件，等待完成后发布。两站安装包、签名和官方插件必须保持完全相同的字节与文件名，不可分别重打包后覆盖同一版本。
7. 确认两站附件可下载后，再提交源码及根目录 `latest.json`、`latest-github.json`，推送 GitHub 的 `main` 分支；Gitee 的 `master` 分支也须同步更新。GitHub 更新入口读取 `main/latest-github.json`，Gitee 入口读取 `master/latest.json`；目录中的 `latest.json` 如作为附件上传，仅用于备份，不代替仓库中的更新入口。
8. 应用先检查 Gitee，连接/清单错误或无新版本时尝试 GitHub；下载同一安装包失败时尝试另一个仓库，仍要求原签名有效。先下载并验签，再启动安装；不会因切换来源而跳过校验。官方插件下载也有相同镜像机制，使用内置哈希和 README 校验。

`release.json` 是本项目的附件清单，网页不会自动读取。私钥、密码、PFX/P12 不得上传。已经安装的旧版本仍使用它构建时的更新入口；旧客户端需要先通过 Gitee 升级到本次支持双来源的版本，或手动安装。

语音/手势的额外模型组件仍按 `component-catalog.json` 指定的独立历史版本分发，不属于这八个插件 ZIP；本次主程序镜像机制不迁移历史组件附件。

默认入口先构建插件，再构建主程序，以固定新插件包的哈希。官方原生插件如需 Windows 签名，必须先签名再打 ZIP；签名后不要修改文件。可用 --reuse-plugins 复用已验证的包，要求 Windows 签名时会检查其中的 runtime.exe。

本地测试显式使用 `python build_app.py --unsigned --exe-only`，不生成自动更新发布信息。
