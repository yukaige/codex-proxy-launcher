import { invoke } from '@tauri-apps/api/core'
import type {
  ActionResult,
  CodexAppInfo,
  CodexProxyApi,
  CodexProxyConfig,
  CodexStatus,
  LaunchResult,
  ProxyTestResult,
  TrafficVerificationResult,
} from '../shared/types'

function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(command, args)
}

export const codexProxy: CodexProxyApi = Object.freeze({
  getConfig: () => call<CodexProxyConfig>('get_config'),
  saveConfig: (config: CodexProxyConfig) =>
    call<CodexProxyConfig>('save_config', { config }),
  detectCodex: () => call<CodexAppInfo>('detect_codex'),
  chooseCodex: () => call<CodexAppInfo>('choose_codex'),
  testProxy: (config: CodexProxyConfig) =>
    call<ProxyTestResult>('test_proxy', { config }),
  copyLaunchScript: (config: CodexProxyConfig) =>
    call<ActionResult>('copy_launch_script', { config }),
  launchCodex: (config: CodexProxyConfig) =>
    call<LaunchResult>('launch_codex', { config }),
  launchCodexDirectly: () =>
    call<LaunchResult>('launch_codex_directly'),
  verifyProxyTraffic: (config: CodexProxyConfig) =>
    call<TrafficVerificationResult>('verify_proxy_traffic', { config }),
  stopCodex: () => call<ActionResult>('stop_codex'),
  getCodexStatus: () => call<CodexStatus>('get_codex_status'),
  openLogs: () => call<ActionResult>('open_logs'),
})
