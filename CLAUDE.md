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
- `crates/sim/PORTING.md` — the 1.x behavioural suite walked file by file, with each test marked ported, superseded, n/a or not-yet. Read it before asking whether a mechanic exists.
- `archive/` — the frozen 1.x build. Read it for rules and balance; never edit it.

## Rules

- **Determinism, at two different bars.** Same seed and same inputs produce the same world; a save reloads bit-exact. **Map generation must reproduce across machines** — a shared seed is a promise between players — so it may not depend on anything platform-varying (transcendental precision, hash iteration order, address-dependent behaviour). **The running simulation need only reproduce for the same binary**, which is the easy case: Rust does not reassociate floats, so one compiler on one instruction set suffices. Nothing may enable fast-math-style float relaxation.

- **The save format must round-trip `f64` bit-exactly, and JSON does not.** Measured, not reasoned: 91,767 of 200,000 sampled `f64` values changed passing through `serde_json` — it writes correct digits and its *parser* is not correctly rounded. A simulation whose state is full of floats cannot use a format like that, and the failure is the worst kind: silent, one ULP, invisible until two runs have diverged far enough to notice. The format is `postcard` (bit patterns, not digits) and `the_save_format_round_trips_floats_bit_exactly` is the guard. **The crate owns the format** — `World::to_bytes`/`from_bytes`, not a serde value the caller encodes how it likes — because a requirement a caller can opt out of is not a requirement, and the guard would go on passing while testing a format nothing used. The version is decoded on its own before the world is parsed, so a save from a newer build reports that rather than "corrupt".

- **The determinism tripwire comes before the thing it defends.** v1's `save-roundtrip.test.ts` — serialize, reload, advance both 90 days, compare everything — is what caught every field someone forgot to persist. The equivalent test exists here before parallel scheduling is introduced, not after. A system schedule that is parallel and unordered is non-deterministic by default; explicit ordering is the fix and it must be defended by a test that would fail without it.

- **Systems propose, one writer applies, and each declares what it may change.** Carried forward from v1 because it is why blast radii stayed small: a system reads the world and returns what it wants to change; it never writes. There is deliberately no generic `{field, value}` escape hatch — that is "write anything" with extra steps, and it leaves a guard test nothing to check. Coarse operations are correct at genuine transaction boundaries (a border sale is stock out + contract credited + treasury paid + ledger booked, because those never happen apart). `systems::WRITE_SETS` names the mutation kinds each system may emit, and it is checked **in both directions**: a system may not emit outside its set, *and* a set may not claim more than the system emits. The second half is the one that rots — a write-set that has quietly become a superset constrains nothing and looks fine.

- **Metric, and honest about it.** Distance is metres, time is seconds; the day loop is a fixed timestep. **The word "tile" does not belong in simulation vocabulary.** v1 documented its own dishonesty here (`archive/src/game/world.ts:233`: a day covered ~5 tiles, so "any real-world unit here would be a fiction the sim cannot back") — this build exists partly to fix that. A vehicle's speed must be a quantity the simulation can actually stand behind.

- **Space is continuous; the grid describes terrain, not buildings.** Buildings sit at free positions with real metric footprints. There is no `tile.buildingId` occupancy model and no row-scan draw order — both were grid artefacts. Pathing is a graph over the road network, not a per-metre flood.

- **Resources are a 3D subsurface field, and the deposit is the thing — not the building.** Richness varies by position *and depth*; deeper costs more; volumes are finite and deplete on a long timescale (real coal deposits are enormous — depletion is a pressure, not a clock). Groundwater is a peer field from day one so wells are never a retrofit. Resources are invisible on the terrain and read through a **geological survey overlay**. A mine is an ordinary fixed-size building that **taps** a deposit and reaches the whole volume; extraction scales with how much machinery is pointed at it, and faster extraction means faster depletion.

- **Citizens are individuals who live somewhere.** ECS entities with a home, a workplace, and a real journey (`transport::Commute` — mode, distance and time, written by the labour pass alongside the workplace, because the two are one decision). **Work has to be reachable**: roughly 2 km on foot, further only with transport. v1's labour model had no geography at all — a citizen in the far south staffed a mine in the far north for free — and fixing that is what lets a mining town exist, and later die when its deposit runs out. That scenario, depletion plus commute together, is the acceptance test for the whole labour model.

- **Transport lifts the reach ceiling physically, never by relaxing the rule.** A bus rides the road network, so it reaches exactly what the republic has built road to; it carries a finite number of people, so reach is a capacity you fund; and it burns fuel, so a commute is an ongoing cost against the same refinery output everything else wants. The bound is on **time** (45 minutes each way), which is what makes a faster road genuinely extend reach rather than merely shorten a trip. Walkers are always hired before riders — a seat spent on someone who could have walked is a seat denied to someone who could not. The counterpart to the mining-town acceptance test is `a_road_and_a_bus_save_the_town_the_mine_left_behind`.

- **Not every system runs every tick.** Labour is daily — people do not change jobs every minute — and running it per tick cost 656 ms per simulated day at only 4,000 citizens, against 1 ms once moved to the day boundary. Contracts are daily for a harder reason: deadlines are day indices, so a per-tick sweep would fine a republic 1,440 times for one missed delivery. That first figure was found by `tests/baselines.rs`, not by reasoning, which is the argument for having it. When adding a system, the first question is what cadence it actually needs; the consequence is real and belongs in a test (people start work the day *after* they arrive).

- **Measure, never estimate.** Performance claims get a recorded baseline before any optimisation: agent counts, pathing cost per simulated day, geology query cost, worldgen and the founding shelf. v1's habit of pinning real measurements in tests (`mapgen-save-performance`, `logistics-routing-performance`) is why its performance conversations were about facts. It keeps paying: `simulated_day_cost` caught the households system calling `residents_of` once per home per tick, each call rebuilding and sorting the whole population — **212 ms per simulated day at 4,000 citizens, 23 ms once counted in one pass**, and it went from quadratic-ish to near-linear. Nothing about that code looked wrong.

- **Weather is state, not the calendar.** Heating demand follows **today's temperature**, never the month — that was v1's explicit rule and it is what makes a cold snap an event a republic can be caught out by. Temperature is a pure function of `(seed, climate, day)` drawn from its own substream, so reading a forecast never perturbs the economy and any future day can be asked about without advancing anything. Climates are authored as twelve monthly means rather than a sinusoid: exact arithmetic, and a table can express a late spring that a symmetric curve cannot.

- **Balance lives in data, behaviour lives in systems.** A list of entity ids inside simulation logic is a smell: a list is a thing you must remember to edit, and whatever you forget lands silently in a fallback. Author the property next to the fields it relates to, and add a guard test so an unauthored case fails the build instead of defaulting quietly. Every field on `BuildingDef` is authored on **every** building for exactly this reason — `heat: 0.0` on a sawmill is a decision somebody made, and a defaulted one would not be.

- **The order things are founded in is a staffing priority.** Labour fills workplaces in commissioning order, so whatever is built last is what goes unmanned when the republic is short of people. That is a real consequence and `scenario::found` is written against it: heat and power are placed together, ahead of the shops. A founding with fewer settlers than jobs is a legitimate hand — but it will be the tail of that list that stands idle, and the trajectory runner is how you find out which.

## Design decisions already made

**Founding is choosing your land**, in two beats, and it replaces both v1's new-game dialog and its generic intro overlay:

1. **The land** — a shelf of candidate maps derived from one master seed, shown as comparable minimaps over a full-screen stage rendering the selected candidate with its real climate. Filters (size, climate) **transform the same seeds** rather than replacing them, so changing climate shows what it does to land already under consideration. Cards carry the picture plus the few stats that actually decide a start — visible because Moscow surveyed before assigning the posting.
2. **The posting** — republic name, difficulty (three presets, independent of land quality), and a briefing specific to the map chosen.

Land quality genuinely varies; nothing guarantees a rich start.

The sim-side half is built (`founding.rs`): `Shelf::derive` gives six candidates from one master seed, each with the decisive stats read from the **engine-owned** survey — never recomputed, so a card cannot advertise a republic the player will not be given. Re-filtering keeps the seeds and re-derives the land. It costs 220 ms for six 10 km candidates, measured; I had told noahs during the interview that it was essentially free, reasoning from v1's tile generator, and it is not.

**Terrain grid resolution is 10 m, and that was decided by measurement.** The sweep (`cell_size_sweep`) found a clean quadratic with no cliff anywhere, so performance does not choose a resolution — the correctness floor does. 10 m is the coarsest size at which the smallest building in the table still covers a cell in both axes; below it a house and a shop become the same rounded square, which is the grid artefact this build exists to remove. The price is 4.8 MB and 36 ms per 10 km map. Resolution is carried on the `Terrain`, not in a constant, so re-measuring stays a one-line experiment and a save always knows what it was written at.

## Open decisions, with their decision points

- **Renderer and shell** — Bevy, custom wgpu, or another. **The stated decision point has now been reached**: the sim runs headless and its costs are recorded in `tests/baselines.rs`. Evidence so far: Workers & Resources runs on a proprietary engine 3Division wrote themselves (one programmer at launch, not Unity); Cities: Skylines II is the cautionary Unity-DOTS case in this exact genre; Bevy matured considerably in 2026 (0.18 editor preview, 0.19 BSN and `bevy_feathers`) but has shipped no large simulation game and takes breaking changes roughly quarterly. Re-check Bevy's UI maturity at decision time — it is the weakest part of that stack and this game is roughly half UI.
- **Which ECS** — `bevy_ecs` standalone is the default because it keeps the Bevy door open at zero cost, but the simulation must not expose engine-specific types at its boundaries. Holding so far: `CitizenRecord` is the serialization boundary and no `Entity` crosses the crate's public surface.
