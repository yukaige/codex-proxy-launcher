import { createReadStream } from 'node:fs'
import { stat } from 'node:fs/promises'
import { join } from 'node:path'
import type {
  CodexProxyConfig,
  TrafficVerificationResult,
} from '../shared/types'
import { buildProxyUrl, validateProxyConfig } from './core'
import type { AppLogger } from './logger'

const MAX_SCAN_BYTES = 64 * 1024 * 1024

export class TrafficVerifier {
  constructor(private readonly logger: AppLogger) {}

  async verify(
    value: unknown,
    launchStartedAt?: Date,
  ): Promise<TrafficVerificationResult> {
    const config = validateProxyConfig(value)
    const netLogPath = join(this.logger.directory, 'codex-net-log.json')
    if (!launchStartedAt) {
      return unverified(
        netLogPath,
        '本次运行尚未通过启动器代理启动，无法关联网络日志。',
      )
    }

    try {
      const details = await stat(netLogPath)
      if (details.mtimeMs + 1_000 < launchStartedAt.getTime()) {
        return unverified(
          netLogPath,
          '网络日志早于本次启动，尚无可用于验证的新流量。',
        )
      }
      if (details.size === 0) {
        return unverified(
          netLogPath,
          '网络日志尚为空，请先在 Codex 中发起一个新请求。',
        )
      }

      const evidence = await scanNetLog(netLogPath, config)
      const result: TrafficVerificationResult = evidence.verified
        ? {
            verified: true,
            message:
              '已在本次 Chromium net-log 中发现通过指定代理建立连接的证据。',
            evidence: evidence.items,
            netLogPath,
          }
        : {
            verified: false,
            message:
              '已读取网络日志，但尚未发现使用当前代理的连接证据。请在 Codex 中发起新请求后重试，并用 Clash 连接日志交叉确认。',
            evidence: evidence.items,
            netLogPath,
          }
      await this.logger.log(
        result.verified ? 'INFO' : 'WARN',
        '实际代理流量验证完成。',
        result,
      )
      return result
    } catch (error) {
      await this.logger.log('WARN', '无法读取 Chromium net-log。', error)
      return unverified(
        netLogPath,
        '找不到或无法读取 Chromium net-log。请确认已开启调试日志并重新代理启动。',
      )
    }
  }
}

async function scanNetLog(
  path: string,
  config: CodexProxyConfig,
): Promise<{ verified: boolean; items: string[] }> {
  const proxyUrl = buildProxyUrl(config).toLowerCase()
  const hostPort = `${config.host.toLowerCase()}:${config.port}`
  let scannedBytes = 0
  let carry = ''
  let addressSeen = false
  let routeEvidenceSeen = false
  let truncated = false
  const stream = createReadStream(path, {
    encoding: 'utf8',
    highWaterMark: 256 * 1024,
  })

  for await (const chunk of stream) {
    scannedBytes += Buffer.byteLength(chunk)
    if (scannedBytes > MAX_SCAN_BYTES) {
      truncated = true
      stream.destroy()
      break
    }
    const searchable = `${carry}${chunk}`.toLowerCase()
    addressSeen ||=
      searchable.includes(proxyUrl) || searchable.includes(hostPort)
    routeEvidenceSeen ||=
      searchable.includes('"proxy_chain"') ||
      searchable.includes('"proxy_server"') ||
      searchable.includes('proxy_server_resolved')
    carry = searchable.slice(-4_096)
  }

  const items: string[] = []
  if (addressSeen) items.push(`日志包含当前代理地址 ${proxyUrl}`)
  if (routeEvidenceSeen) items.push('日志包含 Chromium 代理路由/连接事件')
  if (truncated) items.push('日志超过 64 MiB，仅扫描了开头部分')
  return {
    verified: addressSeen && routeEvidenceSeen,
    items,
  }
}

function unverified(
  netLogPath: string,
  message: string,
): TrafficVerificationResult {
  return {
    verified: false,
    message,
    evidence: [],
    netLogPath,
  }
}
