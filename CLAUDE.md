# Red Republic

A planned-economy city builder. Rust simulation (`crates/sim`), Godot shell (`crates/red-republic-shell` + `godot/`), desktop-only, real-time.

**This file holds only what the code cannot tell you and cannot enforce: decisions, boundaries and taste.** Everything about how something works, what exists, or what a constant is set to belongs in the source — read it there. Design history and this machine's gotchas are in the project memory silo and in git history.

The 1.x TypeScript build is frozen at `D:\archive`. It is a **balance reference only** — port rules, never code, never treat it as a spec. The bar is Workers & Resources: Soviet Republic.

## The goal

**Finished when a stranger could install it, play it, and nothing in it embarrasses you.** Three conditions, all binding:

1. **Nothing the simulation knows is invisible.** Every new simulation fact ships with its UI in the same commit, and is controllable wherever it is a decision rather than a consequence. The exposure guard enforces this; keep it able to.
2. **Feature parity with Workers & Resources.** Built *and visible*. The known gap is depth, not breadth.
3. **A republic survives a decade without an artificial wall** — every wall it hits a design consequence rather than a bug, a missing system or a balance hole.

**Held to the standard of a public release**: no placeholder strings, no debug UI, no crash on a GPU nobody here owns, settings that work, a game that explains itself. Nothing automated can check that clause, so it is the one that drifts.

**No condition covers whether the game is fun.** Noah playing it is the only verdict.

## Not mine to decide

- **Shipping.** Building the installer is finishing. Putting it in front of anyone is Noah's call, in the conversation where it happens. "The installer is done" is not permission.
- **Novel mechanics.** Parity is the floor I build. Inventing past it is Noah's, unasked.
- **Scope exclusions**, so silence is not read as coverage: water and sewage are **deferred** and want real plumbing when they come, not a hauling rule; land purchase is out because W&R has none — the map inside the border is yours from day one. No objective or goal-loop system.

## Settled — do not re-litigate, do not soften

- **No difficulty levels. Ever.** The land is the difficulty. **No tuning pass may soften a start to make it comfortable.**
- **No instant build. Ever.** Anything sourced inside the republic costs labour and materials and no money at all. Paying to make a problem vanish is the shape this game exists to refuse.
- **Real-time is the thesis** — one real second is one in-game second at speed 1. Done requires it to be genuinely worth playing at, not merely correct. That is a standing constraint on the renderer.
- **The look is realism.** Art and audio are in scope.
- **Nobody here but Noah can judge audio.** Do not propose finishing the soundtrack; propose asking him to listen.

## Taste

- **Physicality over abstraction, every time.** If a problem is physical, its solution is a physical thing that exists in the world — built, staffed, stocked, and *somewhere*. A number with no place in the world is the smell.
- **A contested value judgement belongs to the player.** If a decision encodes a political or moral trade-off, the deliverable is a control, not a default. Difficulty is not one of these: where a knob would let the player dial down the subject matter, the subject matter is the game.
- **A want is a way to fail; a comfort is a way to do better.** Nice-to-have goods lift a score, never join it — adding one as the other silently re-marks work the player already did.
- **Tell the player which thing is worst, not what their score is.** A breakdown is actionable and a number is not.
- **Ask what state the player is in when a consequence fires.** That is usually the state where they have least of whatever you meant to take, which is how a penalty ends up costing nothing.
- **Naming voice is period Soviet institutions.**
- **No emojis in product UI. Don't gate reversible actions behind "are you sure?".**

## Architecture

- **`crates/sim` has zero engine dependencies.** A rule, not an accident: it is what keeps the shell decision reversible. The simulation must not expose engine-specific types at its boundaries.
- **Determinism.** Same seed and inputs produce the same world; a save reloads bit-exact. **Map generation must reproduce across machines** — a shared seed is a promise between players — so it may not depend on transcendental precision, hash iteration order or address-dependent behaviour. Never enable fast-math-style float relaxation.
- **The save format must round-trip `f64` bit-exactly**, which disqualifies JSON, and **the crate owns the format** — a requirement a caller can opt out of is not a requirement.
- **Systems propose, one writer applies, and each declares what it may change.** No generic `{field, value}` hatch: that is "write anything" with extra steps and leaves a guard nothing to check. Declarations are checked in **both** directions — the half that rots is a write-set quietly claiming more than its system emits.
- **The player's surface is two verbs, `tick` and `issue`.** A refusal returns a sentence a UI can show, worded beside the error variant. Accepted commands are journalled and refused ones are not, so a save records how its republic came to be.
- **Metric, and honest about it.** Metres and seconds. **The word "tile" does not belong in simulation vocabulary**, and a speed must be a quantity the simulation can stand behind.
- **Space is continuous; the grid describes terrain, not buildings.** No occupancy model, no row-scan draw order.
- **Balance lives in data, behaviour lives in systems.** A list of entity ids inside logic is a smell — whatever you forget to edit lands silently in a fallback. Author the property beside the fields it relates to and guard it so an unauthored case fails the build.
- **Where a rule can be enforced in code, build the enforcement instead of writing it here.** Prefer making an invalid state unrepresentable over guarding against it.
- **Never hand the UI a dictionary per entity; hand it a packed array and slice it.** Keep the engine-owned views coarse.
- **The interface is Godot's, entirely.** Rust hands over figures, names and indices; every sentence a player reads, and every decision about what a screen looks like, is in `godot/ui/`. A `String` crossing the boundary is either a name authored beside the thing it names or a refusal the simulation wrote — never a heading, a label or a paragraph.
- **One theme resource, and no screen styles itself.** `godot/ui/palette.gd` is where a colour or a measurement is chosen; `godot/ui/theme.tres` is generated from it and is the project's default theme, so a control is correct before any script touches it. A screen that sets a colour is either a missing theme variation or a mistake — the exception is tinting by what the simulation says, which reads the palette rather than typing a triple.

## Commands

- `cargo test` — the gate. `cargo clippy --all-targets -- -D warnings`, `cargo fmt` alongside.
- **`cargo run --release --bin trajectory -- <seed> <years> <climate>`** — plays the opening headless, a month a line. Asserts nothing; reading it is the point, and it is the only thing that exercises the opening. Its `Director` is deliberately a bad player, so failure under it does not prove the game unwinnable.
- **`godot --headless --path godot -- --check`** — does the game load? **Rebuild the cdylib first** (`cargo build -p red-republic-shell`) or it tests the previous one. **`cargo test` does not parse GDScript**, so a green gate says nothing about whether the game starts. Run it with a timeout: a parse error hangs the check rather than failing it, and `--import` reports parse errors in seconds. **On a checkout with no `godot/.godot/`, import twice and believe the second**: a pass imports assets, and a resource that references one — `ui/theme.tres` names six fonts — can only resolve on a later pass, so the first reports a parse error for a file that is correct.
- `-- --screen <name>` renders one screen; `-- --shot <path>` captures a frame, aimed with `--pitch`, `--hour`, `--at`. **Never add `--headless` to a capture** — it waits for a frame that never arrives and spins silently for ever. **Look at the image.** Godot culls back faces, so geometry wound the wrong way renders as *nothing*, which reads as "not built yet" and never as "inside out".
- **`pwsh tools/package.ps1`** — builds the Windows installer into `dist/`.
- `python tools/build_icon.py` — regenerate the icon, only when the design changes.
- **`godot --headless --path godot --script res://tools/build_theme.gd`** — rewrites `godot/ui/theme.tres` from `godot/ui/palette.gd`. Run it after touching the palette and commit both; nothing does it for you, and the editor and the game both read the committed resource.
- `python tools/fetch_fonts.py` — re-download the PT faces, only when the type changes.
