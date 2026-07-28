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
    let base = scenario::found(&mut world, 120);

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
    // Sell surplus coal east; that is how a republic earns anything.
    world.trade_policy = red_republic_sim::trade::TradePolicy::new()
        .sell(Resource::Coal, red_republic_sim::trade::Market::East);
    println!(
        "coal in the ground at founding: {:.0} t",
        world
            .geology
            .remaining_of(red_republic_sim::geology::Mineral::Coal)
            .0
    );
    println!();
    println!(
        "{:>10} {:>4} {:>5} {:>5} {:>6} {:>5} {:>8} {:>8} {:>6} {:>8} {:>7} {:>10} {:>9} {:>4}",
        "date",
        "pop",
        "empl",
        "fed%",
        "degC",
        "warm%",
        "coal",
        "food",
        "fuel",
        "moved",
        "lorries",
        "coal left",
        "roubles",
        "dark"
    );

    let months = years * 12;
    let mut taken = 0usize;
    for _ in 0..months {
        // Tonnage the fleet actually put down this month. The freight column
        // the scalar could never have: it is what the lorries delivered, not
        // what a budget allowed.
        let mut moved = Tonnes::ZERO;
        for _ in 0..TICKS_PER_DAY * 30 {
            for m in world.tick() {
                if let red_republic_sim::systems::Mutation::Unload { tonnes, .. } = m {
                    moved += tonnes;
                }
            }
        }
        // Take every tender offered. The runner plays nothing else, but an
        // obligation it never accepts is a mechanism it never exercises — and
        // accepting everything is the stress case worth watching, because it is
        // where a plan that quietly stopped working shows up as a fine.
        let offers: Vec<_> = world.contracts.offers().map(|c| c.id).collect();
        for id in offers {
            if world.contracts.accept(id) {
                taken += 1;
            }
        }
        let date = world.clock.date();
        let held = |r: Resource| -> Tonnes {
            world
                .buildings
                .all()
                .iter()
                .map(|b| b.stock.get(r))
                .sum::<Tonnes>()
        };
        let dark = world
            .buildings
            .all()
            .iter()
            .filter(|b| b.def().power_draw > 0.0 && !b.powered)
            .count();

        // Average provisioning across the estates people actually live in.
        let estates: Vec<f64> = world
            .buildings
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().residents > 0)
            .filter(|b| !world.population.residents_of(b.id).is_empty())
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
            .buildings
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().heat > 0.0)
            .filter(|b| !world.population.residents_of(b.id).is_empty())
            .fold((0u32, 0u32), |(total, ok), b| {
                (total + 1, ok + u32::from(b.heated))
            });
        let warm_share = if housed == 0 {
            1.0
        } else {
            f64::from(warm) / f64::from(housed)
        };

        println!(
            "{:>4}-{:02}-{:02} {:>4} {:>5} {:>4.0}% {:>6.1} {:>4.0}% {:>8.0} {:>8.1} {:>6.2} {:>8.0} {:>3}/{:<3} {:>10.0} {:>9.0} {:>4}",
            date.year,
            date.month,
            date.day,
            world.population.count(),
            world.population.employed(),
            fed * 100.0,
            world.temperature(),
            warm_share * 100.0,
            held(Resource::Coal).0,
            held(Resource::Food).0,
            held(Resource::Fuel).0,
            moved.0,
            world.fleet.running(),
            world.fleet.len(),
            world
                .geology
                .remaining_of(red_republic_sim::geology::Mineral::Coal)
                .0,
            world.treasury.rubles,
            dark,
        );
    }

    let live = world.contracts.active().count();
    let done = world
        .contracts
        .all()
        .iter()
        .filter(|c| c.state == red_republic_sim::contract::ContractState::Done)
        .count();
    let failed = world
        .contracts
        .all()
        .iter()
        .filter(|c| c.state == red_republic_sim::contract::ContractState::Failed)
        .count();
    println!();
    println!(
        "tenders: {taken} accepted · {live} running, {done} delivered, {failed} failed · relations east {:.2} west {:.2}",
        world
            .contracts
            .penalty(red_republic_sim::trade::Market::East),
        world
            .contracts
            .penalty(red_republic_sim::trade::Market::West),
    );
    println!(
        "commuting: {} of {} workers ride a bus",
        world.population.riders(),
        world.population.employed()
    );
}
