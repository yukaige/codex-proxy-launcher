use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::core::{proxy_url, validate_config};
use crate::logger::AppLogger;
use crate::types::{CodexProxyConfig, TrafficVerificationResult};

const MAX_SCAN_BYTES: usize = 64 * 1024 * 1024;

pub fn verify(
    config: &CodexProxyConfig,
    launch_started_at: Option<SystemTime>,
    logger: &AppLogger,
) -> TrafficVerificationResult {
    let net_log_path = logger.directory().join("codex-net-log.json");
    let path_text = net_log_path.to_string_lossy().into_owned();
    if let Err(message) = validate_config(config) {
        return unverified(path_text, message);
    }
    let Some(started_at) = launch_started_at else {
        return unverified(
            path_text,
            "本次运行尚未通过启动器代理启动，无法关联网络日志。".into(),
        );
    };
    let metadata = match fs::metadata(&net_log_path) {
        Ok(value) => value,
        Err(_) => {
            return unverified(
                path_text,
                "找不到或无法读取 Chromium net-log。请确认已开启调试日志并重新代理启动。".into(),
            )
        }
    };
    if metadata
        .modified()
        .ok()
        .and_then(|modified| modified.checked_add(Duration::from_secs(1)))
        .is_some_and(|modified| modified < started_at)
    {
        return unverified(
            path_text,
            "网络日志早于本次启动，尚无可用于验证的新流量。".into(),
        );
    }
    if metadata.len() == 0 {
        return unverified(
            path_text,
            "网络日志尚为空，请先在 Codex 中发起一个新请求。".into(),
        );
    }
    match scan(&net_log_path, config) {
        Ok((verified, evidence)) => {
            let message = if verified {
                "已在本次 Chromium net-log 中发现通过指定代理建立连接的证据。"
            } else {
                "已读取网络日志，但尚未发现使用当前代理的连接证据。请在 Codex 中发起新请求后重试，并用代理软件连接日志交叉确认。"
            };
            logger.log(
                if verified { "INFO" } else { "WARN" },
                "实际代理流量验证完成。",
                Some(message),
            );
            TrafficVerificationResult {
                verified,
                message: message.into(),
                evidence,
                net_log_path: path_text,
            }
        }
        Err(_) => unverified(
            path_text,
            "找不到或无法读取 Chromium net-log。请确认已开启调试日志并重新代理启动。".into(),
        ),
    }
}

fn scan(path: &Path, config: &CodexProxyConfig) -> Result<(bool, Vec<String>), String> {
    let proxy = proxy_url(config)?.to_ascii_lowercase();
    let host_port = format!("{}:{}", config.host.to_ascii_lowercase(), config.port);
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut buffer = [0_u8; 256 * 1024];
    let mut carry = String::new();
    let mut scanned = 0;
    let mut address_seen = false;
    let mut route_seen = false;
    let mut truncated = false;
    loop {
        let size = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if size == 0 {
            break;
        }
        scanned += size;
        if scanned > MAX_SCAN_BYTES {
            truncated = true;
            break;
        }
        let chunk = String::from_utf8_lossy(&buffer[..size]).to_ascii_lowercase();
        let searchable = format!("{carry}{chunk}");
        address_seen |= searchable.contains(&proxy) || searchable.contains(&host_port);
        route_seen |= searchable.contains("\"proxy_chain\"")
            || searchable.contains("\"proxy_server\"")
            || searchable.contains("proxy_server_resolved");
        carry = searchable
            .chars()
            .rev()
            .take(4096)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
    }
    let mut evidence = Vec::new();
    if address_seen {
        evidence.push(format!("日志包含当前代理地址 {proxy}"));
    }
    if route_seen {
        evidence.push("日志包含 Chromium 代理路由/连接事件".into());
    }
    if truncated {
        evidence.push("日志超过 64 MiB，仅扫描了开头部分".into());
    }
    Ok((address_seen && route_seen, evidence))
}

fn unverified(net_log_path: String, message: String) -> TrafficVerificationResult {
    TrafficVerificationResult {
        verified: false,
        message,
        evidence: vec![],
        net_log_path,
    }
}
