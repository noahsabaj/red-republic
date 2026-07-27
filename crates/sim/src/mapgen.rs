//! Generating a world from a seed.
//!
//! Currently the geology; terrain will join it here.
//!
//! # A shared seed is a promise
//!
//! This is the one part of the simulation that must reproduce **across
//! machines**, not merely across runs of one binary — a seed passed between
//! two players has to give them the same ground. So generation uses integer
//! draws and the four exact float operations (`+ - * /`) only. No `sin`, no
//! `powf`, no `hypot`: transcendental and compound functions are permitted a
//! last-bit difference between platforms and libm versions, and one such bit
//! anywhere here is a different republic.
//!
//! [`GEOLOGY_STREAM`] draws from a substream rather than the world's main
//! stream, so generating a map — or generating several to compare, as the
//! founding screen's candidate shelf does — never disturbs anything else.
//!
//! # The numbers are placeholders
//!
//! [`DEFAULT_PLAN`] is shaped to be plausible, not balanced. Deposit counts,
//! extents, depths and tonnages want a headless campaign runner to set them,
//! and until that exists they are honest guesses rather than measurements.

use crate::geology::{Deposit, DepositId, Geology, Layer, Mineral};
use crate::rng::Rng;
use crate::units::{Metres, Point, Tonnes};

/// Substream identifier for geology, so map generation never shares draws with
/// anything else. See [`crate::world::World::substream`].
pub const GEOLOGY_STREAM: u64 = 1;

/// How to scatter one mineral through the ground.
///
/// Ranges are inclusive-low, exclusive-high, and every value is drawn
/// uniformly. Uniform is deliberate for now: a richer distribution is a
/// balance decision, and pretending to one before there is a campaign to
/// measure would just be a more elaborate guess.
#[derive(Debug, Clone, PartialEq)]
pub struct MineralPlan {
    pub mineral: Mineral,
    /// How many separate bodies to place.
    pub bodies: u32,
    pub radius: (Metres, Metres),
    /// Depth to the top of the body.
    pub top: (Metres, Metres),
    pub layers: u32,
    pub layer_thickness: (Metres, Metres),
    pub tonnes_per_layer: (Tonnes, Tonnes),
}

/// The starting geology of a republic.
///
/// Gravel sits shallow and everywhere — it is the bulk material roads are made
/// of and should never be the thing that strands you. Coal and iron are the
/// bodies worth siting a town around. Oil is scarcer and deeper. Groundwater
/// is broad, shallow, and enormous, because an aquifer is not a resource you
/// hunt for so much as one you either have under you or do not.
pub const DEFAULT_PLAN: [MineralPlan; 5] = [
    MineralPlan {
        mineral: Mineral::Gravel,
        bodies: 6,
        radius: (Metres(150.0), Metres(400.0)),
        top: (Metres(0.0), Metres(8.0)),
        layers: 2,
        layer_thickness: (Metres(3.0), Metres(8.0)),
        tonnes_per_layer: (Tonnes(40_000.0), Tonnes(120_000.0)),
    },
    MineralPlan {
        mineral: Mineral::Coal,
        bodies: 4,
        radius: (Metres(200.0), Metres(600.0)),
        top: (Metres(20.0), Metres(120.0)),
        layers: 4,
        layer_thickness: (Metres(8.0), Metres(25.0)),
        tonnes_per_layer: (Tonnes(150_000.0), Tonnes(600_000.0)),
    },
    MineralPlan {
        mineral: Mineral::IronOre,
        bodies: 3,
        radius: (Metres(150.0), Metres(450.0)),
        top: (Metres(30.0), Metres(160.0)),
        layers: 3,
        layer_thickness: (Metres(10.0), Metres(30.0)),
        tonnes_per_layer: (Tonnes(100_000.0), Tonnes(400_000.0)),
    },
    MineralPlan {
        mineral: Mineral::Oil,
        bodies: 2,
        radius: (Metres(250.0), Metres(700.0)),
        top: (Metres(400.0), Metres(1_200.0)),
        layers: 3,
        layer_thickness: (Metres(20.0), Metres(60.0)),
        tonnes_per_layer: (Tonnes(80_000.0), Tonnes(300_000.0)),
    },
    MineralPlan {
        mineral: Mineral::Groundwater,
        bodies: 3,
        radius: (Metres(800.0), Metres(2_000.0)),
        top: (Metres(4.0), Metres(40.0)),
        layers: 1,
        layer_thickness: (Metres(10.0), Metres(40.0)),
        tonnes_per_layer: (Tonnes(500_000.0), Tonnes(2_000_000.0)),
    },
];

/// Draw a metre value uniformly from a range.
///
/// Multiply-and-add only — see the module docs on why nothing fancier is
/// allowed in here.
fn metres_in(rng: &mut Rng, range: (Metres, Metres)) -> Metres {
    Metres(rng.next_range(range.0.0, range.1.0))
}

fn tonnes_in(rng: &mut Rng, range: (Tonnes, Tonnes)) -> Tonnes {
    Tonnes(rng.next_range(range.0.0, range.1.0))
}

/// Generate the geology of a square map `extent` metres on a side.
///
/// Bodies are centred anywhere within the extent — including near an edge,
/// where part of the body simply runs off the map. That is deliberate: a seam
/// that continues past the border is more honest than one that politely stops,
/// and it means the edge of the map is not quietly the richest place to build.
pub fn generate_geology(seed: u64, extent: Metres, plan: &[MineralPlan]) -> Geology {
    let mut rng = Rng::from_seed(seed);
    let mut geology = Geology::new();
    let mut next_id = 1u32;

    // Plan order, then body order — fixed, because the draw order IS the map.
    for entry in plan {
        for _ in 0..entry.bodies {
            let centre = Point::new(
                Metres(rng.next_range(0.0, extent.0)),
                Metres(rng.next_range(0.0, extent.0)),
            );
            let radius = metres_in(&mut rng, entry.radius);
            let top = metres_in(&mut rng, entry.top);

            let layers = (0..entry.layers)
                .map(|_| {
                    let thickness = metres_in(&mut rng, entry.layer_thickness);
                    let tonnes = tonnes_in(&mut rng, entry.tonnes_per_layer);
                    Layer::new(thickness, tonnes)
                })
                .collect();

            geology.insert(Deposit::new(
                DepositId(next_id),
                entry.mineral,
                centre,
                radius,
                top,
                layers,
            ));
            next_id += 1;
        }
    }

    geology
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXTENT: Metres = Metres(10_000.0); // a 10 km republic

    fn generate(seed: u64) -> Geology {
        generate_geology(seed, EXTENT, &DEFAULT_PLAN)
    }

    /// The promise between players: same seed, same ground.
    #[test]
    fn the_same_seed_generates_the_same_ground() {
        assert_eq!(generate(1961), generate(1961));
    }

    #[test]
    fn different_seeds_generate_different_ground() {
        assert_ne!(generate(1961), generate(1962));
    }

    /// The cross-machine tripwire.
    ///
    /// Pins a fingerprint of the whole generated geology for one seed. It
    /// fails if the draw order changes, if the plan changes, or — the case it
    /// really exists for — if generation ever picks up a float operation that
    /// is allowed to differ in its last bit between platforms. Any of those
    /// means two players with the same seed get different republics.
    ///
    /// A deliberate change to worldgen legitimately moves this number. Moving
    /// it to make a test pass without knowing why is how the promise breaks.
    #[test]
    fn generated_ground_is_pinned_across_machines() {
        let g = generate(1961);
        // FNV-1a over the field values, in a fixed order. Hand-rolled rather
        // than DefaultHasher, whose algorithm is not stable across releases.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |x: f64| {
            for byte in x.to_bits().to_le_bytes() {
                h ^= u64::from(byte);
                h = h.wrapping_mul(0x1000_0000_01b3);
            }
        };
        for d in g.all() {
            eat(f64::from(d.id.0));
            eat(d.centre.x.0);
            eat(d.centre.y.0);
            eat(d.radius.0);
            eat(d.top.0);
            for layer in &d.layers {
                eat(layer.thickness.0);
                eat(layer.initial.0);
            }
        }
        assert_eq!(h, 5_663_282_509_811_758_942);
    }

    #[test]
    fn every_planned_body_is_placed() {
        let g = generate(7);
        let expected: u32 = DEFAULT_PLAN.iter().map(|p| p.bodies).sum();
        assert_eq!(g.all().len(), expected as usize);
        for entry in &DEFAULT_PLAN {
            let placed = g
                .all()
                .iter()
                .filter(|d| d.mineral == entry.mineral)
                .count();
            assert_eq!(placed, entry.bodies as usize, "{:?}", entry.mineral);
        }
    }

    #[test]
    fn deposit_ids_are_unique() {
        let g = generate(11);
        let mut ids: Vec<_> = g.all().iter().map(|d| d.id).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before);
    }

    /// Bodies may run off the edge, but their centres stay on the map and
    /// every drawn quantity stays inside the range the plan asked for.
    #[test]
    fn every_body_respects_its_plan() {
        let g = generate(2024);
        for d in g.all() {
            let entry = DEFAULT_PLAN
                .iter()
                .find(|p| p.mineral == d.mineral)
                .expect("every generated mineral is planned");

            assert!((0.0..EXTENT.0).contains(&d.centre.x.0));
            assert!((0.0..EXTENT.0).contains(&d.centre.y.0));
            assert!((entry.radius.0.0..entry.radius.1.0).contains(&d.radius.0));
            assert!((entry.top.0.0..entry.top.1.0).contains(&d.top.0));
            assert_eq!(d.layers.len(), entry.layers as usize);

            for layer in &d.layers {
                assert!(
                    (entry.layer_thickness.0.0..entry.layer_thickness.1.0)
                        .contains(&layer.thickness.0)
                );
                assert!(
                    (entry.tonnes_per_layer.0.0..entry.tonnes_per_layer.1.0)
                        .contains(&layer.initial.0)
                );
                assert_eq!(layer.remaining, layer.initial, "a fresh map is untouched");
            }
        }
    }

    /// The founding shelf reads exactly this to say whether a posting is
    /// coal-rich or coal-poor, so it has to answer at all.
    #[test]
    fn a_generated_map_can_be_asked_what_it_holds() {
        let g = generate(1961);
        for mineral in Mineral::ALL {
            assert!(
                g.remaining_of(mineral).is_positive(),
                "{mineral:?} was planned but the map holds none"
            );
        }
    }

    /// Quality varies — some seeds put coal near the middle of the map and
    /// others put it a long walk away. That spread is what makes a shelf of
    /// candidates a real choice rather than six ways to say yes.
    #[test]
    fn distance_to_coal_from_the_map_centre_varies_by_seed() {
        let centre = Point::new(Metres(EXTENT.0 / 2.0), Metres(EXTENT.0 / 2.0));
        let distances: Vec<f64> = (0..40)
            .map(|seed| {
                generate(seed)
                    .distance_to_nearest(centre, Mineral::Coal)
                    .expect("coal is always planned")
                    .0
            })
            .collect();

        let min = distances.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = distances.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(
            max - min > 1_000.0,
            "coal distance spanned only {:.0} m across 40 seeds — too uniform to choose between",
            max - min
        );
    }
}
