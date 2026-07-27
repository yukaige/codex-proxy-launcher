import { app, BrowserWindow } from 'electron'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { CodexDetector } from './codex-detector'
import { registerIpcHandlers } from './ipc'
import { CodexLauncher } from './launcher'
import { AppLogger } from './logger'
import { ProxyTester } from './proxy-tester'
import { SettingsStore } from './store'
import { TrafficVerifier } from './traffic-verifier'

app.setName('Codex 代理启动器')

function createWindow(): BrowserWindow {
  const window = new BrowserWindow({
    width: 1120,
    height: 820,
    minWidth: 640,
    minHeight: 680,
    title: 'Codex 代理启动器',
    titleBarStyle: 'hiddenInset',
    backgroundColor: '#f4f5f0',
    show: false,
    webPreferences: {
      preload: join(__dirname, '..', 'preload-bundle', 'index.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  })

  window.once('ready-to-show', () => window.show())
  const developmentUrl = process.env.VITE_DEV_SERVER_URL
  if (developmentUrl) {
    void window.loadURL(developmentUrl)
  } else {
    void window.loadFile(join(__dirname, '..', '..', 'dist', 'index.html'))
  }
  return window
}

void app.whenReady().then(() => {
  const logger = new AppLogger(homedir())
  const proxyTester = new ProxyTester(logger)
  const trafficVerifier = new TrafficVerifier(logger)
  const detector = new CodexDetector(logger)
  const launcher = new CodexLauncher(logger, proxyTester, trafficVerifier)
  const store = new SettingsStore(app.getPath('userData'))

  registerIpcHandlers({ detector, launcher, logger, proxyTester, store })
  createWindow()
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow()
    }
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
