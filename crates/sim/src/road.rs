//! The road network, as a graph.
//!
//! # Why a graph and not a grid
//!
//! The archived build routed by flooding a tile grid — five `Int32Array(w*h)`
//! buffers and an O(cells) sweep per query. At its map sizes that was free. At
//! metric scale a 10 km republic is a million cells, and freight runs many
//! queries per simulated day, so the same approach would spend the entire
//! frame budget on pathfinding empty ground.
//!
//! A road network is naturally a graph: a few thousand junctions rather than a
//! million cells, and its size tracks what the player has actually built
//! instead of how big the map is.
//!
//! # Travel time is real
//!
//! A segment's cost is its length divided by a real [`Speed`]. That the answer
//! comes out in seconds a human would recognise is the point — see
//! [`crate::units`] for why the archived build could not say the same.
//!
//! # Determinism
//!
//! Dijkstra's frontier is ordered by `(cost, node)` using [`f64::total_cmp`],
//! which is a total order over every float including ties. Ordering by cost
//! alone would leave equal-cost nodes to pop in whatever order the heap felt
//! like, and two runs could take different equally-short roads.

use crate::units::{Metres, Point, Seconds, Speed};
use serde::{Deserialize, Serialize};
use std::collections::BinaryHeap;

/// A junction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(pub u32);

/// The speed of an ordinary paved road.
pub fn default_road_speed() -> Speed {
    Speed::from_kph(50.0)
}

/// A stretch of road between two junctions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub from: NodeId,
    pub to: NodeId,
    pub length: Metres,
    pub speed: Speed,
}

impl Segment {
    pub fn travel_time(&self) -> Seconds {
        self.speed.time_to_cover(self.length)
    }
}

/// A route through the network.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// Junctions passed through, start first.
    pub nodes: Vec<NodeId>,
    pub distance: Metres,
    pub time: Seconds,
}

/// Frontier entry: a min-heap by cost, then by node, both totally ordered.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Frontier {
    cost: f64,
    node: u32,
}

impl Eq for Frontier {}

impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reversed: BinaryHeap is a max-heap and this is a shortest-path search.
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Every road in the republic.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RoadNetwork {
    nodes: Vec<Point>,
    segments: Vec<Segment>,
    /// For each node, the `(neighbour, segment index)` pairs leaving it, in
    /// the order the segments were laid — the tie-break of last resort.
    adjacency: Vec<Vec<(NodeId, usize)>>,
}

impl RoadNetwork {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, at: Point) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(at);
        self.adjacency.push(Vec::new());
        id
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    pub fn position_of(&self, node: NodeId) -> Option<Point> {
        self.nodes.get(node.0 as usize).copied()
    }

    /// Lay road between two junctions. Two-way: freight and people use it in
    /// both directions, and a one-way system is not a thing the republic
    /// models.
    ///
    /// # Panics
    /// If either node is unknown, or if the two are the same.
    pub fn connect(&mut self, a: NodeId, b: NodeId, speed: Speed) -> usize {
        assert_ne!(a, b, "a road segment needs two different junctions");
        let pa = self.position_of(a).expect("unknown junction");
        let pb = self.position_of(b).expect("unknown junction");
        let length = pa.distance_to(pb);

        let index = self.segments.len();
        self.segments.push(Segment {
            from: a,
            to: b,
            length,
            speed,
        });
        self.adjacency[a.0 as usize].push((b, index));
        self.adjacency[b.0 as usize].push((a, index));
        index
    }

    /// Total length of road laid.
    pub fn total_length(&self) -> Metres {
        self.segments
            .iter()
            .fold(Metres::ZERO, |acc, s| acc + s.length)
    }

    /// The junction at a point, reusing one already within `merge`.
    ///
    /// This is what stops every road being an island: two roads ordered end to
    /// end meet because the second one finds the first one's junction rather
    /// than laying a new node a metre away from it.
    pub fn junction_at(&mut self, at: Point, merge: Metres) -> NodeId {
        match self.nearest_node(at, merge) {
            Some(existing) => existing,
            None => self.add_node(at),
        }
    }

    /// Whether these two junctions already have road between them.
    pub fn are_connected(&self, a: NodeId, b: NodeId) -> bool {
        self.adjacency
            .get(a.0 as usize)
            .is_some_and(|links| links.iter().any(|&(other, _)| other == b))
    }

    /// The speed limit on the road between two adjacent junctions, if there is
    /// road between them. Where two run in parallel, the quicker one.
    pub fn speed_between(&self, a: NodeId, b: NodeId) -> Option<Speed> {
        self.adjacency
            .get(a.0 as usize)?
            .iter()
            .filter(|&&(other, _)| other == b)
            .map(|&(_, segment)| self.segments[segment].speed)
            .max_by(|x, y| x.as_mps().total_cmp(&y.as_mps()))
    }

    /// The junction nearest a point, if one is within `within`.
    ///
    /// Ties break on the lower id so two junctions the same distance away
    /// always resolve the same way.
    pub fn nearest_node(&self, to: Point, within: Metres) -> Option<NodeId> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(i, p)| (p.distance_to(to), NodeId(i as u32)))
            .filter(|(d, _)| d.0 <= within.0)
            .min_by(|(da, ia), (db, ib)| da.0.total_cmp(&db.0).then_with(|| ia.cmp(ib)))
            .map(|(_, id)| id)
    }

    /// Shortest route by travel time.
    pub fn route(&self, from: NodeId, to: NodeId) -> Option<Route> {
        let n = self.nodes.len();
        if from.0 as usize >= n || to.0 as usize >= n {
            return None;
        }
        if from == to {
            return Some(Route {
                nodes: vec![from],
                distance: Metres::ZERO,
                time: Seconds::ZERO,
            });
        }

        let mut best = vec![f64::INFINITY; n];
        let mut came_from: Vec<Option<u32>> = vec![None; n];
        let mut settled = vec![false; n];
        let mut heap = BinaryHeap::new();

        best[from.0 as usize] = 0.0;
        heap.push(Frontier {
            cost: 0.0,
            node: from.0,
        });

        while let Some(Frontier { cost, node }) = heap.pop() {
            let index = node as usize;
            if settled[index] {
                continue;
            }
            settled[index] = true;
            if node == to.0 {
                break;
            }

            for &(neighbour, segment) in &self.adjacency[index] {
                let next = neighbour.0 as usize;
                if settled[next] {
                    continue;
                }
                let step = self.segments[segment].travel_time().0;
                let candidate = cost + step;
                if candidate < best[next] {
                    best[next] = candidate;
                    came_from[next] = Some(node);
                    heap.push(Frontier {
                        cost: candidate,
                        node: neighbour.0,
                    });
                }
            }
        }

        if !settled[to.0 as usize] {
            return None;
        }

        let mut nodes = vec![to];
        let mut cursor = to.0;
        while let Some(previous) = came_from[cursor as usize] {
            nodes.push(NodeId(previous));
            cursor = previous;
        }
        nodes.reverse();

        let distance = nodes
            .windows(2)
            .map(|pair| {
                let a = self.position_of(pair[0]).expect("routed node exists");
                let b = self.position_of(pair[1]).expect("routed node exists");
                a.distance_to(b)
            })
            .fold(Metres::ZERO, |acc, d| acc + d);

        Some(Route {
            nodes,
            distance,
            time: Seconds(best[to.0 as usize]),
        })
    }

    /// How long it takes to travel between two points on the map, using the
    /// road network and walking to and from the nearest junction at each end.
    ///
    /// `None` when either end is further than `access` from any road — which
    /// is the honest answer: an unconnected building is genuinely unreachable
    /// and the caller should treat it as such rather than receive some large
    /// number and quietly serve it anyway.
    pub fn travel_time(
        &self,
        from: Point,
        to: Point,
        access: Metres,
        walking: Speed,
    ) -> Option<Seconds> {
        let a = self.nearest_node(from, access)?;
        let b = self.nearest_node(to, access)?;
        let route = self.route(a, b)?;
        let walk_in = walking.time_to_cover(self.position_of(a)?.distance_to(from));
        let walk_out = walking.time_to_cover(self.position_of(b)?.distance_to(to));
        Some(walk_in + route.time + walk_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Junctions in a line, 1 km apart.
    fn straight(n: u32) -> RoadNetwork {
        let mut r = RoadNetwork::new();
        for i in 0..n {
            r.add_node(Point::new(Metres(f64::from(i) * 1_000.0), Metres(0.0)));
        }
        for i in 0..n - 1 {
            r.connect(NodeId(i), NodeId(i + 1), default_road_speed());
        }
        r
    }

    #[test]
    fn a_segment_takes_the_time_its_length_and_speed_imply() {
        let r = straight(2);
        // 1 km at 50 km/h is 72 seconds. If that stops being recognisable,
        // the metric model has drifted.
        let route = r.route(NodeId(0), NodeId(1)).expect("connected");
        assert!((route.time.0 - 72.0).abs() < 1e-9, "{:?}", route.time);
        assert_eq!(route.distance, Metres(1_000.0));
    }

    #[test]
    fn routing_follows_the_chain_and_accumulates() {
        let r = straight(5);
        let route = r.route(NodeId(0), NodeId(4)).expect("connected");
        assert_eq!(
            route.nodes,
            vec![NodeId(0), NodeId(1), NodeId(2), NodeId(3), NodeId(4)]
        );
        assert_eq!(route.distance, Metres(4_000.0));
        assert!((route.time.0 - 288.0).abs() < 1e-9);
    }

    #[test]
    fn a_route_to_yourself_is_free() {
        let r = straight(3);
        let route = r.route(NodeId(1), NodeId(1)).expect("trivially connected");
        assert_eq!(route.nodes, vec![NodeId(1)]);
        assert_eq!(route.time, Seconds::ZERO);
    }

    #[test]
    fn an_unconnected_junction_has_no_route() {
        let mut r = straight(3);
        let island = r.add_node(Point::new(Metres(0.0), Metres(9_000.0)));
        assert!(r.route(NodeId(0), island).is_none());
        assert!(r.route(island, NodeId(0)).is_none());
    }

    /// The shortcut has to win, or routing is just following insertion order.
    #[test]
    fn the_faster_way_round_is_the_one_taken() {
        let mut r = RoadNetwork::new();
        let a = r.add_node(Point::new(Metres(0.0), Metres(0.0)));
        let long1 = r.add_node(Point::new(Metres(0.0), Metres(5_000.0)));
        let long2 = r.add_node(Point::new(Metres(10_000.0), Metres(5_000.0)));
        let b = r.add_node(Point::new(Metres(10_000.0), Metres(0.0)));

        // The long way is laid first, so insertion order favours it.
        r.connect(a, long1, default_road_speed());
        r.connect(long1, long2, default_road_speed());
        r.connect(long2, b, default_road_speed());
        // The direct road is laid last.
        r.connect(a, b, default_road_speed());

        let route = r.route(a, b).expect("connected");
        assert_eq!(route.nodes, vec![a, b], "took the scenic route");
        assert_eq!(route.distance, Metres(10_000.0));
    }

    /// A motorway that is longer in metres but quicker in minutes must win —
    /// the cost is time, not distance, and this is what proves it.
    #[test]
    fn a_longer_faster_road_beats_a_shorter_slow_one() {
        let mut r = RoadNetwork::new();
        let a = r.add_node(Point::new(Metres(0.0), Metres(0.0)));
        let b = r.add_node(Point::new(Metres(10_000.0), Metres(0.0)));
        let detour = r.add_node(Point::new(Metres(5_000.0), Metres(6_000.0)));

        // Direct, but a crawl.
        r.connect(a, b, Speed::from_kph(10.0));
        // Longer, but a proper road.
        r.connect(a, detour, Speed::from_kph(90.0));
        r.connect(detour, b, Speed::from_kph(90.0));

        let route = r.route(a, b).expect("connected");
        assert_eq!(route.nodes, vec![a, detour, b]);
        assert!(route.distance > Metres(10_000.0), "it really is longer");
    }

    #[test]
    fn routing_is_deterministic_when_two_ways_tie_exactly() {
        let mut r = RoadNetwork::new();
        let a = r.add_node(Point::new(Metres(0.0), Metres(0.0)));
        let up = r.add_node(Point::new(Metres(0.0), Metres(1_000.0)));
        let down = r.add_node(Point::new(Metres(0.0), Metres(-1_000.0)));
        let b = r.add_node(Point::new(Metres(2_000.0), Metres(0.0)));
        for mid in [up, down] {
            r.connect(a, mid, default_road_speed());
            r.connect(mid, b, default_road_speed());
        }
        let first = r.route(a, b).expect("connected");
        for _ in 0..50 {
            assert_eq!(r.route(a, b).expect("connected"), first);
        }
    }

    #[test]
    fn the_nearest_junction_is_found_only_within_reach() {
        let r = straight(3);
        let near = Point::new(Metres(1_050.0), Metres(0.0));
        assert_eq!(r.nearest_node(near, Metres(100.0)), Some(NodeId(1)));
        assert_eq!(r.nearest_node(near, Metres(10.0)), None);
    }

    /// A building nowhere near a road is unreachable, and says so rather than
    /// returning a large number a caller would quietly serve anyway.
    #[test]
    fn travel_between_points_needs_both_ends_on_the_network() {
        let r = straight(3);
        let walking = Speed::from_kph(5.0);
        let access = Metres(200.0);

        let a = Point::new(Metres(50.0), Metres(50.0));
        let b = Point::new(Metres(1_950.0), Metres(50.0));
        let time = r
            .travel_time(a, b, access, walking)
            .expect("both connected");
        // Roughly 2 km of driving plus two short walks — minutes, not hours.
        assert!(
            (2.0..15.0).contains(&time.as_minutes()),
            "{} minutes",
            time.as_minutes()
        );

        let stranded = Point::new(Metres(0.0), Metres(9_000.0));
        assert!(r.travel_time(a, stranded, access, walking).is_none());
    }

    #[test]
    fn total_length_counts_what_was_laid() {
        let r = straight(4);
        assert_eq!(r.total_length(), Metres(3_000.0));
        assert_eq!(r.segment_count(), 3);
        assert_eq!(r.node_count(), 4);
    }

    #[test]
    #[should_panic(expected = "two different junctions")]
    fn a_road_from_a_junction_to_itself_is_refused() {
        let mut r = straight(2);
        r.connect(NodeId(0), NodeId(0), default_road_speed());
    }
}
