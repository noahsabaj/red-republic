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
use crate::journey::{Journey, Medium};
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
    RecoveryVehicle,
    CrewBus,
    Coach,
    Locomotive,
    PassengerTrain,
    Barge,
    Freighter,
}

/// What a vehicle is for.
///
/// Replaces a `recovers: bool`, which was a fine way to divide a world with two
/// kinds of vehicle in it and stopped being one the moment a third arrived: a
/// crew bus recovers nothing, so under the old flag it read as a lorry and
/// dispatch offered it freight it has no bed for. A vehicle has exactly one job,
/// and saying so in the type is what keeps the three pools from ever competing
/// for the same driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Role {
    /// Carries tonnage.
    Freight,
    /// Carries builders between an office and its sites.
    Crew,
    /// Pulls the others out of fields.
    Recovery,
    /// Carries settlers from a frontier post to housing.
    ///
    /// Its own role rather than a second use for [`Role::Crew`], because the
    /// two pools must never compete: a republic that stopped building because
    /// its buses were fetching immigrants — or that stranded a hundred people
    /// at the border because a foundation wanted a gang — would be a republic
    /// where two unrelated decisions share one budget for no reason anybody
    /// chose.
    Passenger,
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
    pub role: Role,
    /// The way it gets about. A lorry prefers roads; everything else is
    /// **confined** to its own network and simply refuses work it cannot
    /// reach — see [`crate::journey::Medium`].
    pub medium: Medium,
    /// Tonnes it can carry in one load. Zero on anything that is not freight.
    pub capacity: Tonnes,
    /// Builders it can carry. Zero on anything that is not a crew bus.
    pub seats: u32,
    /// Speed on the road network.
    pub on_road: Speed,
    /// Speed over open ground. The gap between this and [`VehicleDef::on_road`]
    /// is the entire argument for building a road.
    pub cross_country: Speed,
    /// Tonnes of fuel burnt per kilometre driven.
    pub fuel_per_km: f64,
    /// Tonnes of fuel the tank holds.
    pub tank: Tonnes,
    /// The worst going it can cross empty, on the same `0.0` firm to `1.0`
    /// impassable scale the ground uses. Above `1.0` is a vehicle that
    /// crosses anything.
    pub ground: f64,
    /// How much of that capability a full load costs it. A laden lorry sits
    /// deeper and pulls worse, and a heavy one much more so — which is the
    /// whole reason the big lorry is a road vehicle.
    pub load_penalty: f64,
}

impl VehicleDef {
    pub fn recovers(&self) -> bool {
        self.role == Role::Recovery
    }
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
        medium: Medium::Road,
        name: "Lorry",
        role: Role::Freight,
        capacity: Tonnes(8.0),
        seats: 0,
        on_road: Speed::from_kph(50.0),
        cross_country: Speed::from_kph(15.0),
        // 0.3 kg/km — about 36 litres of diesel per hundred kilometres.
        fuel_per_km: 0.0003,
        tank: Tonnes(0.15),
        ground: 0.75,
        load_penalty: 0.35,
    },
    VehicleDef {
        kind: VehicleKind::HeavyLorry,
        medium: Medium::Road,
        name: "Heavy Lorry",
        role: Role::Freight,
        capacity: Tonnes(20.0),
        seats: 0,
        on_road: Speed::from_kph(45.0),
        cross_country: Speed::from_kph(8.0),
        fuel_per_km: 0.0006,
        tank: Tonnes(0.30),
        ground: 0.55,
        load_penalty: 0.40,
    },
    // Carries nothing and goes everywhere. Its capability is deliberately
    // above one: a recovery vehicle that could itself get stuck would need
    // recovering, and a republic would spend its afternoon sending a second
    // one after the first.
    VehicleDef {
        kind: VehicleKind::RecoveryVehicle,
        medium: Medium::Road,
        name: "Recovery Vehicle",
        role: Role::Recovery,
        capacity: Tonnes::ZERO,
        seats: 0,
        on_road: Speed::from_kph(55.0),
        cross_country: Speed::from_kph(20.0),
        fuel_per_km: 0.0004,
        tank: Tonnes(0.20),
        ground: 1.2,
        load_penalty: 0.0,
    },
    // The second physical hop in construction: builders are employed at an
    // office and this is what puts them on a site. Quicker than a lorry and
    // worse across country than one, which is the trade that makes a remote
    // site want a road before it wants anything else.
    //
    // Its load penalty is zero and that is authored rather than defaulted: a
    // dozen people weigh a rounding error against eight tonnes of brick, so a
    // full bus crosses exactly what an empty one crosses. It is the one vehicle
    // whose capability does not depend on what it is carrying.
    VehicleDef {
        kind: VehicleKind::CrewBus,
        medium: Medium::Road,
        name: "Crew Bus",
        role: Role::Crew,
        capacity: Tonnes::ZERO,
        seats: 10,
        on_road: Speed::from_kph(60.0),
        cross_country: Speed::from_kph(16.0),
        fuel_per_km: 0.00025,
        tank: Tonnes(0.12),
        ground: 0.7,
        load_penalty: 0.0,
    },
    // What brings settlers in from a frontier post. Bigger than a crew bus
    // because a group at the border is a group rather than a gang, and worse
    // across country because it is a road coach: a republic that wants the
    // people arriving at its far post has to build road out to it, which is the
    // same answer trade already gives about the same posts.
    VehicleDef {
        kind: VehicleKind::Coach,
        medium: Medium::Road,
        name: "Coach",
        role: Role::Passenger,
        capacity: Tonnes::ZERO,
        seats: 24,
        on_road: Speed::from_kph(65.0),
        cross_country: Speed::from_kph(12.0),
        fuel_per_km: 0.00035,
        tank: Tonnes(0.20),
        ground: 0.6,
        load_penalty: 0.0,
    },
    // Everything below here is **confined**: it cannot leave its own network,
    // and that is authored rather than enforced by a rule elsewhere. A zero
    // `cross_country` and a zero `ground` say exactly that — a locomotive off
    // the rails is not a slow locomotive, it is a locomotive that is not going
    // anywhere. The planner never asks either question of them, because a
    // journey that leaves the network is one it will not return at all.
    //
    // The trade against a lorry is the same in all three cases and it is a real
    // one: an enormous load, moved for very little fuel, along a line that goes
    // exactly where it was laid and nowhere else. A mine feeding one works
    // across the republic wants rails. A mine feeding six things scattered over
    // a valley wants lorries, and no amount of track will change that.
    VehicleDef {
        kind: VehicleKind::Locomotive,
        medium: Medium::Rail,
        name: "Locomotive",
        role: Role::Freight,
        // Fifteen lorries in one train, and the whole point of building rails.
        capacity: Tonnes(120.0),
        seats: 0,
        on_road: Speed::from_kph(70.0),
        cross_country: Speed::ZERO,
        // Twice a heavy lorry's burn for six times its load: a third of the
        // fuel per tonne-kilometre.
        fuel_per_km: 0.0012,
        tank: Tonnes(1.20),
        ground: 0.0,
        load_penalty: 0.0,
    },
    VehicleDef {
        kind: VehicleKind::PassengerTrain,
        medium: Medium::Rail,
        name: "Passenger Train",
        role: Role::Passenger,
        capacity: Tonnes::ZERO,
        seats: 240,
        on_road: Speed::from_kph(80.0),
        cross_country: Speed::ZERO,
        fuel_per_km: 0.0009,
        tank: Tonnes(0.90),
        ground: 0.0,
        load_penalty: 0.0,
    },
    // The cheapest tonne-kilometre in the republic by a wide margin, and the
    // slowest. It also costs nothing to build the network it rides, because
    // nobody built it — which is the whole character of water: an enormous
    // asset that runs exactly where the land decided it runs.
    VehicleDef {
        kind: VehicleKind::Barge,
        medium: Medium::Water,
        name: "Barge",
        role: Role::Freight,
        capacity: Tonnes(200.0),
        seats: 0,
        on_road: Speed::from_kph(18.0),
        cross_country: Speed::ZERO,
        fuel_per_km: 0.0008,
        tank: Tonnes(1.00),
        ground: 0.0,
        load_penalty: 0.0,
    },
    // Fast, small, and dear enough that it is for what cannot wait rather than
    // for tonnage. A republic that hauls coal by air is a republic that has
    // misunderstood something.
    VehicleDef {
        kind: VehicleKind::Freighter,
        medium: Medium::Air,
        name: "Freighter",
        role: Role::Freight,
        capacity: Tonnes(15.0),
        seats: 0,
        on_road: Speed::from_kph(320.0),
        cross_country: Speed::ZERO,
        fuel_per_km: 0.0090,
        tank: Tonnes(4.00),
        ground: 0.0,
        load_penalty: 0.0,
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
    /// Driving empty to the supplier, or out to a casualty.
    Fetching,
    /// Driving laden to the destination.
    Delivering,
    /// Driving back to its garage.
    Returning,
    /// Stuck. It keeps its job and its plan and is going nowhere until the
    /// ground firms up under it or something comes and pulls it out.
    Bogged {
        /// What it was doing when it stuck, so it can carry on doing it.
        was: Doing,
        /// The day it stuck. How long it has been there is what decides
        /// whether a tow is sent — see [`HELP_AFTER`].
        since_day: u64,
    },
}

/// The three things a vehicle can be in the middle of, without the state of
/// being stuck. Exists so [`VehicleState::Bogged`] can remember what to go
/// back to without the type being able to represent bogged-inside-bogged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Doing {
    Fetching,
    Delivering,
    Returning,
}

impl Doing {
    pub fn state(self) -> VehicleState {
        match self {
            Doing::Fetching => VehicleState::Fetching,
            Doing::Delivering => VehicleState::Delivering,
            Doing::Returning => VehicleState::Returning,
        }
    }
}

impl VehicleState {
    /// What it is in the middle of, if anything — the same answer whether or
    /// not it is currently stuck in it.
    pub fn doing(self) -> Option<Doing> {
        match self {
            VehicleState::Idle => None,
            VehicleState::Fetching => Some(Doing::Fetching),
            VehicleState::Delivering => Some(Doing::Delivering),
            VehicleState::Returning => Some(Doing::Returning),
            VehicleState::Bogged { was, .. } => Some(was),
        }
    }

    pub fn is_bogged(self) -> bool {
        matches!(self, VehicleState::Bogged { .. })
    }
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
    /// A power line or a heat main under construction. Same reasoning as
    /// a road site: it has a bill of materials and somebody has to drive
    /// the steel out to it.
    LineSite(crate::utility::LineSiteId),
}

/// What a vehicle has been sent out to do.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Job {
    /// Take this much of this, from here to there.
    Haul {
        from: BuildingId,
        to: Destination,
        resource: Resource,
        tonnes: Tonnes,
    },
    /// Go and pull that one out.
    ///
    /// A separate kind rather than a haul with odd fields, because the
    /// journey is to a *vehicle* rather than to a place, and the thing that
    /// happens on arrival is nothing like unloading.
    Recover { casualty: VehicleId },
    /// Carry builders from the office out to a site.
    ///
    /// The bus sets off **laden**, which is why this is not a fetch-then-deliver
    /// like a haul: the crew is at the office and the office is the bus's home,
    /// so there is nothing to drive out empty for.
    Ferry { to: Destination, heads: u32 },
    /// Go and bring a crew back.
    ///
    /// Named by the party rather than by the place, because what is being
    /// collected can be standing beside a road that now exists and has no site
    /// left to refer to.
    Collect { party: crate::crews::PartyId },
    /// Fetch a group of settlers from a frontier post and take them to housing.
    ///
    /// Both halves are named at dispatch because the coach has to be able to
    /// make the *whole* trip — post, then estate, then home — before it accepts
    /// the work. A coach that ran dry with a hundred people aboard would be the
    /// stranded-gang failure again, with more people in it.
    Settle {
        group: crate::migration::GroupId,
        to: BuildingId,
    },
}

impl Job {
    /// The haul, if it is one.
    pub fn haul(self) -> Option<(BuildingId, Destination, Resource, Tonnes)> {
        match self {
            Job::Haul {
                from,
                to,
                resource,
                tonnes,
            } => Some((from, to, resource, tonnes)),
            _ => None,
        }
    }

    pub fn casualty(self) -> Option<VehicleId> {
        match self {
            Job::Recover { casualty } => Some(casualty),
            _ => None,
        }
    }

    /// The site a crew is being carried to, if this is a ferry.
    pub fn ferry(self) -> Option<(Destination, u32)> {
        match self {
            Job::Ferry { to, heads } => Some((to, heads)),
            _ => None,
        }
    }

    pub fn party(self) -> Option<crate::crews::PartyId> {
        match self {
            Job::Collect { party } => Some(party),
            _ => None,
        }
    }

    /// The group of settlers being fetched, and where they are being taken.
    pub fn settling(self) -> Option<(crate::migration::GroupId, BuildingId)> {
        match self {
            Job::Settle { group, to } => Some((group, to)),
            _ => None,
        }
    }
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
    /// capped by whatever the road under it allows and slowed by `drag`
    /// wherever it is off road.
    pub fn leg_speed(&self, drag: f64) -> Speed {
        let def = self.def();
        match &self.journey {
            Some(j) => j.leg_speed(def.on_road, def.cross_country, drag),
            None => Speed::ZERO,
        }
    }

    /// How much of its bed is full, `0.0..=1.0`.
    pub fn load_fraction(&self) -> f64 {
        let capacity = self.def().capacity.0;
        if capacity <= 0.0 {
            0.0
        } else {
            (self.cargo.total().0 / capacity).clamp(0.0, 1.0)
        }
    }

    /// How much going it can take as it stands, loaded as it is.
    ///
    /// The showable half of the bogging model: a panel can put this beside
    /// the going on the crossing ahead and say plainly how the odds sit,
    /// *before* the player commits to sending it.
    pub fn capability(&self) -> f64 {
        let def = self.def();
        def.ground - self.load_fraction() * def.load_penalty
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

    /// How many are stuck.
    pub fn bogged(&self) -> usize {
        self.list.iter().filter(|v| v.state.is_bogged()).count()
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

/// How long a vehicle is left to get itself out before a tow is sent.
///
/// A day, because the ground is a daily quantity and a crew gets one go at
/// driving out before anybody is sent after them. Measured rather than picked:
/// longer delays were tried at two, three and five days and bought almost no
/// extra self-releases while more than doubling the days a republic had a lorry
/// standing in a field.
pub const HELP_AFTER: u64 = 1;

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
            // Exactly one role, and the capacity that goes with it. A lorry
            // with no bed and a bus with no seats are both a row somebody left
            // half-authored, and under the old `recovers: bool` neither was
            // expressible as a question — which is why the flag became a role.
            let carries = def.capacity.is_positive();
            let seats = def.seats > 0;
            match def.role {
                Role::Freight => assert!(
                    carries && !seats,
                    "{} hauls freight with {:?} of bed and {} seats",
                    def.name,
                    def.capacity,
                    def.seats
                ),
                Role::Crew => assert!(
                    seats && !carries,
                    "{} carries crews with {} seats and {:?} of bed",
                    def.name,
                    def.seats,
                    def.capacity
                ),
                Role::Recovery => assert!(
                    !seats && !carries,
                    "{} recovers and also carries things",
                    def.name
                ),
                Role::Passenger => assert!(
                    seats && !carries,
                    "{} carries settlers with {} seats and {:?} of bed",
                    def.name,
                    def.seats,
                    def.capacity
                ),
            }
            assert!(def.on_road > Speed::ZERO, "{} cannot move", def.name);

            // **Confinement is authored, not inferred.** A locomotive with a
            // cross-country speed is a locomotive somebody forgot to think
            // about, and a barge with a bogging capability is a barge that can
            // get stuck in a field. Asserting both directions is what stops a
            // new row being added with the road vehicle's fields copied over.
            if def.medium.free_ranging() {
                assert!(def.ground > 0.0, "{} cannot leave a road", def.name);
                assert!(
                    def.load_penalty >= 0.0 && def.load_penalty < def.ground,
                    "{} cannot cross its own yard with a load on",
                    def.name
                );
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
            } else {
                assert_eq!(
                    def.cross_country,
                    Speed::ZERO,
                    "{} rides {} and also drives across country",
                    def.name,
                    def.medium.name()
                );
                assert_eq!(
                    def.ground,
                    0.0,
                    "{} rides {} and also has a view about mud",
                    def.name,
                    def.medium.name()
                );
                assert_eq!(def.load_penalty, 0.0, "{} sinks when laden", def.name);
            }
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
        assert_eq!(v.load_fraction(), 0.0);
        assert_eq!(v.capability(), VehicleKind::Lorry.def().ground);
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
