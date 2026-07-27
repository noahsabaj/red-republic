# Porting checklist: the 1.x behavioural suite

The plan named `archive/src/game/__tests__/` as the porting checklist, on the
grounds that it is the closest thing this design has to a written spec. This is
that checklist, walked file by file.

Every file gets one of four dispositions:

- **Ported** — the rule it protects exists here, with a test that would fail
  without it. The Rust test is named.
- **Superseded** — the *behaviour* it protects was deliberately replaced. The
  new model is named, and so is the reason. These are not gaps; they are the
  point of the rebuild.
- **N/A** — it tested a renderer, a DOM interaction or a browser storage slot.
  This crate has none of those and will not grow them; the shell will.
- **Not yet** — a real gap. Nothing here is blocked; each is work that has not
  been done, and saying so is the point of writing the list down.

Counts: **19 ported, 12 superseded, 10 n/a, 9 not yet.**

---

## Ported

| 1.x test | Where it lives now |
|---|---|
| `auto-trade.test.ts` | `trade::TradePolicy`; `systems::trade`. Rules are applied in the player's order and the first is served first when throughput runs short. |
| `border.test.ts` | `trade::BorderEdge`, `World::place` refusing a customs house away from the border (`PlacementError::NotOnTheBorder`). |
| `climate.test.ts` | `climate.rs`. The four postings are carried over, re-expressed as monthly means; `each_climate_has_a_winter_and_a_summer` and `mid_month_reads_back_the_authored_mean` pin them. |
| `construction-fairness.test.ts` | `systems::construction`. Sites are worked in commissioning order and finished one at a time — the rule the archive learned the hard way. |
| `contracts.test.ts` | `contract.rs` and `systems::contracts`. Offers expire, deadlines fail with a fine, relations sour and decay. |
| `delivery-priority.test.ts` | `systems::logistics`. Urgency is downtime averted, drain is intent (`cover_days`), two passes with housekeeping second. |
| `integer-quantities.test.ts` | Enforced by construction rather than tested: `units::Tonnes` is continuous throughout, and wholeness is an edge property. |
| `logistics.test.ts` | `systems::logistics`, `systems::serve`. |
| `mapgen-sizes.test.ts` | `founding::SIZES`, now named in kilometres rather than tile counts. |
| `mapgen-snapshot.test.ts` | `mapgen::tests::generated_geology_is_pinned_across_machines` and `terrain::tests::generated_ground_is_pinned_across_machines`. Same role, and the same rule: never re-pin to make a change pass. |
| `mapgen.test.ts` | `mapgen.rs`, `terrain.rs`. |
| `mutation-writeset.test.ts` | `systems::WRITE_SETS` plus three guards. **Both directions** are checked — a system may not emit outside its set, and a set may not claim more than the system emits. |
| `power.test.ts` | `systems::power`. |
| `production.test.ts` | `systems::production`. Limiters are kept separate so a stalled building can say which one stopped it. |
| `save-format.test.ts` | `world::SAVE_VERSION`, `World::to_bytes`/`from_bytes`, `a_future_version_is_recognised_before_the_world_is_parsed`. |
| `save-roundtrip.test.ts` | `world::tests::a_reloaded_world_resumes_the_same_future`. The tripwire that catches every field somebody forgot to persist, and it existed here before parallelism was ever on the table. |
| `trade.test.ts` | `trade.rs`, `systems::trade`. |
| `campaign-pacing.test.ts` | `src/bin/trajectory.rs`. Deliberately **not** a test: a trajectory is evidence, and reading one beats any threshold. It has already found four balance gaps that reasoning did not. |
| `logistics-routing-performance.test.ts`, `mapgen-save-performance.test.ts` | `tests/baselines.rs`. Same habit — measured, never estimated. |

## Superseded

| 1.x test | What replaced it, and why |
|---|---|
| `citizens_workers.test.ts` | `citizen.rs`. The archive's labour model had **no geography at all** — a citizen in the far south staffed a mine in the far north for free. Replacing that is most of the reason this build exists. |
| `deposits.test.ts` | `geology.rs`. Tile-visible deposits are gone; a deposit is a 3D body a mine *taps*, read through a survey. |
| `pathfind.test.ts`, `pathfind-bounded.test.ts` | `road.rs`. A grid flood over a million cells was never going to survive metric scale; routing is a graph over what the player built. |
| `topology.test.ts`, `topology-cache-regressions.test.ts` | Same. The caches existed to make grid floods affordable. |
| `roads.test.ts` | `road.rs`. Roads are a graph, not tiles that become road. |
| `weather-sim.test.ts` | `climate.rs`, partially — temperature is carried over because heating depends on it. Conditions, snow depth and river ice are **not yet** (below). |
| `helpers.ts`, `campaign.ts` | `scenario::found`, and the `bare()` fixture in `systems`. |
| `tilemap-cache.test.ts` | Nothing. It tested the offscreen rasteriser whose 16,384 px dimension cap is one of the things that ended 1.x. |
| `logistics-characterization.test.ts` | Partly ported into `systems::logistics` tests; the parts that characterised v1's tile routing went with the router. |
| `mutation-guards.test.ts` | `systems::WRITE_SETS`. |
| `save-slots.test.ts` | A shell concern. The crate owns the **format**; where a save file lives is not simulation. |

## N/A — renderer, DOM or shell

`build-menu-tooltip.test.ts`, `bulk-autobuy.test.ts`, `happiness-breakdown.test.ts`
(UI half), `input.test.ts`, `render-math.test.ts`, `selection.test.ts`,
`storage-display.test.ts`, `planning-mode.test.ts` (UI half),
`construction-pause.test.ts` (UI half), `per-site-policy.test.ts` (UI half).

These tested a canvas renderer, a pointer gesture machine, or a React panel.
The crate has no renderer and must not grow one — that independence is what
keeps the shell decision open.

## Not yet — real gaps, with what each would need

| Missing | What it was in 1.x | What it needs here |
|---|---|---|
| **A physical fleet** (`fleet.test.ts`, `customs-refuel.test.ts`) | Lorries as persistent machines with fuel, positions and jobs. | `FREIGHT_TONNES_PER_DAY` is a scalar placeholder, and its doc comment says so. The *ranking* was the hard-won part and it is ported; the vehicles are not. |
| **Machinery wear** (`machinery.test.ts`) | Per-building machinery drained daily; a dry bin cut output. | A `wear` field on `BuildingDef` and a system. The physical dependency it creates — industry needs machinery to build *and* to run — is a good mechanic and worth having. |
| **Foreign construction labour** (`foreign-labor.test.ts`) | Paid builders in roubles, per-site opt-in. | Needs per-site build policy first. |
| **Auto-buy and bonded imports** (`auto-buy.test.ts`, `bulk-autobuy.test.ts`) | A site's import bill paid at the border, delivered as earmarked virtual imports. | Same: per-site build policy. |
| **Loans** (`loans.test.ts`) | Bloc advances with fixed simple interest. | Contained; `contract.rs` is the natural neighbour. |
| **Happiness** (`happiness-breakdown.test.ts`) | A weighted satisfaction model driving migration. | `Building::provisioned` and `Building::heated` are the measured inputs and already exist. What is missing is what they *do* — see below. |
| **Water and sewage** (`water.test.ts`) | Wells, towers, waste. | Groundwater is already a peer mineral with recharge, specifically so this is not a retrofit. |
| **Weather beyond temperature** (`weather-sim.test.ts`) | Conditions, snow depth, river freeze, drought. | Farm output is currently seasonless. `climate.rs` is the place. |
| **Per-site build policy** (`per-site-policy.test.ts`, `planning-mode.test.ts`, `construction-pause.test.ts`) | Instant build, auto-buy, foreign labour and planning mode, per site. | Blocks three of the rows above. |

---

## The honest note about `provisioned` and `heated`

Both are **measured conditions with no consequence attached yet**. The households
system computes how much of a household's needs the shops met; the heating system
computes whether the boilers reached it. Nothing reads either.

That is deliberate rather than forgotten. Heating already has a real economic
consequence — it burns coal from the same stockpile the power station wants, and
a taiga posting costs measurably more than a plains one. What neither has is an
effect on *people*, and inventing one now would be inventing balance ahead of the
model that should own it. The 1.x answer was a happiness model feeding migration;
that is the row above, and these two fields are its inputs waiting for it.
