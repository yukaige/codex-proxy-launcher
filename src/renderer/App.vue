<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import {
  defaultConfig,
  type CodexAppInfo,
  type CodexStatus,
  type LaunchResult,
  type ProxyTestResult,
  type TrafficVerificationResult,
} from '../shared/types'

type Operation =
  | 'initial'
  | 'detect'
  | 'choose'
  | 'test'
  | 'launch'
  | 'direct'
  | 'verify'
  | 'stop'
  | 'logs'

const config = reactive({ ...defaultConfig })
const appInfo = ref<CodexAppInfo>({
  installed: false,
  isElectron: false,
  runtimeType: 'unknown',
  proxySwitchCompatible: false,
  isRunning: false,
  pidList: [],
})
const status = ref<CodexStatus>({
  isRunning: false,
  pidList: [],
  launchStatus: 'not_started',
  message: '尚未启动。',
})
const proxyTest = ref<ProxyTestResult>()
const launchResult = ref<LaunchResult>()
const trafficResult = ref<TrafficVerificationResult>()
const operation = ref<Operation>()
const notice = ref('')
const initialized = ref(false)
let saveTimer: ReturnType<typeof setTimeout> | undefined

const proxyAddress = computed(
  () =>
    `${config.protocol}://${config.host || '—'}:${
      Number.isFinite(config.port) ? config.port : '—'
    }`,
)
const appServerProxyAddress = computed(() =>
  config.protocol === 'socks5'
    ? proxyAddress.value.replace(/^socks5:/u, 'socks5h:')
    : proxyAddress.value,
)
const canLaunch = computed(
  () => appInfo.value.installed && Boolean(appInfo.value.executablePath),
)
const configValid = computed(
  () =>
    config.host.trim().length > 0 &&
    Number.isInteger(config.port) &&
    config.port >= 1 &&
    config.port <= 65_535,
)
const trafficState = computed(() =>
  launchResult.value?.trafficVerified
    ? 'verified'
    : launchResult.value?.proxyArgsPassed
      ? 'pending'
      : 'idle',
)
const runtimeLabel = computed(() => {
  if (appInfo.value.runtimeType === 'electron') return 'Electron'
  if (appInfo.value.runtimeType === 'codex_chromium') {
    return 'Codex Chromium'
  }
  return '未识别'
})

watch(
  config,
  () => {
    if (!initialized.value) return
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(() => {
      window.codexProxy.saveConfig({ ...config }).catch((error: unknown) => {
        notice.value = readableError(error)
      })
    }, 350)
  },
  { deep: true },
)

onMounted(async () => {
  operation.value = 'initial'
  try {
    const [storedConfig, detected] = await Promise.all([
      window.codexProxy.getConfig(),
      window.codexProxy.detectCodex(),
    ])
    Object.assign(config, storedConfig)
    appInfo.value = detected
    status.value = await window.codexProxy.getCodexStatus()
  } catch (error) {
    notice.value = readableError(error)
  } finally {
    initialized.value = true
    operation.value = undefined
  }
})

async function detectCodex(): Promise<void> {
  await run('detect', async () => {
    appInfo.value = await window.codexProxy.detectCodex()
    status.value = await window.codexProxy.getCodexStatus()
    notice.value = appInfo.value.installed
      ? '已完成 Codex 检测。'
      : '没有在默认位置找到 Codex。'
  })
}

async function chooseCodex(): Promise<void> {
  await run('choose', async () => {
    appInfo.value = await window.codexProxy.chooseCodex()
    notice.value = appInfo.value.executablePath
      ? '已选择 Codex 应用。'
      : (appInfo.value.warning ?? '')
  })
}

async function testProxy(): Promise<void> {
  await run('test', async () => {
    proxyTest.value = await window.codexProxy.testProxy({ ...config })
    notice.value = proxyTest.value.message
  })
}

async function launchCodex(): Promise<void> {
  await run('launch', async () => {
    launchResult.value = await window.codexProxy.launchCodex({ ...config })
    status.value = await window.codexProxy.getCodexStatus()
    appInfo.value = await window.codexProxy.detectCodex()
    notice.value = launchResult.value.message
  })
}

async function launchDirectly(): Promise<void> {
  await run('direct', async () => {
    launchResult.value = await window.codexProxy.launchCodexDirectly()
    status.value = await window.codexProxy.getCodexStatus()
    appInfo.value = await window.codexProxy.detectCodex()
    notice.value = launchResult.value.message
  })
}

async function verifyTraffic(): Promise<void> {
  await run('verify', async () => {
    trafficResult.value = await window.codexProxy.verifyProxyTraffic({
      ...config,
    })
    if (launchResult.value && trafficResult.value.verified) {
      launchResult.value = {
        ...launchResult.value,
        status: 'proxy_verified',
        trafficVerified: true,
      }
    }
    status.value = await window.codexProxy.getCodexStatus()
    notice.value = trafficResult.value.message
  })
}

async function stopCodex(): Promise<void> {
  await run('stop', async () => {
    const result = await window.codexProxy.stopCodex()
    appInfo.value = await window.codexProxy.detectCodex()
    status.value = await window.codexProxy.getCodexStatus()
    notice.value = result.message
  })
}

async function openLogs(): Promise<void> {
  await run('logs', async () => {
    const result = await window.codexProxy.openLogs()
    notice.value = result.message
  })
}

async function run(
  name: Operation,
  action: () => Promise<void>,
): Promise<void> {
  operation.value = name
  notice.value = ''
  try {
    await action()
  } catch (error) {
    notice.value = readableError(error)
  } finally {
    operation.value = undefined
  }
}

function readableError(error: unknown): string {
  return error instanceof Error
    ? error.message
    : '操作失败，请打开日志目录查看详情。'
}
</script>

<template>
  <div
    class="titlebar"
    aria-hidden="true"
  >
    <span>Codex 代理启动器</span>
  </div>
  <main class="shell">
    <header class="hero">
      <div>
        <p class="eyebrow">CODEX FOR macOS</p>
        <h1>代理启动器</h1>
        <p class="lede">
          同时配置 Chromium 与 Codex app-server 代理，并把“已传入”和“已验证流量”分开呈现。
        </p>
      </div>
      <div
        class="flow"
        aria-label="网络路径"
      >
        <span>启动器</span><i>→</i><span>Codex</span><i>→</i>
        <span>Clash</span><i>→</i><span>远程节点</span>
      </div>
    </header>

    <div
      v-if="notice"
      class="notice"
      role="status"
    >
      {{ notice }}
    </div>

    <section class="grid">
      <article class="card app-card">
        <div class="card-heading">
          <div>
            <p class="section-index">01</p>
            <h2>Codex 应用</h2>
          </div>
          <span :class="['pill', appInfo.installed ? 'ok' : 'warn']">
            {{ appInfo.installed ? '已安装' : '未找到' }}
          </span>
        </div>

        <dl class="facts">
          <div class="wide">
            <dt>应用路径</dt>
            <dd :title="appInfo.appPath">
              {{ appInfo.appPath || '未检测到' }}
            </dd>
          </div>
          <div>
            <dt>运行状态</dt>
            <dd>
              <span
                :class="['status-dot', { active: appInfo.isRunning }]"
              />
              {{
                appInfo.isRunning
                  ? `运行中 · PID ${appInfo.pidList.join(', ')}`
                  : '未运行'
              }}
            </dd>
          </div>
          <div>
            <dt>应用类型</dt>
            <dd>{{ runtimeLabel }}</dd>
          </div>
          <div>
            <dt>版本</dt>
            <dd>{{ appInfo.version || '—' }}</dd>
          </div>
          <div>
            <dt>Bundle ID</dt>
            <dd>{{ appInfo.bundleId || '—' }}</dd>
          </div>
          <div>
            <dt>代理参数支持</dt>
            <dd>
              {{
                appInfo.proxySwitchCompatible ? '检测到兼容开关' : '尚未确认'
              }}
            </dd>
          </div>
          <div class="wide">
            <dt>可执行文件</dt>
            <dd :title="appInfo.executablePath">
              {{ appInfo.executablePath || '—' }}
            </dd>
          </div>
        </dl>

        <p
          v-if="appInfo.warning"
          class="inline-warning"
        >
          {{ appInfo.warning }}
        </p>

        <div class="button-row">
          <button
            class="secondary"
            :disabled="Boolean(operation)"
            @click="detectCodex"
          >
            {{ operation === 'detect' ? '检测中…' : '自动检测' }}
          </button>
          <button
            class="ghost"
            :disabled="Boolean(operation)"
            @click="chooseCodex"
          >
            手动选择应用
          </button>
        </div>
      </article>

      <article class="card proxy-card">
        <div class="card-heading">
          <div>
            <p class="section-index">02</p>
            <h2>代理配置</h2>
          </div>
          <label class="switch">
            <input
              v-model="config.enabled"
              type="checkbox"
            />
            <span />
            <b>{{ config.enabled ? '已启用' : '已关闭' }}</b>
          </label>
        </div>

        <div :class="['form-grid', { muted: !config.enabled }]">
          <label>
            <span>协议</span>
            <select
              v-model="config.protocol"
              :disabled="!config.enabled"
            >
              <option value="socks5">SOCKS5</option>
              <option value="http">HTTP</option>
            </select>
          </label>
          <label class="host-field">
            <span>主机</span>
            <input
              v-model.trim="config.host"
              :disabled="!config.enabled"
              autocomplete="off"
            />
          </label>
          <label>
            <span>端口</span>
            <input
              v-model.number="config.port"
              :disabled="!config.enabled"
              type="number"
              min="1"
              max="65535"
            />
          </label>
          <label class="full">
            <span>绕过地址</span>
            <input
              v-model="config.bypassList"
              :disabled="!config.enabled"
              autocomplete="off"
            />
          </label>
        </div>

        <div class="toggles">
          <label>
            <input
              v-model="config.closeExistingInstance"
              type="checkbox"
            />
            启动前退出已有 Codex
          </label>
          <label>
            <input
              v-model="config.enableDebugLog"
              type="checkbox"
            />
            调试日志与 Chromium net-log
          </label>
        </div>

        <div class="address-preview">
          <span>将使用</span>
          <code>--proxy-server={{ proxyAddress }}</code>
          <code>app-server: HTTP(S)_PROXY={{ appServerProxyAddress }}</code>
        </div>
        <div class="button-row">
          <button
            class="secondary"
            :disabled="
              Boolean(operation) || !config.enabled || !configValid
            "
            @click="testProxy"
          >
            {{ operation === 'test' ? '测试中…' : '测试代理' }}
          </button>
          <button
            class="ghost"
            :disabled="Boolean(operation)"
            @click="openLogs"
          >
            打开日志目录
          </button>
        </div>
      </article>
    </section>

    <section class="card launch-card">
      <div class="card-heading">
        <div>
          <p class="section-index">03</p>
          <h2>启动与验证</h2>
        </div>
        <span :class="['pill', appInfo.isRunning ? 'ok' : 'neutral']">
          {{ appInfo.isRunning ? 'Codex 运行中' : '等待启动' }}
        </span>
      </div>

      <div class="verification">
        <div :class="{ done: proxyTest?.success }">
          <span>1</span>
          <strong>代理可达</strong>
          <small>
            {{
              proxyTest
                ? proxyTest.message
                : '等待端口、握手和 HTTPS 测试'
            }}
          </small>
        </div>
        <div :class="{ done: launchResult?.proxyArgsPassed }">
          <span>2</span>
          <strong>参数已传入</strong>
          <small>
            {{
              launchResult?.proxyArgsPassed
                ? 'Chromium 与 app-server 均已配置'
                : '尚未代理启动'
            }}
          </small>
        </div>
        <div
          :class="{
            done: trafficState === 'verified',
            pending: trafficState === 'pending',
          }"
        >
          <span>3</span>
          <strong>实际流量</strong>
          <small>
            {{
              trafficState === 'verified'
                ? '已验证经过代理'
                : trafficState === 'pending'
                  ? '需在 Clash 连接日志中确认'
                  : '尚未验证'
            }}
          </small>
        </div>
      </div>

      <div
        v-if="launchResult?.args?.length"
        class="args-panel"
      >
        <span>本次启动参数</span>
        <code
          v-for="argument in launchResult.args"
          :key="argument"
        >
          {{ argument }}
        </code>
      </div>

      <div class="launch-actions">
        <button
          class="primary"
          :disabled="
            Boolean(operation) ||
            !canLaunch ||
            !config.enabled ||
            !configValid
          "
          @click="launchCodex"
        >
          {{
            operation === 'launch' ? '正在验证并启动…' : '代理启动 Codex'
          }}
        </button>
        <button
          class="secondary"
          :disabled="Boolean(operation) || !canLaunch"
          @click="launchDirectly"
        >
          普通启动
        </button>
        <button
          class="ghost"
          :disabled="Boolean(operation) || !launchResult?.proxyArgsPassed"
          @click="verifyTraffic"
        >
          {{
            operation === 'verify' ? '正在读取 net-log…' : '验证实际流量'
          }}
        </button>
        <button
          class="danger"
          :disabled="Boolean(operation) || !appInfo.isRunning"
          @click="stopCodex"
        >
          退出 Codex
        </button>
      </div>

      <div class="manual-check">
        <strong>确认实际流量</strong>
        <ol>
          <li>在 Clash Verge 打开连接日志并清理旧连接。</li>
          <li>点击“代理启动 Codex”，然后在 Codex 中发起一个新请求。</li>
          <li>
            确认 Clash 中出现 Codex 使用的 OpenAI / ChatGPT
            连接；也可检查日志目录中的
            <code>codex-net-log.json</code>。
          </li>
        </ol>
        <p>
          在完成以上检查前，状态只会是“代理参数已传入，流量未验证”。
        </p>
        <p
          v-if="trafficResult?.evidence.length"
          class="evidence"
        >
          自动证据：{{ trafficResult.evidence.join('；') }}
        </p>
      </div>
    </section>

    <footer>
      <span>状态：{{ status.message }}</span>
      <span>不修改 Codex.app · 不注入 · 不安装证书</span>
    </footer>
  </main>
</template>
