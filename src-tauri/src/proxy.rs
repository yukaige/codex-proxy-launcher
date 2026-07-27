use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use native_tls::TlsConnector;

use crate::core::{proxy_url, validate_config};
use crate::logger::AppLogger;
use crate::types::{CodexProxyConfig, ProxyProtocol, ProxyTestResult, ProxyTestStage};

pub fn test(config: &CodexProxyConfig, logger: &AppLogger) -> ProxyTestResult {
    let started = Instant::now();
    let proxy_address = match validate_config(config).and_then(|_| proxy_url(config)) {
        Ok(value) => value,
        Err(message) => {
            return ProxyTestResult {
                success: false,
                latency: None,
                message,
                proxy_address: "无效地址".into(),
                stage: ProxyTestStage::Validation,
            }
        }
    };

    let result = test_inner(config, Duration::from_secs(5));
    match result {
        Ok(()) => {
            let result = ProxyTestResult {
                success: true,
                latency: Some(started.elapsed().as_millis()),
                message: format!(
                    "代理端口、{} 握手和 HTTPS 请求均成功。",
                    match config.protocol {
                        ProxyProtocol::Socks5 => "SOCKS5",
                        ProxyProtocol::Http => "HTTP",
                    }
                ),
                proxy_address,
                stage: ProxyTestStage::Https,
            };
            logger.log("INFO", "代理连通性测试成功。", Some(&result.message));
            result
        }
        Err(error) => {
            logger.log("WARN", "代理连通性测试失败。", Some(&error.message));
            ProxyTestResult {
                success: false,
                latency: None,
                message: error.message,
                proxy_address,
                stage: error.stage,
            }
        }
    }
}

struct ProxyTestError {
    stage: ProxyTestStage,
    message: String,
}

fn test_inner(config: &CodexProxyConfig, timeout: Duration) -> Result<(), ProxyTestError> {
    let deadline = Instant::now() + timeout;
    let mut stream = connect_tcp(&config.host, config.port, remaining(deadline)?)?;
    stream
        .set_read_timeout(Some(remaining(deadline)?))
        .map_err(tcp_error)?;
    stream
        .set_write_timeout(Some(remaining(deadline)?))
        .map_err(tcp_error)?;
    match config.protocol {
        ProxyProtocol::Socks5 => negotiate_socks(&mut stream)?,
        ProxyProtocol::Http => negotiate_http(&mut stream)?,
    }
    verify_https(stream, remaining(deadline)?)
}

fn connect_tcp(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, ProxyTestError> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| ProxyTestError {
            stage: ProxyTestStage::Tcp,
            message: format!("无法解析代理地址：{error}"),
        })?;
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    let message = match last_error.and_then(|error| error.raw_os_error()) {
        Some(61) => "代理端口拒绝连接。代理软件可能未启动，或监听端口与配置不一致。".into(),
        _ => "无法连接代理端口，请检查地址、防火墙和代理软件状态。".into(),
    };
    Err(ProxyTestError {
        stage: ProxyTestStage::Tcp,
        message,
    })
}

fn negotiate_socks(stream: &mut TcpStream) -> Result<(), ProxyTestError> {
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .map_err(handshake_error)?;
    let mut greeting = [0_u8; 2];
    stream.read_exact(&mut greeting).map_err(handshake_error)?;
    if greeting != [0x05, 0x00] {
        return Err(ProxyTestError {
            stage: ProxyTestStage::Handshake,
            message: "SOCKS5 代理握手失败：代理未接受无需认证的连接。".into(),
        });
    }
    let target = b"www.gstatic.com";
    let mut request = vec![0x05, 0x01, 0x00, 0x03, target.len() as u8];
    request.extend_from_slice(target);
    request.extend_from_slice(&443_u16.to_be_bytes());
    stream.write_all(&request).map_err(handshake_error)?;
    let mut header = [0_u8; 4];
    stream.read_exact(&mut header).map_err(handshake_error)?;
    if header[0] != 0x05 || header[1] != 0x00 {
        return Err(ProxyTestError {
            stage: ProxyTestStage::Handshake,
            message: format!("SOCKS5 代理握手失败：远程连接返回状态 {}。", header[1]),
        });
    }
    let address_len = match header[3] {
        0x01 => 4,
        0x04 => 16,
        0x03 => {
            let mut length = [0_u8; 1];
            stream.read_exact(&mut length).map_err(handshake_error)?;
            length[0] as usize
        }
        _ => {
            return Err(ProxyTestError {
                stage: ProxyTestStage::Handshake,
                message: "SOCKS5 代理返回了未知地址类型。".into(),
            })
        }
    };
    let mut remainder = vec![0_u8; address_len + 2];
    stream.read_exact(&mut remainder).map_err(handshake_error)
}

fn negotiate_http(stream: &mut TcpStream) -> Result<(), ProxyTestError> {
    stream
        .write_all(
            b"CONNECT www.gstatic.com:443 HTTP/1.1\r\nHost: www.gstatic.com:443\r\nProxy-Connection: Keep-Alive\r\n\r\n",
        )
        .map_err(handshake_error)?;
    let response = read_headers(stream).map_err(handshake_error)?;
    let first_line = response.lines().next().unwrap_or_default();
    if !(first_line.starts_with("HTTP/1.0 2") || first_line.starts_with("HTTP/1.1 2")) {
        return Err(ProxyTestError {
            stage: ProxyTestStage::Handshake,
            message: format!(
                "HTTP 代理握手失败：{}。",
                if first_line.is_empty() {
                    "没有收到有效响应"
                } else {
                    first_line
                }
            ),
        });
    }
    Ok(())
}

fn verify_https(stream: TcpStream, timeout: Duration) -> Result<(), ProxyTestError> {
    stream
        .set_read_timeout(Some(timeout))
        .map_err(https_error)?;
    let connector = TlsConnector::new().map_err(https_error)?;
    let mut tls = connector
        .connect("www.gstatic.com", stream)
        .map_err(https_error)?;
    tls.write_all(
        b"HEAD /generate_204 HTTP/1.1\r\nHost: www.gstatic.com\r\nConnection: close\r\n\r\n",
    )
    .map_err(https_error)?;
    let response = read_headers(&mut tls).map_err(https_error)?;
    let first_line = response.lines().next().unwrap_or_default();
    let valid = first_line.starts_with("HTTP/1.0 2")
        || first_line.starts_with("HTTP/1.0 3")
        || first_line.starts_with("HTTP/1.1 2")
        || first_line.starts_with("HTTP/1.1 3");
    if !valid {
        return Err(ProxyTestError {
            stage: ProxyTestStage::Https,
            message: format!("HTTPS 请求失败：{}。", first_line),
        });
    }
    Ok(())
}

fn read_headers(reader: &mut impl Read) -> std::io::Result<String> {
    let mut output = Vec::new();
    let mut byte = [0_u8; 1];
    while output.len() < 64 * 1024 {
        reader.read_exact(&mut byte)?;
        output.push(byte[0]);
        if output.ends_with(b"\r\n\r\n") {
            return Ok(String::from_utf8_lossy(&output).into_owned());
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "代理响应过大",
    ))
}

fn remaining(deadline: Instant) -> Result<Duration, ProxyTestError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|value| !value.is_zero())
        .ok_or_else(|| ProxyTestError {
            stage: ProxyTestStage::Tcp,
            message: "连接代理超时，请检查代理软件是否启动及端口配置。".into(),
        })
}

fn tcp_error(error: impl std::fmt::Display) -> ProxyTestError {
    ProxyTestError {
        stage: ProxyTestStage::Tcp,
        message: format!("无法连接代理端口：{error}"),
    }
}

fn handshake_error(error: impl std::fmt::Display) -> ProxyTestError {
    ProxyTestError {
        stage: ProxyTestStage::Handshake,
        message: format!("代理握手失败：{error}"),
    }
}

fn https_error(error: impl std::fmt::Display) -> ProxyTestError {
    ProxyTestError {
        stage: ProxyTestStage::Https,
        message: format!("HTTPS 请求失败：{error}"),
    }
}
