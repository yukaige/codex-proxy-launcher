import { execFile } from 'node:child_process'
import { join } from 'node:path'
import { promisify } from 'node:util'
import type {
  ActionResult,
  CodexAppInfo,
  CodexProxyConfig,
  CodexStatus,
  LaunchResult,
  ProxyLaunchStatus,
  TrafficVerificationResult,
} from '../shared/types'
import {
  buildLaunchArguments,
  buildProxyEnvironment,
} from './core'
import { findRunningPids } from './codex-detector'
import type { AppLogger } from './logger'
import type { ProxyTester } from './proxy-tester'
import type { TrafficVerifier } from './traffic-verifier'

const execFileAsync = promisify(execFile)

export function buildOpenArguments(
  appPath: string,
  launchArguments: readonly string[],
  environment: Readonly<Record<string, string>> = {},
): string[] {
  return [
    '-n',
    ...Object.entries(environment).flatMap(([name, value]) => [
      '--env',
      `${name}=${value}`,
    ]),
    '-a',
    appPath,
    ...(launchArguments.length > 0
      ? ['--args', ...launchArguments]
      : []),
  ]
}

export class CodexLauncher {
  private launchStatus: ProxyLaunchStatus = 'not_started'
  private statusMessage = '尚未启动。'
  private lastProxyLaunchStartedAt?: Date

  constructor(
    private readonly logger: AppLogger,
    private readonly proxyTester: ProxyTester,
    private readonly trafficVerifier: TrafficVerifier,
  ) {}

  async launchWithProxy(
    info: CodexAppInfo,
    config: CodexProxyConfig,
  ): Promise<LaunchResult> {
    const invalid = validateLaunchTarget(info)
    if (invalid) {
      this.setStatus('app_not_found', invalid)
      return failure('app_not_found', invalid)
    }
    if (!config.enabled) {
      return this.launchDirectly(info)
    }

    const proxyTest = await this.proxyTester.test(config)
    if (!proxyTest.success) {
      const message = `代理不可用，未启动 Codex：${proxyTest.message}`
      this.setStatus('proxy_unreachable', message)
      return failure('proxy_unreachable', message, info.executablePath)
    }

    if (config.closeExistingInstance && info.isRunning) {
      const stopped = await this.stop(info)
      if (!stopped.success) {
        this.setStatus('launch_failed', stopped.message)
        return failure('launch_failed', stopped.message, info.executablePath)
      }
    } else if (!config.closeExistingInstance && info.isRunning) {
      const message =
        'Codex 已在运行。macOS 可能只激活旧实例，新的代理参数无法可靠生效；请启用“启动前退出已有 Codex”。'
      this.setStatus('launch_failed', message)
      return failure('launch_failed', message, info.executablePath)
    }

    await this.logger.ensureDirectory()
    const netLogPath = join(this.logger.directory, 'codex-net-log.json')
    const args = buildLaunchArguments(config, netLogPath)
    const environment = buildProxyEnvironment(config)
    this.lastProxyLaunchStartedAt = new Date()
    const result = await this.launchViaMacOS(
      info,
      args,
      environment,
      true,
    )
    if (result.success) {
      this.setStatus(
        'proxy_unverified',
        'Codex 已使用 Chromium 代理参数和后台代理环境启动，但尚未确认网络请求确实经过代理。',
      )
      return {
        ...result,
        status: 'proxy_unverified',
        message: this.statusMessage,
        proxyArgsPassed: true,
        trafficVerified: false,
      }
    }
    return result
  }

  async verifyProxyTraffic(
    config: CodexProxyConfig,
  ): Promise<TrafficVerificationResult> {
    const result = await this.trafficVerifier.verify(
      config,
      this.lastProxyLaunchStartedAt,
    )
    if (result.verified) {
      this.setStatus('proxy_verified', result.message)
    } else if (this.launchStatus === 'proxy_verified') {
      this.setStatus('proxy_unverified', result.message)
    }
    return result
  }

  async launchDirectly(info: CodexAppInfo): Promise<LaunchResult> {
    const invalid = validateLaunchTarget(info)
    if (invalid) {
      return failure('app_not_found', invalid)
    }
    const result = await this.launchViaMacOS(info, [], {}, false)
    if (result.success) {
      this.setStatus(
        'not_started',
        'Codex 已普通启动；本次没有传入代理参数。',
      )
    }
    return result
  }

  async stop(info: CodexAppInfo): Promise<ActionResult> {
    if (!info.executablePath) {
      return {
        success: false,
        message: '没有可用于识别 Codex 进程的可执行文件路径。',
      }
    }

    const pids = await findRunningPids(info.executablePath)
    if (pids.length === 0) {
      return { success: true, message: 'Codex 当前未运行。' }
    }

    await this.logger.log(
      'INFO',
      '正在通过进程信号请求 Codex 正常退出。',
      { pids },
    )
    for (const pid of pids) {
      try {
        process.kill(pid, 'SIGTERM')
      } catch (error) {
        await this.logger.log(
          'WARN',
          `无法向 Codex PID ${pid} 发送正常终止信号。`,
          error,
        )
      }
    }

    if (await waitUntilStopped(info.executablePath, 5_000)) {
      return { success: true, message: 'Codex 已正常退出。' }
    }

    const message =
      'Codex 无法正常退出。为避免误杀其他应用，已取消代理启动。'
    await this.logger.log('ERROR', message, { remainingPids: pids })
    return { success: false, message }
  }

  async getStatus(info: CodexAppInfo): Promise<CodexStatus> {
    const pidList = info.executablePath
      ? await findRunningPids(info.executablePath)
      : []
    return {
      isRunning: pidList.length > 0,
      pidList,
      launchStatus: this.launchStatus,
      message: this.statusMessage,
    }
  }

  private async launchViaMacOS(
    info: CodexAppInfo,
    args: string[],
    environment: Readonly<Record<string, string>>,
    hasProxyArguments: boolean,
  ): Promise<LaunchResult> {
    if (!info.appPath || !info.executablePath) {
      return failure(
        'app_not_found',
        'Codex 应用或可执行文件不存在。',
      )
    }

    await this.logger.log(
      'INFO',
      '准备通过 macOS 应用启动服务启动 Codex。',
      {
        executablePath: info.executablePath,
        args,
        proxyEnvironmentVariables: Object.keys(environment),
        electronDetected: info.isElectron,
        startedAt: new Date().toISOString(),
      },
    )

    const openArgs = buildOpenArguments(info.appPath, args, environment)
    try {
      // LaunchServices creates Codex as its own responsible application.
      // Do not directly execute the app binary: doing so makes macOS attribute
      // Codex/file-tool privacy requests to this launcher.
      await execFileAsync('/usr/bin/open', openArgs, { timeout: 5_000 })
      const pidList = await waitForRunningPids(info.executablePath, 5_000)
      const pid = pidList[0]
      if (!pid) {
        const message =
          'macOS 启动命令已执行，但没有检测到 Codex 进程。'
        this.setStatus('launch_failed', message)
        return failure(
          'launch_failed',
          message,
          info.executablePath,
          args,
        )
      }

      await this.logger.log(
        'INFO',
        '已通过 macOS 应用启动服务启动 Codex。',
        { openArgs, pid },
      )
      return {
        success: true,
        status: hasProxyArguments
          ? 'launched_with_proxy_args'
          : 'not_started',
        pid,
        message: hasProxyArguments
          ? 'Codex 已收到 Chromium 代理参数，后台 app-server 也已收到代理环境。'
          : 'Codex 已通过 macOS 应用启动服务普通启动。',
        executablePath: info.executablePath,
        args,
        proxyArgsPassed: hasProxyArguments,
        trafficVerified: false,
      }
    } catch (error) {
      const message = `Codex 启动失败：${
        error instanceof Error ? error.message : '未知错误'
      }`
      await this.logger.log('ERROR', message, error)
      this.setStatus('launch_failed', message)
      return failure(
        'launch_failed',
        message,
        info.executablePath,
        args,
      )
    }
  }

  private setStatus(status: ProxyLaunchStatus, message: string): void {
    this.launchStatus = status
    this.statusMessage = message
  }
}

function validateLaunchTarget(info: CodexAppInfo): string | undefined {
  if (!info.installed || !info.appPath) {
    return '没有找到 Codex.app，无法启动。'
  }
  if (!info.executablePath) {
    return info.warning ?? 'Codex 可执行文件不存在，应用可能已损坏。'
  }
  return undefined
}

function failure(
  status: ProxyLaunchStatus,
  message: string,
  executablePath?: string,
  args?: string[],
): LaunchResult {
  return {
    success: false,
    status,
    message,
    ...(executablePath ? { executablePath } : {}),
    ...(args ? { args } : {}),
    proxyArgsPassed: false,
    trafficVerified: false,
  }
}

async function waitUntilStopped(
  executablePath: string,
  timeoutMs: number,
): Promise<boolean> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    if ((await findRunningPids(executablePath)).length === 0) {
      return true
    }
    await new Promise((resolve) => setTimeout(resolve, 200))
  }
  return false
}

async function waitForRunningPids(
  executablePath: string,
  timeoutMs: number,
): Promise<number[]> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    const pidList = await findRunningPids(executablePath)
    if (pidList.length > 0) return pidList
    await new Promise((resolve) => setTimeout(resolve, 200))
  }
  return []
}
