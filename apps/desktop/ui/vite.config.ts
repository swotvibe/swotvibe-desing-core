import { fileURLToPath, URL } from 'node:url'

import tailwindcss from '@tailwindcss/vite'
import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vite'

// A desktop shell serves the built interface from a local origin, so the dev
// server only needs to exist for development. The test configuration is in
// `vite.config.ts` rather than a second file so both use one resolver.
export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    // Bound to IPv4 loopback explicitly: the default resolves to `::1` on some
    // Windows configurations, which a WebView or a local tool may not reach.
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
  },
  build: {
    // The interface ships inside a desktop bundle, so source maps are useful
    // for diagnosing a packaged build rather than for serving the web.
    sourcemap: true,
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
    globals: false,
  },
})
