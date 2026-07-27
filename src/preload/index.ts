import { contextBridge, ipcRenderer } from 'electron'
import { IPC_CHANNELS } from '../shared/ipc'
import type { CodexProxyApi, CodexProxyConfig } from '../shared/types'

const api: CodexProxyApi = Object.freeze({
  getConfig: () => ipcRenderer.invoke(IPC_CHANNELS.configGet),
  saveConfig: (config: CodexProxyConfig) =>
    ipcRenderer.invoke(IPC_CHANNELS.configSave, config),
  detectCodex: () => ipcRenderer.invoke(IPC_CHANNELS.codexDetect),
  chooseCodex: () => ipcRenderer.invoke(IPC_CHANNELS.codexChoose),
  testProxy: (config: CodexProxyConfig) =>
    ipcRenderer.invoke(IPC_CHANNELS.proxyTest, config),
  launchCodex: (config: CodexProxyConfig) =>
    ipcRenderer.invoke(IPC_CHANNELS.codexLaunchProxy, config),
  launchCodexDirectly: () =>
    ipcRenderer.invoke(IPC_CHANNELS.codexLaunchDirect),
  verifyProxyTraffic: (config: CodexProxyConfig) =>
    ipcRenderer.invoke(IPC_CHANNELS.proxyVerifyTraffic, config),
  stopCodex: () => ipcRenderer.invoke(IPC_CHANNELS.codexStop),
  getCodexStatus: () => ipcRenderer.invoke(IPC_CHANNELS.codexStatus),
  openLogs: () => ipcRenderer.invoke(IPC_CHANNELS.logsOpen),
})

contextBridge.exposeInMainWorld('codexProxy', api)
