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
//! cargo run --release --bin trajectory -- [seed] [years]
//! ```

use red_republic_sim::resource::Resource;
use red_republic_sim::scenario;
use red_republic_sim::time::TICKS_PER_DAY;
use red_republic_sim::units::{Metres, Tonnes};
use red_republic_sim::world::{World, WorldSpec};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1961);
    let years: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(3);

    let mut world = World::new(WorldSpec {
        seed,
        extent: Metres(6_000.0),
    });
    let base = scenario::found(&mut world, 120);

    println!("Red Republic — trajectory");
    println!("seed {seed} · {years} years · 6 km republic");
    println!(
        "founded at ({:.0} m, {:.0} m) · {} housing · mine {} · plant {}",
        base.centre.x.0,
        base.centre.y.0,
        base.housing.len(),
        if base.mine.is_some() { "yes" } else { "NO" },
        if base.plant.is_some() { "yes" } else { "NO" },
    );
    println!(
        "shop {} · office {} · crossing {}",
        if base.store.is_some() { "yes" } else { "NO" },
        if base.construction_office.is_some() {
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
        "{:>10} {:>4} {:>5} {:>5} {:>8} {:>8} {:>10} {:>9} {:>4}",
        "date", "pop", "empl", "fed%", "coal", "food", "coal left", "roubles", "dark"
    );

    let months = years * 12;
    for _ in 0..months {
        for _ in 0..TICKS_PER_DAY * 30 {
            world.tick();
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

        println!(
            "{:>4}-{:02}-{:02} {:>4} {:>5} {:>4.0}% {:>8.0} {:>8.1} {:>10.0} {:>9.0} {:>4}",
            date.year,
            date.month,
            date.day,
            world.population.count(),
            world.population.employed(),
            fed * 100.0,
            held(Resource::Coal).0,
            held(Resource::Food).0,
            world
                .geology
                .remaining_of(red_republic_sim::geology::Mineral::Coal)
                .0,
            world.treasury.rubles,
            dark,
        );
    }
}
