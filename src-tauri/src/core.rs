use std::net::{IpAddr, Ipv6Addr};
use std::path::{Path, PathBuf};

use url::{Host, Url};

use crate::types::{CodexProxyConfig, ProxyProtocol};

pub const DEFAULT_BYPASS_LIST: &str = concat!(
    "localhost;127.0.0.1;[::1];<local>;",
    "10.0.0.0/8;172.16.0.0/12;192.168.0.0/16;169.254.0.0/16;",
    "fc00::/7;fe80::/10"
);

pub fn normalize_bypass_list(value: &str) -> String {
    let mut entries = DEFAULT_BYPASS_LIST
        .split(';')
        .map(str::to_string)
        .collect::<Vec<_>>();

    for entry in value.split([';', ',']).map(str::trim) {
        if entry.is_empty() || entry.eq_ignore_ascii_case("<-loopback>") {
            continue;
        }
        if !entries
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(entry))
        {
            entries.push(entry.to_string());
        }
    }

    entries.join(";")
}

pub fn normalize_whitelist_entry(value: &str) -> Result<String, String> {
    let entry = value.trim();
    if entry.is_empty()
        || entry.len() > 512
        || entry.contains([';', ',', '\r', '\n', '\0'])
        || entry.eq_ignore_ascii_case("<-loopback>")
    {
        return Err("白名单地址格式错误。请输入网址、域名、IP 或 CIDR 网段。".into());
    }

    if let Some((address, prefix)) = entry.split_once('/') {
        if let Ok(ip) = address.parse::<IpAddr>() {
            let prefix = prefix
                .parse::<u8>()
                .map_err(|_| "白名单 CIDR 前缀格式错误。".to_string())?;
            let maximum = if ip.is_ipv4() { 32 } else { 128 };
            if prefix > maximum {
                return Err("白名单 CIDR 前缀超出有效范围。".into());
            }
            return Ok(format!("{ip}/{prefix}"));
        }
    }

    if let Ok(ip) = entry.trim_matches(['[', ']']).parse::<IpAddr>() {
        return Ok(match ip {
            IpAddr::V4(_) => ip.to_string(),
            IpAddr::V6(_) => format!("[{ip}]"),
        });
    }

    if let Some(domain) = entry.strip_prefix("*.") {
        validate_host(domain)?;
        return Ok(format!("*.{}", domain.to_ascii_lowercase()));
    }

    let candidate = if entry.contains("://") {
        entry.to_string()
    } else {
        format!("http://{entry}")
    };
    let url = Url::parse(&candidate)
        .map_err(|_| "白名单地址格式错误。请输入网址、域名、IP 或 CIDR 网段。".to_string())?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("白名单网址不能包含用户名或密码。".into());
    }
    let host = match url.host() {
        Some(Host::Domain(domain)) => {
            validate_host(domain)?;
            domain.to_ascii_lowercase()
        }
        Some(Host::Ipv4(address)) => address.to_string(),
        Some(Host::Ipv6(address)) => format!("[{address}]"),
        None => return Err("白名单网址缺少主机名。".into()),
    };
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

pub fn normalize_config(config: &mut CodexProxyConfig) -> Result<(), String> {
    validate_config(config)?;
    let mut candidates = config.whitelist.clone();
    candidates.extend(
        config
            .bypass_list
            .split([';', ','])
            .map(str::trim)
            .filter(|entry| {
                !entry.is_empty()
                    && !entry.eq_ignore_ascii_case("<-loopback>")
                    && !DEFAULT_BYPASS_LIST
                        .split(';')
                        .any(|built_in| built_in.eq_ignore_ascii_case(entry))
            })
            .map(str::to_string),
    );

    let mut whitelist = Vec::new();
    for candidate in candidates {
        let entry = normalize_whitelist_entry(&candidate)?;
        if !whitelist
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&entry))
        {
            whitelist.push(entry);
        }
    }
    config.bypass_list = DEFAULT_BYPASS_LIST.into();
    config.whitelist = whitelist;
    Ok(())
}

fn effective_bypass_list(config: &CodexProxyConfig) -> Result<String, String> {
    let mut entries = normalize_bypass_list(&config.bypass_list);
    for entry in &config.whitelist {
        entries.push(';');
        entries.push_str(&normalize_whitelist_entry(entry)?);
    }
    Ok(normalize_bypass_list(&entries))
}

pub fn validate_config(config: &CodexProxyConfig) -> Result<(), String> {
    validate_host(&config.host)?;
    if config.port == 0 {
        return Err("代理端口必须是 1 到 65535 之间的整数。".into());
    }
    if config.bypass_list.len() > 2048 || config.bypass_list.contains(['\r', '\n', '\0']) {
        return Err("绕过地址格式错误。".into());
    }
    if config.whitelist.len() > 200 {
        return Err("白名单最多可保存 200 项。".into());
    }
    for entry in &config.whitelist {
        normalize_whitelist_entry(entry)?;
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
    let no_proxy = effective_bypass_list(config)?
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
    args.push(format!(
        "--proxy-bypass-list={}",
        effective_bypass_list(config)?
    ));
    if config.enable_debug_log {
        if let Some(path) = net_log_path {
            args.push(format!("--log-net-log={}", path.display()));
            args.push("--net-log-capture-mode=Everything".into());
        }
    }
    Ok(args)
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "windows")]
pub fn validate_app_path(value: &Path) -> Result<PathBuf, String> {
    if !value.is_absolute() {
        return Err("Codex 应用路径必须是绝对路径。".into());
    }
    let name = value
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !name.ends_with(".exe") {
        return Err("请选择有效的 Windows .exe 可执行文件。".into());
    }
    Ok(value.into())
}

#[cfg(target_os = "macos")]
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "windows")]
pub fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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
        assert!(environment.contains(&(
            "NO_PROXY".into(),
            concat!(
                "localhost,127.0.0.1,[::1],",
                "10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,169.254.0.0/16,",
                "fc00::/7,fe80::/10"
            )
            .into()
        )));
    }

    #[test]
    fn removes_negative_loopback_rule_and_preserves_custom_bypasses() {
        assert_eq!(
            normalize_bypass_list("localhost;127.0.0.1;<-loopback>"),
            DEFAULT_BYPASS_LIST
        );
        assert_eq!(
            normalize_bypass_list("example.com,LOCALHOST;<-LOOPBACK>"),
            concat!(
                "localhost;127.0.0.1;[::1];<local>;",
                "10.0.0.0/8;172.16.0.0/12;192.168.0.0/16;169.254.0.0/16;",
                "fc00::/7;fe80::/10;example.com"
            )
        );

        let config = CodexProxyConfig {
            bypass_list: "localhost;127.0.0.1;<-loopback>".into(),
            ..CodexProxyConfig::default()
        };
        let arguments = launch_arguments(&config, None).unwrap();
        assert!(arguments.contains(&format!("--proxy-bypass-list={DEFAULT_BYPASS_LIST}")));
        assert!(arguments
            .iter()
            .all(|argument| !argument.contains("<-loopback>")));
    }

    #[test]
    fn normalizes_and_applies_saved_whitelist_entries() {
        assert_eq!(
            normalize_whitelist_entry("https://Example.COM:8443/path?q=1").unwrap(),
            "example.com:8443"
        );
        assert_eq!(
            normalize_whitelist_entry("http://192.168.1.20:3000/dashboard").unwrap(),
            "192.168.1.20:3000"
        );
        assert_eq!(
            normalize_whitelist_entry("*.Example.LAN").unwrap(),
            "*.example.lan"
        );
        assert_eq!(
            normalize_whitelist_entry("100.64.0.0/10").unwrap(),
            "100.64.0.0/10"
        );
        assert!(normalize_whitelist_entry("https://user:secret@example.com").is_err());

        let mut config = CodexProxyConfig {
            bypass_list: format!("{DEFAULT_BYPASS_LIST};legacy.example.com"),
            whitelist: vec!["https://Example.COM/path".into(), "100.64.0.0/10".into()],
            ..CodexProxyConfig::default()
        };
        normalize_config(&mut config).unwrap();
        assert_eq!(config.bypass_list, DEFAULT_BYPASS_LIST);
        assert_eq!(
            config.whitelist,
            vec!["example.com", "100.64.0.0/10", "legacy.example.com"]
        );
        let arguments = launch_arguments(&config, None).unwrap();
        let bypass = arguments
            .iter()
            .find(|argument| argument.starts_with("--proxy-bypass-list="))
            .unwrap();
        assert!(bypass.contains(";example.com;100.64.0.0/10;legacy.example.com"));
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
        #[cfg(target_os = "macos")]
        assert_eq!(shell_quote("Codex's.app"), "'Codex'\"'\"'s.app'");
        #[cfg(target_os = "windows")]
        assert_eq!(powershell_quote("Codex's.exe"), "'Codex''s.exe'");
    }
}
