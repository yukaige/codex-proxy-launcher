/// <reference types="vite/client" />

import type { CodexProxyApi } from './shared/types'

declare global {
  interface Window {
    codexProxy: CodexProxyApi
  }
}

export {}
