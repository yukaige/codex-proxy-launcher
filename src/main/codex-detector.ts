import { execFile } from 'node:child_process'
import { constants } from 'node:fs'
import { access, readdir, stat } from 'node:fs/promises'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { promisify } from 'node:util'
import type { CodexAppInfo, CodexRuntimeType } from '../shared/types'
import {
  executablePathFor,
  parsePlistJson,
  validateCodexAppPath,
} from './core'
import type { AppLogger } from './logger'

const execFileAsync = promisify(execFile)

interface RuntimeDetection {
  type: CodexRuntimeType
  proxySwitchCompatible: boolean
}

export class CodexDetector {
  constructor(
    private readonly logger: AppLogger,
    private readonly defaultCandidates: string[] = [
      '/Applications/Codex.app',
      join(homedir(), 'Applications', 'Codex.app'),
      '/Applications/ChatGPT.app',
      join(homedir(), 'Applications', 'ChatGPT.app'),
    ],
  ) {}

  async detect(preferredPath?: string): Promise<CodexAppInfo> {
    const candidates = [
      ...(preferredPath ? [validateCodexAppPath(preferredPath)] : []),
      ...this.defaultCandidates,
    ]
    const uniqueCandidates = [...new Set(candidates)]

    for (const candidate of uniqueCandidates) {
      if (await isDirectory(candidate)) {
        return this.inspect(candidate)
      }
    }

    await this.logger.log('WARN', '未在默认位置找到 Codex.app。', {
      candidates: uniqueCandidates,
    })
    return {
      installed: false,
      isElectron: false,
      runtimeType: 'unknown',
      proxySwitchCompatible: false,
      isRunning: false,
      pidList: [],
      warning: '没有找到 Codex.app。请确认已安装，或手动选择应用。',
    }
  }

  async inspect(appPathValue: string): Promise<CodexAppInfo> {
    const appPath = validateCodexAppPath(appPathValue)
    try {
      if (!(await isDirectory(appPath))) {
        return invalidApplication(appPath, 'Codex 路径无效或应用不存在。')
      }

      const metadata = await readMetadata(appPath)
      const executablePath = executablePathFor(
        appPath,
        metadata.executableName,
      )
      await access(executablePath, constants.X_OK)
      const runtime = await detectRuntime(appPath)
      const pidList = await findRunningPids(executablePath)
      const result: CodexAppInfo = {
        installed: true,
        appPath,
        executablePath,
        bundleId: metadata.bundleId,
        version: metadata.version,
        isElectron: runtime.type === 'electron',
        runtimeType: runtime.type,
        proxySwitchCompatible: runtime.proxySwitchCompatible,
        isRunning: pidList.length > 0,
        pidList,
        ...(runtime.type === 'unknown'
          ? { warning: '未检测到 Electron Framework，代理启动参数可能无效。' }
          : {}),
      }

      await this.logger.log('INFO', '检测到 Codex 应用。', {
        appPath,
        executablePath,
        bundleId: metadata.bundleId,
        version: metadata.version,
        runtimeType: runtime.type,
        proxySwitchCompatible: runtime.proxySwitchCompatible,
        pidList,
      })
      return result
    } catch (error) {
      const message = readableInspectionError(error)
      await this.logger.log('ERROR', 'Codex 应用检测失败。', error)
      return invalidApplication(appPath, message)
    }
  }
}

async function readMetadata(appPath: string) {
  const plistPath = join(appPath, 'Contents', 'Info.plist')
  await access(plistPath, constants.R_OK)
  const { stdout } = await execFileAsync(
    '/usr/bin/plutil',
    ['-convert', 'json', '-o', '-', plistPath],
    {
      encoding: 'utf8',
      timeout: 5_000,
      maxBuffer: 1_000_000,
    },
  )
  return parsePlistJson(JSON.parse(stdout))
}

export async function containsElectronFramework(
  appPathValue: string,
): Promise<boolean> {
  const appPath = validateCodexAppPath(appPathValue)
  const frameworksPath = join(appPath, 'Contents', 'Frameworks')
  try {
    const entries = await readdir(frameworksPath, { withFileTypes: true })
    return entries.some(
      (entry) =>
        entry.isDirectory() &&
        /electron.*framework|electron framework/iu.test(entry.name),
    )
  } catch {
    return false
  }
}

export async function detectRuntime(
  appPathValue: string,
): Promise<RuntimeDetection> {
  const appPath = validateCodexAppPath(appPathValue)
  if (await containsElectronFramework(appPath)) {
    return { type: 'electron', proxySwitchCompatible: true }
  }

  const codexFramework = join(
    appPath,
    'Contents',
    'Frameworks',
    'Codex Framework.framework',
  )
  if (await isDirectory(codexFramework)) {
    const frameworkBinary = join(
      codexFramework,
      'Versions',
      'Current',
      'Codex Framework',
    )
    return {
      type: 'codex_chromium',
      proxySwitchCompatible: await binaryContainsProxySwitch(frameworkBinary),
    }
  }
  return { type: 'unknown', proxySwitchCompatible: false }
}

export async function findRunningPids(
  executablePath: string,
): Promise<number[]> {
  try {
    const { stdout } = await execFileAsync(
      '/bin/ps',
      ['-axo', 'pid=,command='],
      {
        encoding: 'utf8',
        timeout: 5_000,
        maxBuffer: 5_000_000,
      },
    )

    return stdout
      .split('\n')
      .map((line) => line.trim().match(/^(\d+)\s+(.+)$/u))
      .filter((match): match is RegExpMatchArray => match !== null)
      .filter((match) => {
        const command = match[2]
        return (
          command === executablePath ||
          command?.startsWith(`${executablePath} `) === true
        )
      })
      .map((match) => Number(match[1]))
      .filter((pid) => Number.isInteger(pid) && pid > 0)
  } catch {
    return []
  }
}

async function isDirectory(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isDirectory()
  } catch {
    return false
  }
}

async function binaryContainsProxySwitch(path: string): Promise<boolean> {
  try {
    await execFileAsync(
      '/usr/bin/grep',
      ['-a', '-m', '1', '-q', 'proxy-server', path],
      {
        timeout: 5_000,
        maxBuffer: 1_000,
      },
    )
    return true
  } catch {
    return false
  }
}

function invalidApplication(
  appPath: string,
  warning: string,
): CodexAppInfo {
  return {
    installed: true,
    appPath,
    isElectron: false,
    runtimeType: 'unknown',
    proxySwitchCompatible: false,
    isRunning: false,
    pidList: [],
    warning,
  }
}

function readableInspectionError(error: unknown): string {
  if (error instanceof SyntaxError) {
    return 'Info.plist 无法读取或内容已损坏。'
  }
  if (error instanceof Error) {
    if (/Info\.plist|ENOENT/iu.test(error.message)) {
      return 'Info.plist 无法读取，或 Codex 可执行文件不存在。'
    }
    if (/EACCES/iu.test(error.message)) {
      return '没有权限读取 Codex 应用或执行文件。'
    }
  }
  return 'Codex 应用结构无法识别，应用可能已损坏。'
}
