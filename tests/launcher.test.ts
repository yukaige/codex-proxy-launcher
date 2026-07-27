import { describe, expect, it } from 'vitest'
import { buildOpenArguments } from '../src/main/launcher'

describe('macOS LaunchServices 启动参数', () => {
  it('通过 open -na 传入独立参数数组', () => {
    expect(
      buildOpenArguments('/Applications/ChatGPT.app', [
        '--proxy-server=socks5://127.0.0.1:7897',
        '--proxy-bypass-list=localhost;127.0.0.1;<-loopback>',
      ], {
        HTTPS_PROXY: 'socks5h://127.0.0.1:7897',
        NO_PROXY: 'localhost,127.0.0.1',
      }),
    ).toEqual([
      '-n',
      '--env',
      'HTTPS_PROXY=socks5h://127.0.0.1:7897',
      '--env',
      'NO_PROXY=localhost,127.0.0.1',
      '-a',
      '/Applications/ChatGPT.app',
      '--args',
      '--proxy-server=socks5://127.0.0.1:7897',
      '--proxy-bypass-list=localhost;127.0.0.1;<-loopback>',
    ])
  })

  it('普通启动时不添加 --args', () => {
    expect(
      buildOpenArguments('/Applications/ChatGPT.app', []),
    ).toEqual(['-n', '-a', '/Applications/ChatGPT.app'])
  })
})
