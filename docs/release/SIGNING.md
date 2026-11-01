# 签名与首次配置

更新签名和 Windows 发布者签名是两种不同用途的签名。

| 类型 | 用途 | 当前流程 |
| --- | --- | --- |
| Tauri 更新密钥 | 应用验证自动更新包是否来自项目发布者 | 主程序默认必需；生成并验证 `.sig` 后才更新 `latest.json` |
| Windows 代码签名证书 | Windows 验证 EXE/安装包发布者 | 配置受信任证书后签名；可要求缺少证书就停止 |

更新密钥不会让 Windows 自动信任发布者。自签名 Windows 证书也不适合作为公开分发的信任来源。Windows SmartScreen 还考虑信誉；有受信任代码签名证书也不保证立即消除提示。参考 [微软代码签名说明](https://learn.microsoft.com/windows/apps/package-and-deploy/code-signing-options) 和 [SmartScreen 说明](https://learn.microsoft.com/windows/apps/package-and-deploy/smartscreen-reputation)。

## 生成第一套更新密钥

以下步骤留给开发者在本机交互终端执行；私钥和密码不应粘贴到聊天中。

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\.tauri" | Out-Null
npm run tauri -- signer generate -w "$env:USERPROFILE\.tauri\shannon.key"
```

生成前确认该路径尚无私钥，不要覆盖已有密钥。在提示中设置密码，并将私钥做离线备份。公钥可以公开，私钥不可提交到项目、上传到发行版或分享。

生成器会显示公钥，并写出 `shannon.key.pub`。将该文件的完整公钥字符串填入 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`；不要填私钥内容。

项目目前已经有一段公钥，尚未在这次修改中替换。如果以前已经发布过使用它的应用，更换公钥后旧客户端无法直接验证新密钥签署的更新，需要规划过渡发行版或让旧用户手动安装。若这是你的第一次正式发布，可在生成第一套密钥后配置自己的公钥。

参考 [Tauri 更新签名文档](https://v2.tauri.app/plugin/updater/#signing-updates)。

## 打包签名更新

```powershell
python build_app.py
# 使用其他本机私钥路径
python build_app.py --updater-key D:\PrivateKeys\shannon.key
```

默认从 `%USERPROFILE%\.tauri\shannon.key` 读取私钥路径，在本机交互终端隐藏输入密码。自动构建可通过安全环境变量 `TAURI_SIGNING_PRIVATE_KEY` 和 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 提供密钥或路径及密码；不要将密码放在命令行、源码或日志里。

缺少密钥时立即停止，不退回无签名发布。构建后会验证安装包 `.sig` 与应用配置中的公钥匹配；内容被改动、签名公钥不匹配或签名缺失均不生成新更新目录。版本必须高于已有 `latest.json`。

输出在 `release/app/`：安装包、`.sig`、`Shannon.exe`、`latest.json` 和 `release.json`。根目录的 `latest.json` 最后更新。上传安装包与插件附件，并发布更新目录后，已有客户端才能发现更新。

## Windows 代码签名

你需要先取得受信任的代码签名证书并按发行机构要求配置私钥/硬件令牌。本项目当前支持 Windows 当前用户证书存储中的证书指纹流程；云签名服务需按供应商另行接入，不能用更新密钥冒充。

取得证书后，在本机终端设置公开的证书指纹：

```powershell
$env:SHANNON_WINDOWS_CERT_THUMBPRINT = "证书的40位SHA1指纹"
python build_app.py --require-windows-signature
```

也可在 Tauri 的 `bundle.windows.certificateThumbprint` 配置指纹。默认使用 SHA-256 和 RFC 3161 时间戳；`SHANNON_WINDOWS_TIMESTAMP_URL` 可设置证书提供方的时间戳地址。需要 Windows SDK 中的 `signtool.exe`。

配置后，官方原生插件会先进行 Windows 签名，再计算 ZIP 哈希和生成目录；主程序和安装包由 Tauri 签名。复用插件包时也检查其签名。签名完成后校验 Windows 信任状态及证书指纹，失败即停止。

`--require-windows-signature` 在没有配置证书时直接停止。没有此选项且尚未配置证书时，仍可生成具有更新签名的安装包，但会明确提示没有 Windows 发布者签名。

## 本地开发构建

```powershell
python build_app.py --unsigned --exe-only --reuse-plugins
```

显式使用 `--unsigned` 才生成无更新签名测试包，输出到 `release/dev/`；不会更新 `latest.json`。`--signed` 保留为兼容选项，现在默认行为已经要求更新签名。
