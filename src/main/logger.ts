import { appendFile, mkdir } from 'node:fs/promises'
import { join } from 'node:path'
import { redactSensitive } from './core'

export type LogLevel = 'INFO' | 'WARN' | 'ERROR'

export class AppLogger {
  readonly directory: string
  readonly filePath: string

  constructor(homeDirectory: string, fileName = 'launcher.log') {
    this.directory = join(homeDirectory, 'Library', 'Logs', 'CodexProxy')
    this.filePath = join(this.directory, fileName)
  }

  async ensureDirectory(): Promise<void> {
    await mkdir(this.directory, { recursive: true, mode: 0o700 })
  }

  async log(
    level: LogLevel,
    message: string,
    detail?: unknown,
  ): Promise<void> {
    await this.ensureDirectory()
    const detailText = detail === undefined ? '' : ` ${safeSerialize(detail)}`
    const line = redactSensitive(
      `${new Date().toISOString()} [${level}] ${message}${detailText}\n`,
    )
    await appendFile(this.filePath, line, { encoding: 'utf8', mode: 0o600 })
  }
}

function safeSerialize(value: unknown): string {
  if (value instanceof Error) {
    return JSON.stringify({
      name: value.name,
      message: value.message,
      stack: value.stack,
    })
  }

  try {
    return JSON.stringify(value)
  } catch {
    return JSON.stringify({ detail: String(value) })
  }
}
