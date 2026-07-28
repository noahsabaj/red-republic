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

Counts: **22 ported, 11 superseded, 10 n/a, 8 not yet.**

---

## Ported

| 1.x test | Where it lives now |
|---|---|
| `auto-trade.test.ts` | `trade::TradePolicy`; `systems::trade`. Rules are applied in the player's order and the first is served first when throughput runs short. |
| `border.test.ts` | `trade::BorderEdge`, `World::place` refusing a customs house away from the border (`PlacementError::NotOnTheBorder`). |
| `climate.test.ts` | `climate.rs`. The four postings are carried over, re-expressed as monthly means; `each_climate_has_a_winter_and_a_summer` and `mid_month_reads_back_the_authored_mean` pin them. |
| `construction-fairness.test.ts` | `systems::construction`. Sites are worked in commissioning order and finished one at a time — the rule the archive learned the hard way. |
| `contracts.test.ts` | `contract.rs` and `systems::contracts`. Offers expire, deadlines fail with a fine, relations sour and decay. |
| `delivery-priority.test.ts` | `systems::dispatch`. Urgency is downtime averted, drain is intent (`cover_days`), two passes with housekeeping second. The ranking survived the fleet unchanged; what changed is that a ranked demand now becomes a job for a lorry rather than an instant transfer. |
| `integer-quantities.test.ts` | Enforced by construction rather than tested: `units::Tonnes` is continuous throughout, and wholeness is an edge property. |
| `logistics.test.ts` | `systems::dispatch`, `systems::serve`. |
| `fleet.test.ts` | `fleet.rs`, `journey.rs`, `systems::fleet`, `systems::commissioning`. Two 1.x rules carried and the rest re-derived: **garages own vehicles** (a Motor Depot's establishment is authored on `BuildingDef`, and `crewed` says how many have drivers) and **a vehicle never accepts a job it cannot finish** (the round trip is priced at dispatch, so running dry is a refusal in the yard). Speeds are real km/h rather than 1.x's road-tile-equivalents per day. |
| `roads.test.ts` | `roadworks.rs`, `systems::construction`, `Mutation::Lay`. The 1.x lifecycle — order, gravel delivered, crew, and **not drivable until complete** — is ported whole; what is superseded is the tile it produced, since a finished road here becomes graph segments with junctions along its length. Grades are authored and a journey leg carries the road's speed limit, so a dirt track and tarmac are a real choice 1.x did not have. **Not** ported from that file: bulldozing a site and refunding its stock, instant-build-by-import, and the site-versus-building placement collision — all of which need a per-site build policy or a demolition model this build has neither of. |
| `mapgen-sizes.test.ts` | `founding::SIZES`, now named in kilometres rather than tile counts. |
| `mapgen-snapshot.test.ts` | `mapgen::tests::generated_geology_is_pinned_across_machines` and `terrain::tests::generated_ground_is_pinned_across_machines`. Same role, and the same rule: never re-pin to make a change pass. |
| `mapgen.test.ts` | `mapgen.rs`, `terrain.rs`. |
| `mutation-writeset.test.ts` | `systems::WRITE_SETS` plus three guards. **Both directions** are checked — a system may not emit outside its set, and a set may not claim more than the system emits. |
| `power.test.ts` | `systems::power`. |
| `production.test.ts` | `systems::production`. Limiters are kept separate so a stalled building can say which one stopped it. |
| `machinery.test.ts` | `BuildingDef::wear` authored on all 28 rows, `systems::WORN_EFFICIENCY`. The archived rule whole: wear is proportional to activity so an idle building wears nothing, and a dry bin is a **soft penalty at 0.5, never a stall**. Guards `a_dry_machinery_bin_halves_output_and_never_stalls_it` and `machines_wear_only_when_they_are_worked` — the second needs an unstaffed twin, which is the only fixture that tells "scales with activity" from "drains daily". What is **not** carried: instant-build pricing and the imported-machinery loop, which want the border and per-site policy. |
| `weather-sim.test.ts` (the farm half) | `systems::growing_conditions`, `BuildingDef::farms`, `ground::Ground::water`. Rain feeds them, frost stops them, drought withers them — ported against continuous ground state rather than v1's weather enum and `droughtAfterDays` counter. Guards `frozen_ground_grows_nothing_however_warm_the_air`, `a_drought_cuts_the_harvest_without_ending_it` and `a_farm_that_cannot_grow_produces_nothing_and_wears_nothing`. **Nothing consults the month**, and the frozen-midsummer case is what proves it. The rest of that file stays superseded, below. |
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
| `weather-sim.test.ts` | `climate.rs` and `ground.rs`. Temperature carried over; precipitation, lying snow, soil moisture and frost are new. They drive cross-country going **and, since 2026-07-28, farm output** — through the root-zone `water` field rather than the topsoil `moisture` one, for reasons measured and recorded in `CLAUDE.md`. The farm half is ported rather than superseded and is listed above. River ice is still **not yet**. |
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

**What is not N/A is what those panels sent.** The commands underneath them are
simulation, and they are counted above as their own row. This classification
covers the tests, never the verbs.

## Not yet — real gaps, with what each would need

| Missing | What it was in 1.x | What it needs here |
|---|---|---|
| **A player command surface** | Every v1 panel sent a command — place, demolish, set a delivery priority, accept a tender, pause a site. Those test files are filed N/A below, which was right about the *tests* (they drove React) and wrong about the commands underneath them, which are simulation. | A `Command` type applied through the same single-writer path systems use, and **recorded as it is applied**. Today every field on `World` is `pub` and the deliberate verbs are four — `place`, `order_road`, `place_built`, `tick` — so a shell can write anything a system may not, and the determinism rule's *same seed and same inputs* has no **inputs** to hold constant. Counted on 2026-07-28: the largest uncounted row, and on the critical path, since the goal's first condition cannot close without it. |
| **Refuelling away from home** (`customs-refuel.test.ts`) | Vehicles topping up at a filling point mid-route. | A vehicle tops up from its own garage at dispatch and the round trip is priced before it leaves, so it cannot strand itself — which makes this a *range* mechanic rather than a safety one. `GasStation` is already in the building table with nothing to do; it becomes real when hauls get long enough that a full tank is not a round trip. |
| **Foreign construction labour** (`foreign-labor.test.ts`) | Paid builders in roubles, per-site opt-in. | Needs per-site build policy first. |
| **Auto-buy and bonded imports** (`auto-buy.test.ts`, `bulk-autobuy.test.ts`) | A site's import bill paid at the border, delivered as earmarked virtual imports. | Same: per-site build policy. |
| **Loans** (`loans.test.ts`) | Bloc advances with fixed simple interest. | Contained; `contract.rs` is the natural neighbour. |
| **Happiness** (`happiness-breakdown.test.ts`) | A weighted satisfaction model driving migration. | `Building::provisioned` and `Building::heated` are the measured inputs and already exist. What is missing is what they *do* — see below. |
| **Water and sewage** (`water.test.ts`) | Wells, towers, waste. | Groundwater is already a peer mineral with recharge, specifically so this is not a retrofit. |
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
