import path from "path"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"
import { inspectAttr } from 'kimi-plugin-inspect-react'
import pkg from './package.json'

// https://vite.dev/config/
export default defineConfig({
  base: './',
  plugins: [inspectAttr(), react()],
  define: {
    // single source of truth: package.json (tauri.conf.json reads it too)
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  server: {
    port: 3000,
    // Tauri's devUrl points at :3000 — fail fast instead of drifting to 3001
    strictPort: true,
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
});
