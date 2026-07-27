use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::core::validate_app_path;
use crate::logger::AppLogger;
use crate::types::{AppPlatform, CodexAppInfo, CodexRuntimeType};

#[cfg(target_os = "macos")]
use serde_json::Value;
#[cfg(target_os = "macos")]
use std::os::unix::fs::PermissionsExt;

#[cfg(target_os = "macos")]
pub fn detect(preferred_path: Option<&str>, home: &Path, logger: &AppLogger) -> CodexAppInfo {
    let mut candidates = Vec::new();
    if let Some(path) = preferred_path {
        candidates.push(PathBuf::from(path));
    }
    candidates.extend([
        PathBuf::from("/Applications/Codex.app"),
        home.join("Applications/Codex.app"),
        PathBuf::from("/Applications/ChatGPT.app"),
        home.join("Applications/ChatGPT.app"),
    ]);
    candidates.dedup();

    for candidate in candidates {
        if candidate.is_dir() {
            return inspect(&candidate, logger);
        }
    }
    logger.log("WARN", "未在默认位置找到 Codex.app。", None);
    missing(
        AppPlatform::Macos,
        "没有找到 Codex.app。请确认已安装，或手动选择应用。",
    )
}

#[cfg(target_os = "windows")]
pub fn detect(preferred_path: Option<&str>, _home: &Path, logger: &AppLogger) -> CodexAppInfo {
    let mut candidates = Vec::new();
    if let Some(path) = preferred_path {
        candidates.push(PathBuf::from(path));
    }
    for base in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
        let Some(directory) = std::env::var_os(base).map(PathBuf::from) else {
            continue;
        };
        candidates.extend([
            directory.join("Programs/Codex/Codex.exe"),
            directory.join("Programs/OpenAI/Codex/Codex.exe"),
            directory.join("Programs/ChatGPT/ChatGPT.exe"),
            directory.join("Programs/OpenAI/ChatGPT/ChatGPT.exe"),
            directory.join("Codex/Codex.exe"),
            directory.join("ChatGPT/ChatGPT.exe"),
        ]);
    }
    candidates.dedup();

    for candidate in candidates {
        if candidate.is_file() {
            return inspect(&candidate, logger);
        }
    }
    logger.log("WARN", "未在默认位置找到 Codex.exe 或 ChatGPT.exe。", None);
    missing(
        AppPlatform::Windows,
        "没有找到 Codex.exe 或 ChatGPT.exe。请确认已安装，或手动选择应用。",
    )
}

pub fn inspect(path: &Path, logger: &AppLogger) -> CodexAppInfo {
    let app_path = match fs::canonicalize(path)
        .map_err(|error| error.to_string())
        .and_then(|path| validate_app_path(&path))
    {
        Ok(path) => path,
        Err(message) => return invalid(path, &message),
    };
    match inspect_inner(&app_path) {
        Ok(info) => {
            logger.log("INFO", "检测到 Codex 应用。", info.app_path.as_deref());
            info
        }
        Err(message) => {
            logger.log("ERROR", "Codex 应用检测失败。", Some(&message));
            invalid(&app_path, &message)
        }
    }
}

#[cfg(target_os = "macos")]
fn inspect_inner(app_path: &Path) -> Result<CodexAppInfo, String> {
    let metadata = read_metadata(app_path)?;
    let executable_path = app_path
        .join("Contents/MacOS")
        .join(&metadata.executable_name);
    let executable_metadata = fs::metadata(&executable_path)
        .map_err(|_| "Info.plist 无法读取，或 Codex 可执行文件不存在。".to_string())?;
    if executable_metadata.permissions().mode() & 0o111 == 0 {
        return Err("Codex 可执行文件没有执行权限。".into());
    }

    let (runtime_type, proxy_switch_compatible) = detect_runtime(app_path, &executable_path);
    let executable = executable_path.to_string_lossy().into_owned();
    let pid_list = find_running_pids(&executable);
    let warning = (runtime_type == CodexRuntimeType::Unknown)
        .then(|| "未检测到 Chromium 运行时，代理启动参数可能无效。".into());
    Ok(CodexAppInfo {
        platform: AppPlatform::Macos,
        installed: true,
        app_path: Some(app_path.to_string_lossy().into_owned()),
        executable_path: Some(executable),
        bundle_id: Some(metadata.bundle_id),
        version: Some(metadata.version),
        is_electron: runtime_type == CodexRuntimeType::Electron,
        runtime_type,
        proxy_switch_compatible,
        is_running: !pid_list.is_empty(),
        pid_list,
        warning,
    })
}

#[cfg(target_os = "windows")]
fn inspect_inner(executable_path: &Path) -> Result<CodexAppInfo, String> {
    if !executable_path.is_file() {
        return Err("Codex 可执行文件不存在。".into());
    }
    let metadata = read_windows_metadata(executable_path);
    let (runtime_type, proxy_switch_compatible) = detect_runtime(
        executable_path.parent().unwrap_or(Path::new("")),
        executable_path,
    );
    let executable = executable_path.to_string_lossy().into_owned();
    let pid_list = find_running_pids(&executable);
    let warning = (runtime_type == CodexRuntimeType::Unknown)
        .then(|| "未检测到 Chromium/Electron 运行时，代理启动参数可能无效。".into());
    Ok(CodexAppInfo {
        platform: AppPlatform::Windows,
        installed: true,
        app_path: Some(executable.clone()),
        executable_path: Some(executable),
        bundle_id: metadata.as_ref().and_then(|value| value.product_name()),
        version: metadata.as_ref().and_then(|value| value.version()),
        is_electron: runtime_type == CodexRuntimeType::Electron,
        runtime_type,
        proxy_switch_compatible,
        is_running: !pid_list.is_empty(),
        pid_list,
        warning,
    })
}

#[cfg(target_os = "macos")]
struct PlistMetadata {
    bundle_id: String,
    version: String,
    executable_name: String,
}

#[cfg(target_os = "macos")]
fn read_metadata(app_path: &Path) -> Result<PlistMetadata, String> {
    let output = Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(app_path.join("Contents/Info.plist"))
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("Info.plist 无法读取或内容已损坏。".into());
    }
    let value: Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "Info.plist 内容无效。")?;
    let object = value
        .as_object()
        .ok_or_else(|| "Info.plist 内容无效。".to_string())?;
    let bundle_id = required_string(
        object.get("CFBundleIdentifier"),
        "Info.plist 缺少 Bundle ID。",
    )?;
    let executable_name = required_string(
        object.get("CFBundleExecutable"),
        "Info.plist 中的可执行文件名无效。",
    )?;
    if Path::new(&executable_name)
        .file_name()
        .and_then(|value| value.to_str())
        != Some(executable_name.as_str())
    {
        return Err("Info.plist 中的可执行文件名无效。".into());
    }
    let version = object
        .get("CFBundleShortVersionString")
        .or_else(|| object.get("CFBundleVersion"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("未知")
        .trim()
        .to_owned();
    Ok(PlistMetadata {
        bundle_id,
        version,
        executable_name,
    })
}

#[cfg(target_os = "macos")]
fn required_string(value: Option<&Value>, message: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| message.to_owned())
}

#[cfg(target_os = "windows")]
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsMetadata {
    file_version: Option<String>,
    product_version: Option<String>,
    product_name: Option<String>,
    original_filename: Option<String>,
}

#[cfg(target_os = "windows")]
impl WindowsMetadata {
    fn version(&self) -> Option<String> {
        self.product_version
            .as_deref()
            .or(self.file_version.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }

    fn product_name(&self) -> Option<String> {
        self.product_name
            .as_deref()
            .or(self.original_filename.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }
}

#[cfg(target_os = "windows")]
fn read_windows_metadata(executable_path: &Path) -> Option<WindowsMetadata> {
    let script = "$v=(Get-Item -LiteralPath $env:CODEX_TARGET_PATH).VersionInfo; \
        [pscustomobject]@{FileVersion=$v.FileVersion;ProductVersion=$v.ProductVersion;\
        ProductName=$v.ProductName;OriginalFilename=$v.OriginalFilename} | \
        ConvertTo-Json -Compress";
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("CODEX_TARGET_PATH", executable_path)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| serde_json::from_slice(&output.stdout).ok())
        .flatten()
}

fn detect_runtime(app_root: &Path, _executable_path: &Path) -> (CodexRuntimeType, bool) {
    #[cfg(target_os = "macos")]
    {
        let frameworks = app_root.join("Contents/Frameworks");
        if let Ok(entries) = fs::read_dir(&frameworks) {
            if entries.flatten().any(|entry| {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                entry.path().is_dir() && name.contains("electron") && name.contains("framework")
            }) {
                return (CodexRuntimeType::Electron, true);
            }
        }
        let binary = frameworks.join("Codex Framework.framework/Versions/Current/Codex Framework");
        if binary.exists() {
            return (
                CodexRuntimeType::CodexChromium,
                file_contains(&binary, b"proxy-server"),
            );
        }
    }
    #[cfg(target_os = "windows")]
    {
        if app_root.join("resources/app.asar").is_file() {
            return (CodexRuntimeType::Electron, true);
        }
        if file_contains(_executable_path, b"proxy-server") {
            return (CodexRuntimeType::CodexChromium, true);
        }
    }
    (CodexRuntimeType::Unknown, false)
}

fn file_contains(path: &Path, needle: &[u8]) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut carry = Vec::new();
    let mut buffer = [0_u8; 256 * 1024];
    loop {
        let Ok(size) = file.read(&mut buffer) else {
            return false;
        };
        if size == 0 {
            return false;
        }
        carry.extend_from_slice(&buffer[..size]);
        if carry.windows(needle.len()).any(|window| window == needle) {
            return true;
        }
        if carry.len() > needle.len() {
            carry.drain(..carry.len() - needle.len());
        }
    }
}

#[cfg(target_os = "macos")]
pub fn find_running_pids(executable_path: &str) -> Vec<u32> {
    let Ok(output) = Command::new("/bin/ps")
        .args(["-axo", "pid=,command="])
        .output()
    else {
        return vec![];
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let split = trimmed.find(char::is_whitespace)?;
            let pid = trimmed[..split].parse::<u32>().ok()?;
            let command = trimmed[split..].trim_start();
            (command == executable_path
                || command
                    .strip_prefix(executable_path)
                    .is_some_and(|rest| rest.starts_with(' ')))
            .then_some(pid)
        })
        .collect()
}

#[cfg(target_os = "windows")]
pub fn find_running_pids(executable_path: &str) -> Vec<u32> {
    let script = "$target=[IO.Path]::GetFullPath($env:CODEX_TARGET_PATH); \
        Get-Process | ForEach-Object { try { if ([string]::Equals(\
        [IO.Path]::GetFullPath($_.Path),$target,[StringComparison]::OrdinalIgnoreCase)) \
        { $_.Id } } catch {} }";
    let Ok(output) = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("CODEX_TARGET_PATH", executable_path)
        .output()
    else {
        return vec![];
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect()
}

fn missing(platform: AppPlatform, warning: &str) -> CodexAppInfo {
    CodexAppInfo {
        platform,
        installed: false,
        app_path: None,
        executable_path: None,
        bundle_id: None,
        version: None,
        is_electron: false,
        runtime_type: CodexRuntimeType::Unknown,
        proxy_switch_compatible: false,
        is_running: false,
        pid_list: vec![],
        warning: Some(warning.into()),
    }
}

fn invalid(path: &Path, warning: &str) -> CodexAppInfo {
    CodexAppInfo {
        platform: current_platform(),
        installed: true,
        app_path: Some(path.to_string_lossy().into_owned()),
        executable_path: None,
        bundle_id: None,
        version: None,
        is_electron: false,
        runtime_type: CodexRuntimeType::Unknown,
        proxy_switch_compatible: false,
        is_running: false,
        pid_list: vec![],
        warning: Some(warning.into()),
    }
}

#[cfg(target_os = "macos")]
fn current_platform() -> AppPlatform {
    AppPlatform::Macos
}

#[cfg(target_os = "windows")]
fn current_platform() -> AppPlatform {
    AppPlatform::Windows
}
