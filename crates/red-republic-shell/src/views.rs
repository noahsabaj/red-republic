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
        out.push(held as f32);
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
pub const WORKPLACE_STRIDE: usize = 7;

/// Every standing workplace and its roster, as
/// `[id, kind_index, staff, jobs, shifts, hours, rule]` per line.
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
    }
    out
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
