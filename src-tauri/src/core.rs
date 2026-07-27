use std::net::{IpAddr, Ipv6Addr};
use std::path::{Path, PathBuf};

use crate::types::{CodexProxyConfig, ProxyProtocol};

pub fn validate_config(config: &CodexProxyConfig) -> Result<(), String> {
    validate_host(&config.host)?;
    if config.port == 0 {
        return Err("代理端口必须是 1 到 65535 之间的整数。".into());
    }
    if config.bypass_list.len() > 2048 || config.bypass_list.contains(['\r', '\n', '\0']) {
        return Err("绕过地址格式错误。".into());
    }
    Ok(())
}

pub fn validate_host(value: &str) -> Result<String, String> {
    let host = value.trim();
    if host.is_empty() {
        return Err("代理主机不能为空。".into());
    }
    if host.len() > 253
        || host
            .chars()
            .any(|c| c.is_whitespace() || "/@?#".contains(c))
    {
        return Err("代理主机格式错误。请输入 IP 地址或主机名。".into());
    }

    let unwrapped = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    if unwrapped.parse::<IpAddr>().is_ok() {
        return Ok(unwrapped.into());
    }

    let valid = host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && label
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
            && label
                .chars()
                .last()
                .is_some_and(|c| c.is_ascii_alphanumeric())
    });
    if !valid {
        return Err("代理主机格式错误。请输入 IP 地址或主机名。".into());
    }
    Ok(host.into())
}

fn display_host(host: &str) -> String {
    if host.parse::<Ipv6Addr>().is_ok() {
        format!("[{host}]")
    } else {
        host.into()
    }
}

pub fn proxy_url(config: &CodexProxyConfig) -> Result<String, String> {
    validate_config(config)?;
    let protocol = match config.protocol {
        ProxyProtocol::Socks5 => "socks5",
        ProxyProtocol::Http => "http",
    };
    Ok(format!(
        "{protocol}://{}:{}",
        display_host(config.host.trim_matches(['[', ']'])),
        config.port
    ))
}

pub fn app_server_proxy_url(config: &CodexProxyConfig) -> Result<String, String> {
    let url = proxy_url(config)?;
    Ok(match config.protocol {
        ProxyProtocol::Socks5 => url.replacen("socks5:", "socks5h:", 1),
        ProxyProtocol::Http => url,
    })
}

pub fn proxy_environment(config: &CodexProxyConfig) -> Result<Vec<(String, String)>, String> {
    validate_config(config)?;
    if !config.enabled {
        return Ok(Vec::new());
    }
    let url = app_server_proxy_url(config)?;
    let mut environment = [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ]
    .into_iter()
    .map(|name| (name.into(), url.clone()))
    .collect::<Vec<_>>();
    let no_proxy = config
        .bypass_list
        .split([';', ','])
        .map(str::trim)
        .filter(|entry| !(entry.is_empty() || entry.starts_with('<') && entry.ends_with('>')))
        .collect::<Vec<_>>()
        .join(",");
    if !no_proxy.is_empty() {
        environment.push(("NO_PROXY".into(), no_proxy.clone()));
        environment.push(("no_proxy".into(), no_proxy));
    }
    Ok(environment)
}

pub fn launch_arguments(
    config: &CodexProxyConfig,
    net_log_path: Option<&Path>,
) -> Result<Vec<String>, String> {
    validate_config(config)?;
    if !config.enabled {
        return Ok(Vec::new());
    }
    let mut args = vec![
        format!("--proxy-server={}", proxy_url(config)?),
        "--disable-quic".into(),
    ];
    if !config.bypass_list.trim().is_empty() {
        args.push(format!("--proxy-bypass-list={}", config.bypass_list.trim()));
    }
    if config.enable_debug_log {
        if let Some(path) = net_log_path {
            args.push(format!("--log-net-log={}", path.display()));
            args.push("--net-log-capture-mode=Everything".into());
        }
    }
    Ok(args)
}

pub fn validate_app_path(value: &Path) -> Result<PathBuf, String> {
    if !value.is_absolute() {
        return Err("Codex 应用路径必须是绝对路径。".into());
    }
    let name = value
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !name.ends_with(".app") {
        return Err("请选择有效的 macOS .app 应用包。".into());
    }
    Ok(value.into())
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_socks_urls_and_environment() {
        let config = CodexProxyConfig::default();
        assert_eq!(proxy_url(&config).unwrap(), "socks5://127.0.0.1:7890");
        assert_eq!(
            app_server_proxy_url(&config).unwrap(),
            "socks5h://127.0.0.1:7890"
        );
        let environment = proxy_environment(&config).unwrap();
        assert!(environment.contains(&("HTTPS_PROXY".into(), "socks5h://127.0.0.1:7890".into())));
        assert!(environment.contains(&("NO_PROXY".into(), "localhost,127.0.0.1".into())));
    }

    #[test]
    fn validates_hosts_and_quotes_shell_values() {
        assert!(validate_host("::1").is_ok());
        assert!(validate_host("proxy.local").is_ok());
        assert!(validate_host("bad host").is_err());
        let invalid = CodexProxyConfig {
            port: 0,
            ..CodexProxyConfig::default()
        };
        assert!(validate_config(&invalid).is_err());
        assert_eq!(shell_quote("Codex's.app"), "'Codex'\"'\"'s.app'");
    }
}
