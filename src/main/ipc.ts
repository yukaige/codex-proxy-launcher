import { dialog, ipcMain, shell } from 'electron'
import { IPC_CHANNELS } from '../shared/ipc'
import type { CodexAppInfo } from '../shared/types'
import type { CodexDetector } from './codex-detector'
import type { CodexLauncher } from './launcher'
import type { AppLogger } from './logger'
import type { ProxyTester } from './proxy-tester'
import type { SettingsStore } from './store'

interface IpcServices {
  detector: CodexDetector
  launcher: CodexLauncher
  logger: AppLogger
  proxyTester: ProxyTester
  store: SettingsStore
}

export function registerIpcHandlers(services: IpcServices): void {
  let latestInfo: CodexAppInfo | undefined
  const detect = async (): Promise<CodexAppInfo> => {
    const selectedPath = await services.store.getSelectedAppPath()
    latestInfo = await services.detector.detect(selectedPath)
    return latestInfo
  }

  ipcMain.handle(IPC_CHANNELS.configGet, async () =>
    services.store.getConfig(),
  )
  ipcMain.handle(IPC_CHANNELS.configSave, async (_event, value: unknown) =>
    services.store.saveConfig(value),
  )
  ipcMain.handle(IPC_CHANNELS.codexDetect, detect)
  ipcMain.handle(IPC_CHANNELS.codexChoose, async () => {
    const result = await dialog.showOpenDialog({
      title: '选择 Codex.app',
      defaultPath: '/Applications',
      properties: ['openFile'],
      filters: [{ name: 'macOS 应用', extensions: ['app'] }],
    })
    const selectedPath = result.filePaths[0]
    if (result.canceled || !selectedPath) {
      return latestInfo ?? detect()
    }

    const inspected = await services.detector.inspect(selectedPath)
    if (inspected.executablePath) {
      await services.store.saveSelectedAppPath(selectedPath)
    }
    latestInfo = inspected
    return inspected
  })
  ipcMain.handle(IPC_CHANNELS.proxyTest, async (_event, value: unknown) =>
    services.proxyTester.test(value),
  )
  ipcMain.handle(
    IPC_CHANNELS.codexLaunchProxy,
    async (_event, value: unknown) => {
      const config = await services.store.saveConfig(value)
      const info = await detect()
      return services.launcher.launchWithProxy(info, config)
    },
  )
  ipcMain.handle(IPC_CHANNELS.codexLaunchDirect, async () => {
    const info = await detect()
    return services.launcher.launchDirectly(info)
  })
  ipcMain.handle(
    IPC_CHANNELS.proxyVerifyTraffic,
    async (_event, value: unknown) => {
      const config = await services.store.saveConfig(value)
      return services.launcher.verifyProxyTraffic(config)
    },
  )
  ipcMain.handle(IPC_CHANNELS.codexStop, async () => {
    const info = await detect()
    return services.launcher.stop(info)
  })
  ipcMain.handle(IPC_CHANNELS.codexStatus, async () => {
    const info = await detect()
    return services.launcher.getStatus(info)
  })
  ipcMain.handle(IPC_CHANNELS.logsOpen, async () => {
    await services.logger.ensureDirectory()
    const error = await shell.openPath(services.logger.directory)
    return error
      ? { success: false, message: `无法打开日志目录：${error}` }
      : { success: true, message: '已打开日志目录。' }
  })
}
