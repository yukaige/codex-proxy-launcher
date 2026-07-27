# Codex 代理启动器

一个专为 macOS 设计的 Codex / ChatGPT 桌面客户端代理启动工具。

它会同时为 Chromium 网络层传入代理参数，并为 Codex app-server
设置代理环境变量，支持本地 SOCKS5 和 HTTP 代理。界面会分别显示
“代理是否可达”“启动参数是否传入”和“实际流量是否有证据”，避免把
应用成功启动误认为代理已经生效。

> 本项目是独立的社区工具，与 OpenAI 无隶属或官方合作关系。

## 为什么需要它

仅在 macOS 系统设置或终端中配置代理，不一定能覆盖 Codex
桌面客户端的所有网络路径：

- Chromium 网络层通常读取 `--proxy-server` 等启动参数；
- Codex app-server 及其他子进程可能读取 `HTTP_PROXY`、
  `HTTPS_PROXY`、`ALL_PROXY` 和 `NO_PROXY`；
- 已经运行的 Codex 实例不会可靠地接受新的启动参数。

本工具把这些配置集中到一个桌面界面中，并通过 macOS LaunchServices
启动 Codex。

这项启动方式也很重要：直接执行 ChatGPT/Codex 的内部可执行文件，
可能让 macOS 将后续的文件、媒体资料或其他隐私权限请求错误归属到
启动器。当前版本使用 `/usr/bin/open` 启动应用，使 Codex 成为独立的
责任应用，避免启动器无故出现在“访问其他 App 数据”等权限提示中。

## 功能

- 自动检测以下默认位置中的 Codex 或 ChatGPT：
  - `/Applications/Codex.app`
  - `~/Applications/Codex.app`
  - `/Applications/ChatGPT.app`
  - `~/Applications/ChatGPT.app`
- 支持手动选择其他位置的 `.app` 应用包；
- 支持 SOCKS5 和 HTTP CONNECT 代理；
- 配置代理主机、端口及 Chromium 绕过地址；
- 启动前检查 TCP 端口、代理握手和 HTTPS 请求；
- 同时配置 Chromium 启动参数和 app-server 代理环境变量；
- 可在代理启动前正常退出已有 Codex 实例；
- 可生成 Chromium net-log，并查找代理路由证据；
- 显示 Codex 路径、版本、Bundle ID、运行状态和进程号；
- 本地保存配置，并对日志中的 Token、Cookie 和认证信息进行脱敏。

## 系统要求

- macOS；
- 已安装 Codex 或 ChatGPT 桌面客户端；
- 正在运行的本地或远程 SOCKS5/HTTP 代理；
- 当前预构建版本仅提供 Apple Silicon（arm64）安装包。

开发和构建需要 Node.js 22.12 或更高版本。

## 安装

1. 从项目的 GitHub Releases 页面下载最新的
   `Codex-代理启动器-*-arm64.dmg`。
2. 打开 DMG，将“Codex 代理启动器”拖入“应用程序”。
3. 首次打开时，macOS 如果提示应用来自未识别的开发者，请在 Finder
   中右键应用并选择“打开”。

当前构建使用本地临时签名，尚未经过 Apple Developer ID 签名和
Apple 公证。请只从本仓库的 Releases 页面或你自己审核后构建的产物
安装。

## 使用方法

1. 先启动 Clash、Surge、V2Ray 或其他代理软件。
2. 打开“Codex 代理启动器”，确认它已检测到正确的 Codex/ChatGPT
   应用。
3. 选择 SOCKS5 或 HTTP，并填写代理主机和端口。
4. 点击“测试代理”。
5. 保持“启动前退出已有 Codex”开启，然后点击“代理启动 Codex”。
6. 在 Codex 中发起一个新请求。
7. 返回启动器点击“验证实际流量”，并结合代理软件的连接日志确认。

更新启动器后，如果 Codex 仍是由旧版本启动的，请先彻底退出 Codex，
再使用新版本重新启动。macOS 的权限责任归属和启动参数只会在新进程
中生效。

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
  └─ macOS LaunchServices
       /usr/bin/open -n --env ... -a Codex.app --args ...
```

SOCKS5 模式下，Chromium 使用 `socks5://`，app-server 环境变量使用
`socks5h://`，让域名解析也通过 SOCKS 代理完成。

## 从源码运行

克隆或下载本仓库后，在项目目录中运行：

```bash
cd codex-proxy-launcher
npm ci
npm run dev
```

`npm run dev` 会启动 Vite 开发服务器和 Electron 应用。

## 检查与测试

```bash
npm run typecheck
npm test
npm run build
```

- `typecheck`：检查 Vue、渲染进程和 Electron 主进程的 TypeScript；
- `test`：运行 Vitest 单元测试；
- `build`：生成渲染端、主进程和 preload 产物。

## 构建 macOS 安装包

```bash
npm ci
npm run dist:mac
```

构建结果位于 `release/`：

- `release/mac-arm64/Codex 代理启动器.app`
- `release/Codex-代理启动器-<version>-arm64.dmg`

构建脚本使用本机 Electron 运行时和 macOS `hdiutil`。默认产物采用
临时签名；正式分发时建议配置 Developer ID、Hardened Runtime 和
Apple 公证流程。

## 项目结构

```text
src/
  main/        Electron 主进程、应用检测、代理测试、启动与日志
  preload/     最小化的 contextBridge API
  renderer/    Vue 3 用户界面
  shared/      IPC 常量及主进程/渲染进程共享类型
tests/         核心单元测试
scripts/       preload 与 DMG 构建脚本
```

## 配置与日志

- 配置由 Electron 写入当前用户的应用数据目录；
- 启动器日志位于 `~/Library/Logs/CodexProxy/launcher.log`；
- 开启调试日志后，Chromium net-log 位于
  `~/Library/Logs/CodexProxy/codex-net-log.json`。

net-log 可能包含访问域名、连接信息等调试数据。提交 Issue 前请先检查
并脱敏，不要直接上传完整日志。

## 安全与隐私边界

本工具：

- 不修改、不替换、不重签名 Codex/ChatGPT 应用；
- 不注入动态库；
- 不关闭 SIP 或 Gatekeeper；
- 不安装系统证书或修改系统代理；
- 不读取 Codex 登录凭据、Cookie 或对话内容；
- 不要求访问 Apple Music、通讯录、日历或其他 App 数据；
- 在主进程中重新校验来自界面的 IPC 输入；
- 写入权限受限的本地配置和日志文件；
- 对日志中的 Authorization、Token、Cookie、API Key 和代理认证信息
  进行脱敏。

调试用 Chromium net-log 由 Codex/ChatGPT 自身生成，不受启动器日志
脱敏逻辑覆盖，分享前必须人工检查。

## 已知限制

- 仅支持 macOS；
- 当前发布脚本只生成 Apple Silicon（arm64）版本；
- 当前不支持需要用户名和密码认证的 SOCKS5/HTTP 代理；
- 必须退出已有 Codex 实例后重新启动，代理参数才能可靠生效；
- “验证实际流量”依赖启用 Chromium net-log；
- Codex/ChatGPT 后续版本可能调整运行时或启动参数，升级客户端后应
  重新验证；
- 代理是否能访问 OpenAI 服务取决于你的代理软件、节点和网络环境。

## 常见问题

### 为什么点击“代理启动”后仍显示未验证？

“已传入参数”和“观察到网络流量”是两件事。请在新启动的 Codex 中
发起一个请求，再返回启动器点击“验证实际流量”，同时检查代理软件的
连接日志。

### 为什么代理测试成功，但 Codex 没有走代理？

请确认启动前已经退出旧实例，并开启“启动前退出已有 Codex”。macOS
可能只激活一个已运行实例，而不会把新参数补充到旧进程。

### 为什么更新后仍出现启动器访问其他 App 数据的提示？

旧 Codex 进程可能仍保留旧启动方式的权限责任归属。彻底退出 Codex，
再通过当前版本重新启动。若问题仍然存在，请在 Issue 中提供 macOS
版本、Codex 版本、启动器版本和经过脱敏的相关日志。

### 为什么找不到 Codex？

点击“手动选择应用”，选择实际安装的 `Codex.app` 或 `ChatGPT.app`。
启动器会读取应用的 `Info.plist` 并检查其运行时兼容性。

## 贡献

欢迎提交 Issue 和 Pull Request。建议在提交前运行：

```bash
npm run typecheck
npm test
npm run build
```

报告问题时请包含可复现步骤、macOS/Codex/启动器版本和代理协议。
请勿提交账号凭据、完整 net-log、Cookie、Token 或未经脱敏的个人数据。

## 许可证

本项目采用 [MIT License](LICENSE)。你可以自由使用、修改和分发本项目，
但须保留原始版权和许可证声明。本软件按“原样”提供，不附带任何担保。
