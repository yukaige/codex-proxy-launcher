export type ProxyProtocol = 'socks5' | 'http'

export interface CodexProxyConfig {
  enabled: boolean
  protocol: ProxyProtocol
  host: string
  port: number
  bypassList: string
  closeExistingInstance: boolean
  enableDebugLog: boolean
}

export type CodexRuntimeType = 'electron' | 'codex_chromium' | 'unknown'
export type AppPlatform = 'macos' | 'windows'

export interface CodexAppInfo {
  platform?: AppPlatform
  installed: boolean
  appPath?: string
  executablePath?: string
  bundleId?: string
  version?: string
  isElectron: boolean
  runtimeType: CodexRuntimeType
  proxySwitchCompatible: boolean
  isRunning: boolean
  pidList: number[]
  warning?: string
}

export type ProxyTestStage = 'validation' | 'tcp' | 'handshake' | 'https'

export interface ProxyTestResult {
  success: boolean
  latency?: number
  message: string
  proxyAddress: string
  stage: ProxyTestStage
}

export type ProxyLaunchStatus =
  | 'not_started'
  | 'app_not_found'
  | 'proxy_unreachable'
  | 'launch_failed'
  | 'launched_with_proxy_args'
  | 'proxy_unverified'
  | 'proxy_verified'

export interface LaunchResult {
  success: boolean
  status: ProxyLaunchStatus
  pid?: number
  message: string
  executablePath?: string
  args?: string[]
  proxyArgsPassed: boolean
  trafficVerified: boolean
}

export interface ActionResult {
  success: boolean
  message: string
}

export interface CodexStatus {
  isRunning: boolean
  pidList: number[]
  launchStatus: ProxyLaunchStatus
  message: string
}

export interface TrafficVerificationResult {
  verified: boolean
  message: string
  evidence: string[]
  netLogPath: string
}

export interface CodexProxyApi {
  getConfig(): Promise<CodexProxyConfig>
  saveConfig(config: CodexProxyConfig): Promise<CodexProxyConfig>
  detectCodex(): Promise<CodexAppInfo>
  chooseCodex(): Promise<CodexAppInfo>
  testProxy(config: CodexProxyConfig): Promise<ProxyTestResult>
  copyLaunchScript(config: CodexProxyConfig): Promise<ActionResult>
  launchCodex(config: CodexProxyConfig): Promise<LaunchResult>
  launchCodexDirectly(): Promise<LaunchResult>
  verifyProxyTraffic(config: CodexProxyConfig): Promise<TrafficVerificationResult>
  stopCodex(): Promise<ActionResult>
  getCodexStatus(): Promise<CodexStatus>
  openLogs(): Promise<ActionResult>
}

export const defaultConfig: Readonly<CodexProxyConfig> = Object.freeze({
  enabled: true,
  protocol: 'socks5',
  host: '127.0.0.1',
  port: 7890,
  bypassList: 'localhost;127.0.0.1;<-loopback>',
  closeExistingInstance: true,
  enableDebugLog: true,
})
