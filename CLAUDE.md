# Red Republic

A planned-economy city builder. Rust, desktop-only. **The simulation is a library with no engine dependency**; the renderer and UI shell are a separate, later, evidence-based decision.

The 1.x build (TypeScript + React + canvas 2D) is **frozen at v1.10.0** and lives in `archive/` in this same repo. It is not a dependency and not a migration source — it is the **design and balance reference**, and the closest thing this project has to a written spec is its test suite (`archive/src/game/__tests__/`). Port rules, never code. Nothing in `archive/` gets fixed, built, or released again.

## Commands

- `cargo test` — the gate. Run before committing.
- `cargo clippy --all-targets -- -D warnings` — lint.
- `cargo fmt` — format.

`archive/` has its own toolchain (bun, vite, vitest) and is deliberately not wired into any of the above.

## Layout

- `crates/sim/` — `red-republic-sim`. All simulation. No renderer, no windowing, no engine. **This crate having zero engine dependencies is a rule, not an accident**: it is what keeps the shell decision open and reversible.
- `archive/` — the frozen 1.x build. Read it for rules and balance; never edit it.

## Rules

- **Determinism, at two different bars.** Same seed and same inputs produce the same world; a save reloads bit-exact. **Map generation must reproduce across machines** — a shared seed is a promise between players — so it may not depend on anything platform-varying (transcendental precision, hash iteration order, address-dependent behaviour). **The running simulation need only reproduce for the same binary**, which is the easy case: Rust does not reassociate floats, so one compiler on one instruction set suffices. Nothing may enable fast-math-style float relaxation.

- **The determinism tripwire comes before the thing it defends.** v1's `save-roundtrip.test.ts` — serialize, reload, advance both 90 days, compare everything — is what caught every field someone forgot to persist. The equivalent test exists here before parallel scheduling is introduced, not after. A system schedule that is parallel and unordered is non-deterministic by default; explicit ordering is the fix and it must be defended by a test that would fail without it.

- **Systems propose, one writer applies.** Carried forward from v1 because it is why blast radii stayed small: a system reads the world and returns what it wants to change; it never writes. There is deliberately no generic `{field, value}` escape hatch — that is "write anything" with extra steps, and it leaves a guard test nothing to check. Coarse operations are correct at genuine transaction boundaries (a border sale is stock out + contract credited + treasury paid + ledger booked, because those never happen apart).

- **Metric, and honest about it.** Distance is metres, time is seconds; the day loop is a fixed timestep. **The word "tile" does not belong in simulation vocabulary.** v1 documented its own dishonesty here (`archive/src/game/world.ts:233`: a day covered ~5 tiles, so "any real-world unit here would be a fiction the sim cannot back") — this build exists partly to fix that. A vehicle's speed must be a quantity the simulation can actually stand behind.

- **Space is continuous; the grid describes terrain, not buildings.** Buildings sit at free positions with real metric footprints. There is no `tile.buildingId` occupancy model and no row-scan draw order — both were grid artefacts. Pathing is a graph over the road network, not a per-metre flood.

- **Resources are a 3D subsurface field, and the deposit is the thing — not the building.** Richness varies by position *and depth*; deeper costs more; volumes are finite and deplete on a long timescale (real coal deposits are enormous — depletion is a pressure, not a clock). Groundwater is a peer field from day one so wells are never a retrofit. Resources are invisible on the terrain and read through a **geological survey overlay**. A mine is an ordinary fixed-size building that **taps** a deposit and reaches the whole volume; extraction scales with how much machinery is pointed at it, and faster extraction means faster depletion.

- **Citizens are individuals who live somewhere.** ECS entities with a home, a workplace, and a real journey. **Work has to be reachable**: roughly 2 km on foot, further only with transport. v1's labour model had no geography at all — a citizen in the far south staffed a mine in the far north for free — and fixing that is what lets a mining town exist, and later die when its deposit runs out. That scenario, depletion plus commute together, is the acceptance test for the whole labour model.

- **Measure, never estimate.** Performance claims get a recorded baseline before any optimisation: agent counts, pathing cost per simulated day, geology query cost. v1's habit of pinning real measurements in tests (`mapgen-save-performance`, `logistics-routing-performance`) is why its performance conversations were about facts.

- **Balance lives in data, behaviour lives in systems.** A list of entity ids inside simulation logic is a smell: a list is a thing you must remember to edit, and whatever you forget lands silently in a fallback. Author the property next to the fields it relates to, and add a guard test so an unauthored case fails the build instead of defaulting quietly.

## Design decisions already made

**Founding is choosing your land**, in two beats, and it replaces both v1's new-game dialog and its generic intro overlay:

1. **The land** — a shelf of candidate maps derived from one master seed, shown as comparable minimaps over a full-screen stage rendering the selected candidate with its real climate. Filters (size, climate) **transform the same seeds** rather than replacing them, so changing climate shows what it does to land already under consideration. Cards carry the picture plus the few stats that actually decide a start — visible because Moscow surveyed before assigning the posting.
2. **The posting** — republic name, difficulty (three presets, independent of land quality), and a briefing specific to the map chosen.

Land quality genuinely varies; nothing guarantees a rich start.

## Open decisions, with their decision points

- **Renderer and shell** — Bevy, custom wgpu, or another. Decide once the sim runs headless and its costs are measured. Evidence so far: Workers & Resources runs on a proprietary engine 3Division wrote themselves (one programmer at launch, not Unity); Cities: Skylines II is the cautionary Unity-DOTS case in this exact genre; Bevy matured considerably in 2026 (0.18 editor preview, 0.19 BSN and `bevy_feathers`) but has shipped no large simulation game and takes breaking changes roughly quarterly. Re-check Bevy's UI maturity at decision time — it is the weakest part of that stack and this game is roughly half UI.
- **Terrain grid resolution** — measure before choosing.
- **Which ECS** — `bevy_ecs` standalone is the default because it keeps the Bevy door open at zero cost, but the simulation must not expose engine-specific types at its boundaries.
