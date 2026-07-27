export const IPC_CHANNELS = Object.freeze({
  configGet: 'codex-proxy:config:get',
  configSave: 'codex-proxy:config:save',
  codexDetect: 'codex-proxy:codex:detect',
  codexChoose: 'codex-proxy:codex:choose',
  proxyTest: 'codex-proxy:proxy:test',
  codexLaunchProxy: 'codex-proxy:codex:launch-proxy',
  codexLaunchDirect: 'codex-proxy:codex:launch-direct',
  proxyVerifyTraffic: 'codex-proxy:proxy:verify-traffic',
  codexStop: 'codex-proxy:codex:stop',
  codexStatus: 'codex-proxy:codex:status',
  logsOpen: 'codex-proxy:logs:open',
})
