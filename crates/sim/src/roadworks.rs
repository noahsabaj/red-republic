//! Roads under construction: ordered, materialled, built, and only then drivable.
//!
//! # Why a road is a site and not a setting
//!
//! [`crate::network::Network`] is the finished graph — junctions and segments,
//! and nothing about how they got there. Until now they got there by somebody
//! calling `connect`, which is to say roads appeared. That is the last free
//! thing left in the simulation: a road is one of the largest investments a
//! republic makes, and it should cost gravel that a quarry dug, lorries that
//! drove it out, and builder-days from the same crew that is not building
//! something else meanwhile.
//!
//! So a road is ordered as a [`RoadSite`], which has exactly the shape of a
//! building site — a bill of materials, builder-days done, and a position
//! goods can be delivered to. The existing construction system works it
//! alongside the buildings, from the same crew and in the same order.
//!
//! # It is not a road until it is finished
//!
//! A site is deliberately **not** in the network. Nothing routes over it,
//! nothing commutes along it, and no lorry is quicker for it existing. That
//! falls out of keeping the two structures apart rather than needing a flag,
//! and it is what makes the moment a road opens a real event.
//!
//! # Junctions along the length, not only at the ends
//!
//! A finished road is laid down as segments no longer than
//! [`JUNCTION_SPACING`], because access to the network is measured from
//! *junctions*: a five-kilometre road with junctions only at its ends would
//! serve the two buildings at those ends and nothing in between. At 200 m
//! spacing anything within about 280 m of the line can reach it, which is what
//! [`crate::citizen::ROAD_ACCESS`] promises.

use crate::journey::Medium;
use crate::network::Network;
use crate::resource::{Resource, Stock};
use crate::units::{Metres, Point, Speed, Tonnes};
use serde::{Deserialize, Serialize};

/// The longest a single segment of a newly laid road may be.
pub const JUNCTION_SPACING: Metres = Metres(200.0);

/// How close a new junction has to be to an existing one to be the same
/// junction. Without this every road is an island.
pub const JUNCTION_MERGE: Metres = Metres(20.0);

/// The shortest road worth ordering. Below it the site is a formality with no
/// segment in it.
pub const MIN_ROAD: Metres = Metres(50.0);

/// How a stretch of way is laid.
///
/// Rails are grades rather than a parallel system, and that is the point: a
/// railway is ordered, materialled, worked by the same crew in the same
/// commissioning queue and undrivable until it is finished, exactly as a road
/// is. What differs is the bill, the speed, and **which network the finished
/// way joins** — and the last of those is one authored field
/// ([`GradeDef::carries`]) rather than a second copy of this whole module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Grade {
    /// Graded earth. Free of materials and cheap in labour, and the grade a
    /// worn corridor promotes into once traffic has made one for itself.
    Dirt,
    Gravel,
    Paved,
    /// A road that can cross open water.
    ///
    /// Its own grade rather than a flag on the others, because what a bridge
    /// costs has nothing to do with what a road costs: it is steel and concrete
    /// by the kilometre and months of a crew, and pricing it as tarmac would
    /// make a river a formality. It is also the *only* grade that may cross
    /// water, which is what makes a river a real division of a republic until
    /// somebody pays to span it.
    Bridge,
    /// Track. Dear to lay, cheap to run, and it goes exactly where it was put.
    Railway,
    /// Track over water. The single most expensive thing in the table.
    RailBridge,
    /// Street track, laid in a road somebody already built.
    Tramway,
    /// Underground. Dearer than anything else per kilometre, and it passes
    /// beneath a river rather than needing a bridge over one.
    MetroTunnel,
}

/// What a grade costs and what it is worth.
///
/// Every field authored on every grade, for the same reason as
/// [`crate::building::BuildingDef`]: a defaulted figure is a decision nobody
/// made. Costs are per kilometre, because that is the unit a road is ordered
/// in and it keeps a long road honestly proportional to a short one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradeDef {
    pub grade: Grade,
    pub name: &'static str,
    /// Which network the finished way joins, and therefore what may ride it.
    /// A lorry on a railway is not slow, it is impossible.
    pub carries: Medium,
    /// What a vehicle may do on it. The vehicle's own road speed still applies,
    /// so a lorry limited to 50 km/h is not faster for a better surface than it
    /// is — but a dirt track holds everything back equally.
    pub speed: Speed,
    /// Tonnes of each material per kilometre.
    pub materials: &'static [(Resource, f64)],
    /// Builder-days per kilometre.
    pub labour: f64,
    /// Whether this grade may be built with street lighting.
    ///
    /// **Paved only, and authored rather than matched on** — the same rule
    /// every other property in this crate follows. Lamps want a kerb, a duct
    /// and a cable, none of which go into a gravel track that traffic is going
    /// to re-cut every spring. It is also what makes lighting a mid-game thing
    /// without a rule saying so: paved needs asphalt needs a refinery.
    pub lamps: bool,
}

/// The road table. First-pass balance, meant to be felt out against the
/// trajectory runner.
///
/// A quarry digs 5 t of gravel a day, so a kilometre of gravel road is a
/// fortnight of one quarry's whole output — which is the point. Roads are the
/// largest thing a small republic can decide to build, and the decision should
/// hurt.
/// What a kilometre of street lighting costs on top of the road under it.
///
/// **Deliberately shares no resource with any grade's own bill.** The site's
/// bill is a flat list looked up with `find`, so a resource appearing twice
/// would be a quantity silently halved — `lamps_do_not_double_up_a_grades_bill`
/// is the guard, because the trap is invisible until somebody adds steel to a
/// paved road.
pub const LAMP_MATERIALS: &[(Resource, f64)] =
    &[(Resource::Steel, 6.0), (Resource::Electronics, 1.5)];

/// Builder-days per kilometre of lighting, on top of the road's own.
pub const LAMP_LABOUR: f64 = 35.0;

/// What a kilometre of lit street draws off the grid, in megawatts.
///
/// Small against a factory and not nothing across a town: twenty kilometres of
/// lit road is 0.4 MW, which is a real share of a first power station. That is
/// the intended shape — lighting the republic is affordable and it is not free,
/// and a grid already at its limit puts the streets out first because the power
/// system serves in commissioning order and the roads come last.
pub const LAMP_MW_PER_KM: f64 = 0.02;

pub const GRADES: &[GradeDef] = &[
    GradeDef {
        grade: Grade::Dirt,
        carries: Medium::Road,
        name: "Dirt Track",
        speed: Speed::from_kph(25.0),
        materials: &[],
        labour: 25.0,
        lamps: false,
    },
    GradeDef {
        grade: Grade::Gravel,
        carries: Medium::Road,
        name: "Gravel Road",
        speed: Speed::from_kph(45.0),
        materials: &[(Resource::Gravel, 60.0)],
        labour: 70.0,
        lamps: false,
    },
    // **Asphalt, and that is a gate rather than a price rise.** A paved road
    // used to be gravel and brick, which a republic has on day one — so the
    // best road in the game was available before anything had been built. It
    // now wants the heavy end of a barrel: pump, refinery, asphalt plant. A
    // gravel road is 45 km/h against this 60, so nothing is *blocked* by the
    // gate; what is behind it is the last fifteen, which is the right thing to
    // put at the end of a chain four buildings deep.
    GradeDef {
        grade: Grade::Paved,
        carries: Medium::Road,
        name: "Paved Road",
        speed: Speed::from_kph(60.0),
        materials: &[(Resource::Gravel, 40.0), (Resource::Asphalt, 25.0)],
        labour: 140.0,
        lamps: true,
    },
    // The most expensive thing a republic can order per kilometre, by a long
    // way, and slower than the tarmac it joins. That is deliberate on both
    // counts: a bridge is a decision about the shape of the republic rather
    // than a piece of road, and a lorry crosses one carefully.
    GradeDef {
        grade: Grade::Bridge,
        carries: Medium::Road,
        name: "Bridge",
        speed: Speed::from_kph(40.0),
        materials: &[
            (Resource::Steel, 120.0),
            (Resource::Bricks, 90.0),
            (Resource::Gravel, 60.0),
        ],
        labour: 520.0,
        lamps: false,
    },
    // A railway is roughly three gravel roads in materials and four in labour,
    // and it carries a hundred and twenty tonnes behind one driver. That is the
    // trade in one line: it costs a great deal and it goes one place.
    GradeDef {
        grade: Grade::Railway,
        carries: Medium::Rail,
        name: "Railway",
        speed: Speed::from_kph(80.0),
        materials: &[(Resource::Steel, 90.0), (Resource::Gravel, 140.0)],
        labour: 260.0,
        lamps: false,
    },
    // The most expensive kilometre a republic can order, and deliberately: a
    // railway bridge is the decision that a river is not going to stop the
    // line, taken once and paid for in steel.
    GradeDef {
        grade: Grade::RailBridge,
        carries: Medium::Rail,
        name: "Railway Bridge",
        speed: Speed::from_kph(50.0),
        materials: &[
            (Resource::Steel, 220.0),
            (Resource::Bricks, 120.0),
            (Resource::Gravel, 90.0),
        ],
        labour: 780.0,
        lamps: false,
    },
    // Street track. Half a railway's steel and no earthworks, because it is
    // laid in a road somebody already built -- which is also why it is slow.
    GradeDef {
        grade: Grade::Tramway,
        carries: Medium::Tram,
        name: "Tramway",
        speed: Speed::from_kph(30.0),
        materials: &[(Resource::Steel, 45.0), (Resource::Gravel, 40.0)],
        labour: 120.0,
        lamps: false,
    },
    // The most expensive kilometre in the republic, and the only way that
    // crosses water without a bridge -- because it goes under the river rather
    // than over it, which is exactly what a tunnel is for.
    GradeDef {
        grade: Grade::MetroTunnel,
        carries: Medium::Metro,
        name: "Metro Tunnel",
        speed: Speed::from_kph(70.0),
        materials: &[
            (Resource::Steel, 180.0),
            (Resource::Bricks, 320.0),
            (Resource::Gravel, 240.0),
            (Resource::Machinery, 12.0),
        ],
        labour: 1_400.0,
        lamps: false,
    },
];

impl GradeDef {
    /// Whether this grade may be laid over open water.
    ///
    /// A property of the authored row rather than a match on the enum, for the
    /// reason every other property in this crate is one.
    pub fn spans_water(&self) -> bool {
        matches!(
            self.grade,
            Grade::Bridge | Grade::RailBridge | Grade::MetroTunnel
        )
    }
}

impl Grade {
    pub fn def(self) -> &'static GradeDef {
        GRADES
            .iter()
            .find(|d| d.grade == self)
            .expect("every grade is in the table — guarded by a test")
    }

    pub fn all() -> impl Iterator<Item = Grade> {
        GRADES.iter().map(|d| d.grade)
    }
}

/// A stable handle to a road under construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RoadSiteId(pub u32);

/// Why a road could not be ordered there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoadError {
    /// Shorter than [`MIN_ROAD`].
    TooShort,
    /// One end is off the map, or on ground that will not take a road.
    Unbuildable,
    /// Street lighting was asked for on a grade that cannot carry it.
    ///
    /// Refused rather than dropped, because a road silently built without the
    /// lamps the player ordered is a night shift that cannot be staffed for a
    /// reason nothing on the screen explains.
    NoLampsOnThisGrade(Grade),
    /// The line crosses open water and this grade cannot.
    ///
    /// The refusal that makes a river a real division of a republic. Before it
    /// existed nothing checked what a road ran *over* — only its two ends — so
    /// a gravel road could be laid straight across a river at the price of
    /// gravel, and `CLAUDE.md` recorded water as impassable while the cheapest
    /// grade in the table crossed it for nothing.
    NeedsABridge,
}

/// What the player is told. Same reasoning as
/// [`crate::building::PlacementError`]'s: the wording lives beside the
/// variants so a new one cannot be added without it.
impl std::fmt::Display for RoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoadError::NoLampsOnThisGrade(grade) => write!(
                f,
                "street lighting wants a kerb and a duct, which {} has not got",
                grade.def().name
            ),
            RoadError::TooShort => write!(
                f,
                "a road shorter than {} m is not worth surveying",
                MIN_ROAD.0
            ),
            RoadError::Unbuildable => {
                write!(
                    f,
                    "one end is off the map or on ground that will not take a road"
                )
            }
            RoadError::NeedsABridge => {
                write!(f, "this line crosses water; only a bridge can span it")
            }
        }
    }
}

impl std::error::Error for RoadError {}

/// A road that has been ordered and not yet opened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoadSite {
    pub id: RoadSiteId,
    pub from: Point,
    pub to: Point,
    pub grade: Grade,
    /// Materials delivered so far.
    pub stock: Stock,
    /// Builder-days worked.
    pub work_done: f64,
    /// Where this sits in the republic's commissioning order — see
    /// [`RoadWorks::order`].
    pub ordered: u64,
    /// Whether this road is being built with street lighting.
    ///
    /// Refused on any grade whose [`GradeDef::lamps`] is false, so this is only
    /// ever true on paved. It adds to the bill and to the builder-days, and the
    /// finished segments carry it into the network.
    #[serde(default)]
    pub lamps: bool,
}

impl RoadSite {
    pub fn length(&self) -> Metres {
        self.from.distance_to(self.to)
    }

    pub fn def(&self) -> &'static GradeDef {
        self.grade.def()
    }

    /// What one kilometre of this exact site costs, lamps included.
    ///
    /// The one place the lighting surcharge is applied, so the bill, the
    /// shortfall, the delivery cap and the has-materials check can never
    /// disagree about what a lit road is made of. An iterator rather than a
    /// `Vec` because [`RoadSite::has_materials`] asks this of every site on
    /// every tick and an allocation there was worth avoiding.
    pub fn per_km(&self) -> impl Iterator<Item = (Resource, f64)> + '_ {
        let lamps = self.lamps;
        self.def()
            .materials
            .iter()
            .copied()
            .chain(LAMP_MATERIALS.iter().copied().filter(move |_| lamps))
    }

    /// The whole bill of materials, in tonnes.
    pub fn materials(&self) -> Vec<(Resource, Tonnes)> {
        let km = self.length().as_km();
        self.per_km()
            .map(|(resource, per_km)| (resource, Tonnes(per_km * km)))
            .collect()
    }

    /// How much of one material it needs in total.
    pub fn material(&self, resource: Resource) -> Tonnes {
        let km = self.length().as_km();
        self.per_km()
            .find(|(r, _)| *r == resource)
            .map(|(_, per_km)| Tonnes(per_km * km))
            .unwrap_or(Tonnes::ZERO)
    }

    /// Builder-days the whole road needs.
    pub fn labour(&self) -> f64 {
        let extra = if self.lamps { LAMP_LABOUR } else { 0.0 };
        (self.def().labour + extra) * self.length().as_km()
    }

    /// Whether the materials for the work still to do are on hand.
    ///
    /// Same rule and same reasoning as [`crate::building::Building::has_materials`]:
    /// the bill is consumed in step with the work, so a road delivered its full
    /// bill once does not then read as short of it.
    pub fn has_materials(&self) -> bool {
        // Deliberately not via `materials()`, which allocates: this is asked of
        // every site on every tick of the construction pass.
        let per_km = self.length().as_km() * (1.0 - self.progress());
        self.per_km()
            .all(|(r, q)| self.stock.get(r).0 + 1e-9 >= q * per_km)
    }

    /// How much of a material the site still has to be brought.
    pub fn material_outstanding(&self, resource: Resource) -> Tonnes {
        let left = 1.0 - self.progress();
        Tonnes(self.material(resource).0 * left).saturating_sub(self.stock.get(resource))
    }

    pub fn is_finished(&self) -> bool {
        self.work_done >= self.labour()
    }

    pub fn progress(&self) -> f64 {
        let labour = self.labour();
        if labour <= 0.0 {
            1.0
        } else {
            (self.work_done / labour).clamp(0.0, 1.0)
        }
    }

    /// Where a lorry pulls up. The middle, because that is where the work is.
    pub fn depot(&self) -> Point {
        Point::new(
            Metres((self.from.x.0 + self.to.x.0) / 2.0),
            Metres((self.from.y.0 + self.to.y.0) / 2.0),
        )
    }

    /// How much of a material the site will accept — exactly what the work
    /// still to do needs, and no more, since a road site is not a warehouse.
    pub fn intake_capacity(&self, resource: Resource) -> Tonnes {
        Tonnes(self.material(resource).0 * (1.0 - self.progress()))
    }
}

/// Every road the republic has ordered and not yet opened.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RoadWorks {
    list: Vec<RoadSite>,
    next_id: u32,
}

impl RoadWorks {
    pub fn new() -> Self {
        Self {
            list: Vec::new(),
            next_id: 1,
        }
    }

    pub fn all(&self) -> &[RoadSite] {
        &self.list
    }

    pub fn get(&self, id: RoadSiteId) -> Option<&RoadSite> {
        self.list.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: RoadSiteId) -> Option<&mut RoadSite> {
        self.list.iter_mut().find(|s| s.id == id)
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn remove(&mut self, id: RoadSiteId) -> Option<RoadSite> {
        let at = self.list.iter().position(|s| s.id == id)?;
        Some(self.list.remove(at))
    }

    /// Order a road.
    ///
    /// `ordered` is where this sits in the republic's commissioning order, and
    /// the caller supplies it because the order spans buildings as well as
    /// roads — see [`crate::world::World::order_road`], which is the only
    /// place that should be calling this.
    pub fn order(
        &mut self,
        from: Point,
        to: Point,
        grade: Grade,
        lamps: bool,
        ordered: u64,
    ) -> Result<RoadSiteId, RoadError> {
        if lamps && !grade.def().lamps {
            return Err(RoadError::NoLampsOnThisGrade(grade));
        }
        // **A bridge is worth surveying at any length, and a road is not.**
        // The minimum exists because a ten-metre road is a formality with no
        // segment in it; a ten-metre bridge is a real structure and the only
        // answer to a stream that width. Holding a bridge to the road minimum
        // made the mechanic unusable in exactly the case it was built for —
        // a river narrower than fifty metres could not be spanned at all, so
        // the player's choice was a fifty-metre bridge over a ten-metre stream
        // or no crossing. Found the day the map generator started making
        // rivers, which is the day anything first tried to cross one.
        if !grade.def().spans_water() && from.distance_to(to).0 < MIN_ROAD.0 {
            return Err(RoadError::TooShort);
        }
        if from.distance_to(to).0 <= 0.0 {
            return Err(RoadError::TooShort);
        }
        let id = RoadSiteId(self.next_id);
        self.next_id += 1;
        self.list.push(RoadSite {
            id,
            from,
            to,
            grade,
            stock: Stock::EMPTY,
            work_done: 0.0,
            ordered,
            lamps,
        });
        Ok(id)
    }
}

/// Lay a finished road into the network.
///
/// Subdivided at [`JUNCTION_SPACING`] so buildings along its length can reach
/// it, and merged into whatever junctions already stand at its ends so two
/// roads ordered end to end become one network rather than two islands.
pub fn open(roads: &mut Network, site: &RoadSite) {
    let lamps = site.lamps;
    let speed = site.def().speed;
    let length = site.length();
    let steps = (length.0 / JUNCTION_SPACING.0).ceil().max(1.0) as u32;

    let mut previous = roads.junction_at(site.from, JUNCTION_MERGE);
    for step in 1..=steps {
        let t = f64::from(step) / f64::from(steps);
        let along = Point::new(
            Metres(site.from.x.0 + (site.to.x.0 - site.from.x.0) * t),
            Metres(site.from.y.0 + (site.to.y.0 - site.from.y.0) * t),
        );
        let next = roads.junction_at(along, JUNCTION_MERGE);
        // A junction that merged onto the one behind it adds no road.
        if next != previous && !roads.are_connected(previous, next) {
            roads.connect_lit(previous, next, speed, lamps);
        }
        previous = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64) -> Point {
        Point::new(Metres(x), Metres(y))
    }

    fn site(grade: Grade, length: f64) -> RoadSite {
        let mut works = RoadWorks::new();
        let id = works
            .order(at(0.0, 0.0), at(length, 0.0), grade, false, 0)
            .expect("long enough");
        works.remove(id).expect("just ordered")
    }

    #[test]
    fn every_grade_is_in_the_table_exactly_once_and_fully_authored() {
        for grade in Grade::all() {
            assert_eq!(GRADES.iter().filter(|d| d.grade == grade).count(), 1);
            let def = grade.def();
            assert!(def.speed > Speed::ZERO, "{} goes nowhere", def.name);
            assert!(def.labour > 0.0, "{} builds itself", def.name);
        }
    }

    /// A better surface costs more and carries faster, or the grades are not a
    /// choice the player is making about anything.
    #[test]
    fn a_better_road_is_faster_and_dearer() {
        let ladder = [Grade::Dirt, Grade::Gravel, Grade::Paved];
        for pair in ladder.windows(2) {
            let (worse, better) = (pair[0].def(), pair[1].def());
            assert!(
                better.speed > worse.speed,
                "{} is no faster than {}",
                better.name,
                worse.name
            );
            assert!(
                better.labour > worse.labour,
                "{} costs no more work than {}",
                better.name,
                worse.name
            );
        }
        // And a dirt track is the one you can build with nothing but people.
        assert!(Grade::Dirt.def().materials.is_empty());
        assert!(!Grade::Gravel.def().materials.is_empty());
    }

    #[test]
    fn a_bill_of_materials_is_proportional_to_the_length() {
        let short = site(Grade::Gravel, 500.0);
        let long = site(Grade::Gravel, 2_000.0);
        assert!(
            (long.material(Resource::Gravel).0 - short.material(Resource::Gravel).0 * 4.0).abs()
                < 1e-9
        );
        assert!((long.labour() - short.labour() * 4.0).abs() < 1e-9);
        // A kilometre of gravel road is a fortnight of one quarry's output.
        let per_km = site(Grade::Gravel, 1_000.0).material(Resource::Gravel);
        assert!((per_km.0 - 60.0).abs() < 1e-9, "{per_km:?}");
    }

    #[test]
    fn a_dirt_track_needs_nobody_to_deliver_anything() {
        let track = site(Grade::Dirt, 1_000.0);
        assert!(track.materials().is_empty());
        assert!(track.has_materials(), "nothing to wait for");
        assert!(track.labour() > 0.0, "it still has to be graded");
    }

    #[test]
    fn a_site_accepts_its_bill_and_not_a_tonne_more() {
        let mut road = site(Grade::Gravel, 1_000.0);
        assert!((road.intake_capacity(Resource::Gravel).0 - 60.0).abs() < 1e-9);
        assert_eq!(road.intake_capacity(Resource::Coal), Tonnes::ZERO);
        assert!(!road.has_materials());
        road.stock.add(Resource::Gravel, Tonnes(60.0));
        assert!(road.has_materials());
    }

    #[test]
    fn a_road_too_short_to_be_a_road_is_refused() {
        let mut works = RoadWorks::new();
        assert_eq!(
            works.order(at(0.0, 0.0), at(10.0, 0.0), Grade::Gravel, false, 0),
            Err(RoadError::TooShort)
        );
        assert!(works.is_empty());
    }

    /// The point of subdividing: a long road has to be reachable along its
    /// length, not only at its ends.
    #[test]
    fn a_long_road_opens_with_junctions_all_the_way_along_it() {
        let mut roads = Network::new();
        let road = site(Grade::Gravel, 1_000.0);
        open(&mut roads, &road);

        assert_eq!(roads.segment_count(), 5, "a kilometre at 200 m a segment");
        assert_eq!(roads.node_count(), 6);
        // A building halfway along and 250 m off the line can still reach it.
        let beside = at(500.0, 250.0);
        assert!(roads.nearest_node(beside, Metres(300.0)).is_some());
        // And the two ends are genuinely connected to each other.
        let a = roads
            .nearest_node(at(0.0, 0.0), Metres(1.0))
            .expect("start");
        let b = roads
            .nearest_node(at(1_000.0, 0.0), Metres(1.0))
            .expect("end");
        assert!(roads.route(a, b).is_some());
    }

    /// Two roads ordered end to end are one network, not two islands.
    #[test]
    fn roads_that_meet_share_a_junction() {
        let mut roads = Network::new();
        let mut works = RoadWorks::new();
        let first = works
            .order(at(0.0, 0.0), at(600.0, 0.0), Grade::Gravel, false, 0)
            .unwrap();
        let second = works
            .order(at(600.0, 0.0), at(600.0, 600.0), Grade::Gravel, false, 1)
            .unwrap();
        open(&mut roads, works.get(first).unwrap());
        let joined = roads.node_count();
        open(&mut roads, works.get(second).unwrap());

        assert_eq!(
            roads.node_count(),
            joined + 3,
            "the shared corner was counted twice"
        );
        let a = roads.nearest_node(at(0.0, 0.0), Metres(1.0)).unwrap();
        let b = roads.nearest_node(at(600.0, 600.0), Metres(1.0)).unwrap();
        assert!(roads.route(a, b).is_some(), "the two roads did not meet");
    }

    /// Ordering the same road twice does not lay two of it.
    #[test]
    fn a_road_laid_over_an_existing_one_adds_no_second_carriageway() {
        let mut roads = Network::new();
        let road = site(Grade::Gravel, 600.0);
        open(&mut roads, &road);
        let (nodes, segments) = (roads.node_count(), roads.segment_count());
        open(&mut roads, &road);
        assert_eq!(
            (roads.node_count(), roads.segment_count()),
            (nodes, segments)
        );
    }
}
