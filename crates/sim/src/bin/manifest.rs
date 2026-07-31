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
use red_republic_sim::citizen::Education;
use red_republic_sim::fleet::VehicleKind;
use red_republic_sim::journey::Medium;
use red_republic_sim::resource::{Form, Resource};

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
    let mut out = String::new();
    out.push_str("{\n");

    let mut canon = Canon::new();
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
    out.push_str(&format!("  \"checksum\": \"{:016x}\",\n", canon.0));

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
                "    {}: {{ \"name\": {}, \"form\": {} }}",
                q(&format!("{r:?}")),
                q(r.name()),
                q(&format!("{:?}", r.form()))
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

    out.push_str("}\n");
    println!("{out}");
}
