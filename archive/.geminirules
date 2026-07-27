# Package Manager & Command Execution Rules

- **Use Bun**: Always use `bun` instead of `npm` or `yarn` for command execution in this codebase.
  - Test runner: `bun run test` (runs Vitest). **Never use `bun test`** (Bun's native runner has incompatible semantics with Vitest).
  - Dev server: `bun run dev` (starts Vite dev server on port 3000).
  - Full gate verification: `bun run check` (`tsc -b && eslint . && vitest run`).
  - Package installation: `bun install` / `bun add`.
