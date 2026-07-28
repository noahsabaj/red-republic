//! The republic's lorries: machines with a position, a load, a tank and a job.
//!
//! # Why this replaced a scalar
//!
//! Freight used to be `FREIGHT_TONNES_PER_DAY`, a number, and its own doc
//! comment admitted what it was standing in for. A scalar moves goods by
//! teleporting them: the tonnage leaves the supplier and lands at the
//! destination in the same instant, at the same rate whatever the distance,
//! whatever the ground, and burning nothing. Nothing about that is visible, and
//! at the game's slowest and most interesting speed — one real second to one
//! simulated second — a republic whose freight is a number is indistinguishable
//! from a republic that is paused.
//!
//! A vehicle is the physical answer, and it makes four things true that a
//! number could not: a haul takes **time proportional to the distance**, it
//! burns **fuel** that the same refinery output is already fighting over, it can
//! be **watched**, and a road is worth building because it is measurably faster
//! than the open ground beside it.
//!
//! # Garages own vehicles
//!
//! Carried from the archived build, where it earned its keep: fleet size is not
//! a setting, it is a consequence of what has been built and staffed. A Motor
//! Depot has an **establishment** — how many of each kind it keeps — authored on
//! the building table beside everything else about it, and [`crewed`] says how
//! many of them have drivers today. Wanting more lorries means building another
//! depot and finding sixteen more people for it.
//!
//! # A vehicle never accepts a job it cannot finish
//!
//! Also carried, and the reason a tank is per-vehicle rather than a pool at the
//! garage. Running dry is a **refusal at dispatch**, not a lorry stranded in a
//! field: the round trip is priced before the job is taken, and a depot with no
//! fuel simply sends nobody out. What *can* strand a vehicle is ground it cannot
//! cross, which is a later phase and a different mechanic — that one is meant to
//! happen, so it is deliberately not prevented here.

use crate::building::{Building, BuildingId};
use crate::journey::Journey;
use crate::resource::{Resource, Stock};
use crate::roadworks::RoadSiteId;
use crate::units::{Metres, Point, Speed, Tonnes};
use serde::{Deserialize, Serialize};

/// A stable handle to a vehicle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VehicleId(pub u32);

/// Every kind of vehicle the republic runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VehicleKind {
    Lorry,
    HeavyLorry,
}

/// What a vehicle is and what it can do.
///
/// Every field is authored on every kind, for the same reason every field of
/// [`crate::building::BuildingDef`] is: a defaulted value is a decision nobody
/// made, and adding a field here is then a compile error at each row rather
/// than a silent zero on the rows somebody forgot.
///
/// The speeds are real quantities in the sense [`crate::units`] means it. The
/// archived build's vehicle speed was measured in road-tile-equivalents per day
/// and carried a comment admitting no real-world unit could be recovered from
/// it; these are kilometres per hour, and a lorry that covers 15 km in an hour
/// of open ground covers 15 km in an hour of open ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleDef {
    pub kind: VehicleKind,
    pub name: &'static str,
    /// Tonnes it can carry in one load.
    pub capacity: Tonnes,
    /// Speed on the road network.
    pub on_road: Speed,
    /// Speed over open ground. The gap between this and [`VehicleDef::on_road`]
    /// is the entire argument for building a road.
    pub cross_country: Speed,
    /// Tonnes of fuel burnt per kilometre driven.
    pub fuel_per_km: f64,
    /// Tonnes of fuel the tank holds.
    pub tank: Tonnes,
}

/// The vehicle table. First-pass balance, meant to be felt out against the
/// trajectory runner rather than reasoned to.
///
/// The heavy lorry is the trade the player actually makes: two and a half times
/// the load, and barely half the speed once it leaves the tarmac. It is worth
/// having on a road and a liability without one.
pub const VEHICLES: &[VehicleDef] = &[
    VehicleDef {
        kind: VehicleKind::Lorry,
        name: "Lorry",
        capacity: Tonnes(8.0),
        on_road: Speed::from_kph(50.0),
        cross_country: Speed::from_kph(15.0),
        // 0.3 kg/km — about 36 litres of diesel per hundred kilometres.
        fuel_per_km: 0.0003,
        tank: Tonnes(0.15),
    },
    VehicleDef {
        kind: VehicleKind::HeavyLorry,
        name: "Heavy Lorry",
        capacity: Tonnes(20.0),
        on_road: Speed::from_kph(45.0),
        cross_country: Speed::from_kph(8.0),
        fuel_per_km: 0.0006,
        tank: Tonnes(0.30),
    },
];

impl VehicleKind {
    pub fn def(self) -> &'static VehicleDef {
        VEHICLES
            .iter()
            .find(|d| d.kind == self)
            .expect("every kind is in the table — guarded by a test")
    }

    /// Every kind, in table order.
    pub fn all() -> impl Iterator<Item = VehicleKind> {
        VEHICLES.iter().map(|d| d.kind)
    }
}

/// What a vehicle is doing.
///
/// Deliberately finer than idle-or-running: the fleet system decides what
/// happens when a journey ends by looking at this, and "arrived somewhere" has
/// three different meanings depending on which way the lorry was pointed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VehicleState {
    /// Parked at its garage, available for work.
    Idle,
    /// Driving empty to the supplier.
    Fetching,
    /// Driving laden to the destination.
    Delivering,
    /// Driving back to its garage.
    Returning,
}

/// Somewhere goods can be delivered.
///
/// Freight used to reach only buildings, which was fine while roads appeared by
/// someone calling `connect`. A road under construction is a site with a bill of
/// materials like any other, and the gravel has to be *driven* to it — so the
/// delivery half of the simulation has to be able to name one.
///
/// Goods are never collected *from* a road site, which is why [`Job::from`] is
/// still a plain [`BuildingId`]: a site is a place work goes into, not a yard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Destination {
    Building(BuildingId),
    RoadSite(RoadSiteId),
}

/// A haul: take this much of this, from here to there.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub from: BuildingId,
    pub to: Destination,
    pub resource: Resource,
    pub tonnes: Tonnes,
}

/// A lorry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vehicle {
    pub id: VehicleId,
    pub kind: VehicleKind,
    /// The garage that owns it. A vehicle outlives its jobs but not its depot.
    pub home: BuildingId,
    pub state: VehicleState,
    /// Where it stands, updated at leg boundaries. Between boundaries the
    /// smooth position comes from [`Journey::position_at`] — this is the last
    /// waypoint it actually reached, which is what the simulation reasons about.
    pub at: Point,
    pub journey: Option<Journey>,
    pub cargo: Stock,
    pub fuel: Tonnes,
    pub job: Option<Job>,
}

impl Vehicle {
    pub fn def(&self) -> &'static VehicleDef {
        self.kind.def()
    }

    pub fn is_idle(&self) -> bool {
        self.state == VehicleState::Idle
    }

    /// What it would burn covering `distance`.
    pub fn fuel_for(&self, distance: Metres) -> Tonnes {
        Tonnes(self.def().fuel_per_km * distance.as_km())
    }

    /// Room left in the load bed.
    pub fn spare_capacity(&self) -> Tonnes {
        self.def().capacity.saturating_sub(self.cargo.total())
    }

    /// The speed it makes on the leg it is currently driving — its own pace,
    /// capped by whatever the road under it allows.
    pub fn leg_speed(&self) -> Speed {
        let def = self.def();
        match &self.journey {
            Some(j) => j.leg_speed(def.on_road, def.cross_country),
            None => Speed::ZERO,
        }
    }
}

/// Every vehicle the republic runs.
///
/// A `Vec` rather than the ECS, and deliberately: [`crate::citizen::Population`]
/// is ECS because it holds thousands and needed hand-written serialization,
/// while vehicles number in the hundreds and a `Vec` derives everything for
/// free. If the fleet ever reaches thousands this is worth revisiting — as is
/// the O(vehicles) per-tick scan, which assumes the same thing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Fleet {
    list: Vec<Vehicle>,
    next_id: u32,
}

impl Fleet {
    pub fn new() -> Self {
        Self {
            list: Vec::new(),
            next_id: 1,
        }
    }

    pub fn all(&self) -> &[Vehicle] {
        &self.list
    }

    pub fn get(&self, id: VehicleId) -> Option<&Vehicle> {
        self.list.iter().find(|v| v.id == id)
    }

    pub fn get_mut(&mut self, id: VehicleId) -> Option<&mut Vehicle> {
        self.list.iter_mut().find(|v| v.id == id)
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// How many are out on a job rather than parked.
    pub fn running(&self) -> usize {
        self.list.iter().filter(|v| !v.is_idle()).count()
    }

    /// Put a new vehicle on the strength, parked at its garage with a full tank.
    ///
    /// It arrives fuelled because otherwise a republic's first lorry could never
    /// move: it would need fuel delivered, and delivering fuel needs a lorry.
    pub fn commission(&mut self, kind: VehicleKind, home: BuildingId, at: Point) -> VehicleId {
        let id = VehicleId(self.next_id);
        self.next_id += 1;
        self.list.push(Vehicle {
            id,
            kind,
            home,
            state: VehicleState::Idle,
            at,
            journey: None,
            cargo: Stock::EMPTY,
            fuel: kind.def().tank,
            job: None,
        });
        id
    }

    /// The vehicles a garage owns, in commissioning order.
    pub fn of_garage(&self, home: BuildingId) -> impl Iterator<Item = &Vehicle> {
        self.list.iter().filter(move |v| v.home == home)
    }

    pub fn count_of(&self, home: BuildingId, kind: VehicleKind) -> u32 {
        self.list
            .iter()
            .filter(|v| v.home == home && v.kind == kind)
            .count() as u32
    }

    /// Everything carried by everything on the road — the tonnage that exists
    /// but is not in any building's bin.
    pub fn cargo_afloat(&self, resource: Resource) -> Tonnes {
        self.list.iter().map(|v| v.cargo.get(resource)).sum()
    }
}

/// How many of a garage's establishment have drivers today.
///
/// The same shape as every other staffing limiter in the simulation: a
/// half-staffed depot runs half its lorries rather than switching off at a
/// threshold nobody can see. It is checked **at dispatch only** — a lorry
/// already out on a job finishes it, because a republic that loses a shift does
/// not abandon its vehicles in a field.
pub fn crewed(garage: &Building) -> u32 {
    if !garage.is_built() {
        return 0;
    }
    let establishment: u32 = garage.def().vehicles.iter().map(|&(_, n)| n).sum();
    (f64::from(establishment) * garage.staffing()).floor() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{BUILDINGS, BuildingKind, Buildings};
    use crate::geology::Geology;
    use crate::terrain::Terrain;

    fn at(x: f64, y: f64) -> Point {
        Point::new(Metres(x), Metres(y))
    }

    /// The guard that makes the table a contract: a kind with no row would
    /// otherwise panic the first time anybody drove one.
    #[test]
    fn every_vehicle_kind_is_in_the_table_exactly_once() {
        for kind in VehicleKind::all() {
            let rows = VEHICLES.iter().filter(|d| d.kind == kind).count();
            assert_eq!(rows, 1, "{kind:?} has {rows} rows");
        }
    }

    /// Every field is authored, and authored plausibly. A zero here is not a
    /// balance choice, it is a lorry that cannot move or cannot carry.
    #[test]
    fn every_vehicle_is_fully_authored() {
        for def in VEHICLES {
            assert!(def.capacity.is_positive(), "{} carries nothing", def.name);
            assert!(def.on_road > Speed::ZERO, "{} cannot move", def.name);
            assert!(
                def.cross_country > Speed::ZERO,
                "{} cannot leave the road",
                def.name
            );
            assert!(
                def.cross_country < def.on_road,
                "{} is no slower off road, so no road is worth building",
                def.name
            );
            assert!(def.fuel_per_km > 0.0, "{} burns nothing", def.name);
            assert!(def.tank.is_positive(), "{} has no tank", def.name);
        }
    }

    /// A tank that could not reach the far side of a republic and back would
    /// make the dispatch-time fuel check refuse every long haul, which reads as
    /// a broken fleet rather than as a balance decision.
    #[test]
    fn a_full_tank_crosses_a_republic_and_comes_back() {
        for def in VEHICLES {
            let range = Metres::from_km(def.tank.0 / def.fuel_per_km);
            assert!(
                range.as_km() >= 100.0,
                "{} has {:.0} km of range, less than a round trip across a 10 km map",
                def.name,
                range.as_km()
            );
        }
    }

    /// Every garage in the building table keeps vehicles that exist, and
    /// anything that keeps none says so by having an empty establishment rather
    /// than by being absent from a list somewhere.
    #[test]
    fn a_garage_keeps_only_vehicles_that_exist() {
        let mut garages = 0;
        for def in BUILDINGS {
            if def.vehicles.is_empty() {
                continue;
            }
            garages += 1;
            for &(kind, count) in def.vehicles {
                assert!(count > 0, "{} keeps zero {kind:?}", def.name);
                assert_eq!(
                    VEHICLES.iter().filter(|v| v.kind == kind).count(),
                    1,
                    "{} keeps a {kind:?}, which is not in the vehicle table",
                    def.name
                );
            }
        }
        assert!(garages > 0, "no building keeps any vehicles at all");
    }

    fn garage() -> (Buildings, BuildingId) {
        let mut b = Buildings::new();
        let t = Terrain::flat(Metres(4_000.0));
        let g = Geology::new();
        let id = b
            .place_built(BuildingKind::MotorDepot, at(1_000.0, 1_000.0), &t, &g)
            .expect("open ground");
        (b, id)
    }

    #[test]
    fn an_unstaffed_garage_runs_nothing_and_a_full_one_runs_everything() {
        let (mut b, id) = garage();
        assert_eq!(crewed(b.get(id).unwrap()), 0, "nobody to drive");

        let def = BuildingKind::MotorDepot.def();
        let establishment: u32 = def.vehicles.iter().map(|&(_, n)| n).sum();
        b.get_mut(id).unwrap().staff = def.workers;
        assert_eq!(crewed(b.get(id).unwrap()), establishment);
    }

    #[test]
    fn a_half_staffed_garage_runs_half_its_lorries() {
        let (mut b, id) = garage();
        let def = BuildingKind::MotorDepot.def();
        let establishment: u32 = def.vehicles.iter().map(|&(_, n)| n).sum();
        b.get_mut(id).unwrap().staff = def.workers / 2;
        let running = crewed(b.get(id).unwrap());
        assert!(
            running > 0 && running <= establishment / 2 + 1,
            "{running} of {establishment} on half a shift"
        );
    }

    /// A site is not a garage. Vehicles arrive when the depot opens, not when
    /// its foundations are poured.
    #[test]
    fn a_half_built_garage_owns_nothing() {
        let mut b = Buildings::new();
        let t = Terrain::flat(Metres(4_000.0));
        let g = Geology::new();
        let id = b
            .place(BuildingKind::MotorDepot, at(1_000.0, 1_000.0), &t, &g)
            .expect("open ground");
        b.get_mut(id).unwrap().staff = BuildingKind::MotorDepot.def().workers;
        assert_eq!(crewed(b.get(id).unwrap()), 0);
    }

    #[test]
    fn a_commissioned_lorry_arrives_parked_and_fuelled() {
        let mut fleet = Fleet::new();
        let id = fleet.commission(VehicleKind::Lorry, BuildingId(1), at(500.0, 500.0));
        let v = fleet.get(id).expect("just commissioned");
        assert!(v.is_idle());
        assert!(v.job.is_none() && v.journey.is_none());
        assert_eq!(v.fuel, VehicleKind::Lorry.def().tank);
        assert_eq!(v.spare_capacity(), VehicleKind::Lorry.def().capacity);
        assert_eq!(fleet.count_of(BuildingId(1), VehicleKind::Lorry), 1);
        assert_eq!(fleet.count_of(BuildingId(2), VehicleKind::Lorry), 0);
    }

    #[test]
    fn fuel_is_charged_against_the_kilometres_actually_driven() {
        let mut fleet = Fleet::new();
        let id = fleet.commission(VehicleKind::Lorry, BuildingId(1), Point::ORIGIN);
        let v = fleet.get(id).unwrap();
        let burn = v.fuel_for(Metres::from_km(100.0));
        assert!((burn.0 - 100.0 * VehicleKind::Lorry.def().fuel_per_km).abs() < 1e-12);
        assert_eq!(v.fuel_for(Metres::ZERO), Tonnes::ZERO);
    }
}
