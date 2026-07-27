import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { AppLogger } from '../src/main/logger'

const temporaryDirectories: string[] = []

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((path) =>
      rm(path, { recursive: true, force: true }),
    ),
  )
})

describe('安全日志', () => {
  it('写入日志前过滤敏感字段', async () => {
    const home = await mkdtemp(join(tmpdir(), 'codex-logger-'))
    temporaryDirectories.push(home)
    const logger = new AppLogger(home)

    await logger.log(
      'INFO',
      '测试',
      'Authorization=Bearer secret Cookie=session-secret',
    )
    const content = await readFile(logger.filePath, 'utf8')
    expect(content).toContain('[REDACTED]')
    expect(content).not.toContain('session-secret')
    expect(content).not.toContain(' secret')
  })
})
