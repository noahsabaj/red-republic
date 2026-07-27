import path from "path"
import { defineConfig } from "vitest/config"

// Standalone config: tests are pure TS (node env) and must not load the
// dev-only plugins from vite.config.ts.
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/__tests__/**/*.test.ts"],
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
});
