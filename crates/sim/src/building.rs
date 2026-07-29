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
use crate::resource::{Form, Resource, Stock};
use crate::units::{Metres, Point, Tonnes};
use serde::{Deserialize, Serialize};

/// Every kind of building. Roads are not here — they are a graph, not a
/// building, and live in [`crate::road`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BuildingKind {
    House,
    Apartment,
    PanelBlock,
    Woodcutter,
    Sawmill,
    GravelQuarry,
    Brickworks,
    CementWorks,
    ConcretePlant,
    PanelWorks,
    CoalMine,
    IronMine,
    SteelMill,
    OilPump,
    Refinery,
    ChemicalPlant,
    AsphaltPlant,
    ElectronicsWorks,
    PowerPlant,
    OilPowerPlant,
    HeatingPlant,
    Farm,
    FoodFactory,
    TextileMill,
    Distillery,
    MachineWorks,
    Store,
    Clinic,
    CultureClub,
    School,
    University,
    Kindergarten,
    Hospital,
    Pharmacy,
    FireStation,
    PoliceStation,
    Courthouse,
    Prison,
    SportsHall,
    Cinema,
    Broadcast,
    Cemetery,
    Hotel,
    TransformerStation,
    Landfill,
    Incinerator,
    Warehouse,
    OpenYard,
    AggregateBay,
    StorageTank,
    GrainSilo,
    Depot,
    ConstructionOffice,
    TrolleybusDepot,
    TramDepot,
    MetroDepot,
    RailwayStation,
    RiverPort,
    Aerodrome,
    DistributionOffice,
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
    /// Which shapes of goods this place will take at all.
    ///
    /// **This is what turns storage from a number into a decision.** A republic
    /// used to be able to keep two hundred tonnes of anything in one shed; a
    /// tank now holds liquids and nothing else, a silo holds grain and cement,
    /// and a heap of gravel is a heap of gravel. [`crate::resource::Form`] is
    /// the property of the goods; this is the property of the place, and
    /// [`Building::intake_capacity`] is where the two meet.
    ///
    /// Authored on every row, and on a producer it is exactly the forms of what
    /// it eats and what it makes — checked by
    /// `every_building_will_hold_what_it_handles`, so a row that gets it wrong
    /// fails the build rather than quietly refusing its own output.
    ///
    /// **A site under construction is exempt**, and deliberately: a bill of
    /// materials is not stock, it is the building arriving in pieces. A tank
    /// that could not accept the bricks it is made of could never be built.
    pub admits: &'static [Form],
    /// Beds for visitors from abroad.
    ///
    /// Its own field rather than a second meaning for `residents`, because the
    /// two are genuinely different things: a resident lives here, is counted in
    /// the census, votes with their feet and is judged by
    /// [`crate::wellbeing::Contentment`]. A tourist occupies a bed, spends hard
    /// currency and goes home. A hotel with `residents` would attract settlers
    /// to live in it and be marked down for having no school.
    pub beds: u32,
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
    /// The way this building stands beside, if it is a terminal.
    ///
    /// `None` on everything that is not one, which is nearly everything. A
    /// station, a wharf and a landing ground are all the same idea — a place
    /// where a confined vehicle can be reached — and this is what says which
    /// network it has to be within [`crate::journey::TERMINAL_REACH`] of before
    /// it may be built at all. Authored rather than matched on by kind, for the
    /// reason `farms` and `transforms` are.
    pub medium: Option<crate::journey::Medium>,
    /// What this building provides to the people near it, and how much of it.
    ///
    /// **A share rather than a flag**, because a clinic is not a hospital. A
    /// republic with one clinic per estate has *some* healthcare and not all of
    /// it, and rounding that to "you have healthcare" would hide exactly the
    /// quiet failure the goal's first condition exists to make visible. Cover
    /// from several buildings adds up and is capped at one.
    ///
    /// This changed a balance figure that was an artefact rather than a
    /// decision: a Polyclinic used to supply *complete* health cover, because
    /// it was the only health building there was. It now supplies rather more
    /// than half, and a republic that wants its people fully looked after
    /// builds a hospital and a pharmacy too.
    pub serves: &'static [(Need, f64)],
    /// Whether the player may set standing orders here.
    ///
    /// A terminal is useless without one: nothing in the republic wants to
    /// deliver to a station, because a station consumes nothing and sells
    /// nothing. A standing order is what makes it a destination — "keep fifty
    /// tonnes of coal here" — and it is the same mechanism a distribution
    /// office runs on, which is why it is a field rather than two systems.
    pub stores_to_order: bool,
}

/// Something the people who live nearby need somebody to provide.
///
/// **Authored on the building rather than matched on by kind.** Contentment
/// used to ask `service_cover(home, BuildingKind::Clinic)` and two more like
/// it, which is a list of ids inside a system: a thing you must remember to
/// edit, where what you forget lands silently in a fallback. It is why the
/// whole services roster below is data rows and nothing in the contentment
/// pass knows how many kinds of building serve health.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Need {
    Health,
    Culture,
    /// Somewhere for children, before they are old enough to work.
    Schooling,
    /// Fire, police and the courts.
    Safety,
}

impl Need {
    pub const ALL: [Need; 4] = [Need::Health, Need::Culture, Need::Schooling, Need::Safety];
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
        transforms: $transforms:expr, waste: $waste:expr, dirt: $pollution:expr,
        on: $medium:expr, orders: $orders:expr,
        serves: [$(($sn:ident, $sq:expr)),* $(,)?],
        holds: [$($fm:ident),* $(,)?], beds: $beds:expr $(,)?
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
            medium: $medium,
            stores_to_order: $orders,
            serves: &[$((Need::$sn, $sq)),*],
            admits: &[$(Form::$fm),*],
            beds: $beds,
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
            transforms: false, waste: 0.0015, dirt: 0.02, on: None, orders: false, serves: [],
            holds: [], beds: 0),
    def!(Apartment, "Apartment Block", 62.0, 14.0, workers: 0, draw: 3.0, out_mw: 0.0, heat: 2.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Planks, 10.0), (Bricks, 30.0), (Steel, 6.0), (Gravel, 8.0)], labour: 300.0, sells: [], taps: None, residents: 48, storage: 8.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.0015, dirt: 0.05, on: None, orders: false, serves: [],
            holds: [], beds: 0),
    // What the panel chain buys, and the only thing in the table that is a
    // *different building* rather than a cheaper version of one. Two and a half
    // times an apartment block's residents for a third of its builder-days per
    // head -- because the walls arrive on a lorry already made. A republic that
    // has not industrialised cannot build one at all, and that is the reward
    // for having done so.
    def!(PanelBlock, "Panel Block", 78.0, 16.0, workers: 0, draw: 6.0, out_mw: 0.0, heat: 4.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(PrefabPanel, 44.0), (Concrete, 30.0), (Steel, 10.0), (Gravel, 12.0)], labour: 260.0, sells: [], taps: None, residents: 120, storage: 12.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.0015, dirt: 0.06, on: None, orders: false, serves: [],
            holds: [], beds: 0),
    def!(Woodcutter, "Woodcutter Post", 20.0, 16.0, workers: 6, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(Wood, 4.0)],
        cost: [(Planks, 4.0)], labour: 50.0, sells: [], taps: None, residents: 0, storage: 30.0,
            wear: 0.01, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.05, dirt: 0.1, on: None, orders: false, serves: [],
            holds: [Open], beds: 0),
    def!(Sawmill, "Sawmill", 34.0, 22.0, workers: 6, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Wood, 2.0)], out: [(Planks, 3.0)],
        cost: [(Bricks, 10.0), (Planks, 6.0), (Steel, 2.0)], labour: 120.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.015, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.2, dirt: 0.4, on: None, orders: false, serves: [],
            holds: [Open], beds: 0),
    def!(GravelQuarry, "Gravel Quarry", 60.0, 60.0, workers: 8, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(Gravel, 5.0)],
        cost: [(Planks, 6.0), (Bricks, 4.0)], labour: 80.0, sells: [], taps: Some(Mineral::Gravel), residents: 0, storage: 60.0,
            wear: 0.02, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.1, dirt: 1.2, on: None, orders: false, serves: [],
            holds: [Aggregate, Open], beds: 0),
    def!(Brickworks, "Brickworks", 40.0, 28.0, workers: 10, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Gravel, 3.0)], out: [(Bricks, 4.0)],
        cost: [(Bricks, 12.0), (Steel, 4.0), (Planks, 4.0)], labour: 130.0, sells: [], taps: None, residents: 0, storage: 50.0,
            wear: 0.015, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.3, dirt: 1.6, on: None, orders: false, serves: [],
            holds: [Aggregate, Open], beds: 0),
    // ---- The mineral binder chain ----------------------------------------
    //
    // Gravel and coal into cement, cement and gravel into concrete, concrete
    // and steel into a wall. Three steps rather than one because that is what
    // makes it a *chain*: each stage is a building somebody has to site, staff
    // and haul to, and a republic can get two thirds of the way and stall.
    def!(CementWorks, "Cement Works", 110.0, 80.0, workers: 18, draw: 18.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Gravel, 4.0), (Coal, 1.5)], out: [(Cement, 3.0)],
        cost: [(Bricks, 30.0), (Steel, 16.0), (Planks, 8.0), (Gravel, 20.0), (Machinery, 5.0)], labour: 300.0, sells: [], taps: None, residents: 0, storage: 60.0,
            wear: 0.03, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.9, dirt: 5.5, on: None, orders: false, serves: [],
            holds: [Aggregate, Bulk, Open], beds: 0),
    def!(ConcretePlant, "Concrete Plant", 70.0, 50.0, workers: 12, draw: 8.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Cement, 2.0), (Gravel, 4.0)], out: [(Concrete, 5.0)],
        cost: [(Bricks, 20.0), (Steel, 12.0), (Planks, 6.0), (Gravel, 14.0), (Machinery, 3.0)], labour: 210.0, sells: [], taps: None, residents: 0, storage: 60.0,
            wear: 0.025, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.4, dirt: 2.0, on: None, orders: false, serves: [],
            holds: [Bulk, Aggregate, Open], beds: 0),
    def!(PanelWorks, "Panel Works", 130.0, 90.0, workers: 20, draw: 14.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Concrete, 4.0), (Steel, 1.0)], out: [(PrefabPanel, 4.0)],
        cost: [(Bricks, 34.0), (Steel, 24.0), (Planks, 10.0), (Gravel, 22.0), (Machinery, 6.0)], labour: 340.0, sells: [], taps: None, residents: 0, storage: 80.0,
            wear: 0.03, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.5, dirt: 1.8, on: None, orders: false, serves: [],
            holds: [Open], beds: 0),
    def!(CoalMine, "Coal Mine", 55.0, 45.0, workers: 14, draw: 6.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(Coal, 6.0)],
        cost: [(Bricks, 15.0), (Steel, 6.0), (Planks, 4.0), (Machinery, 2.0)], labour: 200.0, sells: [], taps: Some(Mineral::Coal), residents: 0, storage: 60.0,
            wear: 0.03, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.5, dirt: 2.0, on: None, orders: false, serves: [],
            holds: [Aggregate, Open], beds: 0),
    def!(IronMine, "Iron Ore Mine", 55.0, 45.0, workers: 14, draw: 6.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(IronOre, 5.0)],
        cost: [(Bricks, 15.0), (Steel, 6.0), (Planks, 4.0), (Machinery, 2.0)], labour: 200.0, sells: [], taps: Some(Mineral::IronOre), residents: 0, storage: 60.0,
            wear: 0.03, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.5, dirt: 2.0, on: None, orders: false, serves: [],
            holds: [Aggregate, Open], beds: 0),
    def!(SteelMill, "Steel Mill", 180.0, 140.0, workers: 20, draw: 40.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(IronOre, 2.0), (Coal, 1.0)], out: [(Steel, 1.5)],
        cost: [(Bricks, 30.0), (Steel, 15.0), (Planks, 8.0), (Gravel, 16.0), (Machinery, 8.0)], labour: 220.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.03, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 1.2, dirt: 6.0, on: None, orders: false, serves: [],
            holds: [Aggregate, Open], beds: 0),
    def!(OilPump, "Oil Pump", 24.0, 24.0, workers: 10, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(Oil, 4.0)],
        cost: [(Bricks, 12.0), (Steel, 10.0), (Machinery, 3.0)], labour: 220.0, sells: [], taps: Some(Mineral::Oil), residents: 0, storage: 40.0,
            wear: 0.025, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.2, dirt: 1.5, on: None, orders: false, serves: [],
            holds: [Liquid, Open], beds: 0),
    // A barrel does not come apart into diesel alone, and this is the one row
    // in the table with two outputs for that reason: a republic that refines
    // for fuel has bitumen whether it wanted any or not. What it does with it
    // is the asphalt plant's business -- and until one is built, the bitumen
    // fills the tanks and backs the refinery up, which is the honest
    // consequence rather than a by-product that politely vanishes.
    def!(Refinery, "Oil Refinery", 160.0, 120.0, workers: 25, draw: 30.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Oil, 3.0)], out: [(Fuel, 2.0), (Bitumen, 0.6)],
        cost: [(Bricks, 30.0), (Steel, 18.0), (Planks, 6.0), (Gravel, 16.0), (Machinery, 6.0)], labour: 420.0, sells: [], taps: None, residents: 0, storage: 60.0,
            wear: 0.03, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 0.8, dirt: 5.0, on: None, orders: false, serves: [],
            holds: [Liquid, Open], beds: 0),
    // Oil into feedstock, and the second thing a refinery's neighbour wants.
    def!(ChemicalPlant, "Chemical Works", 140.0, 100.0, workers: 24, draw: 26.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Oil, 2.5)], out: [(Chemicals, 1.4)],
        cost: [(Bricks, 34.0), (Steel, 26.0), (Planks, 8.0), (Gravel, 20.0), (Machinery, 8.0)], labour: 460.0, sells: [], taps: None, residents: 0, storage: 60.0,
            wear: 0.035, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 1.1, dirt: 7.5, on: None, orders: false, serves: [],
            holds: [Liquid, Open], beds: 0),
    // What the heavy end of the barrel is for. Asphalt is what a paved road is
    // made of, so this is the building that turns "I have a refinery" into "I
    // can surface my republic" -- and until it exists, paved road is an import.
    def!(AsphaltPlant, "Asphalt Plant", 80.0, 60.0, workers: 12, draw: 6.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Bitumen, 0.8), (Gravel, 5.0)], out: [(Asphalt, 5.0)],
        cost: [(Bricks, 18.0), (Steel, 14.0), (Planks, 6.0), (Gravel, 16.0), (Machinery, 4.0)], labour: 220.0, sells: [], taps: None, residents: 0, storage: 60.0,
            wear: 0.025, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.4, dirt: 4.0, on: None, orders: false, serves: [],
            holds: [Liquid, Aggregate, Open], beds: 0),
    // The top of the industrial chain and the best tonne the republic can put
    // in a lorry: a tonne of these is worth ninety-five dollars against coal's
    // one. It is deliberately four buildings deep -- pump, refinery, chemical
    // works, here -- because that distance is what makes it an achievement
    // rather than a building.
    def!(ElectronicsWorks, "Electronics Combine", 120.0, 80.0, workers: 26, draw: 18.0, out_mw: 0.0, heat: 1.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Steel, 0.8), (Chemicals, 1.0)], out: [(Electronics, 1.0)],
        cost: [(Bricks, 40.0), (Steel, 28.0), (Planks, 12.0), (Gravel, 24.0), (Machinery, 10.0)], labour: 520.0, sells: [], taps: None, residents: 0, storage: 50.0,
            wear: 0.03, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 0.6, dirt: 2.2, on: None, orders: false, serves: [],
            holds: [Open, Liquid, Covered], beds: 0),
    def!(PowerPlant, "Coal Power Plant", 150.0, 110.0, workers: 15, draw: 0.0, out_mw: 60.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Coal, 4.0)], out: [],
        cost: [(Bricks, 25.0), (Steel, 12.0), (Planks, 6.0), (Gravel, 12.0), (Machinery, 5.0)], labour: 200.0, sells: [], taps: None, residents: 0, storage: 80.0,
            wear: 0.025, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 1.5, dirt: 8.0, on: None, orders: false, serves: [],
            holds: [Aggregate, Open], beds: 0),
    def!(OilPowerPlant, "Oil Power Plant", 140.0, 100.0, workers: 16, draw: 0.0, out_mw: 70.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Oil, 4.0)], out: [],
        cost: [(Bricks, 30.0), (Steel, 18.0), (Planks, 6.0), (Gravel, 16.0), (Machinery, 8.0)], labour: 400.0, sells: [], taps: None, residents: 0, storage: 80.0,
            wear: 0.025, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 1.0, dirt: 5.5, on: None, orders: false, serves: [],
            holds: [Liquid, Open], beds: 0),
    def!(HeatingPlant, "Heating Plant", 45.0, 35.0, workers: 8, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 8.0, seats: 0, keeps: [],
        in: [(Coal, 1.0)], out: [],
        cost: [(Bricks, 18.0), (Steel, 8.0), (Planks, 4.0), (Machinery, 1.0)], labour: 180.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.015, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.4, dirt: 2.2, on: None, orders: false, serves: [],
            holds: [Aggregate, Open], beds: 0),
    def!(Farm, "Collective Farm", 240.0, 240.0, workers: 10, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [(Crops, 6.0)],
        cost: [(Planks, 12.0), (Bricks, 8.0)], labour: 150.0, sells: [], taps: None, residents: 0, storage: 60.0,
            wear: 0.02, farms: true,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.2, dirt: 0.3, on: None, orders: false, serves: [],
            holds: [Bulk, Open], beds: 0),
    def!(FoodFactory, "Food Factory", 50.0, 35.0, workers: 12, draw: 4.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Crops, 2.5)], out: [(Food, 2.5)],
        cost: [(Bricks, 18.0), (Steel, 6.0), (Planks, 6.0), (Machinery, 1.0)], labour: 200.0, sells: [], taps: None, residents: 0, storage: 50.0,
            wear: 0.015, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.5, dirt: 0.6, on: None, orders: false, serves: [],
            holds: [Bulk, Covered, Open], beds: 0),
    def!(TextileMill, "Textile Mill", 55.0, 35.0, workers: 12, draw: 4.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Crops, 2.0)], out: [(Clothes, 1.2)],
        cost: [(Bricks, 16.0), (Steel, 5.0), (Planks, 6.0), (Machinery, 1.0)], labour: 180.0, sells: [], taps: None, residents: 0, storage: 50.0,
            wear: 0.015, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.4, dirt: 0.9, on: None, orders: false, serves: [],
            holds: [Bulk, Covered, Open], beds: 0),
    // Crops into hard currency, and the third mouth at the same field. A
    // republic with one farm now chooses between feeding its people, clothing
    // them, and earning dollars -- which is the tension the crop chain was
    // missing when only two buildings ate from it.
    def!(Distillery, "State Distillery", 60.0, 40.0, workers: 14, draw: 5.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Crops, 3.0)], out: [(Alcohol, 1.0)],
        cost: [(Bricks, 22.0), (Steel, 10.0), (Planks, 8.0), (Machinery, 2.0)], labour: 240.0, sells: [], taps: None, residents: 0, storage: 50.0,
            wear: 0.02, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.5, dirt: 1.2, on: None, orders: false, serves: [],
            holds: [Bulk, Liquid, Open], beds: 0),
    def!(MachineWorks, "Machine Works", 150.0, 110.0, workers: 22, draw: 20.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Steel, 3.0)], out: [(Machinery, 1.0)],
        cost: [(Bricks, 35.0), (Steel, 20.0), (Planks, 10.0), (Gravel, 16.0), (Machinery, 6.0)], labour: 250.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.02, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 0.7, dirt: 3.0, on: None, orders: false, serves: [],
            holds: [Open], beds: 0),
    // Four things on the shelves: two people need and two they are glad of.
    // Whether the drink and the electrics ever get here is the player's
    // decision — both are worth more at a frontier post than they are on this
    // counter, which is exactly the tension they exist for.
    //
    // It admits `Liquid` because of the alcohol, which is made and hauled in
    // bulk like any other liquid. A state store with a cellar.
    def!(Store, "State Store", 30.0, 20.0, workers: 3, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Planks, 6.0), (Bricks, 8.0)], labour: 80.0, sells: [Food, Clothes, Alcohol, Electronics], taps: None, residents: 0, storage: 30.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.15, dirt: 0.05, on: None, orders: false, serves: [],
            holds: [Covered, Liquid], beds: 0),
    def!(Clinic, "Polyclinic", 45.0, 30.0, workers: 6, draw: 2.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 14.0), (Steel, 4.0), (Planks, 6.0)], labour: 150.0, sells: [], taps: None, residents: 0, storage: 10.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.1, dirt: 0.1, on: None, orders: false, serves: [(Health, 0.55)],
            holds: [], beds: 0),
    def!(CultureClub, "Culture Club", 40.0, 30.0, workers: 4, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Planks, 8.0), (Bricks, 10.0)], labour: 100.0, sells: [], taps: None, residents: 0, storage: 10.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.08, dirt: 0.02, on: None, orders: false, serves: [(Culture, 0.5)],
            holds: [], beds: 0),
    // Where the next generation gets what Moscow sent the first one out with.
    // Its own staff need only to be schooled, which is what stops the chain
    // being circular: a republic can always open a school with the people it
    // was founded with, and everything else follows from that.
    def!(School, "Ten-Year School", 55.0, 35.0, workers: 10, draw: 2.0, out_mw: 0.0, heat: 1.5, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 16.0), (Planks, 10.0), (Steel, 4.0)], labour: 160.0, sells: [], taps: None, residents: 0, storage: 10.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: Some(Teaching::School),
            transforms: false, waste: 0.12, dirt: 0.02, on: None, orders: false, serves: [(Schooling, 0.7)],
            holds: [], beds: 0),
    // And what turns a schooled worker into somebody who can run a refinery.
    // The cost is not only the building: a student is a working-age adult who
    // is not working, so a republic putting people through this is three years
    // short of their hands to be better off afterwards.
    def!(University, "Polytechnic Institute", 90.0, 60.0, workers: 14, draw: 5.0, out_mw: 0.0, heat: 3.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 30.0), (Planks, 14.0), (Steel, 10.0), (Gravel, 12.0)], labour: 320.0, sells: [], taps: None, residents: 0, storage: 20.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: Some(Teaching::University),
            transforms: false, waste: 0.2, dirt: 0.05, on: None, orders: false, serves: [],
            holds: [], beds: 0),
    // What a consumer actually plugs into. High-voltage line to the station,
    // low-voltage station to the street — two hops, because a pylon strung past
    // a factory is not what runs it, and modelling it as one would leave this
    // building with nothing to do.
    def!(TransformerStation, "Transformer Station", 24.0, 20.0, workers: 3, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 8.0), (Steel, 10.0), (Gravel, 6.0)], labour: 90.0, sells: [], taps: None, residents: 0, storage: 10.0,
            wear: 0.005, farms: false,
            needs: Schooled, teaches: None,
            transforms: true, waste: 0.0, dirt: 0.05, on: None, orders: false, serves: [],
            holds: [Open], beds: 0),
    // Where the republic's rubbish goes. It *consumes* waste rather than merely
    // holding it, which is what makes it a consignee the freight ranking already
    // understands: a landfill that runs out of rubbish is a landfill with spare
    // capacity, and the dispatcher will go and fetch some.
    def!(Landfill, "Landfill", 120.0, 90.0, workers: 5, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Waste, 10.0)], out: [],
        cost: [(Gravel, 20.0), (Planks, 6.0)], labour: 100.0, sells: [], taps: None, residents: 0, storage: 400.0,
            wear: 0.01, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.0, dirt: 3.5, on: None, orders: false, serves: [],
            holds: [Aggregate, Open], beds: 0),
    // The other answer, and the trade is explicit: it burns twice what a
    // landfill buries and gives current back for it, at twice the filth.
    def!(Incinerator, "Refuse Incinerator", 80.0, 60.0, workers: 12, draw: 0.0, out_mw: 12.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Waste, 20.0)], out: [],
        cost: [(Bricks, 24.0), (Steel, 16.0), (Gravel, 14.0), (Machinery, 4.0)], labour: 300.0, sells: [], taps: None, residents: 0, storage: 120.0,
            wear: 0.02, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.0, dirt: 7.0, on: None, orders: false, serves: [],
            holds: [Aggregate, Open], beds: 0),
    // ---- The stores -------------------------------------------------------
    //
    // Five kinds, and what separates them is `holds` and nothing else. A
    // republic used to keep two hundred tonnes of *anything* in one shed, which
    // made storage a number rather than a decision; now siting a store is a
    // choice about what goes in it, and a republic that built the wrong one has
    // built a shed its coal will not go in.
    //
    // The capacities are the point of the specialised ones. A tank holds four
    // times a warehouse and takes liquids only; an aggregate bay holds six
    // times and takes heaps only. Generality is what you pay for.
    def!(Warehouse, "Warehouse", 60.0, 30.0, workers: 2, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Planks, 8.0), (Bricks, 10.0)], labour: 90.0, sells: [], taps: None, residents: 0, storage: 200.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.05, dirt: 0.05, on: None, orders: true, serves: [],
            holds: [Covered, Open], beds: 0),
    // The cheapest tonne of storage in the republic, because it is a fenced
    // rectangle of hardstanding. Everything that does not mind the rain.
    def!(OpenYard, "Open Storage Yard", 90.0, 70.0, workers: 2, draw: 0.5, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Gravel, 24.0), (Planks, 4.0)], labour: 60.0, sells: [], taps: None, residents: 0, storage: 400.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.04, dirt: 0.1, on: None, orders: true, serves: [],
            holds: [Open, Aggregate], beds: 0),
    def!(AggregateBay, "Aggregate Store", 100.0, 70.0, workers: 3, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Gravel, 30.0), (Bricks, 20.0), (Steel, 6.0)], labour: 110.0, sells: [], taps: None, residents: 0, storage: 1_200.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.05, dirt: 0.25, on: None, orders: true, serves: [],
            holds: [Aggregate], beds: 0),
    def!(StorageTank, "Storage Tank", 40.0, 40.0, workers: 3, draw: 1.5, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Steel, 40.0), (Gravel, 16.0), (Bricks, 8.0)], labour: 140.0, sells: [], taps: None, residents: 0, storage: 800.0,
            // No wear, and that is authored rather than skipped: a tank is a
            // welded shell with a pump on it. Charging it machinery would have
            // forced it to admit `Open` to receive the spares, and a tank that
            // takes crated goods is not a tank.
            wear: 0.0, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.03, dirt: 0.15, on: None, orders: true, serves: [],
            holds: [Liquid], beds: 0),
    def!(GrainSilo, "Grain Silo", 30.0, 30.0, workers: 3, draw: 1.5, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Steel, 26.0), (Concrete, 18.0), (Gravel, 10.0)], labour: 130.0, sells: [], taps: None, residents: 0, storage: 600.0,
            // Zero for the same reason a tank's is: spares are crated goods,
            // and a silo that took crates would be a warehouse.
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.03, dirt: 0.05, on: None, orders: true, serves: [],
            holds: [Bulk], beds: 0),
    // The municipal yard, and now the one building that keeps snow ploughs.
    // That is deliberate rather than a new depot: clearing the roads is a
    // council job, the yard already existed doing nothing but holding things,
    // and a republic should not have to build a second shed to own a plough.
    def!(Depot, "Council Depot", 80.0, 60.0, workers: 8, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0,
        keeps: [(SnowPlough, 2)],
        in: [(Fuel, 0.15)], out: [],
        cost: [(Bricks, 15.0), (Planks, 10.0)], labour: 120.0, sells: [], taps: None, residents: 0, storage: 300.0,
            wear: 0.015, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.05, dirt: 0.1, on: None, orders: true, serves: [],
            holds: [Open, Aggregate, Liquid, Covered], beds: 0),
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
            transforms: false, waste: 0.2, dirt: 0.3, on: None, orders: false, serves: [],
            holds: [Open, Liquid], beds: 0),
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
            transforms: false, waste: 0.3, dirt: 1.0, on: None, orders: false, serves: [],
            holds: [Liquid, Open], beds: 0),
    // **`orders` here fixes a mechanic that could only ever run down.** A pump
    // has no `inputs`, so `cover_days` returns `None` for it and the resupply
    // ranking has never had a reason to bring one a single tonne of diesel —
    // the founding hand-stocked it with forty tonnes and that was every litre a
    // republic would ever see. Once burnt, the building did nothing, for ever.
    //
    // A standing order is the right answer rather than a fake appetite: what a
    // filling station holds is a decision about how far out the republic wants
    // its lorries to reach, which is exactly the judgement `stores_to_order`
    // exists to hand the player. `a_filling_point_is_kept_filled` is the guard.
    def!(GasStation, "Gas Station", 30.0, 20.0, workers: 4, draw: 1.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 8.0), (Steel, 6.0), (Planks, 4.0)], labour: 90.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.005, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.05, dirt: 0.6, on: None, orders: true, serves: [],
            holds: [Liquid, Open], beds: 0),
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
            transforms: false, waste: 0.25, dirt: 1.0, on: Some(crate::journey::Medium::Road), orders: false, serves: [],
            holds: [Liquid, Open], beds: 0),
    // ---- The services -----------------------------------------------------
    //
    // Eleven buildings and not one line of new logic, which is the point of
    // `serves` being an authored field. Contentment asks what is within reach
    // that provides a need; it does not know how many kinds of building can.
    //
    // Cover is a **share and it adds up**, so no single building is complete
    // provision of anything. A republic with a clinic on every estate has more
    // than half its healthcare and knows it is short; one that builds the
    // hospital and the pharmacy as well has all of it. That is a deliberate
    // change to a figure that was an artefact -- the Polyclinic used to supply
    // *complete* health cover because it was the only health building there was.
    def!(Kindergarten, "Kindergarten", 35.0, 25.0, workers: 8, draw: 1.0, out_mw: 0.0, heat: 1.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 12.0), (Planks, 8.0)], labour: 110.0, sells: [], taps: None, residents: 0, storage: 6.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.1, dirt: 0.02, on: None, orders: false, serves: [(Schooling, 0.3)],
            holds: [], beds: 0),
    def!(Hospital, "District Hospital", 120.0, 70.0, workers: 40, draw: 8.0, out_mw: 0.0, heat: 4.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 70.0), (Steel, 24.0), (Planks, 20.0), (Gravel, 30.0), (Machinery, 8.0)], labour: 540.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.05, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 0.6, dirt: 0.2, on: None, orders: false, serves: [(Health, 0.45)],
            holds: [Open], beds: 0),
    def!(Pharmacy, "Pharmacy", 20.0, 15.0, workers: 3, draw: 0.5, out_mw: 0.0, heat: 0.3, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 6.0), (Planks, 4.0)], labour: 60.0, sells: [], taps: None, residents: 0, storage: 8.0,
            wear: 0.0, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 0.04, dirt: 0.01, on: None, orders: false, serves: [(Health, 0.2)],
            holds: [], beds: 0),
    // Safety is fire, police and the courts, and it is the one need that is
    // never waived for want of demand: the point of a fire station is the day
    // you need it, not the average day.
    def!(FireStation, "Fire Station", 50.0, 35.0, workers: 16, draw: 1.5, out_mw: 0.0, heat: 1.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Fuel, 0.1)], out: [],
        cost: [(Bricks, 20.0), (Steel, 10.0), (Planks, 8.0), (Gravel, 10.0)], labour: 180.0, sells: [], taps: None, residents: 0, storage: 20.0,
            wear: 0.02, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.1, dirt: 0.1, on: None, orders: false, serves: [(Safety, 0.4)],
            holds: [Liquid, Open], beds: 0),
    def!(PoliceStation, "Militia Station", 45.0, 30.0, workers: 14, draw: 1.5, out_mw: 0.0, heat: 1.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [(Fuel, 0.05)], out: [],
        cost: [(Bricks, 18.0), (Steel, 6.0), (Planks, 8.0)], labour: 160.0, sells: [], taps: None, residents: 0, storage: 16.0,
            wear: 0.01, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.1, dirt: 0.05, on: None, orders: false, serves: [(Safety, 0.35)],
            holds: [Liquid, Open], beds: 0),
    def!(Courthouse, "People's Court", 60.0, 40.0, workers: 12, draw: 2.0, out_mw: 0.0, heat: 1.4, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 34.0), (Planks, 14.0), (Steel, 6.0)], labour: 260.0, sells: [], taps: None, residents: 0, storage: 8.0,
            wear: 0.0, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 0.08, dirt: 0.01, on: None, orders: false, serves: [(Safety, 0.25)],
            holds: [], beds: 0),
    // A prison serves nobody's contentment and that zero is authored rather
    // than defaulted: it is somewhere the republic puts people, not something
    // it does for them, and pretending otherwise would be the game taking a
    // position it has no business taking.
    def!(Prison, "Corrective Labour Colony", 130.0, 90.0, workers: 24, draw: 3.0, out_mw: 0.0, heat: 3.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 60.0), (Steel, 20.0), (Gravel, 40.0)], labour: 420.0, sells: [], taps: None, residents: 0, storage: 20.0,
            wear: 0.01, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.5, dirt: 0.1, on: None, orders: false, serves: [],
            holds: [Open], beds: 0),
    def!(SportsHall, "Sports Hall", 70.0, 45.0, workers: 8, draw: 3.0, out_mw: 0.0, heat: 2.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 26.0), (Steel, 14.0), (Planks, 12.0)], labour: 220.0, sells: [], taps: None, residents: 0, storage: 8.0,
            wear: 0.01, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.1, dirt: 0.02, on: None, orders: false, serves: [(Culture, 0.25)],
            holds: [Open], beds: 0),
    def!(Cinema, "Cinema", 45.0, 35.0, workers: 6, draw: 2.5, out_mw: 0.0, heat: 1.2, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 20.0), (Planks, 10.0), (Steel, 4.0)], labour: 170.0, sells: [], taps: None, residents: 0, storage: 6.0,
            wear: 0.005, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.08, dirt: 0.02, on: None, orders: false, serves: [(Culture, 0.2)],
            holds: [Open], beds: 0),
    // The one service with no radius worth speaking of -- it reaches the whole
    // republic, which is what a transmitter is for. Modelled as ordinary cover
    // anyway rather than as a special case, because a republic that has not
    // wired its far estates has not really reached them either.
    def!(Broadcast, "State Radio Centre", 40.0, 30.0, workers: 10, draw: 6.0, out_mw: 0.0, heat: 1.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 16.0), (Steel, 22.0), (Machinery, 4.0)], labour: 200.0, sells: [], taps: None, residents: 0, storage: 6.0,
            wear: 0.02, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 0.05, dirt: 0.02, on: None, orders: false, serves: [(Culture, 0.3)],
            holds: [Open], beds: 0),
    def!(Cemetery, "Cemetery", 110.0, 80.0, workers: 3, draw: 0.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 8.0), (Gravel, 20.0)], labour: 90.0, sells: [], taps: None, residents: 0, storage: 4.0,
            wear: 0.0, farms: false,
            needs: Unschooled, teaches: None,
            transforms: false, waste: 0.0, dirt: 0.0, on: None, orders: false, serves: [(Health, 0.1)],
            holds: [], beds: 0),
    // ---- Tourism ----------------------------------------------------------
    //
    // A second way to earn hard currency, and a geographic one: visitors come
    // from a bloc, arrive at that bloc's frontier post, and have to be *driven*
    // in like any other people. What they pay for is what is within walking
    // distance of where they sleep — culture, shops, clean air — so a hotel in
    // a smog beside nothing earns a fraction of one in a town worth visiting.
    //
    // `beds` rather than `residents`, because a guest is not a citizen: they
    // are not counted in the census, they do not take a job, and the republic
    // is not marked down for failing to school them.
    def!(Hotel, "Intourist Hotel", 60.0, 40.0, workers: 18, draw: 6.0, out_mw: 0.0, heat: 3.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 40.0), (Steel, 16.0), (Planks, 18.0), (Gravel, 16.0)], labour: 380.0, sells: [], taps: None, residents: 0, storage: 30.0,
            wear: 0.01, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.3, dirt: 0.1, on: None, orders: false, serves: [],
            holds: [Covered, Open], beds: 80),
    // The passenger services. What separates them is the way they ride, and
    // that is the *only* thing separating them in code: `seats` says a building
    // runs a service and `on` says over what, so a fifth mode is a data row.
    //
    // Two real trades rather than a ladder of bigger numbers. A trolleybus
    // burns **no oil at all** -- it runs on the republic's own generation --
    // and what it costs is that the wire has to be strung and the buses go
    // where it goes. A tram and a metro carry vastly more for vastly more
    // capital, and the metro is the only one that passes under a river.
    def!(TrolleybusDepot, "Trolleybus Depot", 70.0, 50.0, workers: 14, draw: 6.0, out_mw: 0.0, heat: 0.4, heat_out: 0.0, seats: 520,
        keeps: [(Trolleybus, 4)],
        in: [], out: [],
        cost: [(Bricks, 18.0), (Planks, 10.0), (Steel, 12.0), (Gravel, 8.0)], labour: 170.0, sells: [], taps: None, residents: 0, storage: 40.0,
            wear: 0.02, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.2, dirt: 0.2, on: Some(crate::journey::Medium::Road), orders: false, serves: [],
            holds: [Open], beds: 0),
    def!(TramDepot, "Tram Depot", 100.0, 60.0, workers: 22, draw: 12.0, out_mw: 0.0, heat: 0.6, heat_out: 0.0, seats: 1100,
        keeps: [(Tram, 6)],
        in: [], out: [],
        cost: [(Bricks, 34.0), (Planks, 14.0), (Steel, 30.0), (Gravel, 20.0)], labour: 280.0, sells: [], taps: None, residents: 0, storage: 50.0,
            wear: 0.04, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.3, dirt: 0.3, on: Some(crate::journey::Medium::Tram), orders: false, serves: [],
            holds: [Open], beds: 0),
    def!(MetroDepot, "Metro Depot", 140.0, 80.0, workers: 34, draw: 30.0, out_mw: 0.0, heat: 0.8, heat_out: 0.0, seats: 3600,
        keeps: [(MetroTrain, 4)],
        in: [], out: [],
        cost: [(Bricks, 90.0), (Planks, 20.0), (Steel, 80.0), (Gravel, 70.0), (Machinery, 10.0)], labour: 620.0, sells: [], taps: None, residents: 0, storage: 60.0,
            wear: 0.08, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 0.4, dirt: 0.4, on: Some(crate::journey::Medium::Metro), orders: false, serves: [],
            holds: [Open], beds: 0),
    // The three terminals, and they are one idea three times over: a place a
    // confined vehicle can be reached, which is why `on` and `orders` are
    // authored fields rather than three special cases in the dispatcher.
    //
    // **The garage owns the vehicles here too.** A station is its own depot
    // rather than a loading point beside one, because a small republic has one
    // building at the end of its line and not two — and the rule that a garage
    // owns what it keeps already says how many drivers that costs.
    //
    // Storage is large and deliberately so: a terminal is where a hundred and
    // twenty tonnes lands at once, and one that could hold forty would spend
    // its life refusing trains.
    def!(RailwayStation, "Railway Station", 120.0, 40.0, workers: 18, draw: 3.0, out_mw: 0.0, heat: 0.6, heat_out: 0.0, seats: 700,
        keeps: [(Locomotive, 2), (PassengerTrain, 1)],
        in: [(Fuel, 0.6)], out: [],
        cost: [(Bricks, 40.0), (Steel, 30.0), (Planks, 16.0), (Gravel, 24.0)], labour: 320.0, sells: [], taps: None, residents: 0, storage: 400.0,
            wear: 0.03, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.3, dirt: 1.2, on: Some(crate::journey::Medium::Rail), orders: true, serves: [],
            holds: [Aggregate, Open, Covered, Liquid], beds: 0),
    // Cheaper than a station by a wide margin, because the expensive half of a
    // railway is the railway. A river port is the best value in the republic
    // and it is available only where the water already runs.
    def!(RiverPort, "River Port", 90.0, 50.0, workers: 14, draw: 2.0, out_mw: 0.0, heat: 0.4, heat_out: 0.0, seats: 0,
        keeps: [(Barge, 2)],
        in: [(Fuel, 0.4)], out: [],
        cost: [(Bricks, 24.0), (Steel, 18.0), (Planks, 20.0), (Gravel, 30.0)], labour: 240.0, sells: [], taps: None, residents: 0, storage: 500.0,
            wear: 0.03, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.25, dirt: 0.9, on: Some(crate::journey::Medium::Water), orders: true, serves: [],
            holds: [Aggregate, Open, Covered, Liquid], beds: 0),
    def!(Aerodrome, "Aerodrome", 200.0, 140.0, workers: 22, draw: 5.0, out_mw: 0.0, heat: 0.8, heat_out: 0.0, seats: 0,
        keeps: [(Freighter, 1)],
        in: [(Fuel, 1.6)], out: [],
        cost: [(Bricks, 30.0), (Steel, 40.0), (Planks, 12.0), (Gravel, 90.0)], labour: 420.0, sells: [], taps: None, residents: 0, storage: 120.0,
            wear: 0.05, farms: false,
            needs: Graduate, teaches: None,
            transforms: false, waste: 0.4, dirt: 2.4, on: Some(crate::journey::Medium::Air), orders: true, serves: [],
            // No heaps and no silo: nobody airlifts gravel, and an aerodrome
            // that could would make the cheapest way to move bulk the one with
            // wings. What flies is what is worth flying.
            holds: [Covered, Open, Liquid], beds: 0),
    // The same standing-order mechanism with no vehicles and no network: a
    // place the republic decides to keep things, so that what is wanted in the
    // north is not always fetched from the south. Cheap, and the answer to a
    // republic that has grown past one centre.
    def!(DistributionOffice, "Distribution Office", 60.0, 40.0, workers: 10, draw: 1.0, out_mw: 0.0, heat: 0.5, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 20.0), (Planks, 14.0), (Steel, 6.0)], labour: 160.0, sells: [], taps: None, residents: 0, storage: 300.0,
            wear: 0.01, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.15, dirt: 0.3, on: None, orders: true, serves: [],
            holds: [Open, Covered, Aggregate], beds: 0),
    // The one place that takes every form there is, and that is a decision
    // rather than laziness: a customs house is where imports land and exports
    // stage, so a form it refused would be a class of goods the republic could
    // neither buy nor sell. `every_form_can_cross_the_border` is the guard.
    def!(Customs, "Customs House", 90.0, 60.0, workers: 8, draw: 2.0, out_mw: 0.0, heat: 0.0, heat_out: 0.0, seats: 0, keeps: [],
        in: [], out: [],
        cost: [(Bricks, 20.0), (Steel, 6.0), (Planks, 8.0), (Gravel, 10.0)], labour: 200.0, sells: [], taps: None, residents: 0, storage: 200.0,
            wear: 0.0, farms: false,
            needs: Schooled, teaches: None,
            transforms: false, waste: 0.1, dirt: 0.15, on: None, orders: false, serves: [],
            holds: [Aggregate, Liquid, Bulk, Open, Covered], beds: 0),
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
    /// How much of the drink and household electrics these people wanted was on
    /// a shelf within reach, 0..=1. Set by the households system, never
    /// authored.
    ///
    /// Kept apart from `provisioned` because the two are worth different
    /// *kinds* of thing: falling short of food is a failure and falling short of
    /// televisions is only a missed opportunity. See
    /// [`crate::wellbeing::Contentment::comforts`].
    pub comforted: f64,
    /// How much of the alcohol these people wanted they got, 0..=1. Set by the
    /// households system, never authored.
    ///
    /// Carried on its own because drink is the one comfort with a price
    /// attached: it lifts contentment and costs health, and the second half
    /// needs to know how much of the first was vodka rather than radios.
    pub drink: f64,
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
    /// What the player has told this place to keep on hand, per resource.
    ///
    /// **Authored by the player, never by a system** — the one field on a
    /// building that is an instruction rather than a consequence, which is why
    /// it is set through a command and read by dispatch. Empty on everything
    /// but a terminal or a distribution office, and refused on anything whose
    /// `stores_to_order` is false: a standing order at a coal mine would be an
    /// instruction to fetch coal to the coal.
    pub orders: Stock,
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

    /// Whether this place will take delivery of a resource at all.
    ///
    /// **This is a rule about deliveries, not about contents.** A building's own
    /// output lands in its bin whatever shape it is — a coal mine does not
    /// refuse its own coal — and so does its rubbish, which is why a house with
    /// no `admits` at all still fills a bin the refuse lorry empties. What
    /// [`BuildingDef::admits`] governs is what somebody may *bring* here, which
    /// is the only half a player has a decision in.
    ///
    /// A SITE takes anything: a bill of materials is the building arriving in
    /// pieces, and a storage tank that could not accept the bricks it is made of
    /// could never be built.
    pub fn accepts(&self, resource: Resource) -> bool {
        !self.is_built() || self.def().admits.contains(&resource.form())
    }

    /// How much of a resource this building may hold.
    ///
    /// A SITE may hold at least its bill of materials for that resource, even
    /// when that exceeds the bin it will have once open. Otherwise a building
    /// whose construction needs more brick than it will ever store could never
    /// be built at all.
    pub fn intake_capacity(&self, resource: Resource) -> Tonnes {
        if self.is_built() {
            // A tank will not take coal, and it says so here rather than in the
            // dispatcher: every part of freight reads this one answer, so what
            // is ranked, what a lorry is told to load and what another lorry
            // already carrying counts against can never disagree.
            if !self.def().admits.contains(&resource.form()) {
                return Tonnes::ZERO;
            }
            // **A standing order is a ceiling, not a suggestion.** Applying it
            // here rather than in the ranking means every part of freight
            // respects it at once — what is ranked, what a lorry is told to
            // load, and what another lorry already on its way counts against.
            // A station asked for fifty tonnes and left to fill its four
            // hundred would be a republic hauling coal to a shed for a month.
            if self.def().stores_to_order {
                let ordered = self.orders.get(resource);
                return ordered.min(self.storage_cap());
            }
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
    /// A terminal nowhere near the way it serves.
    ///
    /// The same shape of rule as [`PlacementError::NotOnTheBorder`], and for
    /// the same reason: a station four hundred metres from the nearest rail is
    /// not a station that works badly, it is a building no train can reach.
    NoWayThere(crate::journey::Medium),
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
            PlacementError::NoWayThere(medium) => write!(
                f,
                "nothing could reach it: this must stand within {} m of {}",
                crate::journey::TERMINAL_REACH.0,
                match medium {
                    crate::journey::Medium::Rail => "a railway",
                    crate::journey::Medium::Tram => "a tramway",
                    crate::journey::Medium::Metro => "a metro line",
                    crate::journey::Medium::Water => "navigable water",
                    crate::journey::Medium::Air => "an aerodrome",
                    crate::journey::Medium::Road => "a road",
                }
            ),
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
            comforted: 0.0,
            drink: 0.0,
            content: crate::wellbeing::Contentment::NOTHING,
            work_done: 0.0,
            tapped: None,
            orders: Stock::EMPTY,
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
            comforted: 0.0,
            drink: 0.0,
            content: crate::wellbeing::Contentment::NOTHING,
            work_done: 0.0,
            tapped,
            orders: Stock::EMPTY,
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

    /// A building that will not accept what it runs on is a building that
    /// silently never runs — and it would look exactly like a freight problem.
    ///
    /// Inputs and what a shop sells are **delivered**, so they must be admitted.
    /// So must machinery on anything that wears, which is the half most easily
    /// missed: `wear` is its own field rather than an entry in `inputs`, so a
    /// depot that refused `Open` would run at [`crate::systems::WORN_EFFICIENCY`]
    /// for ever with a parts lorry turning round in the gate.
    ///
    /// Outputs are included because a bin a producer's own goods could not be
    /// *returned* to is a bin that reads wrong even where nothing exercises it.
    #[test]
    fn every_building_will_hold_what_it_handles() {
        for d in BUILDINGS {
            let mut wanted: Vec<Resource> = d
                .inputs
                .iter()
                .chain(d.outputs)
                .map(|&(r, _)| r)
                .chain(d.sells.iter().copied())
                .collect();
            if d.wear > 0.0 {
                wanted.push(Resource::Machinery);
            }
            for resource in wanted {
                assert!(
                    d.admits.contains(&resource.form()),
                    "{} handles {} ({:?}) and will not hold it",
                    d.name,
                    resource.name(),
                    resource.form()
                );
            }
        }
    }

    /// Every shape of goods has somewhere purpose-built to keep it.
    ///
    /// The counterpart of `every_need_can_be_met_by_something_the_republic_can_
    /// build`: a form with no store is a resource the player can make and then
    /// has nowhere to put, which is a permanent bottleneck wearing a storage
    /// mechanic's clothes. A *dedicated* store, not merely any building that
    /// happens to admit it — a steel mill accepting coal is not a coal store.
    #[test]
    fn every_form_can_be_kept_somewhere_purpose_built() {
        for form in Form::ALL {
            let store = BUILDINGS.iter().find(|d| {
                d.admits.contains(&form)
                    && d.stores_to_order
                    && d.inputs.is_empty()
                    && d.outputs.is_empty()
            });
            assert!(
                store.is_some(),
                "nothing in the republic is built to keep {:?}",
                form
            );
        }
    }

    /// Trade is physical and it goes through a building. A form the customs
    /// house would not take is a class of goods that can neither be imported
    /// nor exported, with nothing anywhere to say why.
    #[test]
    fn every_form_can_cross_the_border() {
        let customs = BuildingKind::Customs.def();
        for form in Form::ALL {
            assert!(
                customs.admits.contains(&form),
                "a customs house bars {form:?}"
            );
        }
    }

    /// The rule in one sentence, exercised rather than described: a tank holds
    /// liquids, and coal is not a liquid.
    #[test]
    fn a_tank_will_not_take_coal_and_a_site_will_take_anything() {
        let tank = Building {
            id: BuildingId(1),
            kind: BuildingKind::StorageTank,
            centre: Point::new(Metres(0.0), Metres(0.0)),
            stock: Stock::EMPTY,
            staff: 0,
            powered: false,
            heated: false,
            provisioned: 0.0,
            comforted: 0.0,
            drink: 0.0,
            content: crate::wellbeing::Contentment::NOTHING,
            work_done: BuildingKind::StorageTank.def().labour,
            tapped: None,
            orders: {
                // A standing order for both, so the refusal that follows is
                // about form and not about an empty order book.
                let mut s = Stock::EMPTY;
                s.set(Resource::Fuel, Tonnes(500.0));
                s.set(Resource::Coal, Tonnes(500.0));
                s
            },
        };
        assert!(tank.accepts(Resource::Fuel), "a tank holds fuel");
        assert!(!tank.accepts(Resource::Coal), "a tank is not a coal bunker");
        assert!(tank.intake_capacity(Resource::Fuel).is_positive());
        assert_eq!(tank.intake_capacity(Resource::Coal), Tonnes::ZERO);

        // The exemption, and it is what makes the tank buildable at all: its
        // own bill of materials is steel, gravel and brick, none of which is a
        // liquid.
        let site = Building {
            work_done: 0.0,
            ..tank
        };
        assert!(
            site.accepts(Resource::Bricks),
            "a site refusing its own bill could never be finished"
        );
        assert!(site.intake_capacity(Resource::Steel).is_positive());
    }

    /// Every resource is something the republic can actually come by, and
    /// something it has a use for. An orphan at either end is a row in a table
    /// pretending to be a mechanic — the shape this repository keeps finding.
    ///
    /// Waste is the one deliberate exception on the making side: nothing
    /// produces it on purpose, every building sheds it, and that is the point.
    #[test]
    fn every_resource_can_be_come_by_and_has_a_use() {
        for resource in Resource::ALL {
            if resource != Resource::Waste {
                let made = BUILDINGS
                    .iter()
                    .any(|d| d.outputs.iter().any(|&(r, q)| r == resource && q > 0.0));
                assert!(made, "nothing in the republic produces {}", resource.name());
            }
            let used = BUILDINGS.iter().any(|d| {
                d.inputs.iter().any(|&(r, _)| r == resource)
                    || d.materials.iter().any(|&(r, _)| r == resource)
                    || d.sells.contains(&resource)
            }) || crate::roadworks::GRADES
                .iter()
                .any(|g| g.materials.iter().any(|&(r, _)| r == resource))
                || resource == Resource::Machinery;
            assert!(
                used,
                "{} is produced and nothing anywhere wants it",
                resource.name()
            );
        }
    }

    /// A comfort nobody sells is a lift no republic can ever earn.
    ///
    /// The same shape as `every_need_can_be_met_by_something_the_republic_can_
    /// build`: a good that raises contentment and reaches no shelf would be a
    /// permanent twelve points the player is told about and cannot have.
    /// `is_comfort` is authored on the resource and `sells` on the building, so
    /// without this nothing connects the two.
    #[test]
    fn every_comfort_reaches_a_shelf() {
        let comforts: Vec<Resource> = Resource::ALL
            .into_iter()
            .filter(|r| r.is_comfort())
            .collect();
        assert!(!comforts.is_empty(), "the roster has no comforts in it");
        for resource in comforts {
            let sold = BUILDINGS.iter().any(|d| d.sells.contains(&resource));
            assert!(
                sold,
                "{} raises contentment and no building puts it on a shelf",
                resource.name()
            );
        }
    }

    /// Beds are a hotel's whole reason to exist, and a hotel nobody works in
    /// serves nobody. Authored zero everywhere else, which this also pins.
    #[test]
    fn only_a_staffed_place_offers_beds() {
        let with_beds: Vec<_> = BUILDINGS.iter().filter(|d| d.beds > 0).collect();
        assert!(!with_beds.is_empty(), "nowhere for a visitor to sleep");
        for d in with_beds {
            assert!(d.workers > 0, "{} has beds and nobody to make them", d.name);
            assert_eq!(
                d.residents, 0,
                "{} would house citizens and guests in the same building",
                d.name
            );
        }
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
