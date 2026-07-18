# Red Republic — Planned Economy Builder

Isometric city-builder (React 19 + Vite 7 + TypeScript, canvas renderer). Package manager is **bun** (`bun.lock` is the lockfile — never commit a `package-lock.json`).

## Commands

- `bun run dev` — dev server on port 3000 (`?demo` seeds a developed town, `?seed=N` reproduces a specific map)
- `bun run check` — the full gate: `tsc -b && eslint . && vitest run`. Run before committing.
- `bun run test` — Vitest only. **Never `bun test`** — Bun's native runner grabs `*.test.ts` with incompatible semantics.
- `bun run build` — production build (tsc + vite)

## Architecture

- `src/game/engine.ts` — `GameEngine`: single mutable sim class, fixed-timestep day loop (`advance(dtMs)`, 500 ms = 1 day at 1× speed). All randomness flows through the seeded RNG created from `engine.seed` (deterministic per seed — keep it that way; no bare `Math.random()` in sim code).
- `src/game/config.ts` — all game data/balance (buildings, resources, prices, objectives). Data lives here, behavior in the engine.
- `src/game/weather.ts` — `WeatherTimeline`: deterministic climate per seed (temperature + Markov conditions + snow depth + river freeze), generated sequentially and memoized so `engine.weather` and the exact `forecast()` share one source.
- `src/game/pathfind.ts` — multi-source BFS floods over the road network on reusable generation-stamped scratch buffers. A `FloodResult` is a view valid only until the next flood; `snapshot()` before caching.
- `src/game/input.ts` — DOM-free pointer/keyboard gesture state machine (pan/paint/pinch/select). `GameCanvas.tsx` is a thin Pointer-Events adapter around it.
- `src/game/render.ts` — isometric canvas renderer. Draw order is the row scan: anything world-positioned (buildings, trucks, citizens, placement ghost) must draw at its row inside the scan, never as a post-scan overlay, or it will float over buildings that should occlude it.
- React bridge: the engine `bump()`s a version counter; components subscribe via the hooks in `src/hooks/use-engine.ts` (signature selectors for cheap components, full-version for detail panels). `App.tsx` itself does not subscribe globally.
- `src/components/ui/` and `src/hooks/use-mobile.ts` are vendored shadcn/ui — excluded from linting, don't hand-edit.

## Rules

- **UI never re-computes simulation math.** Panels display engine APIs — `productionRates()`, `importPriceOf()`, `sellableStock()` — and mutate via engine methods (`toggleStaffPriority()`), never by writing engine fields. This is what keeps display and simulation from diverging.
- **One owner for persistent UI surfaces.** Every right-side view is a `PanelMode` owned by `App` (exclusive by construction). Never give a component its own open/close state for a persistent surface — that's how the stockpiles popover ended up overlaying the Five-Year Plan.
- **No emoji; icons come from `src/ui/icons.ts`.** One Lucide-backed registry renders as SVG (`<GameIcon>`) and on canvas (`drawIcon`). config/engine reference icons by name string; `ui-guards.test.ts` fails the build on unknown names, emoji in first-party sources, or invalid Tailwind color-opacity steps (e.g. `bg-red-950/97` silently emits no CSS).
- **Weather is engine state.** `engine.weather` / `engine.forecast()` are the only sources of truth — never derive weather from `season()` or the calendar in UI or render code (heating is temperature-driven via `heatingRequired()`, river ice follows `weather.riverFrozen`). Gameplay multipliers live in config's `WEATHER` table; the timeline draws from its own decorrelated RNG stream so it never perturbs economy randomness.
- Every bug fix lands with a regression test in `src/game/__tests__/`. Engine tests build worlds with `helpers.ts` (`makeEngine()` = flat map, no starting base, **calm weather** — pass `weather:` to script conditions; real-timeline behavior needs `new GameEngine` directly). Remember `pop` clamps to housing capacity at day end — place beds before setting `pop`.
- Dev builds expose `window.__redRepublic = { engine, cam }` for console debugging and automated verification.
