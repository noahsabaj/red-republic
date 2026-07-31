//! Dumps the simulation's authored tables as JSON, for the Godot port to build
//! against.
//!
//! **A throwaway extraction tool, not a feature.** It exists because the port
//! needs the balance table as *data* — sixty-four buildings by thirty fields is
//! not something to transcribe by hand or scrape out of source with a regex,
//! and this crate is going away once the port reaches parity. Reading the real
//! `def()` is the only extraction that cannot drift from what the game actually
//! runs.

use red_republic_sim::building::{BUILDINGS, Need, Priority, Teaching};
use red_republic_sim::citizen::{
    Education, MAX_WALK, ROAD_ACCESS, SCHOOL_AGE, SCHOOL_DAYS, UNIVERSITY_AGE, UNIVERSITY_DAYS,
    WORKING_AGE, walking_speed,
};
use red_republic_sim::climate::{
    CLIMATES, HEAT_DEMAND_CEILING, HEAT_DESIGN_C, HEAT_THRESHOLD_C, WET_DAY_SHARE,
};
use red_republic_sim::fleet::VehicleKind;
use red_republic_sim::ground::{
    DROWNED, DRYING_FULL_AT_C, DRYING_PER_DAY, FREEZE_C, FROST_LAG, FROST_RANGE_C, GROUND_CELL,
    MELT_PER_DEGREE_MM, MIN_TRACK_CELLS, MUD_DRAG, PROMOTE_AT, ROOT_DRYING_PER_DAY,
    ROOT_SATURATION_MM, SATURATION_MM, SNOW_BLOCKS_MM, SNOW_DRAG, WEAR_FADE_PER_DAY, WEAR_PER_PASS,
    WEAR_RELIEF, going,
};
use red_republic_sim::journey::{MIN_LEG_TICKS, Medium, SHUNTING, TERMINAL_REACH};
use red_republic_sim::mapgen::{DEFAULT_PLAN, GEOLOGY_STREAM};
use red_republic_sim::network::{
    AIRWAY_SPEED, FAIRWAY_SPACING, NAVIGABLE_BEAM, NAVIGABLE_SPEED, default_road_speed,
};
use red_republic_sim::resource::{Form, Resource};
use red_republic_sim::shifts::{
    DAYLIGHT_HOURS, MAX_HOURS, MAX_SHIFTS, MIN_HOURS, OVERWORK_HEALTH, OVERWORK_LOYALTY,
    STANDARD_HOURS,
};
use red_republic_sim::terrain::{DEFAULT_TERRAIN, Surface};
use red_republic_sim::transport::{FUEL_PER_SEAT_DAY, MAX_COMMUTE, NIGHT_WALK, STOP_WALK};

fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `f64` that survives a round trip. `{:?}` on an `f64` prints the shortest
/// decimal that reads back bit-identical, which is exactly the guarantee the
/// port needs and exactly what `{}` does not give.
fn n(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{:.1}", v)
    } else {
        format!("{:?}", v)
    }
}

/// A number, or `null` where it is not finite.
///
/// JSON has no infinity, and `going(Surface::Water)` is exactly that: water is
/// impassable rather than merely slow. Writing `inf` produces a file that does
/// not parse — which is how this was found — and writing a large finite number
/// instead would make water something a desperate router could still cross.
fn n_or_null(v: f64) -> String {
    if v.is_finite() { n(v) } else { "null".into() }
}

fn list<T>(items: &[T], f: impl Fn(&T) -> String) -> String {
    format!("[{}]", items.iter().map(f).collect::<Vec<_>>().join(", "))
}

/// Every number in the manifest, in one canonical order, hashed by its **bits**.
///
/// This is the guard that makes shipping the balance table through JSON
/// defensible. The determinism rule says a save must round-trip `f64`
/// bit-exactly, and a balance table read at startup is held to the same bar for
/// the same reason: a figure that arrives one ulp out is a republic that
/// diverges from the one the seed promised, and nothing downstream would ever
/// say so.
///
/// The port recomputes this in GDScript over the parsed manifest and refuses to
/// run if it disagrees — so the claim "the table crossed intact" is checked
/// rather than assumed, and the order of the fields is pinned as a side effect.
struct Canon(u64);

impl Canon {
    fn new() -> Self {
        // FNV-1a, 64-bit. Chosen because it is four lines in both languages and
        // needs no library on either side; this hashes a few thousand numbers
        // once at startup, so speed is not a consideration.
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn push(&mut self, v: f64) {
        if std::env::var_os("CANON_TRACE").is_some() {
            eprintln!("{:?}", v);
        }
        for byte in v.to_bits().to_le_bytes() {
            self.0 ^= u64::from(byte);
            // 0x100_0000_01b3, the FNV-64 prime. Grouped in threes on purpose:
            // written as `0x1000_0000_01b3` it gains a digit and still looks
            // right, which is exactly the mistake this line started as.
            self.0 = self.0.wrapping_mul(0x100_0000_01b3);
        }
    }

    fn push_int(&mut self, v: u32) {
        self.push(f64::from(v));
    }

    fn push_bool(&mut self, v: bool) {
        self.push(if v { 1.0 } else { 0.0 });
    }
}

fn main() {
    // The body is built first and the checksum is written around it at the very
    // end.
    //
    // Not a style choice. The checksum used to be formatted here, ahead of the
    // body, so a block of `canon.push` calls added further down ran *after* the
    // number had already been written — hashing nothing, silently, and with both
    // sides still agreeing because the reader mirrors the same order. It
    // happened once, to the constants describing how far somebody will walk.
    // Building the body first makes that ordering mistake unrepresentable:
    // there is nowhere left to put a push that is not before the hash is read.
    let mut out = String::new();

    let mut canon = Canon::new();
    // Prices are balance, and the dearest end of the table is the whole
    // industrialisation incentive — a tonne of electronics against two hundred
    // of coal. A price that arrives wrong is an economy that is wrong, so they
    // are hashed like everything else.
    for r in Resource::ALL {
        canon.push(r.price_east());
        canon.push(r.price_west());
        canon.push_bool(r.is_comfort());
    }
    for d in BUILDINGS {
        canon.push(d.width.0);
        canon.push(d.depth.0);
        canon.push_int(d.workers);
        canon.push(d.power_draw);
        canon.push(d.power_output);
        canon.push(d.heat);
        canon.push(d.heat_output);
        canon.push_int(d.seats);
        for &(_, c) in d.vehicles {
            canon.push_int(c);
        }
        for &(_, v) in d.inputs {
            canon.push(v);
        }
        for &(_, v) in d.outputs {
            canon.push(v);
        }
        for &(_, v) in d.materials {
            canon.push(v);
        }
        canon.push(d.labour);
        canon.push_int(d.residents);
        canon.push(d.storage);
        canon.push_int(d.beds);
        canon.push(d.wear);
        canon.push_bool(d.farms);
        canon.push_bool(d.transforms);
        canon.push(d.waste);
        canon.push(d.pollution);
        for &(_, v) in d.serves {
            canon.push(v);
        }
        canon.push_bool(d.stores_to_order);
    }
    // Climate is balance: the coldest month decides how much coal a winter
    // costs, and the rain table decides whether the ground is mud.
    // The working day. `STANDARD_HOURS` is what every authored rate in the
    // building table means, so it is balance in the strongest sense: change it
    // and every output figure in the game means something else.
    canon.push(STANDARD_HOURS);
    canon.push(MIN_HOURS);
    canon.push(MAX_HOURS);
    canon.push(DAYLIGHT_HOURS);
    canon.push_int(u32::from(MAX_SHIFTS));
    canon.push(OVERWORK_HEALTH);
    canon.push(OVERWORK_LOYALTY);
    // The ground model: the thaw, the mud and the tracks traffic wears in.
    canon.push(FREEZE_C);
    canon.push(FROST_RANGE_C);
    canon.push(FROST_LAG);
    canon.push(SATURATION_MM);
    canon.push(MELT_PER_DEGREE_MM);
    canon.push(DRYING_PER_DAY);
    canon.push(DRYING_FULL_AT_C);
    canon.push(ROOT_SATURATION_MM);
    canon.push(ROOT_DRYING_PER_DAY);
    canon.push(MUD_DRAG);
    canon.push(DROWNED);
    canon.push(WEAR_PER_PASS);
    canon.push(WEAR_FADE_PER_DAY);
    canon.push(PROMOTE_AT);
    canon.push(SNOW_BLOCKS_MM);
    canon.push(SNOW_DRAG);
    canon.push(WEAR_RELIEF);
    canon.push(GROUND_CELL.0);
    canon.push_int(MIN_TRACK_CELLS as u32);
    for s in [
        Surface::Grass,
        Surface::Forest,
        Surface::Rock,
        Surface::Water,
    ] {
        canon.push(going(s));
    }
    canon.push(HEAT_THRESHOLD_C);
    canon.push(HEAT_DESIGN_C);
    canon.push(HEAT_DEMAND_CEILING);
    canon.push(WET_DAY_SHARE);
    for c in CLIMATES {
        for v in c.monthly_mean_c {
            canon.push(v);
        }
        canon.push(c.daily_swing_c);
        for v in c.monthly_rain_mm {
            canon.push(v);
        }
    }
    canon.push(DEFAULT_TERRAIN.cell_size.0);
    canon.push(DEFAULT_TERRAIN.feature_size.0);
    canon.push_int(DEFAULT_TERRAIN.octaves);
    canon.push(DEFAULT_TERRAIN.relief.0);
    canon.push(DEFAULT_TERRAIN.water_below);
    canon.push(DEFAULT_TERRAIN.forest_above);
    canon.push(DEFAULT_TERRAIN.rock_above);
    canon.push(DEFAULT_TERRAIN.river_catchment);
    canon.push(DEFAULT_TERRAIN.broad_catchment);
    for p in &DEFAULT_PLAN {
        canon.push_int(p.bodies);
        canon.push(p.radius.0.0);
        canon.push(p.radius.1.0);
        canon.push(p.top.0.0);
        canon.push(p.top.1.0);
        canon.push_int(p.layers);
        canon.push(p.layer_thickness.0.0);
        canon.push(p.layer_thickness.1.0);
        canon.push(p.tonnes_per_layer.0.0);
        canon.push(p.tonnes_per_layer.1.0);
    }
    for v in VehicleKind::all() {
        let d = v.def();
        canon.push(d.capacity.0);
        canon.push_int(d.seats);
        canon.push(d.on_road.as_mps() * 3.6);
        canon.push(d.cross_country.as_mps() * 3.6);
        canon.push(d.fuel_per_km);
        canon.push(d.tank.0);
        canon.push(d.ground);
        canon.push(d.load_penalty);
    }
    // People: how far somebody will walk, how long they will travel, and the
    // ages that decide what they are doing with their life.
    canon.push(FAIRWAY_SPACING.0);
    canon.push(NAVIGABLE_BEAM.0);
    canon.push(NAVIGABLE_SPEED.as_kph());
    canon.push(AIRWAY_SPEED.as_kph());
    canon.push(default_road_speed().as_kph());
    canon.push(MAX_WALK.0);
    canon.push(ROAD_ACCESS.0);
    canon.push(walking_speed().as_kph());
    canon.push_int(WORKING_AGE.start);
    canon.push_int(WORKING_AGE.end);
    canon.push_int(SCHOOL_AGE.start);
    canon.push_int(SCHOOL_AGE.end);
    canon.push_int(UNIVERSITY_AGE.start);
    canon.push_int(UNIVERSITY_AGE.end);
    canon.push_int(SCHOOL_DAYS);
    canon.push_int(UNIVERSITY_DAYS);
    canon.push(MAX_COMMUTE.0);
    canon.push(STOP_WALK.0);
    canon.push(NIGHT_WALK.0);
    canon.push(FUEL_PER_SEAT_DAY);
    canon.push(TERMINAL_REACH.0);
    canon.push(MIN_LEG_TICKS);
    canon.push(SHUNTING.as_kph());
    for m in Medium::ALL {
        canon.push(m.commercial_speed().as_kph());
    }

    // ---- the enum rosters, in declaration order, because the port indexes them ----
    out.push_str(&format!(
        "  \"resources\": {},\n",
        list(&Resource::ALL, |r| q(&format!("{r:?}")))
    ));
    out.push_str(&format!(
        "  \"forms\": {},\n",
        list(
            &[
                Form::Aggregate,
                Form::Liquid,
                Form::Bulk,
                Form::Open,
                Form::Covered
            ],
            |f| q(&format!("{f:?}"))
        )
    ));
    out.push_str(&format!(
        "  \"needs\": {},\n",
        list(
            &[Need::Health, Need::Culture, Need::Schooling, Need::Safety],
            |x| q(&format!("{x:?}"))
        )
    ));
    out.push_str(&format!(
        "  \"education\": {},\n",
        list(
            &[
                Education::Unschooled,
                Education::Schooled,
                Education::Graduate
            ],
            |x| q(&format!("{x:?}"))
        )
    ));
    out.push_str(&format!(
        "  \"priorities\": {},\n",
        list(
            &[Priority::Last, Priority::Ordinary, Priority::First],
            |x| q(&format!("{x:?}"))
        )
    ));

    // ---- resource properties ----
    out.push_str("  \"resource_facts\": {\n");
    let rows: Vec<String> = Resource::ALL
        .iter()
        .map(|&r| {
            format!(
                "    {}: {{ \"name\": {}, \"form\": {}, \"price_east\": {}, \"price_west\": {}, \"is_comfort\": {}, \"from_mineral\": {} }}",
                q(&format!("{r:?}")),
                q(r.name()),
                q(&format!("{:?}", r.form())),
                n(r.price_east()),
                n(r.price_west()),
                r.is_comfort(),
                r.from_mineral()
                    .map(|m| q(&format!("{m:?}")))
                    .unwrap_or_else(|| "null".into())
            )
        })
        .collect();
    out.push_str(&rows.join(",\n"));
    out.push_str("\n  },\n");

    // ---- the building table ----
    //
    // The order is emitted explicitly rather than left to the reader's JSON
    // object ordering. Godot's parser happens to preserve insertion order, but
    // a table whose indices depend on that is a table one parser change away
    // from renumbering every building in every save.
    out.push_str(&format!(
        "  \"building_order\": {},\n",
        list(BUILDINGS, |d| q(&format!("{:?}", d.kind)))
    ));
    out.push_str("  \"buildings\": {\n");
    let rows: Vec<String> = BUILDINGS
        .iter()
        .map(|d| {
            let k = d.kind;
            let mut f = String::new();
            f.push_str(&format!("      \"name\": {},\n", q(d.name)));
            f.push_str(&format!("      \"width\": {},\n", n(d.width.0)));
            f.push_str(&format!("      \"depth\": {},\n", n(d.depth.0)));
            f.push_str(&format!("      \"workers\": {},\n", d.workers));
            f.push_str(&format!(
                "      \"priority\": {},\n",
                q(&format!("{:?}", d.priority))
            ));
            f.push_str(&format!("      \"power_draw\": {},\n", n(d.power_draw)));
            f.push_str(&format!("      \"power_output\": {},\n", n(d.power_output)));
            f.push_str(&format!("      \"heat\": {},\n", n(d.heat)));
            f.push_str(&format!("      \"heat_output\": {},\n", n(d.heat_output)));
            f.push_str(&format!("      \"seats\": {},\n", d.seats));
            f.push_str(&format!(
                "      \"vehicles\": {},\n",
                list(d.vehicles, |&(v, c)| format!(
                    "[{}, {}]",
                    q(&format!("{v:?}")),
                    c
                ))
            ));
            f.push_str(&format!(
                "      \"inputs\": {},\n",
                list(d.inputs, |&(r, v)| format!(
                    "[{}, {}]",
                    q(&format!("{r:?}")),
                    n(v)
                ))
            ));
            f.push_str(&format!(
                "      \"outputs\": {},\n",
                list(d.outputs, |&(r, v)| format!(
                    "[{}, {}]",
                    q(&format!("{r:?}")),
                    n(v)
                ))
            ));
            f.push_str(&format!(
                "      \"materials\": {},\n",
                list(d.materials, |&(r, v)| format!(
                    "[{}, {}]",
                    q(&format!("{r:?}")),
                    n(v)
                ))
            ));
            f.push_str(&format!("      \"labour\": {},\n", n(d.labour)));
            f.push_str(&format!(
                "      \"sells\": {},\n",
                list(d.sells, |r| q(&format!("{r:?}")))
            ));
            f.push_str(&format!(
                "      \"taps\": {},\n",
                d.taps
                    .map(|m| q(&format!("{m:?}")))
                    .unwrap_or_else(|| "null".into())
            ));
            f.push_str(&format!("      \"residents\": {},\n", d.residents));
            f.push_str(&format!("      \"storage\": {},\n", n(d.storage)));
            f.push_str(&format!(
                "      \"admits\": {},\n",
                list(d.admits, |x| q(&format!("{x:?}")))
            ));
            f.push_str(&format!("      \"beds\": {},\n", d.beds));
            f.push_str(&format!("      \"wear\": {},\n", n(d.wear)));
            f.push_str(&format!("      \"farms\": {},\n", d.farms));
            f.push_str(&format!(
                "      \"schooling\": {},\n",
                q(&format!("{:?}", d.schooling))
            ));
            f.push_str(&format!(
                "      \"teaches\": {},\n",
                d.teaches
                    .map(|t: Teaching| q(&format!("{t:?}")))
                    .unwrap_or_else(|| "null".into())
            ));
            f.push_str(&format!("      \"transforms\": {},\n", d.transforms));
            f.push_str(&format!("      \"waste\": {},\n", n(d.waste)));
            f.push_str(&format!("      \"pollution\": {},\n", n(d.pollution)));
            f.push_str(&format!(
                "      \"medium\": {},\n",
                d.medium
                    .map(|m: Medium| q(&format!("{m:?}")))
                    .unwrap_or_else(|| "null".into())
            ));
            f.push_str(&format!(
                "      \"serves\": {},\n",
                list(d.serves, |&(need, v)| format!(
                    "[{}, {}]",
                    q(&format!("{need:?}")),
                    n(v)
                ))
            ));
            f.push_str(&format!(
                "      \"stores_to_order\": {}\n",
                d.stores_to_order
            ));
            format!("    {}: {{\n{}    }}", q(&format!("{k:?}")), f)
        })
        .collect();
    out.push_str(&rows.join(",\n"));
    out.push_str("\n  },\n");

    // ---- worldgen plans ----
    //
    // Balance, and swept rather than picked — `river_catchment` was chosen over
    // five thresholds and three seeds against how far the longest channel runs
    // and how many bearings a 2 km road can take without meeting water. A
    // figure like that belongs in the data with the rest, not in a constant.
    let t = &DEFAULT_TERRAIN;
    out.push_str("  \"terrain_plan\": {\n");
    out.push_str(&format!("    \"cell_size\": {},\n", n(t.cell_size.0)));
    out.push_str(&format!("    \"feature_size\": {},\n", n(t.feature_size.0)));
    out.push_str(&format!("    \"octaves\": {},\n", t.octaves));
    out.push_str(&format!("    \"relief\": {},\n", n(t.relief.0)));
    out.push_str(&format!("    \"water_below\": {},\n", n(t.water_below)));
    out.push_str(&format!("    \"forest_above\": {},\n", n(t.forest_above)));
    out.push_str(&format!("    \"rock_above\": {},\n", n(t.rock_above)));
    out.push_str(&format!(
        "    \"river_catchment\": {},\n",
        n(t.river_catchment)
    ));
    out.push_str(&format!(
        "    \"broad_catchment\": {}\n",
        n(t.broad_catchment)
    ));
    out.push_str("  },\n");

    // Climate is balance, and the two halves are authored together on purpose:
    // the taiga is cold and dry, the maritime posting is mild and wet, and those
    // are different problems rather than one dial.
    out.push_str(&format!(
        "  \"network\": {{ \"fairway_spacing_m\": {}, \"navigable_beam_m\": {}, \"navigable_kph\": {}, \"airway_kph\": {}, \"default_road_kph\": {} }},
",
        n(FAIRWAY_SPACING.0),
        n(NAVIGABLE_BEAM.0),
        n(NAVIGABLE_SPEED.as_kph()),
        n(AIRWAY_SPEED.as_kph()),
        n(default_road_speed().as_kph())
    ));

    // People, and the journeys they make.
    out.push_str(&format!(
        "  \"people\": {{ \"max_walk_m\": {}, \"road_access_m\": {}, \"walk_kph\": {}, \"working_age\": [{}, {}], \"school_age\": [{}, {}], \"university_age\": [{}, {}], \"school_days\": {}, \"university_days\": {}, \"max_commute_s\": {}, \"stop_walk_m\": {}, \"night_walk_m\": {}, \"fuel_per_seat_day\": {} }},
",
        n(MAX_WALK.0),
        n(ROAD_ACCESS.0),
        n(walking_speed().as_kph()),
        WORKING_AGE.start,
        WORKING_AGE.end,
        SCHOOL_AGE.start,
        SCHOOL_AGE.end,
        UNIVERSITY_AGE.start,
        UNIVERSITY_AGE.end,
        SCHOOL_DAYS,
        UNIVERSITY_DAYS,
        n(MAX_COMMUTE.0),
        n(STOP_WALK.0),
        n(NIGHT_WALK.0),
        n(FUEL_PER_SEAT_DAY)
    ));
    out.push_str(&format!(
        "  \"journey\": {{ \"terminal_reach_m\": {}, \"min_leg_ticks\": {}, \"shunting_kph\": {}, \"media\": {}, \"commercial_kph\": {} }},
",
        n(TERMINAL_REACH.0),
        n(MIN_LEG_TICKS),
        n(SHUNTING.as_kph()),
        list(&Medium::ALL, |m| q(&format!("{m:?}"))),
        list(&Medium::ALL, |m| n(m.commercial_speed().as_kph()))
    ));

    // The working day. STANDARD_HOURS is what every authored rate in the
    // building table means, so it is balance in the strongest sense: change it
    // and every output figure in the game means something else.
    out.push_str(&format!(
        "  \"shifts\": {{ \"standard_hours\": {}, \"min_hours\": {}, \"max_hours\": {}, \"daylight_hours\": {}, \"max_shifts\": {}, \"overwork_health\": {}, \"overwork_loyalty\": {} }},
",
        n(STANDARD_HOURS),
        n(MIN_HOURS),
        n(MAX_HOURS),
        n(DAYLIGHT_HOURS),
        MAX_SHIFTS,
        n(OVERWORK_HEALTH),
        n(OVERWORK_LOYALTY)
    ));

    out.push_str(
        "  \"ground\": {
",
    );
    out.push_str(&format!(
        "    \"freeze_c\": {},
",
        n(FREEZE_C)
    ));
    out.push_str(&format!(
        "    \"frost_range_c\": {},
",
        n(FROST_RANGE_C)
    ));
    out.push_str(&format!(
        "    \"frost_lag\": {},
",
        n(FROST_LAG)
    ));
    out.push_str(&format!(
        "    \"saturation_mm\": {},
",
        n(SATURATION_MM)
    ));
    out.push_str(&format!(
        "    \"melt_per_degree_mm\": {},
",
        n(MELT_PER_DEGREE_MM)
    ));
    out.push_str(&format!(
        "    \"drying_per_day\": {},
",
        n(DRYING_PER_DAY)
    ));
    out.push_str(&format!(
        "    \"drying_full_at_c\": {},
",
        n(DRYING_FULL_AT_C)
    ));
    out.push_str(&format!(
        "    \"root_saturation_mm\": {},
",
        n(ROOT_SATURATION_MM)
    ));
    out.push_str(&format!(
        "    \"root_drying_per_day\": {},
",
        n(ROOT_DRYING_PER_DAY)
    ));
    out.push_str(&format!(
        "    \"mud_drag\": {},
",
        n(MUD_DRAG)
    ));
    out.push_str(&format!(
        "    \"drowned\": {},
",
        n(DROWNED)
    ));
    out.push_str(&format!(
        "    \"wear_per_pass\": {},
",
        n(WEAR_PER_PASS)
    ));
    out.push_str(&format!(
        "    \"wear_fade_per_day\": {},
",
        n(WEAR_FADE_PER_DAY)
    ));
    out.push_str(&format!(
        "    \"promote_at\": {},
",
        n(PROMOTE_AT)
    ));
    out.push_str(&format!(
        "    \"snow_blocks_mm\": {},
",
        n(SNOW_BLOCKS_MM)
    ));
    out.push_str(&format!(
        "    \"snow_drag\": {},
",
        n(SNOW_DRAG)
    ));
    out.push_str(&format!(
        "    \"wear_relief\": {},
",
        n(WEAR_RELIEF)
    ));
    out.push_str(&format!(
        "    \"cell_size\": {},
",
        n(GROUND_CELL.0)
    ));
    out.push_str(&format!(
        "    \"min_track_cells\": {},
",
        MIN_TRACK_CELLS
    ));
    out.push_str(&format!(
        "    \"going\": {}
",
        list(
            &[
                Surface::Grass,
                Surface::Forest,
                Surface::Rock,
                Surface::Water
            ],
            |s| n_or_null(going(*s))
        )
    ));
    out.push_str(
        "  },
",
    );

    out.push_str(&format!(
        "  \"heat\": {{ \"threshold_c\": {}, \"design_c\": {}, \"demand_ceiling\": {}, \"wet_day_share\": {} }},\n",
        n(HEAT_THRESHOLD_C),
        n(HEAT_DESIGN_C),
        n(HEAT_DEMAND_CEILING),
        n(WET_DAY_SHARE)
    ));
    out.push_str("  \"climates\": [\n");
    let rows: Vec<String> = CLIMATES
        .iter()
        .map(|c| {
            format!(
                "    {{ \"id\": {}, \"name\": {}, \"monthly_mean_c\": {}, \"daily_swing_c\": {}, \"monthly_rain_mm\": {} }}",
                q(&format!("{:?}", c.id)),
                q(c.name),
                list(&c.monthly_mean_c, |v| n(*v)),
                n(c.daily_swing_c),
                list(&c.monthly_rain_mm, |v| n(*v))
            )
        })
        .collect();
    out.push_str(&rows.join(",\n"));
    out.push_str("\n  ],\n");

    out.push_str(&format!("  \"geology_stream\": {},\n", GEOLOGY_STREAM));
    out.push_str("  \"mineral_plan\": [\n");
    let rows: Vec<String> = DEFAULT_PLAN
        .iter()
        .map(|p| {
            format!(
                "    {{ \"mineral\": {}, \"bodies\": {}, \"radius\": [{}, {}], \"top\": [{}, {}], \"layers\": {}, \"layer_thickness\": [{}, {}], \"tonnes_per_layer\": [{}, {}] }}",
                q(&format!("{:?}", p.mineral)),
                p.bodies,
                n(p.radius.0.0),
                n(p.radius.1.0),
                n(p.top.0.0),
                n(p.top.1.0),
                p.layers,
                n(p.layer_thickness.0.0),
                n(p.layer_thickness.1.0),
                n(p.tonnes_per_layer.0.0),
                n(p.tonnes_per_layer.1.0),
            )
        })
        .collect();
    out.push_str(&rows.join(",\n"));
    out.push_str("\n  ],\n");

    // ---- vehicles ----
    out.push_str("  \"vehicles\": {\n");
    let rows: Vec<String> = VehicleKind::all()
        .map(|v| {
            let d = v.def();
            format!(
                "    {}: {{ \"name\": {}, \"role\": {}, \"medium\": {}, \"capacity_t\": {}, \"seats\": {}, \"on_road_kph\": {}, \"cross_country_kph\": {}, \"fuel_per_km\": {}, \"tank_t\": {}, \"ground\": {}, \"load_penalty\": {} }}",
                q(&format!("{v:?}")),
                q(d.name),
                q(&format!("{:?}", d.role)),
                q(&format!("{:?}", d.medium)),
                n(d.capacity.0),
                d.seats,
                n(d.on_road.as_mps() * 3.6),
                n(d.cross_country.as_mps() * 3.6),
                n(d.fuel_per_km),
                n(d.tank.0),
                n(d.ground),
                n(d.load_penalty)
            )
        })
        .collect();
    out.push_str(&rows.join(",\n"));
    out.push_str("\n  }\n");

    // Written here, after every `canon.push` in this file has certainly run.
    // See the note at the top of `main`.
    print!("{{\n  \"checksum\": \"{:016x}\",\n{out}}}\n", canon.0);
}
