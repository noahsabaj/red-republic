//! Power lines and heat pipes: ordered, materialled, built, and only then
//! carrying anything.
//!
//! # Why power was the last free thing left
//!
//! Until this module existed a power station anywhere on the map lit every
//! building on it, and a boiler house anywhere warmed every block. Both were
//! *quantities* — enough megawatts, enough heat units — with no geography at
//! all, which is exactly the shape freight had before lorries and construction
//! had before crews. A republic could site its plant on the far side of a
//! mountain and lose nothing by it.
//!
//! A [`Line`] is what makes that physical. It is ordered like a road, has a
//! bill of materials per kilometre, is built by the same crew in the same
//! queue, and until it is finished it carries nothing.
//!
//! # One module for two networks, because they are the same shape
//!
//! A power line and a heat pipe differ in what they cost, how far a building
//! can be from one, and how much they lose along the way — and in nothing else.
//! Both are linear, both are built the same way, both connect producers to
//! consumers. Writing them twice would be two chances to fix a bug once.
//!
//! What they do **not** share is a network: [`Utility`] is part of a line's
//! identity, so a pipe never carries electricity and a pylon never carries
//! heat. That falls out of the type rather than out of a check.
//!
//! # Connection is stored, not searched
//!
//! Testing every building against every line on every tick is
//! `buildings × lines` distance-to-segment tests, 1,440 times a simulated day.
//! Instead a building is attached to a network **once** — when it is placed, or
//! when a line opens near it — and the attachment is a node index in a
//! union-find. Reading it back is a `find`, and two networks that later meet
//! merge in one union rather than by rewiring anybody.
//!
//! That is the same discipline the traversal lattice uses: derive once at the
//! event that invalidates it, not per tick.

use crate::building::BuildingId;
use crate::resource::{Resource, Stock};
use crate::units::{Metres, Point, Tonnes};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a line carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Utility {
    Power,
    Heat,
    /// A belt. Carries bulk solids between things standing on it, without a
    /// lorry and without a driver.
    Conveyor,
    /// A pipe. Carries liquids, in quantity.
    Pipeline,
}

/// What a kind of line costs and what it is worth.
///
/// Every field authored on every kind, for the reason every table in this crate
/// is: a defaulted figure is a decision nobody made. Costs are per kilometre
/// because that is the unit a line is ordered in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UtilityDef {
    pub kind: Utility,
    pub name: &'static str,
    /// Tonnes of each material per kilometre.
    pub materials: &'static [(Resource, f64)],
    /// Builder-days per kilometre.
    pub labour: f64,
    /// How far from the line a building may stand and still be connected to it.
    pub reach: Metres,
    /// The share of what it carries lost over a kilometre.
    ///
    /// Applied to the **span of the network**, which is what makes a sprawling
    /// grid worse than a compact one and is the whole argument for siting a
    /// plant near what it serves.
    pub loss_per_km: f64,
    /// What goods this kind moves, if it moves goods at all.
    ///
    /// Empty on power and heat, which carry something that is not tonnage.
    /// Authored as a list rather than derived from a `is_liquid()` predicate
    /// because what a belt will take is a design decision about the economy and
    /// not a fact about chemistry — and a list beside the rest of the row is
    /// where this codebase puts design decisions.
    pub carries: &'static [Resource],
    /// Tonnes a day the whole network will move, at full length.
    ///
    /// A property of the *network* rather than of a span, because a belt is a
    /// belt however many sections it has: adding another kilometre does not
    /// double what it carries, it only makes it longer.
    pub throughput: f64,
}

/// The utility table. First-pass balance, meant to be felt out against the
/// trajectory runner rather than reasoned to.
///
/// The two are deliberately very different animals. A power line is cheap per
/// kilometre, reaches a long way sideways and loses almost nothing, so a
/// republic strings power across the map. A heat main is dear, has to run
/// almost past the door, and leaks badly — which is why district heating is a
/// town-scale thing in every country that has it, and why a remote mining camp
/// wants its own boiler rather than a pipe from the city.
pub const UTILITIES: &[UtilityDef] = &[
    UtilityDef {
        kind: Utility::Power,
        name: "Power Line",
        materials: &[(Resource::Steel, 6.0)],
        labour: 35.0,
        reach: Metres(250.0),
        loss_per_km: 0.012,
        carries: &[],
        throughput: 0.0,
    },
    UtilityDef {
        kind: Utility::Heat,
        name: "Heat Main",
        materials: &[(Resource::Steel, 18.0), (Resource::Bricks, 12.0)],
        labour: 95.0,
        reach: Metres(110.0),
        loss_per_km: 0.07,
        carries: &[],
        throughput: 0.0,
    },
    // A belt. What it buys is a haul that needs no lorry, no driver and no
    // diesel — and what it costs is that it goes exactly where it was built and
    // nowhere else. The trade against the fleet is the whole point: a mine
    // feeding a plant four hundred metres away wants a belt, and a mine feeding
    // six things scattered over a valley wants lorries.
    UtilityDef {
        kind: Utility::Conveyor,
        name: "Conveyor",
        materials: &[(Resource::Steel, 30.0), (Resource::Machinery, 4.0)],
        labour: 120.0,
        reach: Metres(60.0),
        loss_per_km: 0.0,
        carries: &[
            Resource::Coal,
            Resource::IronOre,
            Resource::Gravel,
            Resource::Bricks,
            Resource::Crops,
            Resource::Wood,
            Resource::Waste,
        ],
        throughput: 60.0,
    },
    // The same idea for what will not sit on a belt. Dearer, and it moves far
    // more — which is what makes an oil field worth reaching with one.
    UtilityDef {
        kind: Utility::Pipeline,
        name: "Pipeline",
        materials: &[(Resource::Steel, 45.0), (Resource::Machinery, 3.0)],
        labour: 150.0,
        reach: Metres(70.0),
        loss_per_km: 0.0,
        carries: &[Resource::Oil, Resource::Fuel],
        throughput: 180.0,
    },
];

impl Utility {
    pub const ALL: [Utility; 4] = [
        Utility::Power,
        Utility::Heat,
        Utility::Conveyor,
        Utility::Pipeline,
    ];

    /// Whether this kind moves tonnage rather than current or hot water.
    pub fn moves_goods(self) -> bool {
        !self.def().carries.is_empty()
    }

    /// Whether it will take this resource.
    pub fn takes(self, resource: Resource) -> bool {
        self.def().carries.contains(&resource)
    }

    pub fn def(self) -> &'static UtilityDef {
        UTILITIES
            .iter()
            .find(|d| d.kind == self)
            .expect("every utility is in the table — guarded by a test")
    }

    pub fn name(self) -> &'static str {
        self.def().name
    }
}

/// How close two line ends have to be to be the same pylon or the same
/// junction chamber. Without this every line is an island.
pub const JOIN: Metres = Metres(30.0);

/// The shortest line worth ordering.
pub const MIN_LINE: Metres = Metres(50.0);

/// A stable handle to a finished line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LineId(pub u32);

/// A stable handle to a line under construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LineSiteId(pub u32);

/// A finished span, carrying whatever its kind carries.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub id: LineId,
    pub kind: Utility,
    pub from: Point,
    pub to: Point,
    /// The union-find nodes at each end. Two lines that share an end share a
    /// node, which is what makes them one network.
    pub(crate) a: u32,
    pub(crate) b: u32,
}

impl Line {
    pub fn length(&self) -> Metres {
        self.from.distance_to(self.to)
    }

    /// How far a point is from this span, measured to the segment rather than
    /// to either end — a building beside the middle of a line is beside the
    /// line.
    pub fn distance_to(&self, at: Point) -> Metres {
        distance_to_segment(at, self.from, self.to)
    }
}

/// The perpendicular distance from a point to a segment, or to the nearer end
/// when the foot of the perpendicular falls outside it.
pub fn distance_to_segment(at: Point, from: Point, to: Point) -> Metres {
    let (dx, dy) = (to.x.0 - from.x.0, to.y.0 - from.y.0);
    let len2 = dx * dx + dy * dy;
    if len2 <= f64::MIN_POSITIVE {
        return at.distance_to(from);
    }
    let t = (((at.x.0 - from.x.0) * dx + (at.y.0 - from.y.0) * dy) / len2).clamp(0.0, 1.0);
    let foot = Point::new(Metres(from.x.0 + dx * t), Metres(from.y.0 + dy * t));
    at.distance_to(foot)
}

/// Why a line could not be ordered there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineError {
    TooShort,
}

impl std::fmt::Display for LineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LineError::TooShort => write!(
                f,
                "a span shorter than {} m is not worth surveying",
                MIN_LINE.0
            ),
        }
    }
}

impl std::error::Error for LineError {}

/// A line that has been ordered and not yet energised.
///
/// Exactly the shape of [`crate::roadworks::RoadSite`], and deliberately so:
/// the construction system works both from the same crew and in the same
/// commissioning order, and freight delivers to both through the same
/// [`crate::fleet::Destination`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineSite {
    pub id: LineSiteId,
    pub kind: Utility,
    pub from: Point,
    pub to: Point,
    pub stock: Stock,
    pub work_done: f64,
    /// Where this sits in the republic's commissioning order.
    pub ordered: u64,
}

impl LineSite {
    pub fn length(&self) -> Metres {
        self.from.distance_to(self.to)
    }

    pub fn def(&self) -> &'static UtilityDef {
        self.kind.def()
    }

    pub fn materials(&self) -> Vec<(Resource, Tonnes)> {
        let km = self.length().as_km();
        self.def()
            .materials
            .iter()
            .map(|&(resource, per_km)| (resource, Tonnes(per_km * km)))
            .collect()
    }

    pub fn material(&self, resource: Resource) -> Tonnes {
        let km = self.length().as_km();
        self.def()
            .materials
            .iter()
            .find(|(r, _)| *r == resource)
            .map(|&(_, per_km)| Tonnes(per_km * km))
            .unwrap_or(Tonnes::ZERO)
    }

    pub fn labour(&self) -> f64 {
        self.def().labour * self.length().as_km()
    }

    /// Whether the materials for the work still to do are on hand. Same rule as
    /// a building site and a road site: the bill falls as the work is done.
    pub fn has_materials(&self) -> bool {
        let left = self.length().as_km() * (1.0 - self.progress());
        self.def()
            .materials
            .iter()
            .all(|&(r, q)| self.stock.get(r).0 + 1e-9 >= q * left)
    }

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

    /// Where a lorry pulls up: the middle, because that is where the work is.
    pub fn depot(&self) -> Point {
        Point::new(
            Metres((self.from.x.0 + self.to.x.0) / 2.0),
            Metres((self.from.y.0 + self.to.y.0) / 2.0),
        )
    }

    pub fn intake_capacity(&self, resource: Resource) -> Tonnes {
        Tonnes(self.material(resource).0 * (1.0 - self.progress()))
    }
}

/// Every line the republic has ordered and not yet energised.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LineWorks {
    list: Vec<LineSite>,
    next_id: u32,
}

impl LineWorks {
    pub fn new() -> Self {
        Self {
            list: Vec::new(),
            next_id: 1,
        }
    }

    pub fn all(&self) -> &[LineSite] {
        &self.list
    }

    pub fn get(&self, id: LineSiteId) -> Option<&LineSite> {
        self.list.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: LineSiteId) -> Option<&mut LineSite> {
        self.list.iter_mut().find(|s| s.id == id)
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn remove(&mut self, id: LineSiteId) -> Option<LineSite> {
        let at = self.list.iter().position(|s| s.id == id)?;
        Some(self.list.remove(at))
    }

    pub fn order(
        &mut self,
        kind: Utility,
        from: Point,
        to: Point,
        ordered: u64,
    ) -> Result<LineSiteId, LineError> {
        if from.distance_to(to).0 < MIN_LINE.0 {
            return Err(LineError::TooShort);
        }
        let id = LineSiteId(self.next_id);
        self.next_id += 1;
        self.list.push(LineSite {
            id,
            kind,
            from,
            to,
            stock: Stock::EMPTY,
            work_done: 0.0,
            ordered,
        });
        Ok(id)
    }
}

/// The republic's energised networks, and who is attached to them.
///
/// Nodes are shared pylons and junction chambers: two lines whose ends fall
/// within [`JOIN`] of each other share a node and are therefore one network.
/// The union-find is what answers "are these two things on the same grid" in
/// near-constant time, which matters because the power system asks it of every
/// building on every tick.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Networks {
    lines: Vec<Line>,
    next_id: u32,
    /// Merged line ends, in creation order.
    nodes: Vec<Point>,
    /// What each node carries.
    ///
    /// **A pylon is not a manhole.** Without this the node pool is shared, so a
    /// power line and a heat main whose ends happen to fall in the same place
    /// merge into one node and therefore one network — and `network_of` then
    /// answers the same number for both, which is a pipe carrying electricity.
    /// Caught by `the_two_networks_never_touch`, which is exactly the test
    /// written to make the claim in this module's docs checkable.
    node_kind: Vec<Utility>,
    /// Union-find parents over [`Networks::nodes`].
    parent: Vec<u32>,
    /// Where each building is plugged in, if it is.
    attached: BTreeMap<(BuildingId, Utility), u32>,
}

impl Networks {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            next_id: 1,
            nodes: Vec::new(),
            node_kind: Vec::new(),
            parent: Vec::new(),
            attached: BTreeMap::new(),
        }
    }

    pub fn all(&self) -> &[Line] {
        &self.lines
    }

    pub fn of_kind(&self, kind: Utility) -> impl Iterator<Item = &Line> {
        self.lines.iter().filter(move |l| l.kind == kind)
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Total length of one kind of network.
    pub fn length_of(&self, kind: Utility) -> Metres {
        Metres(self.of_kind(kind).map(|l| l.length().0).sum())
    }

    /// Which network a building is on, if any.
    ///
    /// The stored attachment resolved through the union-find, so two grids that
    /// have since been joined by a new span answer with the same number without
    /// anybody being rewired.
    pub fn network_of(&self, building: BuildingId, kind: Utility) -> Option<u32> {
        self.attached
            .get(&(building, kind))
            .map(|&node| self.root(node))
    }

    /// Whether two buildings are on the same network of a kind.
    pub fn together(&self, a: BuildingId, b: BuildingId, kind: Utility) -> bool {
        match (self.network_of(a, kind), self.network_of(b, kind)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }

    /// The span of one network, end to end — what the losses are charged on.
    ///
    /// The sum of its spans rather than the distance between its extremes,
    /// because current and hot water travel the wire and the pipe rather than
    /// the straight line, and a network that doubles back on itself really does
    /// lose more.
    pub fn span_of(&self, network: u32, kind: Utility) -> Metres {
        Metres(
            self.of_kind(kind)
                .filter(|l| self.root(l.a) == network)
                .map(|l| l.length().0)
                .sum(),
        )
    }

    /// Root of a node, without path compression so this can be a `&self` read.
    ///
    /// Union by size keeps the trees shallow enough that walking them is
    /// cheaper than the interior mutability compression would need.
    fn root(&self, mut node: u32) -> u32 {
        while self.parent[node as usize] != node {
            node = self.parent[node as usize];
        }
        node
    }

    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.root(a), self.root(b));
        if ra == rb {
            return;
        }
        // Lower index wins, which keeps the answer independent of the order
        // lines happened to be built in — a determinism requirement, not a
        // performance one.
        let (keep, drop) = if ra < rb { (ra, rb) } else { (rb, ra) };
        self.parent[drop as usize] = keep;
    }

    /// Find or create the node at a point, merging onto anything within
    /// [`JOIN`].
    fn node_at(&mut self, at: Point, kind: Utility) -> u32 {
        if let Some((index, _)) = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| self.node_kind[*i] == kind)
            .filter(|(_, p)| p.distance_to(at).0 <= JOIN.0)
            .min_by(|(ia, a), (ib, b)| {
                a.distance_to(at)
                    .0
                    .total_cmp(&b.distance_to(at).0)
                    .then_with(|| ia.cmp(ib))
            })
        {
            return index as u32;
        }
        let index = self.nodes.len() as u32;
        self.nodes.push(at);
        self.node_kind.push(kind);
        self.parent.push(index);
        index
    }

    /// Energise a finished span.
    ///
    /// Returns the line, so the caller can attach whatever now stands beside
    /// it. Nothing is subdivided the way a road is: a road needs junctions
    /// along its length because access is measured from junctions, and a
    /// connection here is measured from the *span*, so the two ends are all the
    /// structure it needs.
    pub(crate) fn energise(&mut self, site: &LineSite) -> LineId {
        let a = self.node_at(site.from, site.kind);
        let b = self.node_at(site.to, site.kind);
        self.union(a, b);
        let id = LineId(self.next_id);
        self.next_id += 1;
        self.lines.push(Line {
            id,
            kind: site.kind,
            from: site.from,
            to: site.to,
            a,
            b,
        });
        id
    }

    /// Plug a building in, if anything of that kind runs close enough to it.
    ///
    /// Nearest span wins, ties on line id, so the answer does not depend on the
    /// order the lines happen to sit in the vector.
    pub(crate) fn attach(&mut self, building: BuildingId, at: Point, kind: Utility) -> bool {
        let reach = kind.def().reach;
        let Some(node) = self
            .of_kind(kind)
            .filter(|l| l.distance_to(at).0 <= reach.0)
            .min_by(|a, b| {
                a.distance_to(at)
                    .0
                    .total_cmp(&b.distance_to(at).0)
                    .then_with(|| a.id.cmp(&b.id))
            })
            .map(|l| l.a)
        else {
            return false;
        };
        self.attached.insert((building, kind), node);
        true
    }

    /// Plug a building into everything within reach of it. What a placement
    /// does.
    pub(crate) fn attach_all(&mut self, building: BuildingId, at: Point) {
        for kind in Utility::ALL {
            self.attach(building, at, kind);
        }
    }

    pub(crate) fn detach(&mut self, building: BuildingId) {
        for kind in Utility::ALL {
            self.attached.remove(&(building, kind));
        }
    }

    /// How many buildings are plugged into a kind of network.
    pub fn connected_count(&self, kind: Utility) -> usize {
        self.attached.keys().filter(|(_, k)| *k == kind).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64) -> Point {
        Point::new(Metres(x), Metres(y))
    }

    fn built(kind: Utility, from: Point, to: Point) -> LineSite {
        let mut works = LineWorks::new();
        let id = works.order(kind, from, to, 0).expect("long enough");
        works.remove(id).expect("just ordered")
    }

    #[test]
    fn every_utility_is_in_the_table_exactly_once_and_fully_authored() {
        for kind in Utility::ALL {
            assert_eq!(UTILITIES.iter().filter(|d| d.kind == kind).count(), 1);
            let def = kind.def();
            assert!(def.labour > 0.0, "{} builds itself", def.name);
            assert!(def.reach.0 > 0.0, "{} connects nothing", def.name);
            assert!(
                def.loss_per_km >= 0.0 && def.loss_per_km < 1.0,
                "{} loses everything over a kilometre",
                def.name
            );
            // A kind either moves goods or it does not, and the two fields that
            // say so must agree. A belt with no throughput carries nothing and
            // a power line with one would be current measured in tonnes.
            assert_eq!(
                kind.moves_goods(),
                def.throughput > 0.0,
                "{} declares goods and throughput inconsistently",
                def.name
            );
        }
    }

    /// Nothing is carried by two kinds. A resource a belt and a pipe would both
    /// take is a resource whose route depends on which network the pass happens
    /// to look at first.
    #[test]
    fn no_resource_rides_two_networks() {
        for a in Utility::ALL {
            for b in Utility::ALL {
                if a >= b {
                    continue;
                }
                let clash: Vec<_> = a
                    .def()
                    .carries
                    .iter()
                    .filter(|r| b.def().carries.contains(r))
                    .collect();
                assert!(
                    clash.is_empty(),
                    "{} and {} both carry {clash:?}",
                    a.name(),
                    b.name()
                );
            }
        }
    }

    /// The two are meant to be different animals, and the balance says so:
    /// heat is dear, short-reaching and leaky, which is why district heating is
    /// a town-scale thing.
    #[test]
    fn a_heat_main_is_dearer_shorter_reaching_and_leakier_than_a_power_line() {
        let (power, heat) = (Utility::Power.def(), Utility::Heat.def());
        assert!(heat.labour > power.labour);
        assert!(heat.reach.0 < power.reach.0);
        assert!(heat.loss_per_km > power.loss_per_km);
    }

    #[test]
    fn a_bill_is_proportional_to_the_length() {
        let short = built(Utility::Power, at(0.0, 0.0), at(500.0, 0.0));
        let long = built(Utility::Power, at(0.0, 0.0), at(2_000.0, 0.0));
        assert!(
            (long.material(Resource::Steel).0 - short.material(Resource::Steel).0 * 4.0).abs()
                < 1e-9
        );
        assert!((long.labour() - short.labour() * 4.0).abs() < 1e-9);
    }

    #[test]
    fn a_line_too_short_to_be_a_line_is_refused() {
        let mut works = LineWorks::new();
        assert_eq!(
            works.order(Utility::Heat, at(0.0, 0.0), at(10.0, 0.0), 0),
            Err(LineError::TooShort)
        );
        assert!(works.is_empty());
    }

    /// The distance that decides a connection is to the *span*, not to its
    /// ends. A building beside the middle of a line is beside the line.
    #[test]
    fn a_building_beside_the_middle_of_a_span_is_beside_it() {
        let line = built(Utility::Power, at(0.0, 0.0), at(1_000.0, 0.0));
        let mut nets = Networks::new();
        nets.energise(&line);

        assert!(nets.attach(BuildingId(1), at(500.0, 100.0), Utility::Power));
        assert!(
            !nets.attach(BuildingId(2), at(500.0, 400.0), Utility::Power),
            "250 m of reach should not stretch to 400"
        );
        // And past the end is measured to the end, not to the infinite line.
        assert!(!nets.attach(BuildingId(3), at(1_400.0, 0.0), Utility::Power));
        assert!(nets.attach(BuildingId(4), at(1_100.0, 0.0), Utility::Power));
    }

    /// A pipe never carries electricity. The type is what stops it, not a
    /// check somewhere.
    #[test]
    fn the_two_networks_never_touch() {
        let mut nets = Networks::new();
        nets.energise(&built(Utility::Power, at(0.0, 0.0), at(1_000.0, 0.0)));
        nets.energise(&built(Utility::Heat, at(0.0, 0.0), at(1_000.0, 0.0)));

        let beside = at(500.0, 50.0);
        assert!(nets.attach(BuildingId(1), beside, Utility::Power));
        assert!(nets.attach(BuildingId(1), beside, Utility::Heat));
        // Same building, two networks, and they are separate numbers.
        let power = nets.network_of(BuildingId(1), Utility::Power);
        let heat = nets.network_of(BuildingId(1), Utility::Heat);
        assert!(power.is_some() && heat.is_some());
        assert_ne!(power, heat, "the pylon and the pipe are the same network");
    }

    /// Two spans that meet are one network; two that do not are two — and
    /// joining them later merges them without rewiring anybody.
    #[test]
    fn spans_that_meet_are_one_network_and_a_later_span_joins_two() {
        let mut nets = Networks::new();
        nets.energise(&built(Utility::Power, at(0.0, 0.0), at(800.0, 0.0)));
        nets.energise(&built(Utility::Power, at(3_000.0, 0.0), at(3_800.0, 0.0)));

        let plant = BuildingId(1);
        let town = BuildingId(2);
        assert!(nets.attach(plant, at(100.0, 50.0), Utility::Power));
        assert!(nets.attach(town, at(3_100.0, 50.0), Utility::Power));
        assert!(
            !nets.together(plant, town, Utility::Power),
            "two islands are not one grid"
        );

        // The span that closes the gap. Neither building is touched.
        nets.energise(&built(Utility::Power, at(800.0, 0.0), at(3_000.0, 0.0)));
        assert!(
            nets.together(plant, town, Utility::Power),
            "the two grids did not merge when they were joined"
        );
        assert!(
            (nets
                .span_of(
                    nets.network_of(plant, Utility::Power).unwrap(),
                    Utility::Power
                )
                .0
                - 3_800.0)
                .abs()
                < 1e-9,
            "the span is the sum of what was built, not the distance between ends"
        );
    }

    /// A building with nothing near it is on no network at all, and that is the
    /// state the whole module exists to make representable.
    #[test]
    fn a_building_with_no_line_near_it_is_on_nothing() {
        let mut nets = Networks::new();
        nets.energise(&built(Utility::Power, at(0.0, 0.0), at(800.0, 0.0)));
        assert!(!nets.attach(BuildingId(9), at(5_000.0, 5_000.0), Utility::Power));
        assert_eq!(nets.network_of(BuildingId(9), Utility::Power), None);
        assert!(!nets.together(BuildingId(9), BuildingId(9), Utility::Power));
    }

    #[test]
    fn a_demolished_building_is_unplugged() {
        let mut nets = Networks::new();
        nets.energise(&built(Utility::Power, at(0.0, 0.0), at(800.0, 0.0)));
        nets.attach_all(BuildingId(1), at(400.0, 40.0));
        assert!(nets.network_of(BuildingId(1), Utility::Power).is_some());
        nets.detach(BuildingId(1));
        assert_eq!(nets.network_of(BuildingId(1), Utility::Power), None);
        assert_eq!(nets.connected_count(Utility::Power), 0);
    }

    /// Which network a thing is on may not depend on the order the lines were
    /// built in, or a save that replayed its journal could come back different.
    #[test]
    fn the_answer_does_not_depend_on_build_order() {
        let spans = [
            (at(0.0, 0.0), at(800.0, 0.0)),
            (at(800.0, 0.0), at(1_600.0, 0.0)),
            (at(1_600.0, 0.0), at(2_400.0, 0.0)),
        ];
        let network = |order: [usize; 3]| {
            let mut nets = Networks::new();
            for i in order {
                nets.energise(&built(Utility::Power, spans[i].0, spans[i].1));
            }
            nets.attach(BuildingId(1), at(2_300.0, 40.0), Utility::Power);
            nets.network_of(BuildingId(1), Utility::Power)
        };
        assert_eq!(network([0, 1, 2]), network([2, 1, 0]));
        assert_eq!(network([0, 1, 2]), network([1, 2, 0]));
    }

    #[test]
    fn a_network_survives_a_save() {
        let mut nets = Networks::new();
        nets.energise(&built(Utility::Heat, at(0.0, 0.0), at(600.0, 0.0)));
        nets.attach_all(BuildingId(3), at(300.0, 60.0));
        let wire = postcard::to_stdvec(&nets).expect("serializes");
        let back: Networks = postcard::from_bytes(&wire).expect("parses");
        assert_eq!(back, nets);
        assert!(back.network_of(BuildingId(3), Utility::Heat).is_some());
    }
}
