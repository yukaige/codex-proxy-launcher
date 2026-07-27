import { chmod, mkdir, readFile, rename, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { defaultConfig, type CodexProxyConfig } from '../shared/types'
import { validateCodexAppPath, validateProxyConfig } from './core'

interface StoredSettings {
  config: CodexProxyConfig
  selectedAppPath?: string
}

export class SettingsStore {
  private readonly filePath: string

  constructor(userDataDirectory: string) {
    this.filePath = join(userDataDirectory, 'codex-proxy-settings.json')
  }

  async getConfig(): Promise<CodexProxyConfig> {
    const stored = await this.read()
    return stored?.config ?? { ...defaultConfig }
  }

  async saveConfig(value: unknown): Promise<CodexProxyConfig> {
    const config = validateProxyConfig(value)
    const current = await this.read()
    await this.write({
      config,
      ...(current?.selectedAppPath
        ? { selectedAppPath: current.selectedAppPath }
        : {}),
    })
    return config
  }

  async getSelectedAppPath(): Promise<string | undefined> {
    return (await this.read())?.selectedAppPath
  }

  async saveSelectedAppPath(value: string): Promise<void> {
    const selectedAppPath = validateCodexAppPath(value)
    const config = (await this.read())?.config ?? { ...defaultConfig }
    await this.write({ config, selectedAppPath })
  }

  private async read(): Promise<StoredSettings | undefined> {
    try {
      const text = await readFile(this.filePath, 'utf8')
      const value: unknown = JSON.parse(text)
      if (
        typeof value !== 'object' ||
        value === null ||
        Array.isArray(value)
      ) {
        return undefined
      }

      const record = value as Record<string, unknown>
      const config = validateProxyConfig(record.config)
      const selected =
        typeof record.selectedAppPath === 'string'
          ? validateCodexAppPath(record.selectedAppPath)
          : undefined
      return {
        config,
        ...(selected ? { selectedAppPath: selected } : {}),
      }
    } catch {
      return undefined
    }
  }

  private async write(settings: StoredSettings): Promise<void> {
    await mkdir(dirname(this.filePath), { recursive: true, mode: 0o700 })
    const temporaryPath = `${this.filePath}.tmp`
    await writeFile(
      temporaryPath,
      `${JSON.stringify(settings, null, 2)}\n`,
      {
        encoding: 'utf8',
        mode: 0o600,
      },
    )
    await chmod(temporaryPath, 0o600)
    await rename(temporaryPath, this.filePath)
  }
}
