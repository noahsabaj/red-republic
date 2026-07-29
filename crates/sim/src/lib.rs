//! The Red Republic simulation.
//!
//! Headless and engine-independent by construction: this crate has no
//! renderer, no windowing, and no engine dependency, and it must stay that way.
//! That independence is the whole point — it is what lets the shell be chosen
//! on measured evidence once the simulation exists, instead of guessed at now.
//!
//! # Determinism
//!
//! Same seed and same inputs must produce the same world, and a save must
//! reload bit-exact. The archived 1.x build proved this discipline is holdable
//! and that a round-trip test is what catches every missed field; this crate
//! inherits both the requirement and the tripwire.
//!
//! Two different bars, deliberately:
//!
//! - **Map generation** must reproduce across machines, because a shared seed
//!   is a promise between players.
//! - **The running simulation** need only reproduce for the same binary, which
//!   is the easy case: Rust does not reassociate floats, so one compiler and
//!   one instruction set is enough.

#![forbid(unsafe_code)]

pub mod building;
pub mod citizen;
pub mod climate;
pub mod command;
pub mod contract;
pub mod crews;
pub mod fleet;
pub mod founding;
pub mod geology;
pub mod ground;
pub mod journey;
pub mod loan;
pub mod mapgen;
pub mod migration;
pub mod network;
pub mod policy;
pub mod resource;
pub mod rng;
pub mod roadworks;
pub mod scenario;
pub mod systems;
pub mod terrain;
pub mod time;
pub mod tourism;
pub mod trade;
pub mod transport;
pub mod units;
pub mod utility;
pub mod wellbeing;
pub mod world;

pub use building::{Building, BuildingId, BuildingKind, Buildings, PlacementError, Teaching};
pub use citizen::{
    CitizenId, CitizenRecord, Education, HomeCensus, Learning, LifeStage, Population, Wellbeing,
};
pub use climate::{Climate, ClimateId};
pub use command::{Command, Done, Issued, Journal, Outcome, Refused};
pub use contract::{Contract, ContractId, ContractState, Contracts};
pub use crews::{Crews, Party, PartyId};
pub use fleet::{Destination, Fleet, Job, Role, Vehicle, VehicleId, VehicleKind, VehicleState};
pub use founding::{Candidate, Shelf, ShelfFilter};
pub use geology::{Deposit, DepositId, Geology, Layer, Mineral, SurveyReading};
pub use ground::Ground;
pub use journey::Journey;
pub use loan::{Loan, LoanError, Loans, Tier};
pub use migration::{Group, GroupId, Migration};
pub use network::{Network, NodeId, Route};
pub use policy::BuildPolicy;
pub use resource::{Resource, Stock};
pub use rng::{Rng, RngState};
pub use roadworks::{Grade, RoadError, RoadSite, RoadSiteId, RoadWorks};
pub use systems::{Mutation, Stall};
pub use terrain::{Surface, Terrain};
pub use time::{Date, Season, SimClock};
pub use tourism::{Tourism, Visit, VisitId};
pub use trade::{
    BorderCrossing, CrossingId, Frontier, FrontierArc, Market, TradeAction, TradePolicy, TradeRule,
    Treasury,
};
pub use transport::{Commute, Mode};
pub use units::{Metres, Point, Seconds, Speed, Tonnes};
pub use utility::{Line, LineId, LineSite, LineSiteId, LineWorks, Networks, Utility};
pub use wellbeing::Contentment;
pub use world::{SAVE_VERSION, Save, SaveError, World, WorldSpec};

#[cfg(test)]
mod tests {
    /// Guards the float half of the determinism claim above.
    ///
    /// IEEE-754 addition is not associative, and Rust must not pretend it is.
    /// If anything ever enables fast-math-style reassociation — a compiler
    /// flag, a codegen option, a `_fast` intrinsic — these two expressions
    /// collapse to the same value and this test fails. That is the point: the
    /// invariant is stated in the module docs, and this is what stops the
    /// statement from quietly becoming untrue.
    ///
    /// 1e16 has a ulp of 2, so `1e16 + 1.0` rounds back to `1e16`, and the
    /// order the three terms are grouped in decides whether the 1.0 survives.
    #[test]
    fn float_addition_is_not_reassociated() {
        let (a, b, c) = (1e16_f64, -1e16_f64, 1.0_f64);
        assert_eq!((a + b) + c, 1.0, "left grouping must keep the 1.0");
        assert_eq!(a + (b + c), 0.0, "right grouping must round the 1.0 away");
    }
}
