//! The headless trajectory runner.
//!
//! Successor to the archived build's `campaign-pacing.test.ts`, and the tool
//! for re-deriving balance after a deliberate change. It founds a republic on a
//! real generated map, runs it for a few years with nobody playing, and prints
//! what happened month by month.
//!
//! Nothing here asserts. A trajectory is evidence, and reading it is the point:
//! if the coal column goes flat in year two, that is the balance telling you
//! something, and no threshold in a test would have said it as clearly.
//!
//! ```text
//! cargo run --release --bin trajectory -- [seed] [years] [climate]
//! ```
//!
//! `climate` is `plains`, `taiga`, `steppe` or `maritime` — the fastest way to
//! see what a winter costs, because the same republic on the taiga burns a
//! different amount of coal for the same output.

use red_republic_sim::building::BuildingKind;
use red_republic_sim::climate::ClimateId;
use red_republic_sim::command::Command;
use red_republic_sim::resource::Resource;
use red_republic_sim::roadworks::Grade;
use red_republic_sim::scenario;
use red_republic_sim::time::TICKS_PER_DAY;
use red_republic_sim::trade::{Market, TradeAction};
use red_republic_sim::units::{Metres, Point, Tonnes};
use red_republic_sim::world::{World, WorldSpec};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1961);
    let years: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(3);
    let climate = match args.next().as_deref() {
        Some("taiga") => ClimateId::Taiga,
        Some("steppe") => ClimateId::Steppe,
        Some("maritime") => ClimateId::Maritime,
        _ => ClimateId::Plains,
    };

    let mut world = World::new(WorldSpec {
        seed,
        extent: Metres(6_000.0),
        climate,
    });
    // **A posting, not a town.** This runner stood `scenario::town` up for as
    // long as the founding did, and kept doing it after the founding stopped —
    // which made every figure it printed a measurement of a republic nobody is
    // given. Condition 3 of the goal asks whether *a republic* survives a
    // decade, and a decade run against a nineteen-building hand answers a
    // question about a fixture.
    //
    // So it plays the opening now: an empty map, a rouble grant, and the
    // director below deciding what to buy. That is also the only way the
    // opening gets exercised at all outside somebody sitting down to play it.
    let centre = scenario::found(&mut world);

    println!("Red Republic — trajectory");
    println!(
        "seed {seed} · {years} years · 6 km republic · {}",
        climate.def().name
    );
    println!(
        "posted at ({:.0} m, {:.0} m) · nothing built · {:.0} roubles",
        centre.x.0,
        centre.y.0,
        scenario::GRANT_ROUBLES,
    );
    let mut director = Director::new(centre);
    println!(
        "coal in the ground at founding: {:.0} t",
        world
            .geology()
            .remaining_of(red_republic_sim::geology::Mineral::Coal)
            .0
    );
    println!();
    println!(
        "{:>10} {:>4} {:>5} {:>5} {:>6} {:>6} {:>5} {:>8} {:>8} {:>6} {:>8} {:>7} {:>5} {:>8} {:>10} {:>9} {:>4} {:>8} {:>5} {:>9} {:>7} {:>6} {:>10}",
        "date",
        "pop",
        "empl",
        "fed%",
        "degC",
        "soft%",
        "warm%",
        "coal",
        "food",
        "fuel",
        "moved",
        "lorries",
        "stuck",
        "road km",
        "coal left",
        "money",
        "dark",
        // Builders out from their offices, and how many of those are standing
        // somewhere waiting for a bus. The second number is the friction one:
        // a gang with nowhere to be is people the republic is paying to stand
        // in a field, and it is invisible in every other column here.
        "crew/wait",
        // How content the republic's people are, on the same weighted average
        // the shell shows. Below 60% nobody wants to come, and below 35% people
        // start going — so this is the column that says whether a republic is
        // growing, holding, or quietly bleeding.
        "cont%",
        // And who actually came and went: settled / left / turned back at the
        // border for want of a coach. The last of the three is the friction
        // number — people the republic was offered and could not reach.
        "in/out/x",
        // Rubbish nobody has driven to a landfill, and how filthy the town's
        // own air is. Both are invisible in every other column here, and both
        // cost the republic its people's contentment.
        "rubbish",
        "smoke",
        // Lying snow, and how much of it nobody has ploughed. A republic that
        // is 70% unswept in January is one whose lorries are crawling, and no
        // other column here would say so.
        "snow/unswept"
    );

    let months = years * 12;
    let mut taken = 0usize;
    for _ in 0..months {
        // Tonnage the fleet actually put down this month. The freight column
        // the scalar could never have: it is what the lorries delivered, not
        // what a budget allowed.
        let mut moved = Tonnes::ZERO;
        let mut stuck = 0usize;
        // Crews are transient — a gang goes out, works, and is fetched back —
        // so sampling them at the month boundary reports zero for a republic
        // that was building all month. The same trap a fleet check fell into
        // once already, when it sampled one instant of a fleet that holds a job
        // on 12.9% of ticks and found every lorry parked. Peaks, not instants.
        let mut peak_out = 0u32;
        let mut peak_waiting = 0u32;
        for _ in 0..TICKS_PER_DAY * 30 {
            peak_out = peak_out.max(world.crews().all().iter().map(|p| p.heads).sum::<u32>());
            peak_waiting = peak_waiting.max(world.crews().stranded().map(|p| p.heads).sum::<u32>());
            for m in world.tick() {
                match m {
                    red_republic_sim::systems::Mutation::Unload { tonnes, .. } => moved += tonnes,
                    red_republic_sim::systems::Mutation::Bog { .. } => stuck += 1,
                    _ => {}
                }
            }
        }
        // Take every tender offered. The runner plays nothing else, but an
        // obligation it never accepts is a mechanism it never exercises — and
        // accepting everything is the stress case worth watching, because it is
        // where a plan that quietly stopped working shows up as a fine.
        director.month(&mut world);
        let offers: Vec<_> = world.contracts().offers().map(|c| c.id).collect();
        for id in offers {
            if world
                .issue(Command::AcceptContract { contract: id })
                .is_ok()
            {
                taken += 1;
            }
        }
        let date = world.clock().date();
        let held = |r: Resource| -> Tonnes {
            world
                .buildings()
                .all()
                .iter()
                .map(|b| b.stock.get(r))
                .sum::<Tonnes>()
        };
        let dark = world
            .buildings()
            .all()
            .iter()
            .filter(|b| b.def().power_draw > 0.0 && !b.powered)
            .count();

        // Average provisioning across the estates people actually live in.
        let estates: Vec<f64> = world
            .buildings()
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().residents > 0)
            .filter(|b| !world.population().residents_of(b.id).is_empty())
            .map(|b| b.provisioned)
            .collect();
        let fed = if estates.is_empty() {
            0.0
        } else {
            estates.iter().sum::<f64>() / estates.len() as f64
        };

        // How many of the people who need warming are getting it. The column
        // that says whether a winter is being survived or merely endured.
        let (housed, warm) = world
            .buildings()
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().heat > 0.0)
            .filter(|b| !world.population().residents_of(b.id).is_empty())
            .fold((0u32, 0u32), |(total, ok), b| {
                (total + 1, ok + u32::from(b.heated))
            });
        let warm_share = if housed == 0 {
            1.0
        } else {
            f64::from(warm) / f64::from(housed)
        };

        // How the republic is treating the people in it, weighted by how many
        // live in each estate — one wretched outpost does not cancel a working
        // city, and an unweighted mean would say it did.
        let (scored, heads) = world
            .buildings()
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().residents > 0)
            .fold((0.0f64, 0u32), |(scored, heads), b| {
                let here = world.population().residents_of(b.id).len() as u32;
                (scored + b.content.overall() * f64::from(here), heads + here)
            });
        let content = if heads == 0 {
            0.0
        } else {
            scored / f64::from(heads)
        };

        println!(
            "{:>4}-{:02}-{:02} {:>4} {:>5} {:>4.0}% {:>6.1} {:>5.0}% {:>4.0}% {:>8.0} {:>8.1} {:>6.2} {:>8.0} {:>3}/{:<3} {:>5} {:>8} {:>10.0} {:>9.0} {:>4} {:>8} {:>4.0}% {:>9} {:>7.0} {:>5.0}% {:>4.0}%/{:<4.0}",
            date.year,
            date.month,
            date.day,
            world.population().count(),
            world.population().employed(),
            fed * 100.0,
            world.temperature(),
            world.ground().softness() * 100.0,
            warm_share * 100.0,
            held(Resource::Coal).0,
            held(Resource::Food).0,
            held(Resource::Fuel).0,
            moved.0,
            world.fleet().running(),
            world.fleet().len(),
            stuck,
            if world.roadworks().is_empty() {
                format!(
                    "{:.1}",
                    world
                        .network(red_republic_sim::journey::Medium::Road)
                        .total_length()
                        .as_km()
                )
            } else {
                format!(
                    "{:.1}+{}",
                    world
                        .network(red_republic_sim::journey::Medium::Road)
                        .total_length()
                        .as_km(),
                    world.roadworks().len()
                )
            },
            world
                .geology()
                .remaining_of(red_republic_sim::geology::Mineral::Coal)
                .0,
            // Both purses, because which one a republic earns in is decided
            // by the land now rather than by this runner. Roubles first.
            world.treasury().of(Market::East) + world.treasury().of(Market::West),
            dark,
            format!("{peak_out}/{peak_waiting}"),
            content * 100.0,
            format!(
                "{}/{}/{}",
                world.migration().settled(),
                world.migration().left(),
                world.migration().gave_up()
            ),
            held(Resource::Waste).0,
            // The air over the town, not over the map: an average across six
            // kilometres of empty grass says nothing about a valley with a
            // steel works in it.
            world.lattice().pollution_near(centre) * 100.0,
            world.snow_cover() * 100.0,
            world.roads_unswept() * 100.0,
        );
    }

    let live = world.contracts().active().count();
    let done = world
        .contracts()
        .all()
        .iter()
        .filter(|c| c.state == red_republic_sim::contract::ContractState::Done)
        .count();
    let failed = world
        .contracts()
        .all()
        .iter()
        .filter(|c| c.state == red_republic_sim::contract::ContractState::Failed)
        .count();
    println!();
    println!(
        "tenders: {taken} accepted · {live} running, {done} delivered, {failed} failed · relations east {:.2} west {:.2}",
        world
            .contracts()
            .penalty(red_republic_sim::trade::Market::East),
        world
            .contracts()
            .penalty(red_republic_sim::trade::Market::West),
    );
    println!(
        "commuting: {} of {} workers ride a bus",
        world.population().riders(),
        world.population().employed()
    );
    // The founding builds no hotel, so a republic nobody plays reports zeroes
    // here — which is the point rather than a gap. It is the same shape as the
    // missing bus depot and the missing landfill: a whole earner switched off
    // until the player decides to open it, and a line that says so.
    println!(
        "tourism: {} beds free · {} stayed, {} turned back at the border · ₽ {:.0} / $ {:.0}",
        world.free_beds(),
        world.tourism().visited(),
        world.tourism().turned_away(),
        world
            .tourism()
            .earned(red_republic_sim::trade::Market::East),
        world
            .tourism()
            .earned(red_republic_sim::trade::Market::West),
    );
    // The four ways through, and the fleet on each. Water is here beside the
    // built ones on purpose: it is the one network nobody builds, so a run that
    // reports "water 41 km, 0 vehicles" is reporting an asset the republic has
    // and has not used — which is exactly the sort of thing a balance pass
    // needs to be able to see and a column of production figures cannot say.
    let ways: Vec<String> = red_republic_sim::journey::Medium::ALL
        .into_iter()
        .map(|medium| {
            let fleet = world
                .fleet()
                .all()
                .iter()
                .filter(|v| v.def().medium == medium)
                .count();
            format!(
                "{} {:.1} km / {} vehicles",
                medium.name().to_lowercase(),
                world.network(medium).total_length().as_km(),
                fleet
            )
        })
        .collect();
    println!("ways: {}", ways.join(" · "));

    // Every building the republic ended with, and the state that decides
    // whether it does anything at all.
    //
    // The columns above can only say that production was zero. They cannot say
    // *which* building stopped, and a republic that produces nothing produces
    // nothing for one reason per building — unbuilt, unpowered, unstaffed, or
    // starved of an input. Reading the runner is the point of the runner, and
    // without this the most important question it raises is the one it cannot
    // answer.
    // Sites that are ordered and not yet finished, and what each is short of.
    // A line nobody delivers to looks exactly like a line nobody ordered from
    // every other figure this tool prints.
    let works: Vec<String> = world
        .lineworks()
        .all()
        .iter()
        .map(|l| {
            let short: Vec<String> = l
                .materials()
                .into_iter()
                .filter(|(_, bill)| bill.is_positive())
                .map(|(r, bill)| {
                    format!(
                        "{} {:.0}/{:.0} t",
                        r.name().to_lowercase(),
                        bill.0 - l.material_outstanding(r).0,
                        bill.0
                    )
                })
                .collect();
            format!(
                "  {:<22} {:.0}% built · {}",
                format!("{:?} line", l.kind),
                l.progress() * 100.0,
                if short.is_empty() {
                    "nothing wanted".to_string()
                } else {
                    short.join(" · ")
                }
            )
        })
        .collect();
    if !works.is_empty() {
        println!();
        println!("lines under construction:");
        for line in works {
            println!("{line}");
        }
    }

    println!();
    println!("roster:");
    for b in world.buildings().all() {
        let d = b.def();
        let mut why: Vec<String> = Vec::new();
        if !b.is_built() {
            why.push(format!("{:.0}% built", b.progress() * 100.0));
        }
        if d.power_draw > 0.0 && !b.powered {
            why.push("no power".into());
        }
        if d.heat > 0.0 && !b.heated {
            why.push("cold".into());
        }
        if d.workers > 0 {
            why.push(format!("{}/{} staff", b.staff, d.workers));
        }
        for (r, _) in d.inputs {
            if b.stock.get(*r) == Tonnes::ZERO {
                why.push(format!("no {}", r.name().to_lowercase()));
            }
        }
        println!(
            "  {:<22} {}",
            d.name,
            if why.is_empty() {
                "running".to_string()
            } else {
                why.join(" · ")
            }
        );
    }
}

/// The autopilot: what a competent player would buy, in the order they would
/// buy it.
///
/// **It exists because condition 3 became unmeasurable the day the map went
/// empty.** "A republic survives a decade" is a question about a republic, and
/// a runner that stands the old founding hand up is measuring a fixture — so
/// every figure this tool printed described a town nobody is given. It has to
/// play the opening or it is not reading the game.
///
/// It is deliberately dumb: one thing at a time, in a fixed order, paid for
/// however it can be. A cleverer director would be a better player and a worse
/// instrument, because what this is for is finding out whether the opening is
/// *possible* rather than how well it can be played. If a fixed sensible order
/// cannot get a republic off the ground, no player order will.
struct Director {
    centre: Point,
    /// How far through [`Director::PLAN`] it has got.
    step: usize,
    /// Whether builders have been hired from abroad yet.
    hired: bool,
    /// Whether the track to the crossing has been ordered.
    road: bool,
    /// Whether coal has been put on sale.
    selling: bool,
    /// Whether the standing import rules have been set.
    buying: bool,
    /// How many months running the current step has been refused.
    stuck: u32,
    /// What it has said out loud, so a decade does not print the same line
    /// three hundred times.
    said: Vec<String>,
}

impl Director {
    /// The order a republic gets built in, and every entry is load-bearing.
    ///
    /// A **Construction Office** first, because everything after it is cheaper
    /// once the republic has crews of its own — contractors cost several times
    /// what your own builders do. A **Motor Depot** second, because a republic
    /// without lorries starves beside its own full bins. Then somewhere to
    /// live, power, and a **Customs House**, which is the only way a rouble
    /// ever comes back in.
    const PLAN: &'static [BuildingKind] = &[
        BuildingKind::ConstructionOffice,
        BuildingKind::MotorDepot,
        BuildingKind::Apartment,
        BuildingKind::CoalMine,
        BuildingKind::PowerPlant,
        BuildingKind::TransformerStation,
        BuildingKind::Customs,
        // The materials chain, and it comes early for a reason the first run
        // found the hard way: until the republic can make gravel, brick and
        // planks, a site it builds *itself* has nothing to be built out of, and
        // every one of them stalls. See `MATERIALS` below.
        BuildingKind::GravelQuarry,
        BuildingKind::Woodcutter,
        BuildingKind::Sawmill,
        BuildingKind::Brickworks,
        BuildingKind::Store,
        BuildingKind::Farm,
        BuildingKind::FoodFactory,
        BuildingKind::HeatingPlant,
        BuildingKind::BusDepot,
        BuildingKind::Apartment,
        BuildingKind::Clinic,
        BuildingKind::School,
    ];

    /// What the republic has to be able to make before its own crews are worth
    /// using.
    ///
    /// **The first run switched to own crews the moment the office had staff,
    /// and everything after that stalled.** Contractors bring their own
    /// materials; your crews do not, so a republic that owns no quarry, no
    /// sawmill and no brickworks puts down foundations nothing will ever
    /// deliver to. Three buildings in three years, against eleven when it kept
    /// paying. A player would have seen it in a week.
    /// What the republic buys in until it can make it, and how much it keeps at
    /// the crossing.
    ///
    /// A stock level rather than an order: `Buy { up_to }` tops the customs
    /// house back up to this figure as lorries take it away, which is what a
    /// standing import policy is. The figures are a few weeks of consumption at
    /// the rates the building table authors — enough that a lorry breaking down
    /// does not stop the plant, small enough that the grant is not spent on a
    /// mountain of coal in month one.
    const IMPORTS: &'static [(Resource, f64)] = &[
        (Resource::Food, 120.0),
        (Resource::Coal, 200.0),
        (Resource::Fuel, 60.0),
        (Resource::Machinery, 20.0),
        // Six tonnes a kilometre, and without it the grid is an order that
        // never becomes a wire: the first span was accepted, sat as a site for
        // ten years waiting for steel that was never bought, and every building
        // in the republic read `no power` beside a fully staffed power station.
        (Resource::Steel, 80.0),
    ];

    const MATERIALS: &'static [BuildingKind] = &[
        BuildingKind::GravelQuarry,
        BuildingKind::Sawmill,
        BuildingKind::Brickworks,
    ];

    fn new(centre: Point) -> Self {
        Self {
            centre,
            step: 0,
            hired: false,
            road: false,
            selling: false,
            buying: false,
            stuck: 0,
            said: Vec::new(),
        }
    }

    fn say(&mut self, line: String) {
        if !self.said.contains(&line) {
            println!("  {line}");
            self.said.push(line);
        }
    }

    /// One month of decisions.
    fn month(&mut self, world: &mut World) {
        self.build_next(world);
        self.hire(world);
        self.trade(world);
        self.grid(world);
    }

    /// String the power line, and then keep stringing it.
    ///
    /// **A plant that is not wired to anything lights nothing, including
    /// itself**, and this runner never laid a single span — so a republic that
    /// had bought a power station, a transformer and a coal mine sat dark for
    /// ten years, and the mine never cut a tonne because a mine draws six
    /// megawatts. Every building read `no power` while a fully staffed plant
    /// stood beside them.
    ///
    /// One span a month, and the town centre is the hub. That is not the
    /// cheapest grid a player could draw, but a star from the middle of the town
    /// is what somebody actually builds, and this is meant to be a plain player
    /// rather than a good one. A span is a *site* like any other — it wants
    /// steel and builder-days — so ordering one at a time is also what keeps
    /// the queue honest.
    fn grid(&mut self, world: &mut World) {
        use red_republic_sim::utility::Utility;

        // Nothing to carry until something makes current.
        let Some(plant) = world
            .buildings()
            .all()
            .iter()
            .find(|b| b.def().power_output > 0.0 && b.is_built())
            .map(|b| (b.id, b.centre))
        else {
            return;
        };
        // One span under construction at a time.
        if !world.lineworks().is_empty() {
            return;
        }
        // The plant to the hub first: until that exists there is no network for
        // anything else to join.
        if world
            .utilities()
            .network_of(plant.0, Utility::Power)
            .is_none()
        {
            match world.issue(Command::OrderLine {
                kind: Utility::Power,
                from: plant.1,
                to: self.centre,
            }) {
                Ok(_) => self.say("stringing the grid from the power station".into()),
                Err(why) => self.say(format!("no line from the power station: {why}")),
            }
            return;
        }
        // Then out to whatever draws and is not yet plugged in, nearest first,
        // so the grid grows outward rather than leaping across the map.
        let mut waiting: Vec<_> = world
            .buildings()
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().power_draw > 0.0)
            .filter(|b| world.utilities().network_of(b.id, Utility::Power).is_none())
            .map(|b| (b.centre, b.def().name))
            .collect();
        waiting.sort_by(|a, b| {
            self.centre
                .distance_to(a.0)
                .0
                .total_cmp(&self.centre.distance_to(b.0).0)
        });
        let Some(&(at, name)) = waiting.first() else {
            return;
        };
        match world.issue(Command::OrderLine {
            kind: Utility::Power,
            from: self.centre,
            to: at,
        }) {
            Ok(_) => self.say(format!("stringing the grid out to the {name}")),
            Err(why) => self.say(format!("no line to the {name}: {why}")),
        }
    }

    /// Put the next thing on the plan up, if there is nothing already going.
    ///
    /// **One site at a time**, which is what a republic with one office and one
    /// gang can actually work — queuing five would only mean four of them
    /// standing empty, and would hide whether the first one ever finished.
    fn build_next(&mut self, world: &mut World) {
        if self.step >= Self::PLAN.len() {
            return;
        }
        // **Two on the go, not one.** The first version waited for every site to
        // finish, so a single site that could never finish — one the republic
        // ordered itself with no materials to build it from — stopped the plan
        // dead for the rest of the run and every column after it printed a flat
        // line. Two is still a queue a single office can work, and it means a
        // stuck foundation costs the republic momentum rather than everything.
        let going = world
            .buildings()
            .all()
            .iter()
            .filter(|b| !b.is_built())
            .count();
        if going >= 2 {
            return;
        }
        let kind = Self::PLAN[self.step];
        // **A customs house goes to the border, not to the town.** It is the
        // one building in the table that has to stand somewhere the republic
        // did not choose, and looking for a site near the centre found ground
        // that would take it and a command that refused it — which stalled the
        // whole plan on step seven for two and a half simulated years, with the
        // director printing one line about it and then nothing at all.
        // **And it goes to an EASTERN post, because the grant is roubles.**
        // This asked for the nearest crossing of any bloc, and on most seeds
        // that is a Western one — where a purse holding 2,500,000 roubles and
        // no dollars can buy precisely nothing. Trade then failed silently in
        // both directions for a decade: `affordable` came out zero on every
        // tick, no goods ever crossed, and the republic sat inert with a
        // customs house it could not use. `scenario::GRANT_ROUBLES` says which
        // bloc's posts the land reaches is the first thing that matters about a
        // posting, and until now the one thing that plays the opening ignored
        // it. Falling back to any post keeps a republic with no Eastern
        // crossing playable, and says so rather than pretending.
        let around = if kind == BuildingKind::Customs {
            let east = world
                .frontier()
                .nearest_crossing(self.centre, Some(Market::East));
            if east.is_none() {
                self.say("no Eastern post in reach — this posting must earn dollars first".into());
            }
            match east.or_else(|| world.frontier().nearest_crossing(self.centre, None)) {
                Some(post) => post.at,
                None => {
                    self.say("this republic has no frontier post at all".into());
                    self.step += 1;
                    return;
                }
            }
        } else {
            self.centre
        };
        let Some(at) = scenario::find_site(world, kind, around, Metres(1_400.0)) else {
            self.say(format!("nowhere within reach takes a {}", kind.def().name));
            self.step += 1;
            return;
        };
        // Build it with your own crews if there is an office with people in it,
        // and pay a Bloc firm otherwise. That is the whole shape of the opening
        // and the reason contractors exist at all.
        let own = world
            .buildings()
            .all()
            .iter()
            .any(|b| b.kind == BuildingKind::ConstructionOffice && b.is_built() && b.staff > 0)
            && Self::MATERIALS.iter().all(|&want| {
                world
                    .buildings()
                    .all()
                    .iter()
                    .any(|b| b.kind == want && b.is_built() && b.staff > 0)
            });
        let outcome = if own {
            world.issue(Command::Place { kind, at })
        } else {
            world.issue(Command::ContractBuild {
                kind,
                at,
                market: Market::East,
            })
        };
        match outcome {
            Ok(_) => {
                self.say(format!(
                    "{} a {}",
                    if own { "building" } else { "contracting" },
                    kind.def().name
                ));
                self.step += 1;
                self.stuck = 0;
            }
            Err(why) => {
                // **Give up on it rather than trying for ever.** The first
                // version stayed on a refused step, so one building the
                // director could not site stopped the plan dead and every
                // column after it printed a flat line that read as balance.
                // A player would move on; so does this.
                self.say(format!("cannot start a {}: {why}", kind.def().name));
                self.stuck += 1;
                if self.stuck >= 3 {
                    self.stuck = 0;
                    self.step += 1;
                }
            }
        }
    }

    /// Buy builders from the Eastern Bloc once there is an office to put them
    /// in. A blank map has nobody at all, so the first crews are always bought.
    fn hire(&mut self, world: &mut World) {
        if self.hired {
            return;
        }
        let Some(office) = world
            .buildings()
            .all()
            .iter()
            .find(|b| b.kind == BuildingKind::ConstructionOffice && b.is_built())
            .map(|b| b.id)
        else {
            return;
        };
        match world.issue(Command::HireForeign {
            market: Market::East,
            office,
            heads: 20,
        }) {
            Ok(_) => {
                self.hired = true;
                self.say("hired twenty builders from the Eastern Bloc".into());
            }
            Err(why) => self.say(format!("cannot hire abroad: {why}")),
        }
    }

    /// Sell coal, and lay a track to the post that clears it.
    ///
    /// **Not a fixed market.** A customs house clears only for the bloc whose
    /// post it stands at, and which post is nearest is decided by the land.
    /// Selling east from a Western crossing earns nothing at all — which is
    /// what this runner reported, in its first run after trade became
    /// geographic, as a flat zero in the roubles column for two years.
    fn trade(&mut self, world: &mut World) {
        let Some(house) = world
            .buildings()
            .all()
            .iter()
            .find(|b| b.kind == BuildingKind::Customs && b.is_built())
            .map(|b| b.centre)
        else {
            return;
        };
        let bloc = world.bloc_near(house);
        // **The opening is an import problem, and this runner never once
        // treated it as one.** A coal mine draws six megawatts, a power plant
        // burns coal to make them, and a republic founded on empty ground has
        // neither — so the circle cannot be broken from the inside and the
        // first tonne has to come over the border. Fuel and machinery are the
        // same shape: the construction office and both depots consume them from
        // the day they open, and nothing in the republic makes either for
        // years. Without these three rules the republic is inert for a decade,
        // which is exactly what it was.
        if !self.buying {
            let mut all = true;
            for &(resource, up_to) in Self::IMPORTS {
                if world
                    .issue(Command::AddTradeRule {
                        resource,
                        market: bloc,
                        action: TradeAction::Buy {
                            up_to: Tonnes(up_to),
                        },
                    })
                    .is_err()
                {
                    all = false;
                }
            }
            if all {
                self.buying = true;
                self.say(format!("buying coal, fuel and machinery from the {bloc:?}"));
            }
        }
        // Coal goes on sale only once the republic actually digs it. Selling it
        // from the day the house opens is what the old plan did, and it is
        // worse than useless now that coal is also bought: the sell rule and
        // the buy rule meet at the same customs house, so the republic would
        // buy a tonne over the border and sell it straight back.
        if !self.selling
            && world
                .buildings()
                .all()
                .iter()
                .any(|b| b.kind == BuildingKind::CoalMine && b.is_built() && b.staff > 0)
            && world
                .issue(Command::AddTradeRule {
                    resource: Resource::Coal,
                    market: bloc,
                    action: TradeAction::Sell,
                })
                .is_ok()
        {
            self.selling = true;
            self.say(format!("selling coal to the {bloc:?}ern bloc"));
        }
        if !self.road {
            match world.issue(Command::OrderRoad {
                from: self.centre,
                to: house,
                grade: Grade::Dirt,
                lamps: false,
            }) {
                Ok(_) => {
                    self.road = true;
                    self.say(format!(
                        "ordered a dirt track to the crossing: {:.1} km",
                        self.centre.distance_to(house).as_km()
                    ));
                }
                Err(why) => self.say(format!("no track to the crossing: {why}")),
            }
        }
    }
}
