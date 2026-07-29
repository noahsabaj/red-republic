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

use red_republic_sim::climate::ClimateId;
use red_republic_sim::command::Command;
use red_republic_sim::resource::Resource;
use red_republic_sim::scenario;
use red_republic_sim::time::TICKS_PER_DAY;
use red_republic_sim::units::{Metres, Tonnes};
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
    // The founding hand, rather than a number this runner picked. It was 120
    // here and the founding offered more jobs than that, so the tail of the
    // priority order — the customs house — stood empty and the money column
    // printed a flat zero for a decade. That read as balance.
    let base = scenario::found(&mut world, scenario::SETTLERS);

    println!("Red Republic — trajectory");
    println!(
        "seed {seed} · {years} years · 6 km republic · {}",
        climate.def().name
    );
    println!(
        "founded at ({:.0} m, {:.0} m) · {} housing · mine {} · plant {}",
        base.centre.x.0,
        base.centre.y.0,
        base.housing.len(),
        if base.mine.is_some() { "yes" } else { "NO" },
        if base.plant.is_some() { "yes" } else { "NO" },
    );
    println!(
        "shop {} · office {} · garage {} · crossing {}",
        if base.store.is_some() { "yes" } else { "NO" },
        if base.construction_office.is_some() {
            "yes"
        } else {
            "NO"
        },
        if base.motor_depot.is_some() {
            "yes"
        } else {
            "NO"
        },
        if base.customs.is_some() { "yes" } else { "NO" },
    );
    // Sell surplus coal to whichever bloc this republic's crossing belongs to;
    // that is how it earns anything.
    //
    // **Not a fixed market.** A customs house clears only for the bloc whose
    // frontier post it stands at, and which post the founding opens is decided
    // by the land. Selling east from a Western crossing earns nothing at all —
    // which is what this runner reported, in its first run after trade became
    // geographic, as a flat zero in the roubles column for two years.
    //
    // Issued as a command rather than written into the world, because this
    // runner is held to exactly the boundary the shell is: it is a separate
    // crate, so every field on `World` is out of its reach and the only way it
    // can change anything is to ask. That is the point of running it this way —
    // if the player API is awkward, the trajectory runner finds out first.
    let bloc = base
        .customs
        .and_then(|id| world.buildings().get(id).map(|b| b.centre))
        .map(|at| world.bloc_near(at))
        .unwrap_or(red_republic_sim::trade::Market::East);
    println!("the crossing is on the {bloc:?}ern frontier; selling coal there");
    world
        .issue(Command::AddTradeRule {
            resource: Resource::Coal,
            market: bloc,
            action: red_republic_sim::trade::TradeAction::Sell,
        })
        .expect("adding a trade rule cannot fail");
    // And order a track out to the crossing, which is the one long haul a
    // founded republic makes. A dirt track because there is no gravel quarry
    // in the founding — the cheapest road there is, and still weeks of the
    // crew's time, which is the trade the column below is here to show.
    if let Some(customs) = base.customs {
        let crossing = world.buildings().get(customs).expect("just founded").centre;
        match world.issue(Command::OrderRoad {
            from: base.centre,
            to: crossing,
            grade: red_republic_sim::roadworks::Grade::Dirt,
        }) {
            Ok(_) => println!(
                "ordered a dirt track to the crossing: {:.1} km",
                base.centre.distance_to(crossing).as_km()
            ),
            // The refusal prints its own reason rather than a Debug dump,
            // which is the whole argument for commands carrying one.
            Err(why) => println!("no track to the crossing: {why}"),
        }
    }
    println!(
        "coal in the ground at founding: {:.0} t",
        world
            .geology()
            .remaining_of(red_republic_sim::geology::Mineral::Coal)
            .0
    );
    println!();
    println!(
        "{:>10} {:>4} {:>5} {:>5} {:>6} {:>6} {:>5} {:>8} {:>8} {:>6} {:>8} {:>7} {:>5} {:>8} {:>10} {:>9} {:>4} {:>8} {:>5} {:>9} {:>7} {:>6}",
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
        "smoke"
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
            "{:>4}-{:02}-{:02} {:>4} {:>5} {:>4.0}% {:>6.1} {:>5.0}% {:>4.0}% {:>8.0} {:>8.1} {:>6.2} {:>8.0} {:>3}/{:<3} {:>5} {:>8} {:>10.0} {:>9.0} {:>4} {:>8} {:>4.0}% {:>9} {:>7.0} {:>5.0}%",
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
            world.treasury().of(bloc),
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
            world.lattice().pollution_near(base.centre) * 100.0,
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
}
