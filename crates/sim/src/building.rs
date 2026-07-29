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

use crate::fleet::VehicleKind;
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
    School,
    University,
    TransformerStation,
    Landfill,
    Incinerator,
    Warehouse,
    Depot,
    ConstructionOffice,
    MotorDepot,
    GasStation,
    BusDepot,
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
    /// Heat units wanted on a design-cold day. Housing wants heat; a factory
    /// does not. Authored on every building rather than defaulted, so a new
    /// kind cannot quietly decide it never needs warming.
    pub heat: f64,
    /// Heat units produced. Only a boiler house has this.
    pub heat_output: f64,
    /// Commuters this building can carry to work each day. Only a bus depot
    /// has this — see [`crate::transport`].
    pub seats: u32,
    /// The vehicles this building keeps, and how many of each — a garage's
    /// establishment.
    ///
    /// Authored here beside everything else about the building rather than as a
    /// list of kinds inside the fleet system: a list in logic is a thing you
    /// must remember to edit, and what you forget lands silently in a fallback.
    /// An empty establishment is a decision too — a sawmill keeps no lorries,
    /// and that is stated rather than defaulted.
    pub vehicles: &'static [(VehicleKind, u32)],
    /// Tonnes consumed per day at full efficiency.
    pub inputs: &'static [(Resource, f64)],
    /// Tonnes produced per day at full efficiency.
    pub outputs: &'static [(Resource, f64)],
    /// Tonnes of material the site consumes before it can open.
    pub materials: &'static [(Resource, f64)],
    /// Builder-days of work the site needs. Ported from the archived table.
    pub labour: f64,
    /// What this building puts on shelves for citizens to take.
    ///
    /// Authored here rather than special-cased in the households system: a list
    /// of building kinds inside a system is a thing you must remember to edit,
    /// and what you forget lands silently in a fallback.
    pub sells: &'static [Resource],
    /// The body this building works, if it is an extractor.
    pub taps: Option<Mineral>,
    /// How many people can live here.
    pub residents: u32,
    /// Tonnes of each resource it can hold.
    pub storage: f64,
    /// Tonnes of machinery worn out per day **at full activity** — the
    /// industrialisation tax, ported from the archived `machinery.test.ts`.
    ///
    /// Scales with activity, so an idle building wears nothing and a heating
    /// plant on a warm day wears nothing either. A dry bin never stalls the
    /// building: it runs *worn*, at [`crate::systems::WORN_EFFICIENCY`].
    ///
    /// Authored on every row for the usual reason — `wear: 0.0` on a house is
    /// a decision somebody made, and a defaulted one would not be.
    pub wear: f64,
    /// Whether output answers to growing conditions rather than only to
    /// staffing and power.
    ///
    /// A flag rather than a check against `BuildingKind::Farm`, because a list
    /// of ids inside a system is a thing you must remember to edit and what
    /// you forget lands silently in a fallback. See
    /// [`crate::systems::growing_conditions`].
    pub farms: bool,
    /// The least somebody must have been taught to hold a job here.
    ///
    /// Authored on every row, including the ones with no jobs at all, for the
    /// reason every other field is: a defaulted requirement is a decision
    /// nobody made. It is what makes a school worth building — a republic that
    /// never builds one raises a generation that cannot run its own mines.
    pub schooling: crate::citizen::Education,
    /// What this building teaches, and to whom.
    ///
    /// `None` on everything that is not a place of education. Authored beside
    /// the rest rather than matched on by kind inside the schooling pass, for
    /// the usual reason.
    pub teaches: Option<Teaching>,
    /// Whether this is what a consumer plugs into.
    ///
    /// A flag rather than a check against `BuildingKind::TransformerStation`,
    /// for the reason `farms` is one: a list of ids inside a system is a thing
    /// you must remember to edit, and what you forget lands in a fallback
    /// nobody sees.
    pub transforms: bool,
    /// Tonnes of rubbish per day at full activity.
    ///
    /// On housing this is **per resident**, because what a block throws away is
    /// a function of how many people live in it and not of how large it is. On
    /// everything else it is the building's own, scaled by how hard it is
    /// working — an idle factory throws nothing away.
    pub waste: f64,
    /// Units of pollution per day at full activity.
    ///
    /// Authored on every row including the clean ones, because `pollution: 0.0`
    /// on a school is a decision somebody made and a defaulted zero would not
    /// be.
    pub pollution: f64,
}

/// What a place of education does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Teaching {
    /// Children of school age, up to [`crate::citizen::Education::Schooled`].
    School,
    /// Adults who finished school, up to
    /// [`crate::citizen::Education::Graduate`] — and while they attend, they
    /// are not available for work.
    University,
}

macro_rules! def {
    (
        $kind:ident, $name:literal, $w:expr, $d:expr,
        workers: $workers:expr, draw: $draw:expr, out_mw: $out_mw:expr,
        heat: $heat:expr, heat_out: $heat_out:expr, seats: $seats:expr,
        keeps: [$(($vk:ident, $vn:expr)),* $(,)?],
        in: [$(($ir:ident, $iq:expr)),* $(,)?],
        out: [$(($or:ident, $oq:expr)),* $(,)?],
        cost: [$(($cr:ident, $cq:expr)),* $(,)?], labour: $labour:expr,
        sells: [$($sr:ident),* $(,)?],
        taps: $taps:expr, residents: $residents:expr, storage: $storage:expr,
        wear: $wear:expr, farms: $farms:expr,
        needs: $schooling:ident, teaches: $teaches:expr,
        transforms: $transforms:expr, waste: $waste:expr, dirt: $pollution:expr $(,)?
    ) => {
        BuildingDef {
            kind: BuildingKind::$kind,
            name: $name,
            width: Metres($w),
            depth: Metres($d),
            workers: $workers,
            power_draw: $draw,
            power_output: $out_mw,
            heat: $heat,
            heat_output: $heat_out,
            seats: $seats,
            vehicles: &[$((VehicleKind::$vk, $vn)),*],
            inputs: &[$((Resource::$ir, $iq)),*],
            outputs: &[$((Resource::$or, $oq)),*],
            materials: &[$((Resource::$cr, $cq)),*],
            labour: $labour,
            sells: &[$(Resource::$sr),*],
            taps: $taps,
            residents: $residents,
            storage: $storage,
            wear: $wear,
            farms: $farms,
            schooling: crate::citizen::Education::$schooling,
            teaches: $teaches,
            transforms: $transforms,
            waste: $waste,
            pollution: $pollution,
        }
    };
}

/// The building table. Rates and staffing ported from the archived balance;
/// footprints are real metric sizes the archived build could not express.
pub const BUILDINGS: &[BuildingDef] = &[
    def!(House, "Small House", 12.0, 10.0, workers: 0, draw: 0.0, out_mw: 0.0, heat: 0.5, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Planks, 6.0), (Bricks, 4.0)], labour: 60.0, sells: [], taps: None, residents: 6, storage: 2.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.0015, dirt: 0.02),
    def!(Apartment, "Apartment Block", 62.0, 14.0, workers: 0, draw: 3.0, out_mw: 0.0, heat: 2.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Planks, 10.0), (Bricks, 30.0), (Steel, 6.0), (Gravel, 8.0)], labour: 300.0, sells: [], taps: None, residents: 48, storage: 8.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.0015, dirt: 0.05),
    def!(Woodcutter, "Woodcutter Post", 20.0, 16.0, workers: 6, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(Wood, 4.0)],
        cost: [(Planks, 4.0)], labour: 50.0, sells: [], taps: None, residents: 0, storage: 30.0,
            wear: 0.01, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.05, dirt: 0.1),
    def!(Sawmill, "Sawmill", 34.0, 22.0, workers: 6, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Wood, 2.0)], out: [(Planks, 3.0)],
        cost: [(Bricks, 10.0), (Planks, 6.0), (Steel, 2.0)], labour: 120.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.015, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.2, dirt: 0.4),
    def!(GravelQuarry, "Gravel Quarry", 60.0, 60.0, workers: 8, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(Gravel, 5.0)],
        cost: [(Planks, 6.0), (Bricks, 4.0)], labour: 80.0, sells: [], taps: Some(Mineral::Gravel), residents: 0, storage: 60.0,
            wear: 0.02, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.1, dirt: 1.2),
    def!(Brickworks, "Brickworks", 40.0, 28.0, workers: 10, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Gravel, 3.0)], out: [(Bricks, 4.0)],
        cost: [(Bricks, 12.0), (Steel, 4.0), (Planks, 4.0)], labour: 130.0, sells: [], taps: None, residents: 0, storage: 50.0,
            wear: 0.015, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.3, dirt: 1.6),
    def!(CoalMine, "Coal Mine", 55.0, 45.0, workers: 14, draw: 6.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(Coal, 6.0)],
        cost: [(Bricks, 15.0), (Steel, 6.0), (Planks, 4.0), (Machinery, 2.0)], labour: 200.0, sells: [], taps: Some(Mineral::Coal), residents: 0, storage: 60.0,
            wear: 0.03, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.5, dirt: 2.0),
    def!(IronMine, "Iron Ore Mine", 55.0, 45.0, workers: 14, draw: 6.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(IronOre, 5.0)],
        cost: [(Bricks, 15.0), (Steel, 6.0), (Planks, 4.0), (Machinery, 2.0)], labour: 200.0, sells: [], taps: Some(Mineral::IronOre), residents: 0, storage: 60.0,
            wear: 0.03, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.5, dirt: 2.0),
    def!(SteelMill, "Steel Mill", 180.0, 140.0, workers: 20, draw: 40.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(IronOre, 2.0), (Coal, 1.0)], out: [(Steel, 1.5)],
        cost: [(Bricks, 30.0), (Steel, 15.0), (Planks, 8.0), (Gravel, 16.0), (Machinery, 8.0)], labour: 220.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.03, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 1.2, dirt: 6.0),
    def!(OilPump, "Oil Pump", 24.0, 24.0, workers: 10, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(Oil, 4.0)],
        cost: [(Bricks, 12.0), (Steel, 10.0), (Machinery, 3.0)], labour: 220.0, sells: [], taps: Some(Mineral::Oil), residents: 0, storage: 40.0,
            wear: 0.025, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.2, dirt: 1.5),
    def!(Refinery, "Oil Refinery", 160.0, 120.0, workers: 25, draw: 30.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Oil, 3.0)], out: [(Fuel, 2.0)],
        cost: [(Bricks, 30.0), (Steel, 18.0), (Planks, 6.0), (Gravel, 16.0), (Machinery, 6.0)], labour: 420.0, sells: [], taps: None, residents: 0, storage: 60.0,
            wear: 0.03, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 0.8, dirt: 5.0),
    def!(PowerPlant, "Coal Power Plant", 150.0, 110.0, workers: 15, draw: 0.0, out_mw: 60.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Coal, 4.0)], out: [],
        cost: [(Bricks, 25.0), (Steel, 12.0), (Planks, 6.0), (Gravel, 12.0), (Machinery, 5.0)], labour: 200.0, sells: [], taps: None, residents: 0, storage: 80.0,
            wear: 0.025, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 1.5, dirt: 8.0),
    def!(OilPowerPlant, "Oil Power Plant", 140.0, 100.0, workers: 16, draw: 0.0, out_mw: 70.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Oil, 4.0)], out: [],
        cost: [(Bricks, 30.0), (Steel, 18.0), (Planks, 6.0), (Gravel, 16.0), (Machinery, 8.0)], labour: 400.0, sells: [], taps: None, residents: 0, storage: 80.0,
            wear: 0.025, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 1.0, dirt: 5.5),
    def!(HeatingPlant, "Heating Plant", 45.0, 35.0, workers: 8, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 8.0, seats: 0, keeps: [],
        in: [(Coal, 1.0)], out: [],
        cost: [(Bricks, 18.0), (Steel, 8.0), (Planks, 4.0), (Machinery, 1.0)], labour: 180.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.015, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.4, dirt: 2.2),
    def!(Farm, "Collective Farm", 240.0, 240.0, workers: 10, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(Crops, 6.0)],
        cost: [(Planks, 12.0), (Bricks, 8.0)], labour: 150.0, sells: [], taps: None, residents: 0, storage: 60.0,
            wear: 0.02, farms: true,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.2, dirt: 0.3),
    def!(FoodFactory, "Food Factory", 50.0, 35.0, workers: 12, draw: 4.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Crops, 2.5)], out: [(Food, 2.5)],
        cost: [(Bricks, 18.0), (Steel, 6.0), (Planks, 6.0), (Machinery, 1.0)], labour: 200.0, sells: [], taps: None, residents: 0, storage: 50.0,
            wear: 0.015, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.5, dirt: 0.6),
    def!(TextileMill, "Textile Mill", 55.0, 35.0, workers: 12, draw: 4.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Crops, 2.0)], out: [(Clothes, 1.2)],
        cost: [(Bricks, 16.0), (Steel, 5.0), (Planks, 6.0), (Machinery, 1.0)], labour: 180.0, sells: [], taps: None, residents: 0, storage: 50.0,
            wear: 0.015, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.4, dirt: 0.9),
    def!(MachineWorks, "Machine Works", 150.0, 110.0, workers: 22, draw: 20.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Steel, 3.0)], out: [(Machinery, 1.0)],
        cost: [(Bricks, 35.0), (Steel, 20.0), (Planks, 10.0), (Gravel, 16.0), (Machinery, 6.0)], labour: 250.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.02, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 0.7, dirt: 3.0),
    def!(Store, "State Store", 30.0, 20.0, workers: 3, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Planks, 6.0), (Bricks, 8.0)], labour: 80.0, sells: [Food, Clothes], taps: None, residents: 0, storage: 30.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.15, dirt: 0.05),
    def!(Clinic, "Polyclinic", 45.0, 30.0, workers: 6, draw: 2.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 14.0), (Steel, 4.0), (Planks, 6.0)], labour: 150.0, sells: [], taps: None, residents: 0, storage: 10.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.1, dirt: 0.1),
    def!(CultureClub, "Culture Club", 40.0, 30.0, workers: 4, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Planks, 8.0), (Bricks, 10.0)], labour: 100.0, sells: [], taps: None, residents: 0, storage: 10.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.08, dirt: 0.02),
    // Where the next generation gets what Moscow sent the first one out with.
    // Its own staff need only to be schooled, which is what stops the chain
    // being circular: a republic can always open a school with the people it
    // was founded with, and everything else follows from that.
    def!(School, "Ten-Year School", 55.0, 35.0, workers: 10, draw: 2.0, out_mw: 0.0, heat: 1.5, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 16.0), (Planks, 10.0), (Steel, 4.0)], labour: 160.0, sells: [], taps: None, residents: 0, storage: 10.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: Some(Teaching::School),
            transforms: false, waste: 0.12, dirt: 0.02),
    // And what turns a schooled worker into somebody who can run a refinery.
    // The cost is not only the building: a student is a working-age adult who
    // is not working, so a republic putting people through this is three years
    // short of their hands to be better off afterwards.
    def!(University, "Polytechnic Institute", 90.0, 60.0, workers: 14, draw: 5.0, out_mw: 0.0, heat: 3.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 30.0), (Planks, 14.0), (Steel, 10.0), (Gravel, 12.0)], labour: 320.0, sells: [], taps: None, residents: 0, storage: 20.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: Some(Teaching::University),
            transforms: false, waste: 0.2, dirt: 0.05),
    // What a consumer actually plugs into. High-voltage line to the station,
    // low-voltage station to the street — two hops, because a pylon strung past
    // a factory is not what runs it, and modelling it as one would leave this
    // building with nothing to do.
    def!(TransformerStation, "Transformer Station", 24.0, 20.0, workers: 3, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 8.0), (Steel, 10.0), (Gravel, 6.0)], labour: 90.0, sells: [], taps: None, residents: 0, storage: 10.0,
            wear: 0.005, farms: false,
            needs: Schooled, teaches: None,
            transforms: true, waste: 0.0, dirt: 0.05),
    // Where the republic's rubbish goes. It *consumes* waste rather than merely
    // holding it, which is what makes it a consignee the freight ranking already
    // understands: a landfill that runs out of rubbish is a landfill with spare
    // capacity, and the dispatcher will go and fetch some.
    def!(Landfill, "Landfill", 120.0, 90.0, workers: 5, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Waste, 10.0)], out: [],
        cost: [(Gravel, 20.0), (Planks, 6.0)], labour: 100.0, sells: [], taps: None, residents: 0, storage: 400.0,
            wear: 0.01, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.0, dirt: 3.5),
    // The other answer, and the trade is explicit: it burns twice what a
    // landfill buries and gives current back for it, at twice the filth.
    def!(Incinerator, "Refuse Incinerator", 80.0, 60.0, workers: 12, draw: 0.0, out_mw: 12.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Waste, 20.0)], out: [],
        cost: [(Bricks, 24.0), (Steel, 16.0), (Gravel, 14.0), (Machinery, 4.0)], labour: 300.0, sells: [], taps: None, residents: 0, storage: 120.0,
            wear: 0.02, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.0, dirt: 7.0),
    def!(Warehouse, "Warehouse", 60.0, 30.0, workers: 2, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Planks, 8.0), (Bricks, 10.0)], labour: 90.0, sells: [], taps: None, residents: 0, storage: 200.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.05, dirt: 0.05),
    def!(Depot, "Council Depot", 80.0, 60.0, workers: 4, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 15.0), (Planks, 10.0)], labour: 120.0, sells: [], taps: None, residents: 0, storage: 300.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.05, dirt: 0.1),
    // The republic's builders, and the machinery they build with. Nothing goes
    // up without one: an office employs the crews, owns the plant, and runs the
    // bus that puts a gang on a site.
    //
    // **Sixteen staff and two buses**, which is one full gang of ten with six
    // in hand — enough that a bus can be fetching one crew home while the other
    // is taking one out. Two full gangs at once wants a second office, which is
    // the shape every other capacity in this republic has.
    //
    // Two numbers here were got wrong first and both are worth naming. Twenty
    // staff and two buses made the founding offer 134 jobs against 120
    // settlers, and since the customs house is last in the staffing order the
    // default republic quietly lost the ability to trade at all —
    // `the_founding_hand_can_staff_itself` is what stops that recurring. Then
    // **one** bus turned out to be a cliff rather than a saving: `crewed`
    // floors, so a single-vehicle establishment runs zero buses the moment the
    // office is one person short, where a depot of seven degrades smoothly.
    // An establishment of one is an on/off switch wearing a capacity's clothes.
    //
    // Its `wear` is zero and that is a decision rather than an omission: an
    // office's plant is worn by the builder-days its crews actually work, in
    // `systems::construction`, and a flat daily rate on top would charge a
    // republic for diggers standing in the yard. The `Machinery` appetite
    // declared here is what the resupply ranking reads; it is spent out on the
    // sites.
    def!(ConstructionOffice, "Construction Office", 35.0, 25.0, workers: 16, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0,
        keeps: [(CrewBus, 2)],
        in: [(Machinery, 0.3), (Fuel, 0.12)], out: [],
        cost: [(Bricks, 10.0), (Planks, 8.0), (Machinery, 1.0)], labour: 110.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.2, dirt: 0.3),
    // The republic's haulage. Its establishment is where the fleet comes from —
    // wanting more lorries means another depot and sixteen more people for it,
    // never a number in a settings file. Its fuel is drawn by the vehicles
    // themselves, per kilometre driven, so the rate declared here is only the
    // appetite the resupply ranking reads.
    def!(MotorDepot, "Motor Depot", 90.0, 70.0, workers: 16, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0,
        keeps: [(Lorry, 4), (HeavyLorry, 2), (RecoveryVehicle, 1)],
        in: [(Fuel, 0.2)], out: [],
        cost: [(Bricks, 18.0), (Planks, 12.0), (Steel, 6.0), (Gravel, 8.0)], labour: 150.0, sells: [], taps: None, residents: 0, storage: 60.0,
            wear: 0.02, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.3, dirt: 1.0),
    def!(GasStation, "Gas Station", 30.0, 20.0, workers: 4, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 8.0), (Steel, 6.0), (Planks, 4.0)], labour: 90.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.005, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.05, dirt: 0.6),
    // The one building that changes what "within reach" means. Its fuel is
    // burnt by the labour pass in proportion to seats actually filled, not by
    // production — see `crate::transport`.
    //
    // It also keeps the republic's coaches, which is what fetches settlers in
    // from a frontier post. That is the same building doing two things people-
    // shaped rather than a second depot: a republic that has decided to move
    // people around has decided it once.
    def!(BusDepot, "Bus Depot", 70.0, 50.0, workers: 12, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 400,
        keeps: [(Coach, 2)],
        in: [(Fuel, 0.8)], out: [],
        cost: [(Bricks, 16.0), (Planks, 10.0), (Steel, 5.0), (Gravel, 8.0)], labour: 140.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.02, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.25, dirt: 1.0),
    def!(Customs, "Customs House", 90.0, 60.0, workers: 8, draw: 2.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 20.0), (Steel, 6.0), (Planks, 8.0), (Gravel, 10.0)], labour: 200.0, sells: [], taps: None, residents: 0, storage: 200.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.1, dirt: 0.15),
];

impl BuildingDef {
    /// Whether some system other than `production` burns this building's
    /// inputs.
    ///
    /// A boiler house burns coal against today's temperature, a bus depot burns
    /// fuel against seats actually filled, and a garage burns fuel against
    /// kilometres actually driven. All three throttle to demand, so letting the
    /// production system burn them as well at a flat daily rate would
    /// double-charge them — and burning at a flat rate is worse than wrong, it
    /// means a boiler consumes a January's coal in July.
    ///
    /// Expressed as a property of the authored data rather than as a list of
    /// kinds, so a new building that burns its own inputs declares it by having
    /// the field that says so.
    pub fn burns_its_own_inputs(&self) -> bool {
        self.heat_output > 0.0 || self.seats > 0 || !self.vehicles.is_empty()
    }
}

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
    /// Whether the grid is feeding it — set by the power system, never authored.
    pub powered: bool,
    /// Whether the boilers are reaching it — set by the heating system, never
    /// authored. Meaningless on anything with no heat demand, and `true` all
    /// summer, when there is nothing to reach it with.
    pub heated: bool,
    /// How much of its residents' needs the shops met, 0..=1. Set by the
    /// households system, never authored. Meaningless on anything that is not
    /// housing.
    pub provisioned: f64,
    /// How well the republic is serving the people who live here, component by
    /// component. Set by the contentment system, never authored. Meaningless on
    /// anything that is not housing.
    ///
    /// Stored as the breakdown rather than the score, because "your people are
    /// at 61%" is not something a player can act on and "fed, warm, no doctor,
    /// no work" is.
    pub content: crate::wellbeing::Contentment,
    /// Builder-days worked on this site so far. A building is a SITE until
    /// this reaches its def's `labour`, and a site produces nothing, employs
    /// nobody, and houses nobody.
    pub work_done: f64,
    /// The body this building works, once sited.
    pub tapped: Option<crate::geology::DepositId>,
}

impl Building {
    pub fn def(&self) -> &'static BuildingDef {
        self.kind.def()
    }

    /// Whether the site is finished and open.
    pub fn is_built(&self) -> bool {
        self.work_done >= self.def().labour
    }

    /// How far along the site is, `0.0..=1.0`.
    pub fn progress(&self) -> f64 {
        if self.def().labour <= 0.0 {
            1.0
        } else {
            (self.work_done / self.def().labour).clamp(0.0, 1.0)
        }
    }

    /// Whether the materials for the work still to do are on hand.
    ///
    /// A site that has been delivered nothing waits — the archived build's
    /// rule, and what makes freight priority matter during a build-out. But the
    /// bill is consumed **in step with the work** (see `Mutation::Build`), so a
    /// site that has had its full bill delivered once must not then read as
    /// short of it. The requirement falls as the work is done, and the total a
    /// site consumes over its life is exactly its bill.
    ///
    /// This used to demand the whole bill at every moment, which meant a site
    /// worked for one tick and then stalled until freight topped it back up
    /// — twice over the bill in total. That was invisible while freight was a
    /// scalar and a delivery landed the instant it was ranked. With lorries it
    /// is minutes or hours per top-up, and a build-out slowed to a crawl for a
    /// reason nothing about the construction code showed.
    pub fn has_materials(&self) -> bool {
        // One `def()` lookup, not two: this runs for every building on every
        // tick of the construction pass, and the table is searched linearly.
        let def = self.def();
        let left = self.work_left(def);
        def.materials
            .iter()
            .all(|&(r, q)| self.stock.get(r).0 + 1e-9 >= q * left)
    }

    /// How much of a material the site still has to be brought.
    pub fn material_outstanding(&self, resource: Resource) -> Tonnes {
        let def = self.def();
        let left = self.work_left(def);
        let wanted = def
            .materials
            .iter()
            .find(|(r, _)| *r == resource)
            .map(|&(_, q)| q * left)
            .unwrap_or(0.0);
        Tonnes(wanted).saturating_sub(self.stock.get(resource))
    }

    /// The share of the build still to do, `0.0..=1.0`.
    fn work_left(&self, def: &BuildingDef) -> f64 {
        if def.labour <= 0.0 {
            0.0
        } else {
            1.0 - (self.work_done / def.labour).clamp(0.0, 1.0)
        }
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

    /// How much of a resource this building may hold.
    ///
    /// A SITE may hold at least its bill of materials for that resource, even
    /// when that exceeds the bin it will have once open. Otherwise a building
    /// whose construction needs more brick than it will ever store could never
    /// be built at all.
    pub fn intake_capacity(&self, resource: Resource) -> Tonnes {
        if self.is_built() {
            return self.storage_cap();
        }
        let needed = self
            .def()
            .materials
            .iter()
            .find(|(r, _)| *r == resource)
            .map(|&(_, q)| Tonnes(q))
            .unwrap_or(Tonnes::ZERO);
        Tonnes(self.storage_cap().0.max(needed.0))
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
    /// A customs house away from the border. Trade is physical: a crossing has
    /// to be at the crossing.
    NotOnTheBorder,
}

/// What the player is told, in the register the rest of the game speaks in.
///
/// A refusal that cannot say why is a refusal nobody can act on, and the
/// archived build's reason strings are what drove both its toasts and the
/// tooltip on every greyed-out button. Written here, next to the variants, so
/// adding a variant without its wording is a compile error rather than a
/// silent fallback.
impl std::fmt::Display for PlacementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlacementError::Unbuildable => write!(f, "the ground here will not take it"),
            PlacementError::Occupied => write!(f, "something already stands here"),
            PlacementError::NothingToTap(mineral) => {
                write!(f, "there is no {} beneath this ground", mineral.name())
            }
            PlacementError::NotOnTheBorder => {
                write!(f, "a customs house must stand at the national border")
            }
        }
    }
}

impl std::error::Error for PlacementError {}

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

    /// How many buildings the republic has ever commissioned.
    ///
    /// A [`BuildingId`] is drawn from this same sequence, so a building's id
    /// *is* its place in the commissioning order — which is what lets
    /// [`crate::roadworks::RoadSite`] take a number from the same run and be
    /// ranked against buildings by the construction system.
    pub fn commissioned(&self) -> u64 {
        u64::from(self.next_id) - 1
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
        self.list
            .iter()
            .filter(|b| b.is_built())
            .map(|b| b.def().residents)
            .sum()
    }

    /// Every job the republic offers, staffed or not.
    pub fn jobs(&self) -> u32 {
        self.list
            .iter()
            .filter(|b| b.is_built())
            .map(|b| b.def().workers)
            .sum()
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
            powered: false,
            heated: true,
            provisioned: 0.0,
            content: crate::wellbeing::Contentment::NOTHING,
            work_done: 0.0,
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
            powered: false,
            heated: true,
            provisioned: 0.0,
            content: crate::wellbeing::Contentment::NOTHING,
            work_done: 0.0,
            tapped,
        });
        Ok(id)
    }

    /// Put a building up already finished — the founding grant, and what tests
    /// use when construction is not what they are testing.
    pub fn place_built(
        &mut self,
        kind: BuildingKind,
        centre: Point,
        terrain: &crate::terrain::Terrain,
        geology: &crate::geology::Geology,
    ) -> Result<BuildingId, PlacementError> {
        let id = self.place(kind, centre, terrain, geology)?;
        if let Some(b) = self.get_mut(id) {
            b.work_done = b.def().labour;
        }
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
            .place_built(
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
            .place_built(
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
        b.place_built(
            BuildingKind::Apartment,
            Point::new(Metres(200.0), Metres(200.0)),
            &t,
            &g,
        )
        .unwrap();
        b.place_built(
            BuildingKind::House,
            Point::new(Metres(400.0), Metres(200.0)),
            &t,
            &g,
        )
        .unwrap();
        b.place_built(
            BuildingKind::Sawmill,
            Point::new(Metres(600.0), Metres(200.0)),
            &t,
            &g,
        )
        .unwrap();
        assert_eq!(b.housing(), 48 + 6);
        assert_eq!(b.jobs(), 6);
    }

    /// A site is not a building. It houses nobody and employs nobody until it
    /// opens — otherwise a republic could staff a factory by ordering one.
    #[test]
    fn a_site_houses_and_employs_nobody_until_it_opens() {
        let t = flat();
        let g = Geology::new();
        let mut b = Buildings::new();
        let flats = b
            .place(
                BuildingKind::Apartment,
                Point::new(Metres(200.0), Metres(200.0)),
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
        assert_eq!(b.housing(), 0, "a site is not a home");
        assert_eq!(b.jobs(), 0, "a site is not a workplace");
        assert!(!b.get(flats).unwrap().is_built());
        assert_eq!(b.get(flats).unwrap().progress(), 0.0);

        b.get_mut(flats).unwrap().work_done = BuildingKind::Apartment.def().labour;
        assert!(b.get(flats).unwrap().is_built());
        assert_eq!(b.housing(), 48);
    }

    #[test]
    fn a_site_knows_whether_its_materials_have_arrived() {
        let t = flat();
        let g = Geology::new();
        let mut b = Buildings::new();
        let id = b
            .place(
                BuildingKind::Woodcutter,
                Point::new(Metres(200.0), Metres(200.0)),
                &t,
                &g,
            )
            .unwrap();
        assert!(!b.get(id).unwrap().has_materials());
        // A woodcutter post wants four tonnes of planks.
        b.get_mut(id)
            .unwrap()
            .stock
            .add(Resource::Planks, Tonnes(4.0));
        assert!(b.get(id).unwrap().has_materials());
    }

    /// A site may need more of a material than it will ever store. Capping its
    /// intake by its finished bin would make such a building unbuildable.
    #[test]
    fn a_site_may_take_in_more_than_it_will_hold_when_open() {
        let t = flat();
        let g = Geology::new();
        let mut b = Buildings::new();
        let id = b
            .place(
                BuildingKind::SteelMill,
                Point::new(Metres(500.0), Metres(500.0)),
                &t,
                &g,
            )
            .unwrap();
        let site = b.get(id).unwrap();
        // 30 t of brick to build; 40 t of anything once open.
        assert_eq!(site.intake_capacity(Resource::Bricks), Tonnes(40.0));
        // And once open it is bounded by the bin again.
        b.get_mut(id).unwrap().work_done = BuildingKind::SteelMill.def().labour;
        assert_eq!(
            b.get(id).unwrap().intake_capacity(Resource::Bricks),
            Tonnes(40.0)
        );
    }
}
