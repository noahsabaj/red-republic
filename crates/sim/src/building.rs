//! Buildings: what they are, and where they stand.
//!
//! # Free positions, real footprints
//!
//! A building has a metric footprint and a continuous position. There is no
//! cell it occupies and no `tile.buildingId` — placement is a rectangle
//! overlap test against other rectangles, and buildability is a terrain query
//! over the cells that rectangle covers.
//!
//! Footprints are the honest sizes of the things they represent: a house is a
//! house, a panel block is sixty metres long, an integrated steel works is a
//! couple of hundred metres across. The archived build could not express that
//! — everything was one or four tiles — and it is the main thing metric scale
//! buys.
//!
//! # Rates are per day
//!
//! Inputs and outputs are tonnes per day, ported from the archived balance
//! table where they were tuned against each other. The simulation ticks in
//! minutes, so a system scales them by elapsed time rather than assuming a
//! tick is a day.
//!
//! # Mines tap, they do not own
//!
//! A building that extracts declares the [`Mineral`] it works. Placement
//! requires a body of that mineral under the site — see
//! [`crate::geology::Geology::tappable_at`] — and once built it draws on the
//! whole body, not on the ground under its own footprint.

use crate::geology::Mineral;
use crate::resource::{Resource, Stock};
use crate::units::{Metres, Point, Tonnes};
use serde::{Deserialize, Serialize};

/// Every kind of building. Roads are not here — they are a graph, not a
/// building, and live in [`crate::road`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BuildingKind {
    House,
    Apartment,
    Woodcutter,
    Sawmill,
    GravelQuarry,
    Brickworks,
    CoalMine,
    IronMine,
    SteelMill,
    OilPump,
    Refinery,
    PowerPlant,
    OilPowerPlant,
    HeatingPlant,
    Farm,
    FoodFactory,
    TextileMill,
    MachineWorks,
    Store,
    Clinic,
    CultureClub,
    Warehouse,
    Depot,
    ConstructionOffice,
    MotorDepot,
    GasStation,
    Customs,
}

/// What a building is and does.
///
/// Footprints are placeholders in the same sense as the geology plan: plausible
/// real sizes, not balanced ones. Worker counts, power and rates are ported
/// from the archived table, where they *were* balanced against each other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildingDef {
    pub kind: BuildingKind,
    pub name: &'static str,
    /// Footprint across, in metres.
    pub width: Metres,
    /// Footprint deep, in metres.
    pub depth: Metres,
    /// Jobs at full staffing.
    pub workers: u32,
    /// Megawatts drawn. Generators declare their output in `power_output`.
    pub power_draw: f64,
    pub power_output: f64,
    /// Tonnes consumed per day at full efficiency.
    pub inputs: &'static [(Resource, f64)],
    /// Tonnes produced per day at full efficiency.
    pub outputs: &'static [(Resource, f64)],
    /// The body this building works, if it is an extractor.
    pub taps: Option<Mineral>,
    /// How many people can live here.
    pub residents: u32,
    /// Tonnes of each resource it can hold.
    pub storage: f64,
}

macro_rules! def {
    (
        $kind:ident, $name:literal, $w:expr, $d:expr,
        workers: $workers:expr, draw: $draw:expr, out_mw: $out_mw:expr,
        in: [$(($ir:ident, $iq:expr)),* $(,)?],
        out: [$(($or:ident, $oq:expr)),* $(,)?],
        taps: $taps:expr, residents: $residents:expr, storage: $storage:expr $(,)?
    ) => {
        BuildingDef {
            kind: BuildingKind::$kind,
            name: $name,
            width: Metres($w),
            depth: Metres($d),
            workers: $workers,
            power_draw: $draw,
            power_output: $out_mw,
            inputs: &[$((Resource::$ir, $iq)),*],
            outputs: &[$((Resource::$or, $oq)),*],
            taps: $taps,
            residents: $residents,
            storage: $storage,
        }
    };
}

/// The building table. Rates and staffing ported from the archived balance;
/// footprints are real metric sizes the archived build could not express.
pub const BUILDINGS: &[BuildingDef] = &[
    def!(House, "Small House", 12.0, 10.0, workers: 0, draw: 0.0, out_mw: 0.0,
        in: [], out: [], taps: None, residents: 6, storage: 2.0),
    def!(Apartment, "Apartment Block", 62.0, 14.0, workers: 0, draw: 3.0, out_mw: 0.0,
        in: [], out: [], taps: None, residents: 48, storage: 8.0),
    def!(Woodcutter, "Woodcutter Post", 20.0, 16.0, workers: 6, draw: 0.0, out_mw: 0.0,
        in: [], out: [(Wood, 4.0)], taps: None, residents: 0, storage: 30.0),
    def!(Sawmill, "Sawmill", 34.0, 22.0, workers: 6, draw: 0.0, out_mw: 0.0,
        in: [(Wood, 2.0)], out: [(Planks, 3.0)], taps: None, residents: 0, storage: 40.0),
    def!(GravelQuarry, "Gravel Quarry", 60.0, 60.0, workers: 8, draw: 0.0, out_mw: 0.0,
        in: [], out: [(Gravel, 5.0)], taps: Some(Mineral::Gravel), residents: 0, storage: 60.0),
    def!(Brickworks, "Brickworks", 40.0, 28.0, workers: 10, draw: 0.0, out_mw: 0.0,
        in: [(Gravel, 3.0)], out: [(Bricks, 4.0)], taps: None, residents: 0, storage: 50.0),
    def!(CoalMine, "Coal Mine", 55.0, 45.0, workers: 14, draw: 6.0, out_mw: 0.0,
        in: [], out: [(Coal, 6.0)], taps: Some(Mineral::Coal), residents: 0, storage: 60.0),
    def!(IronMine, "Iron Ore Mine", 55.0, 45.0, workers: 14, draw: 6.0, out_mw: 0.0,
        in: [], out: [(IronOre, 5.0)], taps: Some(Mineral::IronOre), residents: 0, storage: 60.0),
    def!(SteelMill, "Steel Mill", 180.0, 140.0, workers: 20, draw: 40.0, out_mw: 0.0,
        in: [(IronOre, 2.0), (Coal, 1.0)], out: [(Steel, 1.5)], taps: None, residents: 0, storage: 40.0),
    def!(OilPump, "Oil Pump", 24.0, 24.0, workers: 10, draw: 1.0, out_mw: 0.0,
        in: [], out: [(Oil, 4.0)], taps: Some(Mineral::Oil), residents: 0, storage: 40.0),
    def!(Refinery, "Oil Refinery", 160.0, 120.0, workers: 25, draw: 30.0, out_mw: 0.0,
        in: [(Oil, 3.0)], out: [(Fuel, 2.0)], taps: None, residents: 0, storage: 60.0),
    def!(PowerPlant, "Coal Power Plant", 150.0, 110.0, workers: 15, draw: 0.0, out_mw: 60.0,
        in: [(Coal, 4.0)], out: [], taps: None, residents: 0, storage: 80.0),
    def!(OilPowerPlant, "Oil Power Plant", 140.0, 100.0, workers: 16, draw: 0.0, out_mw: 70.0,
        in: [(Oil, 4.0)], out: [], taps: None, residents: 0, storage: 80.0),
    def!(HeatingPlant, "Heating Plant", 45.0, 35.0, workers: 8, draw: 1.0, out_mw: 0.0,
        in: [(Coal, 1.0)], out: [], taps: None, residents: 0, storage: 40.0),
    def!(Farm, "Collective Farm", 240.0, 240.0, workers: 10, draw: 0.0, out_mw: 0.0,
        in: [], out: [(Crops, 6.0)], taps: None, residents: 0, storage: 60.0),
    def!(FoodFactory, "Food Factory", 50.0, 35.0, workers: 12, draw: 4.0, out_mw: 0.0,
        in: [(Crops, 2.5)], out: [(Food, 2.5)], taps: None, residents: 0, storage: 50.0),
    def!(TextileMill, "Textile Mill", 55.0, 35.0, workers: 12, draw: 4.0, out_mw: 0.0,
        in: [(Crops, 2.0)], out: [(Clothes, 1.2)], taps: None, residents: 0, storage: 50.0),
    def!(MachineWorks, "Machine Works", 150.0, 110.0, workers: 22, draw: 20.0, out_mw: 0.0,
        in: [(Steel, 3.0)], out: [(Machinery, 1.0)], taps: None, residents: 0, storage: 40.0),
    def!(Store, "State Store", 30.0, 20.0, workers: 3, draw: 1.0, out_mw: 0.0,
        in: [], out: [], taps: None, residents: 0, storage: 30.0),
    def!(Clinic, "Polyclinic", 45.0, 30.0, workers: 6, draw: 2.0, out_mw: 0.0,
        in: [], out: [], taps: None, residents: 0, storage: 10.0),
    def!(CultureClub, "Culture Club", 40.0, 30.0, workers: 4, draw: 1.0, out_mw: 0.0,
        in: [], out: [], taps: None, residents: 0, storage: 10.0),
    def!(Warehouse, "Warehouse", 60.0, 30.0, workers: 2, draw: 1.0, out_mw: 0.0,
        in: [], out: [], taps: None, residents: 0, storage: 200.0),
    def!(Depot, "Council Depot", 80.0, 60.0, workers: 4, draw: 1.0, out_mw: 0.0,
        in: [], out: [], taps: None, residents: 0, storage: 300.0),
    def!(ConstructionOffice, "Construction Office", 35.0, 25.0, workers: 10, draw: 0.0, out_mw: 0.0,
        in: [], out: [], taps: None, residents: 0, storage: 40.0),
    def!(MotorDepot, "Motor Depot", 90.0, 70.0, workers: 16, draw: 1.0, out_mw: 0.0,
        in: [], out: [], taps: None, residents: 0, storage: 60.0),
    def!(GasStation, "Gas Station", 30.0, 20.0, workers: 4, draw: 1.0, out_mw: 0.0,
        in: [], out: [], taps: None, residents: 0, storage: 40.0),
    def!(Customs, "Customs House", 90.0, 60.0, workers: 8, draw: 2.0, out_mw: 0.0,
        in: [], out: [], taps: None, residents: 0, storage: 200.0),
];

impl BuildingKind {
    pub fn def(self) -> &'static BuildingDef {
        BUILDINGS
            .iter()
            .find(|d| d.kind == self)
            .expect("every kind is in the table — guarded by a test")
    }
}

/// A stable handle to a placed building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BuildingId(pub u32);

/// A building standing on the map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Building {
    pub id: BuildingId,
    pub kind: BuildingKind,
    /// Centre of the footprint.
    pub centre: Point,
    pub stock: Stock,
    /// People actually turning up — set by the labour system, never authored.
    pub staff: u32,
    /// The body this building works, once sited.
    pub tapped: Option<crate::geology::DepositId>,
}

impl Building {
    pub fn def(&self) -> &'static BuildingDef {
        self.kind.def()
    }

    /// How much of its work it can do, `0.0..=1.0`, from staffing alone.
    ///
    /// Other factors (power, inputs, machinery) multiply this — each is a
    /// separate limiter and they are deliberately not folded together, so a
    /// stalled building can always say *which* thing stalled it.
    pub fn staffing(&self) -> f64 {
        let jobs = self.def().workers;
        if jobs == 0 {
            1.0
        } else {
            (f64::from(self.staff) / f64::from(jobs)).clamp(0.0, 1.0)
        }
    }

    /// The rectangle it occupies: `(min_x, min_y, max_x, max_y)` in metres.
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let d = self.def();
        (
            self.centre.x.0 - d.width.0 / 2.0,
            self.centre.y.0 - d.depth.0 / 2.0,
            self.centre.x.0 + d.width.0 / 2.0,
            self.centre.y.0 + d.depth.0 / 2.0,
        )
    }

    pub fn overlaps(&self, other: &Building) -> bool {
        let (ax0, ay0, ax1, ay1) = self.bounds();
        let (bx0, by0, bx1, by1) = other.bounds();
        ax0 < bx1 && bx0 < ax1 && ay0 < by1 && by0 < ay1
    }

    pub fn storage_cap(&self) -> Tonnes {
        Tonnes(self.def().storage)
    }
}

/// Why a building could not go there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementError {
    /// Off the map, or the ground will not take it.
    Unbuildable,
    /// Something is already standing there.
    Occupied,
    /// An extractor with no body of its mineral beneath.
    NothingToTap(Mineral),
}

/// Every building in the republic.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Buildings {
    list: Vec<Building>,
    next_id: u32,
}

impl Buildings {
    pub fn new() -> Self {
        Self {
            list: Vec::new(),
            next_id: 1,
        }
    }

    pub fn all(&self) -> &[Building] {
        &self.list
    }

    pub fn get(&self, id: BuildingId) -> Option<&Building> {
        self.list.iter().find(|b| b.id == id)
    }

    pub fn get_mut(&mut self, id: BuildingId) -> Option<&mut Building> {
        self.list.iter_mut().find(|b| b.id == id)
    }

    pub fn of_kind(&self, kind: BuildingKind) -> impl Iterator<Item = &Building> {
        self.list.iter().filter(move |b| b.kind == kind)
    }

    /// Total housing capacity.
    pub fn housing(&self) -> u32 {
        self.list.iter().map(|b| b.def().residents).sum()
    }

    /// Every job the republic offers, staffed or not.
    pub fn jobs(&self) -> u32 {
        self.list.iter().map(|b| b.def().workers).sum()
    }

    /// Would this go here? The same checks [`Buildings::place`] makes, without
    /// committing — what a placement preview asks.
    pub fn can_place(
        &self,
        kind: BuildingKind,
        centre: Point,
        terrain: &crate::terrain::Terrain,
        geology: &crate::geology::Geology,
    ) -> Result<Option<crate::geology::DepositId>, PlacementError> {
        let def = kind.def();
        if !terrain.area_is_buildable(centre, def.width, def.depth) {
            return Err(PlacementError::Unbuildable);
        }

        let candidate = Building {
            id: BuildingId(0),
            kind,
            centre,
            stock: Stock::EMPTY,
            staff: 0,
            tapped: None,
        };
        if self.list.iter().any(|b| b.overlaps(&candidate)) {
            return Err(PlacementError::Occupied);
        }

        match def.taps {
            None => Ok(None),
            Some(mineral) => geology
                .tappable_at(centre)
                .into_iter()
                .find(|&id| geology.get(id).is_some_and(|d| d.mineral == mineral))
                .map(Some)
                .ok_or(PlacementError::NothingToTap(mineral)),
        }
    }

    /// Put a building up.
    pub fn place(
        &mut self,
        kind: BuildingKind,
        centre: Point,
        terrain: &crate::terrain::Terrain,
        geology: &crate::geology::Geology,
    ) -> Result<BuildingId, PlacementError> {
        let tapped = self.can_place(kind, centre, terrain, geology)?;
        let id = BuildingId(self.next_id);
        self.next_id += 1;
        self.list.push(Building {
            id,
            kind,
            centre,
            stock: Stock::EMPTY,
            staff: 0,
            tapped,
        });
        Ok(id)
    }

    pub fn demolish(&mut self, id: BuildingId) -> bool {
        let before = self.list.len();
        self.list.retain(|b| b.id != id);
        self.list.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geology::{Deposit, DepositId, Geology, Layer};
    use crate::terrain::{Surface, Terrain};

    fn flat() -> Terrain {
        Terrain::flat(Metres(2_000.0))
    }

    fn coal_under(centre: Point) -> Geology {
        let mut g = Geology::new();
        g.insert(Deposit::new(
            DepositId(1),
            Mineral::Coal,
            centre,
            Metres(300.0),
            Metres(40.0),
            vec![Layer::new(Metres(20.0), Tonnes(500_000.0))],
        ));
        g
    }

    #[test]
    fn every_kind_appears_exactly_once_in_the_table() {
        // The guard that makes `kind.def()` safe: an unauthored kind would
        // otherwise panic at the first placement rather than at build time.
        let mut kinds: Vec<_> = BUILDINGS.iter().map(|d| d.kind).collect();
        let before = kinds.len();
        kinds.sort();
        kinds.dedup();
        assert_eq!(kinds.len(), before, "a kind is in the table twice");
        for d in BUILDINGS {
            assert_eq!(d.kind.def().kind, d.kind);
        }
    }

    /// Footprints must be real sizes, not tile counts wearing metres.
    #[test]
    fn footprints_are_plausible_real_structures() {
        for d in BUILDINGS {
            assert!(d.width.0 >= 10.0, "{} is {}m wide", d.name, d.width.0);
            assert!(d.width.0 <= 300.0, "{} is {}m wide", d.name, d.width.0);
            assert!(d.depth.0 >= 10.0, "{} is {}m deep", d.name, d.depth.0);
            assert!(d.depth.0 <= 300.0, "{} is {}m deep", d.name, d.depth.0);
        }
        // The scale the archived build could not express: a panel block is
        // several times a house, and a steel works dwarfs both.
        assert!(BuildingKind::Apartment.def().width > BuildingKind::House.def().width * 4.0);
        assert!(BuildingKind::SteelMill.def().width > BuildingKind::Apartment.def().width * 2.0);
    }

    #[test]
    fn only_extractors_tap_and_each_taps_what_it_produces() {
        for d in BUILDINGS {
            let Some(mineral) = d.taps else { continue };
            let produced = d
                .outputs
                .iter()
                .any(|(r, _)| r.from_mineral() == Some(mineral));
            assert!(
                produced,
                "{} taps {mineral:?} but does not produce it",
                d.name
            );
        }
    }

    #[test]
    fn a_building_goes_up_on_open_ground() {
        let mut b = Buildings::new();
        let id = b
            .place(
                BuildingKind::House,
                Point::new(Metres(500.0), Metres(500.0)),
                &flat(),
                &Geology::new(),
            )
            .expect("open ground");
        assert_eq!(b.all().len(), 1);
        assert_eq!(b.get(id).unwrap().kind, BuildingKind::House);
    }

    #[test]
    fn two_buildings_cannot_share_ground() {
        let mut b = Buildings::new();
        let t = flat();
        let g = Geology::new();
        let at = Point::new(Metres(500.0), Metres(500.0));
        b.place(BuildingKind::Apartment, at, &t, &g).expect("first");

        // Well inside the 62 m frontage of the block already there.
        let overlapping = Point::new(Metres(520.0), Metres(500.0));
        assert_eq!(
            b.place(BuildingKind::House, overlapping, &t, &g),
            Err(PlacementError::Occupied)
        );

        // Just clear of it.
        let clear = Point::new(Metres(560.0), Metres(500.0));
        assert!(b.place(BuildingKind::House, clear, &t, &g).is_ok());
    }

    #[test]
    fn nothing_is_built_on_water_or_off_the_map() {
        let mut t = flat();
        let g = Geology::new();
        let mut b = Buildings::new();
        t.set_surface(Point::new(Metres(505.0), Metres(505.0)), Surface::Water);
        assert_eq!(
            b.place(
                BuildingKind::House,
                Point::new(Metres(500.0), Metres(500.0)),
                &t,
                &g
            ),
            Err(PlacementError::Unbuildable)
        );
        assert_eq!(
            b.place(
                BuildingKind::House,
                Point::new(Metres(1.0), Metres(1.0)),
                &t,
                &g
            ),
            Err(PlacementError::Unbuildable)
        );
    }

    /// The mechanic: a mine needs a body under it, and once sited it holds a
    /// handle to the whole body rather than to the ground it stands on.
    #[test]
    fn a_mine_needs_a_body_beneath_it_and_remembers_which() {
        let t = flat();
        let at = Point::new(Metres(700.0), Metres(700.0));
        let mut b = Buildings::new();

        assert_eq!(
            b.place(BuildingKind::CoalMine, at, &t, &Geology::new()),
            Err(PlacementError::NothingToTap(Mineral::Coal))
        );

        let g = coal_under(at);
        let id = b
            .place(BuildingKind::CoalMine, at, &t, &g)
            .expect("over coal");
        assert_eq!(b.get(id).unwrap().tapped, Some(DepositId(1)));
    }

    /// A coal mine over an iron body is not a coal mine. The archived build
    /// checked the deposit under the footprint; this checks the mineral too.
    #[test]
    fn a_mine_will_not_tap_the_wrong_mineral() {
        let t = flat();
        let at = Point::new(Metres(700.0), Metres(700.0));
        let mut g = Geology::new();
        g.insert(Deposit::new(
            DepositId(1),
            Mineral::IronOre,
            at,
            Metres(300.0),
            Metres(40.0),
            vec![Layer::new(Metres(20.0), Tonnes(500_000.0))],
        ));
        let mut b = Buildings::new();
        assert_eq!(
            b.place(BuildingKind::CoalMine, at, &t, &g),
            Err(PlacementError::NothingToTap(Mineral::Coal))
        );
        assert!(b.place(BuildingKind::IronMine, at, &t, &g).is_ok());
    }

    #[test]
    fn staffing_is_the_fraction_of_jobs_filled() {
        let mut b = Buildings::new();
        let id = b
            .place(
                BuildingKind::CoalMine,
                Point::new(Metres(700.0), Metres(700.0)),
                &flat(),
                &coal_under(Point::new(Metres(700.0), Metres(700.0))),
            )
            .expect("over coal");
        assert_eq!(b.get(id).unwrap().staffing(), 0.0);
        b.get_mut(id).unwrap().staff = 7;
        assert!((b.get(id).unwrap().staffing() - 0.5).abs() < 1e-12);
        // Overstaffing is capped rather than producing above capacity.
        b.get_mut(id).unwrap().staff = 100;
        assert_eq!(b.get(id).unwrap().staffing(), 1.0);
    }

    #[test]
    fn a_building_with_no_jobs_is_always_fully_working() {
        let mut b = Buildings::new();
        let id = b
            .place(
                BuildingKind::House,
                Point::new(Metres(500.0), Metres(500.0)),
                &flat(),
                &Geology::new(),
            )
            .unwrap();
        assert_eq!(b.get(id).unwrap().staffing(), 1.0);
    }

    #[test]
    fn demolishing_frees_the_ground() {
        let t = flat();
        let g = Geology::new();
        let mut b = Buildings::new();
        let at = Point::new(Metres(500.0), Metres(500.0));
        let id = b.place(BuildingKind::Apartment, at, &t, &g).unwrap();
        assert_eq!(
            b.place(BuildingKind::House, at, &t, &g),
            Err(PlacementError::Occupied)
        );
        assert!(b.demolish(id));
        assert!(b.place(BuildingKind::House, at, &t, &g).is_ok());
        assert!(!b.demolish(id), "demolishing twice is not a second success");
    }

    #[test]
    fn housing_and_jobs_total_across_the_republic() {
        let t = flat();
        let g = Geology::new();
        let mut b = Buildings::new();
        b.place(
            BuildingKind::Apartment,
            Point::new(Metres(200.0), Metres(200.0)),
            &t,
            &g,
        )
        .unwrap();
        b.place(
            BuildingKind::House,
            Point::new(Metres(400.0), Metres(200.0)),
            &t,
            &g,
        )
        .unwrap();
        b.place(
            BuildingKind::Sawmill,
            Point::new(Metres(600.0), Metres(200.0)),
            &t,
            &g,
        )
        .unwrap();
        assert_eq!(b.housing(), 48 + 6);
        assert_eq!(b.jobs(), 6);
    }
}
