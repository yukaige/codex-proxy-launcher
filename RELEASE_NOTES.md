修复 Windows 商店版 Codex / ChatGPT 的启动报错，并修复 Windows GNU 测试程序缺少清单的问题。

- 商店版应用通过 Windows 注册的应用入口普通启动，避免“该进程没有程序包标识符”错误。
- 商店版代理启动会明确提示当前无法传入 app-server 所需的代理环境变量，避免误报代理已生效。
- Windows GNU 测试程序直接包含 Common Controls v6 清单；`npm test` 可直接运行。
- 发布脚本不再在测试结束后修改测试程序。

Windows x64：下载 `windows-x64.zip`，解压后运行 EXE；请保留同目录的 `WebView2Loader.dll`。
`SHA256SUMS.txt` 提供下载文件的 SHA-256 校验值。

发布物没有商业代码签名。Windows 商店版的代理启动仍受系统包激活接口限制。
