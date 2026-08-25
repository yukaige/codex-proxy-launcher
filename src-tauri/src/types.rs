use serde::{Deserialize, Serialize};

use crate::core::DEFAULT_BYPASS_LIST;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexProxyConfig {
    pub enabled: bool,
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
    pub bypass_list: String,
    #[serde(default)]
    pub whitelist: Vec<String>,
    pub close_existing_instance: bool,
    pub enable_debug_log: bool,
}

impl Default for CodexProxyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            protocol: ProxyProtocol::Socks5,
            host: "127.0.0.1".into(),
            port: 7890,
            bypass_list: DEFAULT_BYPASS_LIST.into(),
            whitelist: Vec::new(),
            close_existing_instance: true,
            enable_debug_log: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyProtocol {
    Socks5,
    Http,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexRuntimeType {
    Electron,
    CodexChromium,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AppPlatform {
    #[cfg(target_os = "macos")]
    Macos,
    #[cfg(target_os = "windows")]
    Windows,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAppInfo {
    pub platform: AppPlatform,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub is_electron: bool,
    pub runtime_type: CodexRuntimeType,
    pub proxy_switch_compatible: bool,
    pub is_running: bool,
    pub pid_list: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyTestStage {
    Validation,
    Tcp,
    Handshake,
    Https,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyTestResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency: Option<u128>,
    pub message: String,
    pub proxy_address: String,
    pub stage: ProxyTestStage,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyLaunchStatus {
    NotStarted,
    AppNotFound,
    ProxyUnreachable,
    LaunchFailed,
    LaunchedWithProxyArgs,
    ProxyUnverified,
    ProxyVerified,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub success: bool,
    pub status: ProxyLaunchStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    pub proxy_args_passed: bool,
    pub traffic_verified: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub success: bool,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexStatus {
    pub is_running: bool,
    pub pid_list: Vec<u32>,
    pub launch_status: ProxyLaunchStatus,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficVerificationResult {
    pub verified: bool,
    pub message: String,
    pub evidence: Vec<String>,
    pub net_log_path: String,
}
