# 更新日志

## v0.3.3

- 支持自动检测当前用户安装的 Windows 商店版 Codex / ChatGPT。
- 优先读取 AppxManifest.xml 中的实际桌面入口，修复 Codex 包使用 ChatGPT.exe 时选错程序的问题；已有配置会自动纠正。
- 统一 WindowsApps 目录映射和扩展路径的比较，修复运行状态、正常退出和启动结果识别。
- 启动时使用应用所在目录，并在启动失败时记录进程退出状态。
- 复制的 PowerShell 启动脚本使用相同的进程识别逻辑。
- 调整桌面库构建配置，避免 Windows GNU 构建超过 DLL 导出符号上限。
