import { createConnection, type Socket } from 'node:net'
import { connect as connectTls, type TLSSocket } from 'node:tls'
import type {
  CodexProxyConfig,
  ProxyProtocol,
  ProxyTestResult,
  ProxyTestStage,
} from '../shared/types'
import {
  buildProxyUrl,
  ValidationError,
  validateProxyConfig,
} from './core'
import type { AppLogger } from './logger'

class ProxyTestError extends Error {
  override name = 'ProxyTestError'

  constructor(
    readonly stage: ProxyTestStage,
    message: string,
  ) {
    super(message)
  }
}

export class ProxyTester {
  constructor(private readonly logger: AppLogger) {}

  async test(value: unknown, timeoutMs = 5_000): Promise<ProxyTestResult> {
    const startedAt = Date.now()
    let config: CodexProxyConfig
    let proxyAddress = '无效地址'

    try {
      config = validateProxyConfig(value)
      proxyAddress = buildProxyUrl(config)
    } catch (error) {
      const message =
        error instanceof ValidationError ? error.message : '代理地址格式错误。'
      const result: ProxyTestResult = {
        success: false,
        message,
        proxyAddress,
        stage: 'validation',
      }
      await this.logger.log('WARN', '代理配置校验失败。', result)
      return result
    }

    let socket: Socket | undefined
    let secureSocket: TLSSocket | undefined
    try {
      const deadline = Date.now() + timeoutMs
      socket = await connectTcp(
        config.host,
        config.port,
        remaining(deadline),
      )
      await negotiateProxy(socket, config.protocol, remaining(deadline))
      secureSocket = await verifyHttps(socket, remaining(deadline))

      const result: ProxyTestResult = {
        success: true,
        latency: Date.now() - startedAt,
        message: `代理端口、${config.protocol.toUpperCase()} 握手和 HTTPS 请求均成功。`,
        proxyAddress,
        stage: 'https',
      }
      await this.logger.log('INFO', '代理连通性测试成功。', result)
      return result
    } catch (error) {
      const classified = classifyError(error)
      const result: ProxyTestResult = {
        success: false,
        message: classified.message,
        proxyAddress,
        stage: classified.stage,
      }
      await this.logger.log('WARN', '代理连通性测试失败。', {
        ...result,
        error,
      })
      return result
    } finally {
      secureSocket?.destroy()
      socket?.destroy()
    }
  }
}

function remaining(deadline: number): number {
  const value = deadline - Date.now()
  if (value <= 0) {
    throw new ProxyTestError(
      'tcp',
      '连接代理超时，请检查 Clash 是否启动及端口配置。',
    )
  }
  return value
}

function connectTcp(
  host: string,
  port: number,
  timeoutMs: number,
): Promise<Socket> {
  return new Promise((resolve, reject) => {
    const socket = createConnection({ host, port })
    const timer = setTimeout(() => {
      socket.destroy()
      reject(
        new ProxyTestError(
          'tcp',
          '连接代理超时，请检查 Clash 是否启动及端口配置。',
        ),
      )
    }, timeoutMs)

    socket.once('connect', () => {
      clearTimeout(timer)
      socket.removeAllListeners('error')
      resolve(socket)
    })
    socket.once('error', (error: NodeJS.ErrnoException) => {
      clearTimeout(timer)
      socket.destroy()
      if (error.code === 'ECONNREFUSED') {
        reject(
          new ProxyTestError(
            'tcp',
            '代理端口拒绝连接。Clash 可能未启动，或监听端口与配置不一致。',
          ),
        )
        return
      }
      if (error.code === 'ETIMEDOUT') {
        reject(
          new ProxyTestError(
            'tcp',
            '连接代理超时，请检查地址、防火墙和 Clash 状态。',
          ),
        )
        return
      }
      reject(new ProxyTestError('tcp', `无法连接代理端口：${error.message}`))
    })
  })
}

async function negotiateProxy(
  socket: Socket,
  protocol: ProxyProtocol,
  timeoutMs: number,
): Promise<void> {
  if (protocol === 'socks5') {
    const greeting = await exchange(
      socket,
      Buffer.from([0x05, 0x01, 0x00]),
      (buffer) => buffer.length >= 2,
      timeoutMs,
    )
    if (greeting[0] !== 0x05 || greeting[1] !== 0x00) {
      throw new ProxyTestError(
        'handshake',
        'SOCKS5 代理握手失败：代理未接受无需认证的 SOCKS5 连接。',
      )
    }

    const target = Buffer.from('www.gstatic.com', 'ascii')
    const request = Buffer.concat([
      Buffer.from([0x05, 0x01, 0x00, 0x03, target.length]),
      target,
      Buffer.from([0x01, 0xbb]),
    ])
    const response = await exchange(
      socket,
      request,
      hasCompleteSocksResponse,
      timeoutMs,
    )
    if (response[0] !== 0x05 || response[1] !== 0x00) {
      throw new ProxyTestError(
        'handshake',
        `SOCKS5 代理握手失败：远程连接返回状态 ${response[1] ?? '未知'}。`,
      )
    }
    return
  }

  const request = Buffer.from(
    [
      'CONNECT www.gstatic.com:443 HTTP/1.1',
      'Host: www.gstatic.com:443',
      'Proxy-Connection: Keep-Alive',
      '',
      '',
    ].join('\r\n'),
    'ascii',
  )
  const response = await exchange(
    socket,
    request,
    (buffer) => buffer.includes('\r\n\r\n'),
    timeoutMs,
  )
  const firstLine = response.toString('latin1').split('\r\n')[0] ?? ''
  if (!/^HTTP\/1\.[01] 2\d\d\b/u.test(firstLine)) {
    throw new ProxyTestError(
      'handshake',
      `HTTP 代理握手失败：${firstLine || '没有收到有效响应'}。`,
    )
  }
}

function hasCompleteSocksResponse(buffer: Buffer): boolean {
  if (buffer.length < 5) return false
  const addressType = buffer[3]
  if (addressType === 0x01) return buffer.length >= 10
  if (addressType === 0x04) return buffer.length >= 22
  if (addressType === 0x03) {
    const nameLength = buffer[4]
    return nameLength !== undefined && buffer.length >= 7 + nameLength
  }
  return true
}

function exchange(
  socket: Socket,
  payload: Buffer,
  isComplete: (buffer: Buffer) => boolean,
  timeoutMs: number,
): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    let collected = Buffer.alloc(0)
    const cleanup = () => {
      clearTimeout(timer)
      socket.off('data', onData)
      socket.off('error', onError)
      socket.off('close', onClose)
    }
    const onData = (chunk: Buffer) => {
      collected = Buffer.concat([collected, chunk])
      if (collected.length > 64 * 1024) {
        cleanup()
        reject(
          new ProxyTestError('handshake', '代理握手响应过大，已中止测试。'),
        )
      } else if (isComplete(collected)) {
        cleanup()
        resolve(collected)
      }
    }
    const onError = (error: Error) => {
      cleanup()
      reject(
        new ProxyTestError('handshake', `代理握手失败：${error.message}`),
      )
    }
    const onClose = () => {
      cleanup()
      reject(
        new ProxyTestError('handshake', '代理在握手完成前关闭了连接。'),
      )
    }
    const timer = setTimeout(() => {
      cleanup()
      reject(new ProxyTestError('handshake', '代理握手超时。'))
    }, timeoutMs)

    socket.on('data', onData)
    socket.once('error', onError)
    socket.once('close', onClose)
    socket.write(payload)
  })
}

function verifyHttps(socket: Socket, timeoutMs: number): Promise<TLSSocket> {
  return new Promise((resolve, reject) => {
    const secureSocket = connectTls({
      socket,
      servername: 'www.gstatic.com',
      rejectUnauthorized: true,
    })
    let response = ''
    const cleanup = () => {
      clearTimeout(timer)
      secureSocket.off('secureConnect', onSecure)
      secureSocket.off('data', onData)
      secureSocket.off('error', onError)
      secureSocket.off('close', onClose)
    }
    const onSecure = () => {
      secureSocket.write(
        'HEAD /generate_204 HTTP/1.1\r\nHost: www.gstatic.com\r\nConnection: close\r\n\r\n',
      )
    }
    const onData = (chunk: Buffer) => {
      response += chunk.toString('latin1')
      if (response.includes('\r\n\r\n')) {
        const firstLine = response.split('\r\n')[0] ?? ''
        cleanup()
        if (/^HTTP\/1\.[01] [23]\d\d\b/u.test(firstLine)) {
          resolve(secureSocket)
        } else {
          reject(
            new ProxyTestError(
              'https',
              `HTTPS 请求失败：${firstLine || '响应无效'}。`,
            ),
          )
        }
      }
    }
    const onError = (error: Error) => {
      cleanup()
      reject(new ProxyTestError('https', `HTTPS 请求失败：${error.message}`))
    }
    const onClose = () => {
      if (!response.includes('\r\n\r\n')) {
        cleanup()
        reject(
          new ProxyTestError('https', 'HTTPS 请求在收到响应前被关闭。'),
        )
      }
    }
    const timer = setTimeout(() => {
      cleanup()
      secureSocket.destroy()
      reject(new ProxyTestError('https', 'HTTPS 请求超时。'))
    }, timeoutMs)

    secureSocket.once('secureConnect', onSecure)
    secureSocket.on('data', onData)
    secureSocket.once('error', onError)
    secureSocket.once('close', onClose)
  })
}

function classifyError(error: unknown): ProxyTestError {
  if (error instanceof ProxyTestError) {
    return error
  }
  return new ProxyTestError(
    'tcp',
    error instanceof Error ? `代理测试失败：${error.message}` : '代理测试失败。',
  )
}
