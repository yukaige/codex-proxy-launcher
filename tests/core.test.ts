import { describe, expect, it } from 'vitest'
import { defaultConfig } from '../src/shared/types'
import {
  buildBypassArgument,
  buildLaunchArguments,
  buildProxyEnvironment,
  buildProxyUrl,
  executablePathFor,
  parsePlistJson,
  redactSensitive,
  validateCodexAppPath,
  validatePort,
  ValidationError,
} from '../src/main/core'

describe('代理配置核心逻辑', () => {
  it('生成 SOCKS5 和 IPv6 代理 URL', () => {
    expect(buildProxyUrl({ ...defaultConfig })).toBe(
      'socks5://127.0.0.1:7890',
    )
    expect(
      buildProxyUrl({
        ...defaultConfig,
        protocol: 'http',
        host: '::1',
        port: 8080,
      }),
    ).toBe('http://[::1]:8080')
  })

  it('校验端口范围和整数类型', () => {
    expect(validatePort(1)).toBe(1)
    expect(validatePort(65_535)).toBe(65_535)
    for (const value of [0, 65_536, 7.5, '7890']) {
      expect(() => validatePort(value)).toThrow(ValidationError)
    }
  })

  it('生成 bypass 参数', () => {
    expect(
      buildBypassArgument('localhost;127.0.0.1;<-loopback>'),
    ).toEqual([
      '--proxy-bypass-list=localhost;127.0.0.1;<-loopback>',
    ])
    expect(buildBypassArgument('  ')).toEqual([])
  })

  it('生成完整启动参数并可选写入 net-log', () => {
    expect(
      buildLaunchArguments(
        { ...defaultConfig },
        '/tmp/codex-net-log.json',
      ),
    ).toEqual([
      '--proxy-server=socks5://127.0.0.1:7890',
      '--disable-quic',
      '--proxy-bypass-list=localhost;127.0.0.1;<-loopback>',
      '--log-net-log=/tmp/codex-net-log.json',
      '--net-log-capture-mode=Everything',
    ])
    expect(
      buildLaunchArguments({ ...defaultConfig, enabled: false }),
    ).toEqual([])
  })

  it('为 Codex app-server 生成大小写代理环境变量', () => {
    expect(buildProxyEnvironment({ ...defaultConfig })).toEqual({
      HTTP_PROXY: 'socks5h://127.0.0.1:7890',
      HTTPS_PROXY: 'socks5h://127.0.0.1:7890',
      ALL_PROXY: 'socks5h://127.0.0.1:7890',
      http_proxy: 'socks5h://127.0.0.1:7890',
      https_proxy: 'socks5h://127.0.0.1:7890',
      all_proxy: 'socks5h://127.0.0.1:7890',
      NO_PROXY: 'localhost,127.0.0.1',
      no_proxy: 'localhost,127.0.0.1',
    })
    expect(
      buildProxyEnvironment({ ...defaultConfig, enabled: false }),
    ).toEqual({})
  })
})

describe('Codex 应用元数据', () => {
  it('只接受绝对 .app 路径', () => {
    expect(validateCodexAppPath('/Applications/ChatGPT.app')).toBe(
      '/Applications/ChatGPT.app',
    )
    expect(() => validateCodexAppPath('ChatGPT.app')).toThrow(
      ValidationError,
    )
    expect(() => validateCodexAppPath('/Applications/ChatGPT')).toThrow(
      ValidationError,
    )
  })

  it('解析 Info.plist JSON 并生成可执行路径', () => {
    const metadata = parsePlistJson({
      CFBundleIdentifier: 'com.openai.codex',
      CFBundleExecutable: 'ChatGPT',
      CFBundleShortVersionString: '26.1',
    })
    expect(metadata).toEqual({
      bundleId: 'com.openai.codex',
      executableName: 'ChatGPT',
      version: '26.1',
    })
    expect(
      executablePathFor('/Applications/ChatGPT.app', 'ChatGPT'),
    ).toBe('/Applications/ChatGPT.app/Contents/MacOS/ChatGPT')
  })
})

describe('日志脱敏', () => {
  it('过滤 Token、Cookie、Authorization 和代理密码', () => {
    const value = redactSensitive(
      [
        'Authorization: Bearer secret-value',
        'Cookie=session-secret',
        'OPENAI_TOKEN=sk-abcdefghijklmnopqrstuvwxyz',
        'socks5://user:password@127.0.0.1:7890',
      ].join(' '),
    )
    expect(value).not.toContain('secret-value')
    expect(value).not.toContain('session-secret')
    expect(value).not.toContain('abcdefghijklmnopqrstuvwxyz')
    expect(value).not.toContain('password')
    expect(value).toContain('[REDACTED]')
  })
})
