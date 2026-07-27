import { isIP } from 'node:net'
import { basename, isAbsolute, join, normalize, resolve } from 'node:path'
import type { CodexProxyConfig, ProxyProtocol } from '../shared/types'

export class ValidationError extends Error {
  override name = 'ValidationError'
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function requireBoolean(record: Record<string, unknown>, key: string): boolean {
  const value = record[key]
  if (typeof value !== 'boolean') {
    throw new ValidationError(`${key} 必须是布尔值。`)
  }
  return value
}

function validateHost(hostValue: unknown): string {
  if (typeof hostValue !== 'string' || hostValue.trim() === '') {
    throw new ValidationError('代理主机不能为空。')
  }

  const host = hostValue.trim()
  if (host.length > 253 || /[\s/@?#]/u.test(host)) {
    throw new ValidationError('代理主机格式错误。请输入 IP 地址或主机名。')
  }

  const unwrapped =
    host.startsWith('[') && host.endsWith(']') ? host.slice(1, -1) : host
  if (isIP(unwrapped) !== 0) {
    return unwrapped
  }

  const labels = host.split('.')
  if (
    labels.some(
      (label) =>
        label.length === 0 ||
        label.length > 63 ||
        !/^[a-zA-Z0-9](?:[a-zA-Z0-9-]*[a-zA-Z0-9])?$/u.test(label),
    )
  ) {
    throw new ValidationError('代理主机格式错误。请输入 IP 地址或主机名。')
  }
  return host
}

export function validatePort(value: unknown): number {
  if (
    typeof value !== 'number' ||
    !Number.isInteger(value) ||
    value < 1 ||
    value > 65_535
  ) {
    throw new ValidationError('代理端口必须是 1 到 65535 之间的整数。')
  }
  return value
}

export function validateProxyConfig(value: unknown): CodexProxyConfig {
  if (!isRecord(value)) {
    throw new ValidationError('代理配置格式错误。')
  }

  const protocol = value.protocol
  if (protocol !== 'socks5' && protocol !== 'http') {
    throw new ValidationError('代理协议只能是 SOCKS5 或 HTTP。')
  }

  const bypassValue = value.bypassList
  if (
    typeof bypassValue !== 'string' ||
    bypassValue.length > 2_048 ||
    /[\r\n\0]/u.test(bypassValue)
  ) {
    throw new ValidationError('绕过地址格式错误。')
  }

  return {
    enabled: requireBoolean(value, 'enabled'),
    protocol,
    host: validateHost(value.host),
    port: validatePort(value.port),
    bypassList: bypassValue.trim(),
    closeExistingInstance: requireBoolean(value, 'closeExistingInstance'),
    enableDebugLog: requireBoolean(value, 'enableDebugLog'),
  }
}

function displayHost(host: string): string {
  return isIP(host) === 6 ? `[${host}]` : host
}

export function buildProxyUrl(config: CodexProxyConfig): string {
  const normalized = validateProxyConfig({
    ...config,
    enabled: true,
    bypassList: '',
    closeExistingInstance: false,
    enableDebugLog: false,
  })
  return `${normalized.protocol}://${displayHost(normalized.host)}:${normalized.port}`
}

export function buildProxyEnvironment(
  config: CodexProxyConfig,
): Record<string, string> {
  const normalized = validateProxyConfig(config)
  if (!normalized.enabled) {
    return {}
  }

  // Let the Codex app-server resolve names through the SOCKS proxy as well.
  // Chromium uses --proxy-server separately and does not accept socks5h.
  const proxyUrl =
    normalized.protocol === 'socks5'
      ? `socks5h://${displayHost(normalized.host)}:${normalized.port}`
      : buildProxyUrl(normalized)
  const noProxy = normalized.bypassList
    .split(/[;,]/u)
    .map((entry) => entry.trim())
    .filter((entry) => entry && !/^<.*>$/u.test(entry))
    .join(',')

  const environment: Record<string, string> = {
    HTTP_PROXY: proxyUrl,
    HTTPS_PROXY: proxyUrl,
    ALL_PROXY: proxyUrl,
    http_proxy: proxyUrl,
    https_proxy: proxyUrl,
    all_proxy: proxyUrl,
  }
  if (noProxy) {
    environment.NO_PROXY = noProxy
    environment.no_proxy = noProxy
  }
  return environment
}

export function buildBypassArgument(bypassList: string): string[] {
  const normalized = bypassList.trim()
  return normalized ? [`--proxy-bypass-list=${normalized}`] : []
}

export function buildLaunchArguments(
  config: CodexProxyConfig,
  netLogPath?: string,
): string[] {
  const normalized = validateProxyConfig(config)
  if (!normalized.enabled) {
    return []
  }

  const args = [
    `--proxy-server=${buildProxyUrl(normalized)}`,
    '--disable-quic',
    ...buildBypassArgument(normalized.bypassList),
  ]

  if (normalized.enableDebugLog && netLogPath) {
    args.push(
      `--log-net-log=${netLogPath}`,
      '--net-log-capture-mode=Everything',
    )
  }
  return args
}

export function validateCodexAppPath(value: string): string {
  if (!isAbsolute(value)) {
    throw new ValidationError('Codex 应用路径必须是绝对路径。')
  }

  const normalized = normalize(resolve(value))
  if (!basename(normalized).toLowerCase().endsWith('.app')) {
    throw new ValidationError('请选择有效的 macOS .app 应用包。')
  }
  return normalized
}

export interface PlistMetadata {
  bundleId: string
  version: string
  executableName: string
}

export function parsePlistJson(value: unknown): PlistMetadata {
  if (!isRecord(value)) {
    throw new ValidationError('Info.plist 内容无效。')
  }

  const bundleId = value.CFBundleIdentifier
  const executableName = value.CFBundleExecutable
  const shortVersion = value.CFBundleShortVersionString
  const buildVersion = value.CFBundleVersion

  if (typeof bundleId !== 'string' || bundleId.trim() === '') {
    throw new ValidationError('Info.plist 缺少 Bundle ID。')
  }
  if (
    typeof executableName !== 'string' ||
    executableName.trim() === '' ||
    basename(executableName) !== executableName
  ) {
    throw new ValidationError('Info.plist 中的可执行文件名无效。')
  }

  const version =
    typeof shortVersion === 'string' && shortVersion.trim()
      ? shortVersion.trim()
      : typeof buildVersion === 'string' && buildVersion.trim()
        ? buildVersion.trim()
        : '未知'

  return {
    bundleId: bundleId.trim(),
    version,
    executableName,
  }
}

export function executablePathFor(
  appPath: string,
  executableName: string,
): string {
  return join(
    validateCodexAppPath(appPath),
    'Contents',
    'MacOS',
    basename(executableName),
  )
}

export function redactSensitive(input: string): string {
  return input
    .replace(
      /\b(authorization)\b\s*[:=]\s*(?:Bearer\s+)?[^\s,;]+/giu,
      '$1=[REDACTED]',
    )
    .replace(
      /\b(cookie|set-cookie|openai[_-]?token|api[_-]?key)\b\s*[:=]\s*[^\s,;]+/giu,
      '$1=[REDACTED]',
    )
    .replace(
      /\b(Bearer)\s+[A-Za-z0-9._~+/=-]+/giu,
      '$1 [REDACTED]',
    )
    .replace(
      /\b(https?|socks5):\/\/([^/\s:@]+):([^@\s/]+)@/giu,
      '$1://[REDACTED]@',
    )
    .replace(/\b(sk-[A-Za-z0-9_-]{12,})\b/gu, '[REDACTED_TOKEN]')
}

export function protocolLabel(protocol: ProxyProtocol): string {
  return protocol === 'socks5' ? 'SOCKS5' : 'HTTP'
}
