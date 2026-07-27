use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::core::validate_app_path;
use crate::logger::AppLogger;
use crate::types::{CodexAppInfo, CodexRuntimeType};

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
    CodexAppInfo {
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
        warning: Some("没有找到 Codex.app。请确认已安装，或手动选择应用。".into()),
    }
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

    let (runtime_type, proxy_switch_compatible) = detect_runtime(app_path);
    let executable = executable_path.to_string_lossy().into_owned();
    let pid_list = find_running_pids(&executable);
    let warning = (runtime_type == CodexRuntimeType::Unknown)
        .then(|| "未检测到 Chromium 运行时，代理启动参数可能无效。".into());
    Ok(CodexAppInfo {
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

struct PlistMetadata {
    bundle_id: String,
    version: String,
    executable_name: String,
}

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
        .and_then(|v| v.to_str())
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

fn required_string(value: Option<&Value>, message: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| message.to_owned())
}

fn detect_runtime(app_path: &Path) -> (CodexRuntimeType, bool) {
    let frameworks = app_path.join("Contents/Frameworks");
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

fn invalid(path: &Path, warning: &str) -> CodexAppInfo {
    CodexAppInfo {
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
