修复 Windows 商店版 Codex / ChatGPT 的自动检测与启动问题。

- 从安装清单读取真正的桌面入口，兼容 Codex 使用 `ChatGPT.exe` 的安装包，并自动纠正已有配置中的入口。
- 修复 WindowsApps 目录映射、扩展路径导致的运行状态、正常退出和启动结果识别错误。
- 启动时设置应用工作目录；失败时记录退出状态，便于排查。
- 同步修复复制的 PowerShell 启动脚本。

下载：

- **Windows x64**：下载 `windows-x64.zip`，解压后运行 EXE；请保留同目录的 `WebView2Loader.dll`，无需安装器。
- **macOS Apple Silicon**：下载 `macos-arm64.dmg`，打开后将应用拖到 Applications。
- `SHA256SUMS.txt` 提供发布文件的 SHA-256 校验值。

仍需安装 Codex / ChatGPT 桌面客户端并运行本机 SOCKS5 或 HTTP 代理。
发布物没有商业代码签名；macOS 版本未公证。
Windows 和 macOS 发布物分别在自有 Windows 电脑和 Apple Silicon Mac 上编译，GitHub 仅用于托管下载。
本次修复通过了回归测试和平台构建检查；尚未完成真实客户端的代理重启及业务流量端到端验证。
