增加 Windows 商店版 Codex / ChatGPT 的启动器单应用代理启动。

- 通过 Windows 包激活接口传入 Chromium 代理参数，保留应用包身份。
- 启动器临时写入 Codex `.env` 供 app-server 读取，检测到 app-server 启动后恢复原文件；启动中断时可在下次打开启动器时恢复。
- 无需系统代理或 TUN。商店版请使用启动器按钮，复制脚本不支持该流程。

Windows x64：下载 `windows-x64.zip`，解压后运行 EXE；请保留同目录的 `WebView2Loader.dll`。
`SHA256SUMS.txt` 提供下载文件的 SHA-256 校验值。

发布物没有商业代码签名。启动期间新开的 Codex CLI 可能读到短暂存在的代理值；app-server 后续自行重启时可能需要重新通过启动器启动。请结合代理连接日志验证实际流量。
