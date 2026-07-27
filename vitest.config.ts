import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    coverage: {
      include: ['src/main/**/*.ts', 'src/shared/**/*.ts'],
    },
  },
})
