use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

#[cfg(target_os = "windows")]
use crate::core::powershell_quote;
#[cfg(target_os = "macos")]
use crate::core::shell_quote;
use crate::core::{
    app_server_proxy_url, launch_arguments, proxy_environment, proxy_url, validate_app_path,
    validate_config,
};
use crate::detector::find_running_pids;
use crate::logger::AppLogger;
use crate::proxy;
use crate::traffic;
use crate::types::{
    ActionResult, CodexAppInfo, CodexProxyConfig, CodexStatus, LaunchResult, ProxyLaunchStatus,
    ProxyProtocol, TrafficVerificationResult,
};

pub struct LauncherState {
    launch_status: ProxyLaunchStatus,
    status_message: String,
    last_proxy_launch_started_at: Option<SystemTime>,
}

impl Default for LauncherState {
    fn default() -> Self {
        Self {
            launch_status: ProxyLaunchStatus::NotStarted,
            status_message: "尚未启动。".into(),
            last_proxy_launch_started_at: None,
        }
    }
}

pub fn launch_with_proxy(
    info: &CodexAppInfo,
    config: &CodexProxyConfig,
    state: &Mutex<LauncherState>,
    logger: &AppLogger,
) -> LaunchResult {
    if let Some(message) = invalid_target(info) {
        set_status(state, ProxyLaunchStatus::AppNotFound, &message);
        return failure(ProxyLaunchStatus::AppNotFound, &message, None, None);
    }
    if !config.enabled {
        return launch_directly(info, state, logger);
    }
    let proxy_test = proxy::test(config, logger);
    if !proxy_test.success {
        let message = format!("代理不可用，未启动 Codex：{}", proxy_test.message);
        set_status(state, ProxyLaunchStatus::ProxyUnreachable, &message);
        return failure(
            ProxyLaunchStatus::ProxyUnreachable,
            &message,
            info.executable_path.clone(),
            None,
        );
    }

    if config.close_existing_instance && info.is_running {
        let stopped = stop(info, logger);
        if !stopped.success {
            set_status(state, ProxyLaunchStatus::LaunchFailed, &stopped.message);
            return failure(
                ProxyLaunchStatus::LaunchFailed,
                &stopped.message,
                info.executable_path.clone(),
                None,
            );
        }
    } else if !config.close_existing_instance && info.is_running {
        let message =
            "Codex 已在运行，新的代理参数无法可靠应用到旧实例；请启用“启动前退出已有 Codex”。";
        set_status(state, ProxyLaunchStatus::LaunchFailed, message);
        return failure(
            ProxyLaunchStatus::LaunchFailed,
            message,
            info.executable_path.clone(),
            None,
        );
    }

    if let Err(error) = logger.ensure_directory() {
        return failure(
            ProxyLaunchStatus::LaunchFailed,
            &format!("无法创建日志目录：{error}"),
            info.executable_path.clone(),
            None,
        );
    }
    let net_log_path = logger.directory().join("codex-net-log.json");
    let args = match launch_arguments(config, Some(&net_log_path)) {
        Ok(value) => value,
        Err(message) => {
            return failure(
                ProxyLaunchStatus::LaunchFailed,
                &message,
                info.executable_path.clone(),
                None,
            )
        }
    };
    let environment = match proxy_environment(config) {
        Ok(value) => value,
        Err(message) => {
            return failure(
                ProxyLaunchStatus::LaunchFailed,
                &message,
                info.executable_path.clone(),
                None,
            )
        }
    };
    if let Ok(mut value) = state.lock() {
        value.last_proxy_launch_started_at = Some(SystemTime::now());
    }
    let result = launch_via_platform(info, &args, &environment, true, state, logger);
    if result.success {
        let message =
            "Codex 已使用 Chromium 代理参数和后台代理环境启动，但尚未确认网络请求确实经过代理。";
        set_status(state, ProxyLaunchStatus::ProxyUnverified, message);
        LaunchResult {
            status: ProxyLaunchStatus::ProxyUnverified,
            message: message.into(),
            proxy_args_passed: true,
            traffic_verified: false,
            ..result
        }
    } else {
        result
    }
}

pub fn launch_directly(
    info: &CodexAppInfo,
    state: &Mutex<LauncherState>,
    logger: &AppLogger,
) -> LaunchResult {
    if let Some(message) = invalid_target(info) {
        return failure(ProxyLaunchStatus::AppNotFound, &message, None, None);
    }
    let result = launch_via_platform(info, &[], &[], false, state, logger);
    if result.success {
        set_status(
            state,
            ProxyLaunchStatus::NotStarted,
            "Codex 已普通启动；本次没有传入代理参数。",
        );
    }
    result
}

pub fn stop(info: &CodexAppInfo, logger: &AppLogger) -> ActionResult {
    let Some(executable) = info.executable_path.as_deref() else {
        return ActionResult {
            success: false,
            message: "没有可用于识别 Codex 进程的可执行文件路径。".into(),
        };
    };
    let pids = find_running_pids(executable);
    if pids.is_empty() {
        return ActionResult {
            success: true,
            message: "Codex 当前未运行。".into(),
        };
    }
    logger.log(
        "INFO",
        "正在通过进程信号请求 Codex 正常退出。",
        Some(&format!("{pids:?}")),
    );
    if !request_graceful_stop(&pids) {
        let message = "Codex 没有可关闭的主窗口。为避免强制终止其他进程，已取消代理启动。";
        logger.log("ERROR", message, Some(&format!("{pids:?}")));
        return ActionResult {
            success: false,
            message: message.into(),
        };
    }
    if wait_until(Duration::from_secs(5), || {
        find_running_pids(executable).is_empty()
    }) {
        return ActionResult {
            success: true,
            message: "Codex 已正常退出。".into(),
        };
    }
    let message = "Codex 无法正常退出。为避免误杀其他应用，已取消代理启动。";
    logger.log("ERROR", message, Some(&format!("{pids:?}")));
    ActionResult {
        success: false,
        message: message.into(),
    }
}

pub fn verify_proxy_traffic(
    config: &CodexProxyConfig,
    state: &Mutex<LauncherState>,
    logger: &AppLogger,
) -> TrafficVerificationResult {
    let started = state
        .lock()
        .ok()
        .and_then(|value| value.last_proxy_launch_started_at);
    let result = traffic::verify(config, started, logger);
    if result.verified {
        set_status(state, ProxyLaunchStatus::ProxyVerified, &result.message);
    } else if state
        .lock()
        .is_ok_and(|value| value.launch_status == ProxyLaunchStatus::ProxyVerified)
    {
        set_status(state, ProxyLaunchStatus::ProxyUnverified, &result.message);
    }
    result
}

pub fn status(info: &CodexAppInfo, state: &Mutex<LauncherState>) -> CodexStatus {
    let pid_list = info
        .executable_path
        .as_deref()
        .map(find_running_pids)
        .unwrap_or_default();
    let (launch_status, message) = state
        .lock()
        .map(|value| (value.launch_status, value.status_message.clone()))
        .unwrap_or((ProxyLaunchStatus::NotStarted, "尚未启动。".into()));
    CodexStatus {
        is_running: !pid_list.is_empty(),
        pid_list,
        launch_status,
        message,
    }
}

#[cfg(target_os = "macos")]
fn launch_via_platform(
    info: &CodexAppInfo,
    launch_args: &[String],
    environment: &[(String, String)],
    has_proxy_arguments: bool,
    state: &Mutex<LauncherState>,
    logger: &AppLogger,
) -> LaunchResult {
    let (Some(app_path), Some(executable)) =
        (info.app_path.as_deref(), info.executable_path.as_deref())
    else {
        return failure(
            ProxyLaunchStatus::AppNotFound,
            "Codex 应用或可执行文件不存在。",
            None,
            None,
        );
    };
    let open_args = build_open_arguments(app_path, launch_args, environment);
    logger.log(
        "INFO",
        "准备通过 macOS 应用启动服务启动 Codex。",
        Some(&format!("{open_args:?}")),
    );
    match Command::new("/usr/bin/open").args(&open_args).status() {
        Ok(status) if status.success() => {
            let pids = wait_for_pids(executable, Duration::from_secs(5));
            let Some(pid) = pids.first().copied() else {
                let message = "macOS 启动命令已执行，但没有检测到 Codex 进程。";
                set_status(state, ProxyLaunchStatus::LaunchFailed, message);
                return failure(
                    ProxyLaunchStatus::LaunchFailed,
                    message,
                    Some(executable.into()),
                    Some(launch_args.to_vec()),
                );
            };
            LaunchResult {
                success: true,
                status: if has_proxy_arguments {
                    ProxyLaunchStatus::LaunchedWithProxyArgs
                } else {
                    ProxyLaunchStatus::NotStarted
                },
                pid: Some(pid),
                message: if has_proxy_arguments {
                    "Codex 已收到 Chromium 代理参数，后台 app-server 也已收到代理环境。".into()
                } else {
                    "Codex 已通过 macOS 应用启动服务普通启动。".into()
                },
                executable_path: Some(executable.into()),
                args: Some(launch_args.to_vec()),
                proxy_args_passed: has_proxy_arguments,
                traffic_verified: false,
            }
        }
        Ok(status) => {
            let message = format!("Codex 启动失败：open 退出状态 {status}");
            set_status(state, ProxyLaunchStatus::LaunchFailed, &message);
            failure(
                ProxyLaunchStatus::LaunchFailed,
                &message,
                Some(executable.into()),
                Some(launch_args.to_vec()),
            )
        }
        Err(error) => {
            let message = format!("Codex 启动失败：{error}");
            set_status(state, ProxyLaunchStatus::LaunchFailed, &message);
            failure(
                ProxyLaunchStatus::LaunchFailed,
                &message,
                Some(executable.into()),
                Some(launch_args.to_vec()),
            )
        }
    }
}

#[cfg(target_os = "windows")]
fn launch_via_platform(
    info: &CodexAppInfo,
    launch_args: &[String],
    environment: &[(String, String)],
    has_proxy_arguments: bool,
    state: &Mutex<LauncherState>,
    logger: &AppLogger,
) -> LaunchResult {
    let Some(executable) = info.executable_path.as_deref() else {
        return failure(
            ProxyLaunchStatus::AppNotFound,
            "Codex 可执行文件不存在。",
            None,
            None,
        );
    };
    logger.log(
        "INFO",
        "准备通过 Windows 创建独立 Codex 进程。",
        Some(&format!("{launch_args:?}")),
    );
    let mut command = Command::new(executable);
    command.args(launch_args).envs(environment.iter().cloned());
    match command.spawn() {
        Ok(_) => {
            let pids = wait_for_pids(executable, Duration::from_secs(5));
            let Some(pid) = pids.first().copied() else {
                let message = "Windows 启动命令已执行，但没有检测到 Codex 进程。";
                set_status(state, ProxyLaunchStatus::LaunchFailed, message);
                return failure(
                    ProxyLaunchStatus::LaunchFailed,
                    message,
                    Some(executable.into()),
                    Some(launch_args.to_vec()),
                );
            };
            LaunchResult {
                success: true,
                status: if has_proxy_arguments {
                    ProxyLaunchStatus::LaunchedWithProxyArgs
                } else {
                    ProxyLaunchStatus::NotStarted
                },
                pid: Some(pid),
                message: if has_proxy_arguments {
                    "Codex 已收到 Chromium 代理参数，后台 app-server 也已收到代理环境。".into()
                } else {
                    "Codex 已在 Windows 上普通启动。".into()
                },
                executable_path: Some(executable.into()),
                args: Some(launch_args.to_vec()),
                proxy_args_passed: has_proxy_arguments,
                traffic_verified: false,
            }
        }
        Err(error) => {
            let message = format!("Codex 启动失败：{error}");
            set_status(state, ProxyLaunchStatus::LaunchFailed, &message);
            failure(
                ProxyLaunchStatus::LaunchFailed,
                &message,
                Some(executable.into()),
                Some(launch_args.to_vec()),
            )
        }
    }
}

#[cfg(target_os = "macos")]
fn request_graceful_stop(pids: &[u32]) -> bool {
    for pid in pids {
        let _ = Command::new("/bin/kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
    true
}

#[cfg(target_os = "windows")]
fn request_graceful_stop(pids: &[u32]) -> bool {
    let ids = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let script = "$requested=$false; foreach($id in $env:CODEX_PROCESS_IDS.Split(',')) { \
        try { $p=Get-Process -Id ([int]$id) -ErrorAction Stop; \
        if($p.MainWindowHandle -ne 0 -and $p.CloseMainWindow()){$requested=$true} } catch {} }; \
        if(-not $requested){exit 1}";
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("CODEX_PROCESS_IDS", ids)
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(target_os = "macos")]
pub fn build_open_arguments(
    app_path: &str,
    launch_args: &[String],
    environment: &[(String, String)],
) -> Vec<String> {
    let mut args = vec!["-n".into()];
    for (name, value) in environment {
        args.extend(["--env".into(), format!("{name}={value}")]);
    }
    args.extend(["-a".into(), app_path.into()]);
    if !launch_args.is_empty() {
        args.push("--args".into());
        args.extend_from_slice(launch_args);
    }
    args
}

#[cfg(target_os = "macos")]
pub fn build_launch_script(
    info: &CodexAppInfo,
    config: &CodexProxyConfig,
    log_directory: &Path,
) -> Result<String, String> {
    validate_config(config)?;
    if !config.enabled {
        return Err("请先启用代理，再复制代理启动脚本。".into());
    }
    let app_path = info
        .app_path
        .as_deref()
        .ok_or_else(|| "没有找到可用于生成脚本的 Codex 应用。".to_string())?;
    let executable = info
        .executable_path
        .as_deref()
        .ok_or_else(|| "没有找到可用于生成脚本的 Codex 应用。".to_string())?;
    let app_path_buf = validate_app_path(Path::new(app_path))?;
    let executable_name = Path::new(executable)
        .file_name()
        .ok_or_else(|| "Codex 可执行文件路径无效。".to_string())?;
    let expected = app_path_buf.join("Contents/MacOS").join(executable_name);
    if expected != Path::new(executable) {
        return Err("Codex 可执行文件路径与应用包不匹配。".into());
    }

    let net_log_path = log_directory.join("codex-net-log.json");
    let launch_args = launch_arguments(config, Some(&net_log_path))?;
    let environment = proxy_environment(config)?;
    let open_args = build_open_arguments(app_path, &launch_args, &environment);
    let proxy_test_url = match config.protocol {
        ProxyProtocol::Socks5 => app_server_proxy_url(config)?,
        ProxyProtocol::Http => proxy_url(config)?,
    };
    let mut lines = vec![
        "set -euo pipefail".into(),
        String::new(),
        format!("CODEX_EXECUTABLE={}", shell_quote(executable)),
        String::new(),
        "echo \"正在测试代理连接…\"".into(),
        "/usr/bin/curl --fail --silent --show-error --head --max-time 5 \\".into(),
        format!("  --noproxy '' --proxy {} \\", shell_quote(&proxy_test_url)),
        "  'https://www.gstatic.com/generate_204' >/dev/null".into(),
        String::new(),
        "find_codex_pids() {".into(),
        "  /bin/ps -axo pid=,command= | /usr/bin/awk -v target=\"$CODEX_EXECUTABLE\" '".into(),
        "    {".into(),
        "      pid = $1".into(),
        "      sub(/^[[:space:]]*[0-9]+[[:space:]]+/, \"\", $0)".into(),
        "      if ($0 == target || index($0, target \" \") == 1) print pid".into(),
        "    }".into(),
        "  '".into(),
        "}".into(),
        String::new(),
    ];
    if config.close_existing_instance {
        lines.extend(
            [
                "PIDS=\"$(find_codex_pids)\"",
                "if [[ -n \"$PIDS\" ]]; then",
                "  echo \"正在退出已有 Codex 实例…\"",
                "  while IFS= read -r pid; do",
                "    /bin/kill -TERM \"$pid\"",
                "  done <<< \"$PIDS\"",
                "",
                "  for _attempt in {1..25}; do",
                "    [[ -z \"$(find_codex_pids)\" ]] && break",
                "    /bin/sleep 0.2",
                "  done",
                "  if [[ -n \"$(find_codex_pids)\" ]]; then",
                "    echo \"Codex 无法正常退出，已取消代理启动。\" >&2",
                "    exit 1",
                "  fi",
                "fi",
                "",
            ]
            .into_iter()
            .map(str::to_owned),
        );
    } else {
        lines.extend(
            [
                "if [[ -n \"$(find_codex_pids)\" ]]; then",
                "  echo \"Codex 已在运行，新的代理参数无法可靠生效，已取消启动。\" >&2",
                "  exit 1",
                "fi",
                "",
            ]
            .into_iter()
            .map(str::to_owned),
        );
    }
    if config.enable_debug_log {
        lines.push(format!(
            "/bin/mkdir -p {}",
            shell_quote(&log_directory.to_string_lossy())
        ));
        lines.push(String::new());
    }
    lines.extend([
        "echo \"正在通过代理启动 Codex…\"".into(),
        format_shell_command("/usr/bin/open", &open_args),
        String::new(),
        "for _attempt in {1..25}; do".into(),
        "  [[ -n \"$(find_codex_pids)\" ]] && break".into(),
        "  /bin/sleep 0.2".into(),
        "done".into(),
        "if [[ -z \"$(find_codex_pids)\" ]]; then".into(),
        "  echo \"启动命令已执行，但没有检测到 Codex 进程。\" >&2".into(),
        "  exit 1".into(),
        "fi".into(),
        "echo \"Codex 已通过代理启动。\"".into(),
    ]);
    Ok(wrap_for_terminal(&lines))
}

#[cfg(target_os = "windows")]
pub fn build_launch_script(
    info: &CodexAppInfo,
    config: &CodexProxyConfig,
    log_directory: &Path,
) -> Result<String, String> {
    validate_config(config)?;
    if !config.enabled {
        return Err("请先启用代理，再复制代理启动脚本。".into());
    }
    let app_path = info
        .app_path
        .as_deref()
        .ok_or_else(|| "没有找到可用于生成脚本的 Codex 应用。".to_string())?;
    let executable = info
        .executable_path
        .as_deref()
        .ok_or_else(|| "没有找到可用于生成脚本的 Codex 应用。".to_string())?;
    let selected = validate_app_path(Path::new(app_path))?;
    if !selected.eq(Path::new(executable)) {
        return Err("Codex 可执行文件路径与所选应用不匹配。".into());
    }

    let net_log_path = log_directory.join("codex-net-log.json");
    let launch_args = launch_arguments(config, Some(&net_log_path))?;
    let environment = proxy_environment(config)?;
    let proxy_test_url = match config.protocol {
        ProxyProtocol::Socks5 => app_server_proxy_url(config)?,
        ProxyProtocol::Http => proxy_url(config)?,
    };
    let argument_line = launch_args
        .iter()
        .map(|argument| windows_argument_quote(argument))
        .collect::<Vec<_>>()
        .join(" ");

    let mut lines = vec![
        "& {".into(),
        "  $ErrorActionPreference = 'Stop'".into(),
        format!("  $CodexExecutable = {}", powershell_quote(executable)),
        String::new(),
        "  Write-Host '正在测试代理连接…'".into(),
        "  & curl.exe --fail --silent --show-error --head --max-time 5 `".into(),
        format!(
            "    --noproxy \"\" --proxy {} `",
            powershell_quote(&proxy_test_url)
        ),
        "    'https://www.gstatic.com/generate_204' | Out-Null".into(),
        String::new(),
        "  function Get-CodexProcesses {".into(),
        "    Get-Process | ForEach-Object {".into(),
        "      try {".into(),
        "        if ([string]::Equals([IO.Path]::GetFullPath($_.Path), \
         [IO.Path]::GetFullPath($CodexExecutable), \
         [StringComparison]::OrdinalIgnoreCase)) { $_ }"
            .into(),
        "      } catch {}".into(),
        "    }".into(),
        "  }".into(),
        String::new(),
    ];
    if config.close_existing_instance {
        lines.extend(
            [
                "  $existing = @(Get-CodexProcesses)",
                "  if ($existing.Count -gt 0) {",
                "    Write-Host '正在退出已有 Codex 实例…'",
                "    $requested = $false",
                "    foreach ($process in $existing) {",
                "      if ($process.MainWindowHandle -ne 0 -and $process.CloseMainWindow()) {",
                "        $requested = $true",
                "      }",
                "    }",
                "    if (-not $requested) {",
                "      throw 'Codex 没有可关闭的主窗口，已取消代理启动。'",
                "    }",
                "    for ($attempt = 0; $attempt -lt 25; $attempt++) {",
                "      if (@(Get-CodexProcesses).Count -eq 0) { break }",
                "      Start-Sleep -Milliseconds 200",
                "    }",
                "    if (@(Get-CodexProcesses).Count -gt 0) {",
                "      throw 'Codex 无法正常退出，已取消代理启动。'",
                "    }",
                "  }",
                "",
            ]
            .into_iter()
            .map(str::to_owned),
        );
    } else {
        lines.extend(
            [
                "  if (@(Get-CodexProcesses).Count -gt 0) {",
                "    throw 'Codex 已在运行，新的代理参数无法可靠生效，已取消启动。'",
                "  }",
                "",
            ]
            .into_iter()
            .map(str::to_owned),
        );
    }
    if config.enable_debug_log {
        lines.push(format!(
            "  New-Item -ItemType Directory -Force -Path {} | Out-Null",
            powershell_quote(&log_directory.to_string_lossy())
        ));
        lines.push(String::new());
    }
    lines.extend([
        "  Write-Host '正在通过代理启动 Codex…'".into(),
        "  $startInfo = New-Object System.Diagnostics.ProcessStartInfo".into(),
        "  $startInfo.FileName = $CodexExecutable".into(),
        "  $startInfo.UseShellExecute = $false".into(),
        format!(
            "  $startInfo.Arguments = {}",
            powershell_quote(&argument_line)
        ),
        "  $startInfo.WorkingDirectory = Split-Path -Parent $CodexExecutable".into(),
    ]);
    for (name, value) in environment {
        lines.push(format!(
            "  $startInfo.EnvironmentVariables[{}] = {}",
            powershell_quote(&name),
            powershell_quote(&value)
        ));
    }
    lines.extend([
        "  [Diagnostics.Process]::Start($startInfo) | Out-Null".into(),
        String::new(),
        "  for ($attempt = 0; $attempt -lt 25; $attempt++) {".into(),
        "    if (@(Get-CodexProcesses).Count -gt 0) { break }".into(),
        "    Start-Sleep -Milliseconds 200".into(),
        "  }".into(),
        "  if (@(Get-CodexProcesses).Count -eq 0) {".into(),
        "    throw '启动命令已执行，但没有检测到 Codex 进程。'".into(),
        "  }".into(),
        "  Write-Host 'Codex 已通过代理启动。'".into(),
        "}".into(),
    ]);
    Ok(format!("{}\r\n", lines.join("\r\n")))
}

#[cfg(target_os = "windows")]
fn windows_argument_quote(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".into();
    }
    if !value
        .chars()
        .any(|character| character.is_whitespace() || character == '"' || character == '\t')
    {
        return value.into();
    }

    let mut result = String::from("\"");
    let mut backslashes = 0;
    for character in value.chars() {
        if character == '\\' {
            backslashes += 1;
        } else if character == '"' {
            result.push_str(&"\\".repeat(backslashes * 2 + 1));
            result.push('"');
            backslashes = 0;
        } else {
            result.push_str(&"\\".repeat(backslashes));
            backslashes = 0;
            result.push(character);
        }
    }
    result.push_str(&"\\".repeat(backslashes * 2));
    result.push('"');
    result
}

#[cfg(target_os = "macos")]
fn format_shell_command(command: &str, args: &[String]) -> String {
    if args.is_empty() {
        return command.into();
    }
    let mut lines = vec![format!("{command} \\")];
    for (index, argument) in args.iter().enumerate() {
        lines.push(format!(
            "  {}{}",
            shell_quote(argument),
            if index + 1 < args.len() { " \\" } else { "" }
        ));
    }
    lines.join("\n")
}

#[cfg(target_os = "macos")]
fn wrap_for_terminal(lines: &[String]) -> String {
    let mut delimiter = "CODEX_PROXY_SCRIPT".to_string();
    while lines.contains(&delimiter) {
        delimiter.push_str("_END");
    }
    format!(
        "/bin/zsh <<'{}'\n{}\n{}\n",
        delimiter,
        lines.join("\n"),
        delimiter
    )
}

fn invalid_target(info: &CodexAppInfo) -> Option<String> {
    if !info.installed || info.app_path.is_none() {
        Some("没有找到 Codex.app，无法启动。".into())
    } else if info.executable_path.is_none() {
        Some(
            info.warning
                .clone()
                .unwrap_or_else(|| "Codex 可执行文件不存在，应用可能已损坏。".into()),
        )
    } else {
        None
    }
}

fn failure(
    status: ProxyLaunchStatus,
    message: &str,
    executable_path: Option<String>,
    args: Option<Vec<String>>,
) -> LaunchResult {
    LaunchResult {
        success: false,
        status,
        pid: None,
        message: message.into(),
        executable_path,
        args,
        proxy_args_passed: false,
        traffic_verified: false,
    }
}

fn set_status(state: &Mutex<LauncherState>, status: ProxyLaunchStatus, message: &str) {
    if let Ok(mut value) = state.lock() {
        value.launch_status = status;
        value.status_message = message.into();
    }
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(200));
    }
    false
}

fn wait_for_pids(executable: &str, timeout: Duration) -> Vec<u32> {
    let mut pids = Vec::new();
    wait_until(timeout, || {
        pids = find_running_pids(executable);
        !pids.is_empty()
    });
    pids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CodexProxyConfig, CodexRuntimeType};

    fn info() -> CodexAppInfo {
        CodexAppInfo {
            platform: test_platform(),
            installed: true,
            app_path: Some("/Applications/ChatGPT.app".into()),
            executable_path: Some("/Applications/ChatGPT.app/Contents/MacOS/ChatGPT".into()),
            bundle_id: None,
            version: None,
            is_electron: true,
            runtime_type: CodexRuntimeType::Electron,
            proxy_switch_compatible: true,
            is_running: false,
            pid_list: vec![],
            warning: None,
        }
    }

    #[cfg(target_os = "macos")]
    fn test_platform() -> crate::types::AppPlatform {
        crate::types::AppPlatform::Macos
    }

    #[cfg(target_os = "windows")]
    fn test_platform() -> crate::types::AppPlatform {
        crate::types::AppPlatform::Windows
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn builds_open_arguments() {
        let args = build_open_arguments(
            "/Applications/ChatGPT.app",
            &["--disable-quic".into()],
            &[("HTTPS_PROXY".into(), "socks5h://127.0.0.1:7890".into())],
        );
        assert_eq!(
            args,
            vec![
                "-n",
                "--env",
                "HTTPS_PROXY=socks5h://127.0.0.1:7890",
                "-a",
                "/Applications/ChatGPT.app",
                "--args",
                "--disable-quic"
            ]
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn builds_paste_safe_script() {
        let script = build_launch_script(
            &info(),
            &CodexProxyConfig::default(),
            Path::new("/Users/test/Library/Logs/CodexProxy"),
        )
        .unwrap();
        assert!(script.starts_with("/bin/zsh <<'CODEX_PROXY_SCRIPT'\nset -euo pipefail"));
        assert!(script.ends_with("\nCODEX_PROXY_SCRIPT\n"));
        assert!(script.contains("'--proxy-server=socks5://127.0.0.1:7890'"));
        assert!(script.contains("'HTTP_PROXY=socks5h://127.0.0.1:7890'"));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn builds_windows_powershell_script() {
        let mut windows_info = info();
        windows_info.platform = crate::types::AppPlatform::Windows;
        windows_info.app_path = Some(r"C:\Program Files\Codex\Codex.exe".into());
        windows_info.executable_path = windows_info.app_path.clone();
        let script = build_launch_script(
            &windows_info,
            &CodexProxyConfig::default(),
            Path::new(r"C:\Users\test\AppData\Local\CodexProxy\Logs"),
        )
        .unwrap();
        assert!(script.starts_with("& {\r\n"));
        assert!(script.contains("$startInfo.UseShellExecute = $false"));
        assert!(script.contains("--proxy-server=socks5://127.0.0.1:7890"));
        assert!(script.contains("$startInfo.EnvironmentVariables['HTTP_PROXY']"));
    }
}
