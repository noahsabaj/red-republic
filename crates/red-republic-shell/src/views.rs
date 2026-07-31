//! The panel-facing reads: what the UI asks the republic about itself.
//!
//! [`crate::marshal`] is geometry — where things are, so they can be drawn.
//! This is everything else: what is in a yard, why a building has stopped, what
//! the weather will do, how a crossing sits before a lorry is committed to it.
//!
//! # Nothing here computes
//!
//! Every number is read from a view the simulation already owns. That rule is
//! load-bearing rather than tidy: the archived build's UI never re-derived
//! simulation maths either, and the moment a panel starts working out its own
//! answer there are two versions of the balance and only one of them is tested.
//! If a panel needs a number that does not exist, the number gets added to the
//! simulation — not to this file.
//!
//! # Bulk goes packed
//!
//! Same measured rule as the geometry: a dictionary per entity cost 8,640 µs at
//! 1,205 buildings against 27 µs for a flat array. Anything that scales with
//! the size of the republic comes back as `PackedFloat32Array` with a stride
//! documented on the function. Single values come back as themselves, because a
//! raw call is 0.21 µs and a chatty small interface is free.

use godot::prelude::*;
use red_republic_sim::journey::Medium;
use red_republic_sim::resource::Resource;
use red_republic_sim::units::Point;
use red_republic_sim::{BuildingId, Metres, World};

/// Floats per deposit in [`deposits`].
pub const DEPOSIT_STRIDE: usize = 8;

/// Every body of mineral the survey has found.
///
/// `[mineral, x, y, radius, top, remaining, initial, working_depth]` per body.
/// Read from `Geology::survey`, which is the engine-owned view the founding
/// screen already reads — so a card and an overlay cannot disagree about what
/// is under the ground.
///
/// Resources are invisible on the terrain by design. This is the overlay that
/// makes them legible, and without it the whole three-dimensional subsurface is
/// something the simulation knows and the player cannot.
pub fn deposits(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for reading in world.geology().survey() {
        out.push(mineral_index(reading.mineral) as f32);
        out.push(reading.centre.x.0 as f32);
        out.push(reading.centre.y.0 as f32);
        out.push(reading.radius.0 as f32);
        out.push(reading.top.0 as f32);
        out.push(reading.remaining.0 as f32);
        out.push(reading.initial.0 as f32);
        out.push(reading.working_depth.0 as f32);
    }
    out
}

fn mineral_index(m: red_republic_sim::Mineral) -> usize {
    red_republic_sim::Mineral::ALL
        .iter()
        .position(|&x| x == m)
        .unwrap_or(0)
}

/// How hard the going is over the whole traversal lattice, row-major.
///
/// One value per lattice cell, **`0.0` firm to `1.0` impassable** — a badness,
/// not a quality, and the direction is the whole trap. An earlier version of
/// this comment said the opposite, the overlay ramp was built from the comment
/// rather than the source, and a bone-dry July map came out painted entirely
/// red. `going_is_a_badness_and_not_a_quality` pins it where the meaning lives.
/// The lattice is
/// 100 m where the terrain is 10 m — ten thousand cells on a 10 km map against
/// the terrain's million — which is exactly what makes an overlay of it cheap
/// enough to rebuild whenever the ground changes.
///
/// The ground being state rather than calendar is one of the simulation's
/// sharper ideas and it was entirely invisible: the worst going of the year
/// arrives a few weeks into spring, on its own, and nobody could see it.
pub fn going_field(world: &World) -> PackedFloat32Array {
    let lattice = world.lattice();
    let crossing = world.crossing();
    let cells = lattice.cells();
    let mut out = PackedFloat32Array::new();
    for y in 0..cells {
        for x in 0..cells {
            let index = (y * cells + x) as usize;
            out.push(crossing.going_in(index) as f32);
        }
    }
    out
}

/// How worn each lattice cell is, row-major, `0.0` untouched to `1.0` a made
/// track.
///
/// Traffic packs the ground it crosses and a corridor past the threshold is
/// promoted into the road network. Showing the wear is what turns that from a
/// road appearing out of nowhere into a road you watched form.
pub fn wear_field(world: &World) -> PackedFloat32Array {
    let lattice = world.lattice();
    let cells = lattice.cells();
    let mut out = PackedFloat32Array::new();
    for y in 0..cells {
        for x in 0..cells {
            out.push(lattice.wear_at((y * cells + x) as usize) as f32);
        }
    }
    out
}

/// Floats per road site in [`road_sites`].
pub const SITE_STRIDE: usize = 6;

/// Roads ordered and not yet drivable.
///
/// `[ax, ay, bx, by, progress, speed_kph]`. Nothing routes over a site, so a
/// player looking at a half-built road needs to see that it is half-built
/// rather than wonder why no lorry will use it.
pub fn road_sites(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for site in world.roadworks().all() {
        out.push(site.from.x.0 as f32);
        out.push(site.from.y.0 as f32);
        out.push(site.to.x.0 as f32);
        out.push(site.to.y.0 as f32);
        out.push(site.progress() as f32);
        out.push((site.grade.def().speed.as_mps() * 3.6) as f32);
    }
    out
}

/// Floats per crew in [`crew_parties`].
pub const CREW_STRIDE: usize = 5;

/// Every building crew the republic has out.
///
/// `[x, y, heads, state, office]` per gang, where state is 0 riding a bus,
/// 1 working a site, 2 standing waiting for a lift.
///
/// This has to be visible or the whole construction rework is invisible: work
/// happens where the builders are, so a site with nobody on it looks identical
/// to a site with a gang on it and no materials. The waiting state is the one
/// that matters most — a gang standing beside a finished building is people the
/// office cannot post anywhere until a bus fetches them.
pub fn crew_parties(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for party in world.crews().all() {
        // Where a working gang is standing comes from the site itself rather
        // than from a copy on the party, so a panel and the map can never
        // disagree about where the work is.
        let at = party
            .working
            .and_then(|site| world.place_of(site))
            .unwrap_or(party.at);
        out.push(at.x.0 as f32);
        out.push(at.y.0 as f32);
        out.push(party.heads as f32);
        out.push(match (party.riding, party.working) {
            (Some(_), _) => 0.0,
            (None, Some(_)) => 1.0,
            (None, None) => 2.0,
        });
        out.push(party.office.0 as f32);
    }
    out
}

/// What every yard in the republic is holding, by resource.
///
/// One total per `Resource::ALL`, so the stockpile table is a single read of
/// thirteen floats rather than a walk over every building from GDScript.
pub fn stockpiles(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for resource in Resource::ALL {
        let held: f64 = world
            .buildings()
            .all()
            .iter()
            .map(|b| b.stock.get(resource).0)
            .sum();
        // **A stockpile is a quantity, so it leaves here non-negative.** Every
        // row of the HUD's stockpile table read `-0.0 t` on a republic holding
        // nothing at all — a negative zero, which `print` hides and `%.1f`
        // faithfully signs. `absf()` and `+ 0.0` in GDScript both made it go
        // away, which is what identified it; `signf()` does not distinguish, so
        // it looked for a while like the formatter's fault rather than the
        // value's.
        //
        // `max` rather than a formatting guard in the panel, because the panel
        // is not the only thing that will ever read this and a tonnage that can
        // arrive signed is a fact about the view, not about the label. In Rust
        // `(-0.0f64).max(0.0)` is `+0.0`, so this normalises the sign and
        // clamps any genuine negative in the same move.
        out.push(held.max(0.0) as f32);
    }
    out
}

/// The weather ahead: `[temperature_c, rain_mm]` per day, starting today.
///
/// Temperature is a pure function of `(seed, climate, day)` drawn from its own
/// substream, so asking about a future day perturbs nothing and costs nothing.
/// Heating demand follows **today's temperature and never the month**, which is
/// what makes a cold snap something a republic can be caught out by — and a
/// forecast is the only thing that makes being caught out feel like a mistake
/// rather than an ambush.
pub fn forecast(world: &World, days: u64) -> PackedFloat32Array {
    let today = world.clock().day_index();
    let mut out = PackedFloat32Array::new();
    for offset in 0..days {
        let (temperature, rain) = world.weather_on_day(today + offset);
        out.push(temperature as f32);
        out.push(rain as f32);
    }
    out
}

/// Going at one point, for a placement or a route the player is considering.
pub fn going_at(world: &World, x: f64, y: f64) -> f64 {
    world.going_at(Point::new(Metres(x), Metres(y)))
}

/// How far a point is from foreign soil.
pub fn distance_to_border(world: &World, x: f64, y: f64) -> f64 {
    world.distance_to_border(Point::new(Metres(x), Metres(y))).0
}

/// Floats per frontier sample in [`frontier_line`].
pub const FRONTIER_STRIDE: usize = 4;

/// The frontier as a polyline: `[x, y, bloc, along]` per sample.
///
/// Sampled around the perimeter rather than handed over as arcs, so the shell
/// draws a line without knowing how the frontier is parameterised. `bloc` is 0
/// East, 1 West.
///
/// This has to be visible. The whole perimeter is border and the two blocs hold
/// different stretches of it, so "which way is west" is a fact about the land
/// that decides which currency a republic earns — and a player who cannot see
/// it is guessing at the most consequential thing about their posting.
pub fn frontier_line(world: &World, samples: usize) -> PackedFloat32Array {
    let frontier = world.frontier();
    let mut out = PackedFloat32Array::new();
    for i in 0..=samples {
        let along = red_republic_sim::Frontier::TURNS * i as f64 / samples as f64;
        let at = frontier.point_at(along);
        out.push(at.x.0 as f32);
        out.push(at.y.0 as f32);
        out.push(bloc_index(frontier.bloc_at(along)) as f32);
        out.push(along as f32);
    }
    out
}

/// Floats per post in [`crossings`].
pub const CROSSING_STRIDE: usize = 4;

/// The frontier posts: `[x, y, bloc, id]`.
///
/// Placed at worldgen — you do not build one, you build road out to one. That
/// makes them the most important thing on an unexplored map after the geology,
/// and they need to be on it from the first frame.
pub fn crossings(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for crossing in world.frontier().crossings() {
        out.push(crossing.at.x.0 as f32);
        out.push(crossing.at.y.0 as f32);
        out.push(bloc_index(crossing.bloc) as f32);
        out.push(crossing.id.0 as f32);
    }
    out
}

/// 0 East, 1 West. Kept in one place so the shell never has to match on a
/// simulation enum.
pub fn bloc_index(bloc: red_republic_sim::Market) -> usize {
    match bloc {
        red_republic_sim::Market::East => 0,
        red_republic_sim::Market::West => 1,
    }
}

/// Who the republic is made of: life stages then education levels.
///
/// `[infants, pupils, students, workers, retired, unschooled, schooled,
/// graduates]`, in the order `LifeStage::ALL` and `Education::ALL` declare.
///
/// This is the whole of the demographic model made visible in one read. A
/// republic whose pupils outnumber its workers is a republic about to be short
/// of hands, and a republic with no graduates is one that cannot open a
/// refinery — neither of which is legible from a population count.
pub fn demographics(world: &World) -> PackedInt32Array {
    let mut out = PackedInt32Array::new();
    for count in world.population().by_stage() {
        out.push(count as i32);
    }
    for count in world.population().by_education() {
        out.push(count as i32);
    }
    out
}

/// How the republic is treating its people, component by component.
///
/// One float per `Contentment::NAMES`, then **`overall`, then the comfort
/// lift** — each `0.0..=1.0`, averaged over homes and **weighted by how many
/// people live in them**: one wretched outpost does not cancel a working city,
/// and an equal average over blocks would say it did.
///
/// The breakdown rather than the score, because "your people are at 61%" is not
/// something a player can act on and "fed, warm, no doctor, no work" is.
///
/// The lift comes back apart from the components on purpose. Drink and
/// household electrics are not a way to fail — they add on top — so a panel
/// showing them in the same column as Warmth would be inviting a player to fix
/// the wrong thing.
pub fn contentment(world: &World) -> PackedFloat32Array {
    let census = world.population().census_by_home();
    // Sized from the roster rather than from a number typed here. A literal
    // `6` outlived the sixth component by exactly one milestone: `zip` stopped
    // at six, the array came back one short of what `contentment_names` said,
    // and the panel that checked the two agreed showed nothing at all.
    let mut totals = [0.0f64; red_republic_sim::Contentment::NAMES.len()];
    let mut overall = 0.0;
    let mut lift = 0.0;
    let mut heads = 0u32;
    for building in world.buildings().all() {
        if !building.is_built() || building.def().residents == 0 {
            continue;
        }
        let here = census.get(&building.id).copied().unwrap_or_default();
        if here.residents == 0 {
            continue;
        }
        let weight = f64::from(here.residents);
        for (total, part) in totals.iter_mut().zip(building.content.parts()) {
            *total += part * weight;
        }
        overall += building.content.overall() * weight;
        lift += building.content.lift() * weight;
        heads += here.residents;
    }
    let mut out = PackedFloat32Array::new();
    if heads == 0 {
        // Sized from the roster plus the two trailing figures, never typed. A
        // literal `6` here outlived the sixth component by exactly one
        // milestone and the panel that checked the lengths agreed showed
        // nothing at all.
        for _ in 0..(red_republic_sim::Contentment::NAMES.len() + 2) {
            out.push(0.0);
        }
        return out;
    }
    for total in totals {
        out.push((total / f64::from(heads)) as f32);
    }
    out.push((overall / f64::from(heads)) as f32);
    out.push((lift / f64::from(heads)) as f32);
    out
}

/// The names of the contentment components, in the same order.
pub fn contentment_names() -> PackedStringArray {
    let mut out = PackedStringArray::new();
    for name in red_republic_sim::Contentment::NAMES {
        out.push(name);
    }
    out
}

/// People coming and going: `[waiting_heads, groups, settled, left, gave_up]`.
///
/// The three tallies are cumulative and they are the only way a slow bleed is
/// visible at all. A republic losing forty people a year looks exactly like a
/// republic standing still unless somebody counts.
pub fn migration_totals(world: &World) -> PackedInt32Array {
    let migration = world.migration();
    let mut out = PackedInt32Array::new();
    out.push(migration.waiting_heads() as i32);
    out.push(migration.all().len() as i32);
    out.push(migration.settled() as i32);
    out.push(migration.left() as i32);
    out.push(migration.gave_up() as i32);
    out
}

/// Floats per party in [`newcomers`].
pub const NEWCOMER_STRIDE: usize = 5;

/// People standing at the frontier: `[x, y, heads, days_waited, visiting]`,
/// where `visiting` is 0 for settlers and 1 for tourists.
///
/// They have to be **on the map**. An immigrant who materialised in an
/// apartment block would be the click-a-button-and-it-happens shape this build
/// exists to refuse — and a group standing at a post that the republic has
/// built no road to is a decision the player can see and act on, but only if it
/// is drawn.
///
/// **One view for both, because on the map they are one thing**: people at a
/// post waiting for a coach that may never come. They differ in why they came
/// and in what happens when they arrive, and neither of those is visible from
/// six hundred metres up. What is visible is a crowd at the border, and they
/// draw on the same pool of coaches — so showing them apart would hide the one
/// decision they actually share.
pub fn newcomers(world: &World) -> PackedFloat32Array {
    let today = world.clock().day_index();
    let mut out = PackedFloat32Array::new();
    for group in world.migration().all() {
        out.push(group.at.x.0 as f32);
        out.push(group.at.y.0 as f32);
        out.push(group.heads as f32);
        out.push(group.waited(today) as f32);
        out.push(0.0);
    }
    for visit in world.tourism().all() {
        // Only the ones still at the border. A party asleep in a hotel is
        // inside a building, and drawing it on the roof would be a lie.
        if visit.is_staying() {
            continue;
        }
        out.push(visit.at.x.0 as f32);
        out.push(visit.at.y.0 as f32);
        out.push(visit.heads as f32);
        out.push(visit.waited(today) as f32);
        out.push(1.0);
    }
    out
}

/// Tourism at a glance:
/// `[staying, waiting, visited, turned_away, free_beds, roubles, dollars]`.
///
/// The tallies are cumulative and they are the only way the mechanic is visible
/// at all — a hotel that earned nine hundred dollars last year looks exactly
/// like a hotel that earned nothing unless somebody counts. `turned_away` is the
/// friction number, in the same sense migration's is: visitors the republic was
/// offered and could not reach.
pub fn tourism_totals(world: &World) -> PackedFloat32Array {
    use red_republic_sim::trade::Market;
    let tourism = world.tourism();
    let mut out = PackedFloat32Array::new();
    out.push(tourism.staying_heads() as f32);
    out.push(tourism.waiting_heads() as f32);
    out.push(tourism.visited() as f32);
    out.push(tourism.turned_away() as f32);
    out.push(world.free_beds() as f32);
    out.push(tourism.earned(Market::East) as f32);
    out.push(tourism.earned(Market::West) as f32);
    out
}

/// Floats per span in [`utility_lines`].
pub const LINE_STRIDE: usize = 5;

/// Every energised span: `[ax, ay, bx, by, kind]`, kind 0 power, 1 heat.
///
/// The networks have to be on the map. A plant lights only what it is strung to
/// and a boiler warms only what a main runs past, so "why is this block cold"
/// is a question about a line the player either drew or did not — and that is
/// unanswerable if the lines are invisible.
pub fn utility_lines(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for line in world.utilities().all() {
        out.push(line.from.x.0 as f32);
        out.push(line.from.y.0 as f32);
        out.push(line.to.x.0 as f32);
        out.push(line.to.y.0 as f32);
        out.push(utility_index(line.kind) as f32);
    }
    out
}

/// Floats per span in [`utility_sites`].
pub const LINE_SITE_STRIDE: usize = 6;

/// Spans ordered and not yet carrying: `[ax, ay, bx, by, progress, kind]`.
///
/// Drawn differently from a finished one for the same reason a road site is: a
/// player looking at a half-strung line needs to see that it is half-strung
/// rather than wonder why the lights are still out.
pub fn utility_sites(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for site in world.lineworks().all() {
        out.push(site.from.x.0 as f32);
        out.push(site.from.y.0 as f32);
        out.push(site.to.x.0 as f32);
        out.push(site.to.y.0 as f32);
        out.push(site.progress() as f32);
        out.push(utility_index(site.kind) as f32);
    }
    out
}

/// 0 power, 1 heat. In one place so the shell never matches on a simulation
/// enum.
pub fn utility_index(kind: red_republic_sim::Utility) -> usize {
    red_republic_sim::Utility::ALL
        .iter()
        .position(|&k| k == kind)
        .unwrap_or(0)
}

/// The networks at a glance: kilometres of each kind, then how many buildings
/// are plugged into each, then `dark` and `cold`.
///
/// Laid out by `Utility::ALL` rather than by naming power and heat, because
/// the roster grew from two to four the moment belts existed and a view with
/// two hardcoded entries is a view that silently stops mentioning half of what
/// the republic has built.
///
/// The last two are what the player acts on: buildings that want current or
/// heat and are not getting it. A republic can be short of generation or short
/// of wire, and those are different problems with the same symptom — which is
/// why the kilometres are on the same line as the failures.
pub fn utility_totals(world: &World) -> PackedFloat32Array {
    use red_republic_sim::Utility;
    let mut out = PackedFloat32Array::new();
    for kind in Utility::ALL {
        out.push(world.utilities().length_of(kind).as_km() as f32);
    }
    for kind in Utility::ALL {
        out.push(world.utilities().connected_count(kind) as f32);
    }
    let dark = world
        .buildings()
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().power_draw > 0.0 && !b.powered)
        .count();
    let cold = world
        .buildings()
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().heat > 0.0 && !b.heated)
        .count();
    out.push(dark as f32);
    out.push(cold as f32);
    out
}

/// How dirty each lattice cell is, row-major, `0.0` clean to `1.0` foul.
///
/// The same shape as the going and wear overlays, and on the same lattice.
/// Pollution is invisible on the ground by design — it is smoke and it is in
/// the soil — so an overlay is the only way it is a thing the player can be
/// asked to plan around.
pub fn pollution_field(world: &World) -> PackedFloat32Array {
    let lattice = world.lattice();
    let cells = lattice.cells();
    let mut out = PackedFloat32Array::new();
    for y in 0..cells {
        for x in 0..cells {
            out.push(lattice.pollution_at((y * cells + x) as usize) as f32);
        }
    }
    out
}

/// How buried each lattice cell is, row-major, `0.0` swept to `1.0` under snow.
///
/// **A badness, like going and unlike wear** — the same trap that painted a
/// bone-dry map entirely red once already, so the direction is stated here and
/// the ramp is built from it. The simulation stores the opposite (`cleared`,
/// where one is swept), and it is inverted *here* rather than in the shader so
/// there is one place to check it against the source.
///
/// Snow is why a republic keeps ploughs, and where the snow is *not* is the
/// only thing on the map that says whether the ploughs are winning.
pub fn snow_field(world: &World) -> PackedFloat32Array {
    let lattice = world.lattice();
    let cells = lattice.cells();
    let mut out = PackedFloat32Array::new();
    let lying = world.snow_cover();
    for y in 0..cells {
        for x in 0..cells {
            let index = (y * cells + x) as usize;
            out.push((lying * (1.0 - lattice.cleared_at(index))) as f32);
        }
    }
    out
}

/// Mean health and mean loyalty across the republic, `[health, loyalty]`.
pub fn wellbeing(world: &World) -> PackedFloat32Array {
    let (health, loyalty) = world.population().mean_wellbeing();
    let mut out = PackedFloat32Array::new();
    out.push(health as f32);
    out.push(loyalty as f32);
    out
}

/// Every span of one network as `[ax, ay, bx, by, speed_kph]`.
///
/// One view for all four ways rather than one each, because the shell should
/// not have to know how many there are: `Medium::ALL` is the roster, and a
/// fifth network becomes drawable without anybody remembering to add a getter.
/// Flat coordinates — the caller lifts them onto the ground, exactly as it does
/// for utility spans.
pub fn ways(world: &World, medium: Medium) -> PackedFloat32Array {
    let net = world.network(medium);
    let mut out = PackedFloat32Array::new();
    for segment in net.segments() {
        let Some((from, to)) = net.segment_ends(segment) else {
            continue;
        };
        out.push(from.x.0 as f32);
        out.push(from.y.0 as f32);
        out.push(to.x.0 as f32);
        out.push(to.y.0 as f32);
        out.push((segment.speed.as_mps() * 3.6) as f32);
    }
    out
}

/// How much of each way there is, in kilometres, in `Medium::ALL` order.
///
/// Water is in here beside the rest and that is the point: a republic can see
/// at a glance that it has forty kilometres of river and no port, which is the
/// only way the one network nobody builds becomes a thing anybody notices.
pub fn way_lengths(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for medium in Medium::ALL {
        out.push(world.network(medium).total_length().as_km() as f32);
    }
    out
}

/// What a place has been told to keep, as `[resource_index, held, ordered]`
/// per line. Empty for anything that does not keep goods to order.
pub fn standing_orders(world: &World, building: BuildingId) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for (resource, held, ordered) in world.standing_orders(building) {
        let index = Resource::ALL
            .iter()
            .position(|&r| r == resource)
            .unwrap_or_default();
        out.push(index as f32);
        out.push(held.0 as f32);
        out.push(ordered.0 as f32);
    }
    out
}

/// The republic's fleet by medium: how many vehicles ride each way, in
/// `Medium::ALL` order, then how many of those are out on a job.
pub fn fleet_by_medium(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for medium in Medium::ALL {
        out.push(
            world
                .fleet()
                .all()
                .iter()
                .filter(|v| v.def().medium == medium)
                .count() as f32,
        );
    }
    for medium in Medium::ALL {
        out.push(
            world
                .fleet()
                .all()
                .iter()
                .filter(|v| v.def().medium == medium && !v.is_idle())
                .count() as f32,
        );
    }
    out
}

/// Floats per workplace in [`workplaces`].
pub const WORKPLACE_STRIDE: usize = 8;

/// Every standing workplace and its roster, as
/// `[id, kind_index, staff, jobs, shifts, hours, rule, standing]` per line.
///
/// `standing` is the labour plan: `0` last, `1` ordinary, `2` first — the index
/// into [`red_republic_sim::Priority::ALL`]. It rides along with the roster
/// rather than in a view of its own because it answers the same question the
/// panel is already asking. A row reading `9/16` is only half an answer; *why*
/// it is nine is the standing, and a player who cannot see it cannot tell a
/// works the republic has no hands for from one it has ranked below everything
/// else.
///
/// `rule` says where this building's working day comes from, so the panel can
/// show it without asking three more questions: `0` the national standard, `1` a
/// rule about its kind, `2` an exception for this building alone. That is the
/// difference between "12" and "12, because you set it here" — and it is what
/// tells a player which control to reach for to change it.
///
/// Sites and buildings nobody works are left out. A roster on a house is a
/// control that does nothing, and the command refuses one.
pub fn workplaces(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    let policy = world.shift_policy();
    for b in world.buildings().all() {
        if !b.is_built() || b.def().workers == 0 {
            continue;
        }
        let kind = red_republic_sim::building::BUILDINGS
            .iter()
            .position(|d| d.kind == b.kind)
            .unwrap_or_default();
        let rule = if policy.of_building(b.id).is_some() {
            2.0
        } else if policy.of_kind(b.kind).is_some() {
            1.0
        } else {
            0.0
        };
        out.push(b.id.0 as f32);
        out.push(kind as f32);
        out.push(b.staff as f32);
        out.push(b.jobs() as f32);
        out.push(f32::from(b.shifts));
        out.push(b.hours as f32);
        out.push(rule);
        out.push(
            red_republic_sim::Priority::ALL
                .iter()
                .position(|p| *p == b.priority)
                .unwrap_or_default() as f32,
        );
    }
    out
}

// ---- One building, in front of the player ----------------------------------
//
// The inspector's reads. Everything below answers a question about a single
// building the player has clicked on, which is the one place in this interface
// where a chatty small interface would be wrong: a panel that asked twenty
// questions of one building would cross the boundary twenty times a frame while
// it was open.

/// Floats per building in [`building_state`].
pub const BUILDING_STATE_STRIDE: usize = 28;

/// Everything the inspector needs about one building, in one read.
///
/// Empty when there is no such building — which is how a panel finds out that
/// the thing it was showing has been pulled down, without a second binding for
/// asking.
///
/// In order:
///
/// | # | what |
/// |---|------|
/// | 0 | kind, as an index into `BUILDINGS` |
/// | 1, 2 | centre, in metres |
/// | 3 | `1.0` once it is open, `0.0` while it is a site |
/// | 4 | how far the site has got, `0.0..=1.0` |
/// | 5, 6 | builder-days worked, and builder-days the site needs |
/// | 7, 8 | people turning up, and posts to fill |
/// | 9, 10 | crews rostered, and hours in one of them |
/// | 11 | standing, as an index into `Priority::ALL` |
/// | 12 | on the grid: `-1` draws none, `0` dark, `1` fed |
/// | 13 | megawatts it draws |
/// | 14 | warmed: `-1` wants none, `0` cold, `1` warm |
/// | 15 | heat it wants |
/// | 16, 17 | people living here, and how many could |
/// | 18 | beds for visitors |
/// | 19 | tonnes of one good it can hold |
/// | 20 | who is building it: `-1` the republic, else a bloc index |
/// | 21 | the body it works, `0` for none |
/// | 22 | why it is stopped: `-1` it is not, else an index into `STALLS` |
/// | 23, 24, 25 | provisions, comforts and drink reaching the people here |
/// | 26, 27 | megawatts and heat it *makes* |
///
/// The three household shares are `-1` on anything nobody lives in, because
/// `0.0` there would read as a failure to feed people who do not exist.
pub fn building_state(world: &World, id: BuildingId) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    let Some(b) = world.buildings().get(id) else {
        return out;
    };
    let def = b.def();
    let census = world.population().census_by_home();
    let living = census.get(&id).map_or(0, |c| c.residents);
    let housing = def.residents > 0;

    let kind = red_republic_sim::building::BUILDINGS
        .iter()
        .position(|d| d.kind == b.kind)
        .unwrap_or_default();

    out.push(kind as f32);
    out.push(b.centre.x.0 as f32);
    out.push(b.centre.y.0 as f32);
    out.push(if b.is_built() { 1.0 } else { 0.0 });
    out.push(b.progress() as f32);
    out.push(b.work_done as f32);
    out.push(def.labour as f32);
    out.push(b.staff as f32);
    out.push(b.jobs() as f32);
    out.push(f32::from(b.shifts));
    out.push(b.hours as f32);
    out.push(
        red_republic_sim::Priority::ALL
            .iter()
            .position(|p| *p == b.priority)
            .unwrap_or_default() as f32,
    );
    // A building that draws nothing is not "unpowered", and a panel that
    // painted it red for being off a grid it never wanted would be sending the
    // player to string wire to a house.
    out.push(if def.power_draw <= 0.0 {
        -1.0
    } else if b.powered {
        1.0
    } else {
        0.0
    });
    out.push(def.power_draw as f32);
    out.push(if def.heat <= 0.0 {
        -1.0
    } else if b.heated {
        1.0
    } else {
        0.0
    });
    out.push(def.heat as f32);
    out.push(living as f32);
    out.push(def.residents as f32);
    out.push(def.beds as f32);
    out.push(def.storage as f32);
    out.push(b.contractor.map_or(-1.0, |m| bloc_index(m) as f32));
    out.push(b.tapped.map_or(0.0, |d| d.0 as f32));
    out.push(stall_index(world, id));
    out.push(if housing { b.provisioned as f32 } else { -1.0 });
    out.push(if housing { b.comforted as f32 } else { -1.0 });
    out.push(if housing { b.drink as f32 } else { -1.0 });
    // What it makes, beside what it draws. A power plant reading only "holds 80
    // t each" was the panel hiding the single fact the building exists for —
    // `power_output` and `heat_output` are authored on every row of the table
    // and reached no screen at all.
    out.push(def.power_output as f32);
    out.push(def.heat_output as f32);
    out
}

/// Why a building is stopped, as an index the shell can name — or `-1`.
///
/// The simulation's own answer rather than the panel working it out from the
/// figures beside it. A panel that decided "no staff" for itself would be a
/// second copy of `stall_reason`'s ordering, and the order matters: a works with
/// nobody in it *and* no power is stopped for want of people, and telling the
/// player to string wire would be telling them to fix the wrong thing.
fn stall_index(world: &World, id: BuildingId) -> f32 {
    use red_republic_sim::Stall;
    match red_republic_sim::systems::stall_reason(world, id) {
        None => -1.0,
        Some(Stall::NoStaff) => 0.0,
        Some(Stall::NoPower) => 1.0,
        Some(Stall::NoInputs) => 2.0,
    }
}

/// How many reasons a building can be stopped for, so a shell naming them
/// cannot name fewer than there are.
pub const STALL_COUNT: usize = 3;

/// What is in one building's yard: `[resource_index, tonnes]` per line.
///
/// Only what it is actually holding. A row per resource would be thirty lines of
/// zero on a house, and the thing a player is looking for — the empty bin that
/// stopped the works — is on the `inputs` list rather than in the yard.
pub fn building_stock(world: &World, id: BuildingId) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    let Some(b) = world.buildings().get(id) else {
        return out;
    };
    for (index, resource) in Resource::ALL.iter().enumerate() {
        let held = b.stock.get(*resource);
        if !held.is_positive() {
            continue;
        }
        out.push(index as f32);
        out.push(held.0 as f32);
    }
    out
}

/// What a site is still waiting for: `[resource_index, wanted, delivered]` per
/// line of its bill of materials.
///
/// **The answer to the only question a half-built thing raises.** A site with a
/// gang on it and no bricks looks exactly like a site nobody has been sent to,
/// and the bill is what tells them apart — read against the yard, so a line
/// showing 4 of 12 tonnes is four tonnes that arrived rather than four the
/// simulation is guessing at.
pub fn site_bill(world: &World, id: BuildingId) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    let Some(b) = world.buildings().get(id) else {
        return out;
    };
    for &(resource, wanted) in b.def().materials {
        let index = Resource::ALL
            .iter()
            .position(|&r| r == resource)
            .unwrap_or_default();
        out.push(index as f32);
        out.push(wanted as f32);
        out.push(b.stock.get(resource).0 as f32);
    }
    out
}

// ---- The export plan --------------------------------------------------------

/// Floats per rule in [`trade_rules`].
pub const TRADE_RULE_STRIDE: usize = 4;

/// The republic's standing instructions to its customs houses, **in the
/// player's own order**: `[resource_index, bloc, action, up_to_tonnes]`.
///
/// `action` is `0` sell and `1` buy; `up_to` is meaningless on a sell and comes
/// back as zero. The order is the decision — the first rule is served first when
/// throughput or hard currency runs short — so this is a list rather than a set,
/// and it is handed over exactly as it is stored.
///
/// It replaces a binding that formatted each rule into an English sentence in
/// Rust. Composing that sentence is Godot's, entirely.
pub fn trade_rules(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for rule in &world.trade_policy().rules {
        out.push(
            Resource::ALL
                .iter()
                .position(|&r| r == rule.resource)
                .unwrap_or_default() as f32,
        );
        out.push(bloc_index(rule.market) as f32);
        match rule.action {
            red_republic_sim::TradeAction::Sell => {
                out.push(0.0);
                out.push(0.0);
            }
            red_republic_sim::TradeAction::Buy { up_to } => {
                out.push(1.0);
                out.push(up_to.0 as f32);
            }
        }
    }
    out
}

// ---- Tenders and advances ---------------------------------------------------

/// Floats per tender in [`contracts`].
pub const CONTRACT_STRIDE: usize = 10;

/// Every tender, offered or running:
/// `[id, resource, bloc, tonnes, delivered, price_per_tonne, days_to_deliver,
/// days_to_answer, state, fine]`.
///
/// Both day figures are counted from today rather than handed over as absolute
/// day indices, because that is the form every one of them is read in and the
/// alternative is the panel subtracting — which is a panel doing arithmetic
/// about a deadline it did not set. They go negative once the day has passed.
///
/// `state` indexes `ContractState`: `0` on the table, `1` running, `2`
/// delivered, `3` failed. `fine` is what failing right now would cost, which is
/// the number that makes accepting one a decision rather than free money.
pub fn contracts(world: &World) -> PackedFloat32Array {
    use red_republic_sim::ContractState;
    let today = world.clock().day_index() as i64;
    let mut out = PackedFloat32Array::new();
    for c in world.contracts().all() {
        out.push(c.id.0 as f32);
        out.push(
            Resource::ALL
                .iter()
                .position(|&r| r == c.resource)
                .unwrap_or_default() as f32,
        );
        out.push(bloc_index(c.market) as f32);
        out.push(c.amount.0 as f32);
        out.push(c.delivered.0 as f32);
        out.push(c.price_per_tonne as f32);
        out.push((c.deadline_day as i64 - today) as f32);
        out.push((c.offer_expires_day as i64 - today) as f32);
        out.push(match c.state {
            ContractState::Offer => 0.0,
            ContractState::Active => 1.0,
            ContractState::Done => 2.0,
            ContractState::Failed => 3.0,
        });
        out.push(c.fine() as f32);
    }
    out
}

/// How many states a tender can be in, so a shell naming them cannot name
/// fewer than there are.
pub const CONTRACT_STATES: usize = 4;

/// How sour each bloc is on the republic, in `Market::ALL` order, `0.0..=1.0`.
///
/// A failed tender or a defaulted advance leaves a mark that moves **both**
/// prices — the bloc pays less for what it buys and charges more for what it
/// sells — and it decays on its own. Without this the player watches their
/// export income fall and has nothing to attribute it to.
pub fn bloc_relations(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for market in red_republic_sim::Market::ALL {
        out.push(world.contracts().penalty(market) as f32);
    }
    out
}

/// Floats per advance in [`loans`].
pub const LOAN_STRIDE: usize = 5;

/// What the republic owes: `[bloc, principal, owed, repaid, days_left]`.
///
/// `owed` is principal plus the interest that was fixed when the money was
/// taken, so what is still outstanding is `owed - repaid` — which the panel
/// subtracts because both halves are worth showing separately: a republic that
/// has paid two thirds of a bad advance is in a different position from one that
/// has paid none of a good one.
pub fn loans(world: &World) -> PackedFloat32Array {
    let today = world.clock().day_index();
    let mut out = PackedFloat32Array::new();
    for loan in world.loans().all() {
        out.push(bloc_index(loan.market) as f32);
        out.push(loan.principal as f32);
        out.push(loan.owed as f32);
        out.push(loan.repaid as f32);
        out.push(loan.days_left(today) as f32);
    }
    out
}

/// Floats per rung in [`loan_tiers`].
pub const TIER_STRIDE: usize = 4;

/// What the blocs will advance: `[principal, interest_share, term_days,
/// total_owed]` per rung of the ladder.
///
/// `total_owed` is the arithmetic the panel would otherwise do — principal times
/// one plus the interest — and it is here rather than there because it is the
/// number the player is actually deciding against, and because balance does not
/// belong in a panel however small the sum.
pub fn loan_tiers() -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for tier in red_republic_sim::loan::TIERS {
        out.push(tier.principal as f32);
        out.push(tier.interest as f32);
        out.push(tier.term_days as f32);
        out.push((tier.principal * (1.0 + tier.interest)) as f32);
    }
    out
}

// ---- The journal ------------------------------------------------------------

/// Floats per entry in [`journal`].
pub const JOURNAL_STRIDE: usize = 9;

/// How many kinds of command the journal can hold.
///
/// Exported because the journal screen names them, and a screen naming twenty-one
/// of twenty-two verbs prints a blank line for the twenty-second — which reads as
/// a command that did nothing. `main.gd`'s `--check` compares its table against
/// this, so the two cannot silently disagree.
///
/// # How this is held true
///
/// Three links, and they are worth naming because none of them alone is enough:
///
/// 1. [`verb_of`] is an exhaustive match, so a new `Command` variant **fails the
///    build** rather than falling into somebody else's row.
/// 2. [`every_verb_has_an_index_and_the_count_says_how_many`] fails if this
///    number and the roster of variants disagree in size.
/// 3. `verb_of` debug-asserts its own answer is inside this range, so an arm
///    added without bumping the number trips the first time it is journalled —
///    which the trajectory runner and every debug build reach.
///
/// What is left is an arm added, the number left alone, and the new command
/// never issued in a debug build. That is the residual, stated rather than
/// papered over.
pub const VERBS: usize = 22;

/// Which kind of command this is, as a stable index.
///
/// **Exhaustive on purpose.** A `_ =>` arm would let a command added to the
/// simulation land silently in somebody else's row of the journal, which is
/// exactly the "list of ids inside logic" the project's rules refuse. Adding a
/// variant breaks this match, and the number above.
fn verb_of(command: &red_republic_sim::Command) -> usize {
    let verb = verb_number(command);
    debug_assert!(
        verb < VERBS,
        "verb {verb} is outside the {VERBS} the shell says there are; \
         an arm was added to verb_of and VERBS was not bumped"
    );
    verb
}

fn verb_number(command: &red_republic_sim::Command) -> usize {
    use red_republic_sim::Command as C;
    match command {
        C::Place { .. } => 0,
        C::ContractBuild { .. } => 1,
        C::Demolish { .. } => 2,
        C::OrderRoad { .. } => 3,
        C::OrderLine { .. } => 4,
        C::RecallCrew { .. } => 5,
        C::SetImportPolicy { .. } => 6,
        C::ClearImportPolicy { .. } => 7,
        C::SetStandingOrder { .. } => 8,
        C::HireForeign { .. } => 9,
        C::AcceptContract { .. } => 10,
        C::DeclineContract { .. } => 11,
        C::AddTradeRule { .. } => 12,
        C::RemoveTradeRule { .. } => 13,
        C::MoveTradeRule { .. } => 14,
        C::TakeLoan { .. } => 15,
        C::RepayLoan { .. } => 16,
        C::SetNationalShiftHours { .. } => 17,
        C::SetShiftHours { .. } => 18,
        C::SetShifts { .. } => 19,
        C::SetPriority { .. } => 20,
        C::NameRepublic { .. } => 21,
    }
}

/// A window on everything the player has done: `[tick, day, verb, a, b, c, d,
/// e, f]` per entry, oldest first.
///
/// Windowed rather than handed over whole, because a decade of play is tens of
/// thousands of entries and a screen shows thirty of them.
///
/// The six trailing figures mean whatever the verb says they mean, and nothing
/// else in this crate interprets them — composing a sentence out of a verb and
/// its figures is the interface's work, and the interface is Godot's. What a
/// verb carries:
///
/// | verb | a | b | c | d | e | f |
/// |------|---|---|---|---|---|---|
/// | place, contract | kind | bloc | x | y | | |
/// | demolish, recall crew | building | | | | | |
/// | order road | grade | lamps | x | y | x | y |
/// | order line | utility | | x | y | x | y |
/// | import policy | building | post | | | | |
/// | standing order | building | resource | tonnes | | | |
/// | hire abroad | office | bloc | heads | | | |
/// | tender | tender | | | | | |
/// | trade rule | resource | bloc | action | tonnes | from | to |
/// | advance | bloc | tier | amount | | | |
/// | shift hours | kind or building | hours | which of the two | | | |
/// | crews, standing | building | value | | | | |
///
/// A figure a verb has no use for comes back as zero, and a name — the one
/// string any command carries — comes back through [`journal_text`], because it
/// is the player's own words rather than a number.
pub fn journal(world: &World, from: usize, count: usize) -> PackedFloat32Array {
    use red_republic_sim::Command as C;
    let ticks_per_day = red_republic_sim::time::TICKS_PER_DAY as f32;
    let entries = world.journal().entries();
    let mut out = PackedFloat32Array::new();
    for entry in entries.iter().skip(from).take(count) {
        let mut args = [0.0f32; 6];
        match entry.command {
            C::Place { kind, at } => {
                args[0] = kind_index(kind) as f32;
                args[1] = -1.0;
                args[2] = at.x.0 as f32;
                args[3] = at.y.0 as f32;
            }
            C::ContractBuild { kind, at, market } => {
                args[0] = kind_index(kind) as f32;
                args[1] = bloc_index(market) as f32;
                args[2] = at.x.0 as f32;
                args[3] = at.y.0 as f32;
            }
            C::Demolish { building } => args[0] = building.0 as f32,
            C::RecallCrew { site } => args[0] = site_index(site) as f32,
            C::OrderRoad {
                from,
                to,
                grade,
                lamps,
            } => {
                args[0] = red_republic_sim::roadworks::GRADES
                    .iter()
                    .position(|d| d.grade == grade)
                    .unwrap_or_default() as f32;
                args[1] = if lamps { 1.0 } else { 0.0 };
                args[2] = from.x.0 as f32;
                args[3] = from.y.0 as f32;
                args[4] = to.x.0 as f32;
                args[5] = to.y.0 as f32;
            }
            C::OrderLine { kind, from, to } => {
                args[0] = utility_index(kind) as f32;
                args[2] = from.x.0 as f32;
                args[3] = from.y.0 as f32;
                args[4] = to.x.0 as f32;
                args[5] = to.y.0 as f32;
            }
            C::SetImportPolicy { site, crossing } => {
                args[0] = site.map_or(0.0, |s| site_index(s) as f32);
                args[1] = crossing.map_or(0.0, |c| c.0 as f32);
            }
            C::ClearImportPolicy { site } => args[0] = site_index(site) as f32,
            C::SetStandingOrder {
                building,
                resource,
                tonnes,
            } => {
                args[0] = building.0 as f32;
                args[1] = Resource::ALL
                    .iter()
                    .position(|&r| r == resource)
                    .unwrap_or_default() as f32;
                args[2] = tonnes.0 as f32;
            }
            C::HireForeign {
                market,
                office,
                heads,
            } => {
                args[0] = office.0 as f32;
                args[1] = bloc_index(market) as f32;
                args[2] = heads as f32;
            }
            C::AcceptContract { contract } | C::DeclineContract { contract } => {
                args[0] = contract.0 as f32
            }
            C::AddTradeRule {
                resource,
                market,
                action,
            } => {
                args[0] = Resource::ALL
                    .iter()
                    .position(|&r| r == resource)
                    .unwrap_or_default() as f32;
                args[1] = bloc_index(market) as f32;
                match action {
                    red_republic_sim::TradeAction::Sell => args[2] = 0.0,
                    red_republic_sim::TradeAction::Buy { up_to } => {
                        args[2] = 1.0;
                        args[3] = up_to.0 as f32;
                    }
                }
            }
            C::RemoveTradeRule { index } => args[4] = index as f32,
            C::MoveTradeRule { from, to } => {
                args[4] = from as f32;
                args[5] = to as f32;
            }
            C::TakeLoan { market, tier } => {
                args[0] = bloc_index(market) as f32;
                args[1] = tier as f32;
                args[2] = red_republic_sim::loan::TIERS
                    .get(tier as usize)
                    .map_or(0.0, |t| t.principal as f32);
            }
            C::RepayLoan { market, amount } => {
                args[0] = bloc_index(market) as f32;
                args[1] = -1.0;
                args[2] = amount as f32;
            }
            C::SetNationalShiftHours { hours } => {
                args[0] = -1.0;
                args[1] = hours as f32;
            }
            C::SetShiftHours { scope, hours } => {
                // The third figure says which of the two `a` is, because a
                // building id and a kind index are both small numbers and a
                // journal line that guessed would name the wrong thing half the
                // time.
                args[0] = match scope {
                    red_republic_sim::command::ShiftScope::Kind(kind) => kind_index(kind) as f32,
                    red_republic_sim::command::ShiftScope::Building(id) => id.0 as f32,
                };
                args[1] = hours.map_or(-1.0, |h| h as f32);
                args[2] = match scope {
                    red_republic_sim::command::ShiftScope::Kind(_) => 0.0,
                    red_republic_sim::command::ShiftScope::Building(_) => 1.0,
                };
            }
            C::SetShifts { building, shifts } => {
                args[0] = building.0 as f32;
                args[1] = f32::from(shifts);
            }
            C::SetPriority { building, priority } => {
                args[0] = building.0 as f32;
                args[1] = red_republic_sim::Priority::ALL
                    .iter()
                    .position(|p| *p == priority)
                    .unwrap_or_default() as f32;
            }
            C::NameRepublic { .. } => {}
        }
        out.push(entry.tick as f32);
        out.push((entry.tick as f32 / ticks_per_day).floor());
        out.push(verb_of(&entry.command) as f32);
        for arg in args {
            out.push(arg);
        }
    }
    out
}

/// The one string a command carries, or empty.
///
/// Today that is only the name of the republic, which is the player's own words
/// rather than anything this crate wrote — the single case where a `String`
/// crossing this boundary is neither an authored name nor a refusal, and it is
/// the player's own text coming back to them.
pub fn journal_text(world: &World, index: usize) -> GString {
    use red_republic_sim::Command as C;
    match world.journal().entries().get(index).map(|e| &e.command) {
        Some(C::NameRepublic { name }) => GString::from(name.as_str()),
        _ => GString::from(""),
    }
}

fn kind_index(kind: red_republic_sim::BuildingKind) -> usize {
    red_republic_sim::building::BUILDINGS
        .iter()
        .position(|d| d.kind == kind)
        .unwrap_or_default()
}

/// A destination as the one number a journal line needs: a building id, or zero
/// for a road or a line site, which have their own numbering and no name.
fn site_index(site: red_republic_sim::Destination) -> u32 {
    match site {
        red_republic_sim::Destination::Building(id) => id.0,
        red_republic_sim::Destination::RoadSite(_) | red_republic_sim::Destination::LineSite(_) => {
            0
        }
    }
}

/// The working-hours rule for each kind that has one, as `[kind_index, hours]`.
///
/// Only the kinds with a rule of their own: the panel lists every kind the
/// republic has standing anyway, and sending a row for each would be sending the
/// national standard back a hundred times over.
pub fn kind_shift_rules(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for (kind, hours) in world.shift_policy().kind_rules() {
        let index = red_republic_sim::building::BUILDINGS
            .iter()
            .position(|d| d.kind == kind)
            .unwrap_or_default();
        out.push(index as f32);
        out.push(hours as f32);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use red_republic_sim::Command as C;
    use red_republic_sim::{BuildingId, ContractId, Market, Point, Tonnes};

    /// Every verb has an index, and [`VERBS`] says how many there are.
    ///
    /// The roster below is written out rather than derived, because there is no
    /// way to enumerate an enum's variants in Rust — which is exactly why the
    /// count is checked here rather than trusted. The compiler already refuses a
    /// `Command` variant with no arm in [`verb_number`]; what it cannot see is a
    /// new arm with the count left where it was, and the size assertion below is
    /// the half that catches that.
    #[test]
    fn every_verb_has_an_index_and_the_count_says_how_many() {
        let at = Point::new(red_republic_sim::Metres(0.0), red_republic_sim::Metres(0.0));
        let site = red_republic_sim::Destination::Building(BuildingId(1));
        let every: Vec<C> = vec![
            C::Place {
                kind: red_republic_sim::BuildingKind::House,
                at,
            },
            C::ContractBuild {
                kind: red_republic_sim::BuildingKind::House,
                at,
                market: Market::East,
            },
            C::Demolish {
                building: BuildingId(1),
            },
            C::OrderRoad {
                from: at,
                to: at,
                grade: red_republic_sim::Grade::Dirt,
                lamps: false,
            },
            C::OrderLine {
                kind: red_republic_sim::Utility::Power,
                from: at,
                to: at,
            },
            C::RecallCrew { site },
            C::SetImportPolicy {
                site: Some(site),
                crossing: None,
            },
            C::ClearImportPolicy { site },
            C::SetStandingOrder {
                building: BuildingId(1),
                resource: Resource::Coal,
                tonnes: Tonnes(1.0),
            },
            C::HireForeign {
                market: Market::East,
                office: BuildingId(1),
                heads: 1,
            },
            C::AcceptContract {
                contract: ContractId(1),
            },
            C::DeclineContract {
                contract: ContractId(1),
            },
            C::AddTradeRule {
                resource: Resource::Coal,
                market: Market::East,
                action: red_republic_sim::TradeAction::Sell,
            },
            C::RemoveTradeRule { index: 0 },
            C::MoveTradeRule { from: 0, to: 0 },
            C::TakeLoan {
                market: Market::East,
                tier: 0,
            },
            C::RepayLoan {
                market: Market::East,
                amount: 1.0,
            },
            C::SetNationalShiftHours { hours: 8.0 },
            C::SetShiftHours {
                scope: red_republic_sim::command::ShiftScope::Building(BuildingId(1)),
                hours: None,
            },
            C::SetShifts {
                building: BuildingId(1),
                shifts: 1,
            },
            C::SetPriority {
                building: BuildingId(1),
                priority: red_republic_sim::Priority::First,
            },
            C::NameRepublic {
                name: String::from("a"),
            },
        ];

        assert_eq!(
            every.len(),
            VERBS,
            "VERBS says {VERBS} and the roster here has {}. One of them has \
             moved and the journal screen names whichever is shorter.",
            every.len()
        );

        let mut seen: Vec<usize> = every.iter().map(verb_of).collect();
        seen.sort_unstable();
        assert_eq!(
            seen,
            (0..VERBS).collect::<Vec<_>>(),
            "two commands share a verb index, or one is missing: composing a \
             journal line would put one command's figures under another's words"
        );
    }
}
