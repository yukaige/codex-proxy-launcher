import { mkdir, mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { containsElectronFramework } from '../src/main/codex-detector'

const temporaryDirectories: string[] = []

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((path) =>
      rm(path, { recursive: true, force: true }),
    ),
  )
})

describe('Electron Framework 检测', () => {
  it('识别应用包中的 Electron Framework', async () => {
    const root = await mkdtemp(join(tmpdir(), 'codex-detector-'))
    temporaryDirectories.push(root)
    const appPath = join(root, 'Codex.app')
    await mkdir(
      join(
        appPath,
        'Contents',
        'Frameworks',
        'Electron Framework.framework',
      ),
      { recursive: true },
    )

    await expect(containsElectronFramework(appPath)).resolves.toBe(true)
  })

  it('未找到 Framework 时返回 false', async () => {
    const root = await mkdtemp(join(tmpdir(), 'codex-detector-'))
    temporaryDirectories.push(root)
    const appPath = join(root, 'Codex.app')
    await mkdir(join(appPath, 'Contents', 'Frameworks'), {
      recursive: true,
    })

    await expect(containsElectronFramework(appPath)).resolves.toBe(false)
  })
})
