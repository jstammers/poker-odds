import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import wasm from 'vite-plugin-wasm'
import topLevelAwait from 'vite-plugin-top-level-await'
import path from 'path'

export default defineConfig({
  // GitHub Pages serves project repos at /repo-name/.
  // VITE_BASE_URL is injected by the CI workflow; defaults to '/' for local dev.
  base: process.env.VITE_BASE_URL ?? '/',

  plugins: [
    react(),
    wasm(),
    topLevelAwait(),
  ],
  resolve: {
    alias: {
      // After running `npm run wasm`, the wasm-pack output lands in web/wasm/
      'poker-odds-wasm': path.resolve(__dirname, './wasm/poker_odds.js'),
    },
  },
  worker: {
    format: 'es',
    plugins: () => [wasm(), topLevelAwait()],
  },
  build: {
    target: 'esnext',
  },
  optimizeDeps: {
    exclude: ['poker-odds-wasm'],
  },
})
