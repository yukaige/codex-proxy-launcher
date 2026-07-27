# Codex 代理启动器

一个面向 macOS 和 Windows 的 Codex / ChatGPT 桌面客户端代理启动工具。
桌面外壳使用 Tauri 2，网络检测与启动逻辑由 Rust 实现。

它会同时为 Chromium 网络层传入代理参数，并为 Codex app-server 设置
代理环境变量。界面会分别显示“代理是否可达”“启动参数是否传入”和
“实际流量是否有证据”，避免把应用成功启动误认为代理已经生效。

> 本项目是独立的社区工具，与 OpenAI 无隶属或官方合作关系。

## 功能

- 自动检测 macOS 的 `Codex.app` / `ChatGPT.app`；
- 自动检测 Windows 常见安装目录中的 `Codex.exe` / `ChatGPT.exe`；
- 支持手动选择其他位置的 `.app` 或 `.exe`；
- 支持 SOCKS5 和 HTTP CONNECT 代理；
- 启动前检查 TCP 端口、代理握手和 HTTPS 请求；
- 同时配置 Chromium 参数和 app-server 代理环境变量；
- 复制与当前配置对应的 zsh（macOS）或 PowerShell（Windows）脚本；
- 启动前请求已有 Codex 实例正常退出，不强制终止进程；
- 生成 Chromium net-log，并查找代理路由证据；
- 本地保存配置，并对启动器日志中的 Token、Cookie 和认证信息脱敏。

## 系统要求

- macOS 11 或更高版本；或
- Windows 10 1803 或更高版本（x64）；
- 已安装 Codex 或 ChatGPT 桌面客户端；
- 正在运行的 SOCKS5/HTTP 代理。

Windows 界面使用系统的 Microsoft Edge WebView2。Windows 10 1803
及之后的版本通常已自带；如果 EXE 无法打开，请先安装 WebView2
Evergreen Runtime。

Linux 暂不在本项目支持范围内，因为目前没有可供启动的 Codex Linux
桌面客户端。

## 下载

从 [GitHub Releases](https://github.com/yukaige/codex-proxy-launcher/releases)
下载对应文件：

- Windows x64：`Codex-Proxy-Launcher-<版本>-windows-x64.exe`
- macOS Apple Silicon：`.dmg`

Windows 版不提供 MSI 或安装器。下载 EXE 后可放到任意目录直接运行，
删除该文件即可移除程序。当前发布物没有商业代码签名，Windows
SmartScreen 或 macOS Gatekeeper 可能显示未知发布者提示；请只从本仓库
下载，或审核源码后自行构建。

## 使用方法

1. 启动 Clash、Surge、V2Ray 或其他代理软件。
2. 打开代理启动器并确认检测到正确的 Codex/ChatGPT 应用。
3. 选择 SOCKS5 或 HTTP，填写代理主机和端口。
4. 点击“测试代理”。
5. 保持“启动前退出已有 Codex”开启，点击“代理启动 Codex”。
6. 在 Codex 中发起一个新请求。
7. 返回启动器点击“验证实际流量”，并结合代理软件的连接日志确认。

“复制启动脚本”会根据当前系统生成可直接粘贴运行的脚本：macOS 使用
隔离的 zsh 子进程，Windows 使用 PowerShell 代码块。脚本不会修改当前
终端的持久配置。

## 三种验证状态

| 状态 | 表示什么 | 不表示什么 |
| --- | --- | --- |
| 代理可达 | 端口连接、代理握手和 HTTPS 请求成功 | Codex 已使用该代理 |
| 参数已传入 | Codex 由启动器带代理参数启动 | 已观察到真实业务流量 |
| 实际流量已验证 | 本次 Chromium net-log 中发现当前代理地址及路由事件 | 代理服务自身绝对可信 |

实际网络路径应以本次 net-log 和 Clash 等代理软件的连接日志为准。

## 工作原理

```text
Codex 代理启动器
  ├─ Chromium 参数
  │    --proxy-server
  │    --proxy-bypass-list
  │    --disable-quic
  │    --log-net-log（可选）
  │
  ├─ app-server 环境变量
  │    HTTP_PROXY / HTTPS_PROXY / ALL_PROXY
  │    http_proxy / https_proxy / all_proxy
  │    NO_PROXY / no_proxy
  │
  └─ 平台启动
       macOS: LaunchServices /usr/bin/open
       Windows: 独立创建 Codex.exe 进程
```

SOCKS5 模式下，Chromium 使用 `socks5://`，app-server 环境变量使用
`socks5h://`，让域名解析也通过 SOCKS 代理完成。

## 从源码运行

需要 Node.js 20 或更高版本、Rust 1.77.2 或更高版本，以及对应平台的
Tauri 2 开发依赖。

```bash
npm ci
npm run dev
```

检查和测试：

```bash
npm run typecheck
npm test
npm run build
```

## 构建发布物

macOS Apple Silicon DMG：

```bash
npm run dist:mac
```

Windows 便携 EXE（不生成 MSI/NSIS）：

```powershell
npm run dist:windows
```

Windows 结果位于：

```text
src-tauri\target\release\codex-proxy-launcher.exe
```

发布 GitHub Release 后，仓库中的 Windows Actions 工作流会在
`windows-latest` 上重新检查源码、编译该 EXE，并直接上传到对应 Release。

## 配置与日志

- 配置写入当前用户的 Tauri 应用数据目录；
- macOS 日志：`~/Library/Logs/CodexProxy/`；
- Windows 日志：当前用户的本地应用日志目录；
- `launcher.log` 是经过脱敏的启动器日志；
- `codex-net-log.json` 是 Codex/Chromium 生成的网络调试日志。

net-log 可能包含访问域名和连接信息，不受启动器的日志脱敏逻辑覆盖。
提交 Issue 前请人工检查，不要直接上传完整文件。

## 安全与隐私边界

本工具：

- 不修改、不替换、不重签名 Codex/ChatGPT；
- 不注入动态库；
- 不安装系统证书或修改系统代理；
- 不读取 Codex 登录凭据、Cookie 或对话内容；
- 不强制结束已有 Codex 进程；
- 在 Rust 后端重新校验来自界面的命令输入；
- 对自身日志中的 Authorization、Token、Cookie、API Key 和代理认证
  信息进行脱敏。

## 已知限制

- Windows 当前只发布 x64 便携 EXE；
- 当前不支持 Linux；
- 当前不支持带用户名和密码认证的 SOCKS5/HTTP 代理；
- 必须退出已有 Codex 实例后重新启动，代理参数才能可靠生效；
- “验证实际流量”依赖启用 Chromium net-log；
- Codex/ChatGPT 后续版本可能调整安装位置、运行时或启动参数。

## 常见问题

### Windows EXE 为什么没有安装界面？

这是有意的。Windows 版是单文件便携程序，不生成 MSI/NSIS，也不会写入
系统安装目录。配置和日志仍会保存在当前用户的应用数据目录。

### 为什么找不到 Codex？

点击“手动选择应用”。macOS 选择实际安装的 `Codex.app` 或
`ChatGPT.app`；Windows 选择 `Codex.exe` 或 `ChatGPT.exe`。

### 为什么代理测试成功，但 Codex 没有走代理？

请确认旧实例已经退出，并开启“启动前退出已有 Codex”。已经运行的实例
不会可靠接收新的代理参数。

### 为什么点击“代理启动”后仍显示未验证？

“已传入参数”和“观察到网络流量”是两件事。请在新启动的 Codex 中发起
一个请求，再返回启动器点击“验证实际流量”，同时检查代理软件连接日志。

## 许可证

本项目采用 [MIT License](LICENSE)。
