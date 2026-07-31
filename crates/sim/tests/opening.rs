//! Can a republic be founded on empty ground and still be alive years later?
//!
//! **Nothing in this repository asked that until now, and the cost of not
//! asking was four defects on `main` that between them left every republic
//! producing nothing at all.** Not one figure moved in a decade: coal in the
//! ground bit-identical to the founding number, `fed%` at zero for all hundred
//! and twenty months, zero tonnes hauled, zero of eleven lorries ever
//! dispatched. Through all of it there were 423 green lib tests, 12 green
//! baselines and three green CI jobs, and `--check` reported `build check ok`.
//!
//! Every fixture in the suite hands the simulation a town that already has
//! citizens in it and buildings standing, so every one of them tests a republic
//! nobody is ever given. The only thing that played the *opening* was a binary
//! nothing ran in CI, and it asserted nothing by design.
//!
//! # What this asserts, and what it deliberately does not
//!
//! It does not assert a balance. It does not check a population, a contentment
//! figure or a rate. Those are all things a tuning pass should be free to move,
//! and a test that pinned them would fail honestly every time somebody did
//! their job.
//!
//! What it asserts is that the republic is **not inert**: that some coal came
//! out of the ground, that freight moved, that somebody was fed, and that the
//! lights came on somewhere. Each of those was exactly zero on `main` this
//! morning, and each is zero only if something is broken rather than merely
//! badly balanced. They are floors a republic clears by functioning at all.
//!
//! # It plays through the player's door
//!
//! [`Director`] issues commands and reads views; it never reaches into state.
//! So a failure here is a failure of the game as somebody would actually meet
//! it, and never an artefact of a fixture reaching past the surface. That is
//! also why the `fixtures` feature is not used and must not be: a test that
//! stood the buildings up itself would be testing the thing the last four bugs
//! hid behind.
//!
//! # What it was watched to catch, and what it was not
//!
//! Each of the four defects fixed on 2026-07-30 was re-introduced against this
//! file one at a time. The results corrected something I had asserted:
//!
//! - **Customs houses needing a roster**: all four tests fail. This was the
//!   single binding constraint. An unstaffed crossing has `activity() == 0`, so
//!   nothing crossed the border in either direction and a republic on empty
//!   ground had no first tonne of anything.
//! - **Line sites getting no freight**: the eight-year test fails — the grid is
//!   never built, so nothing is ever lit and the mine never cuts.
//! - **The labour pass sorting on id alone**: *all four still pass.* The commit
//!   that fixed it called it "the one that had been fatal", and that was wrong.
//!   Once a crossing needs no roster the republic bootstraps on imports, the
//!   population reaches ninety-odd, and greedy fill stops mattering — the office
//!   takes its sixteen and there are still eighty people for everything else.
//!   It is a real defect and it wrecks the *early* republic, which is why it has
//!   unit guards of its own in `citizen.rs`; it is not what made the republic
//!   inert.
//!
//! That last line is the honest shape of this file: it is a floor, not a
//! microscope. It catches a republic that does not function. It does not catch a
//! republic that functions badly, and nothing here should be read as claiming
//! otherwise.
//!
//! # A failure here is not proof the game is unwinnable
//!
//! The director is a deliberately plain player — one thing at a time, in a
//! fixed order. A cleverer one would be a better player and a worse instrument.
//! But a republic that cuts no coal, hauls no freight and feeds nobody under a
//! sensible fixed order is not a hard game, it is a broken one, and telling
//! those two apart is the whole reason this exists.

use red_republic_sim::climate::ClimateId;
use red_republic_sim::command::Command;
use red_republic_sim::director::Director;
use red_republic_sim::geology::Mineral;
use red_republic_sim::resource::Resource;
use red_republic_sim::scenario;
use red_republic_sim::time::TICKS_PER_DAY;
use red_republic_sim::units::{Metres, Tonnes};
use red_republic_sim::world::{World, WorldSpec};

/// What a republic managed to do with the years it was given.
struct Life {
    /// Tonnes of coal taken out of the ground, ever.
    mined: Tonnes,
    /// Tonnes the fleet actually put down, ever.
    hauled: Tonnes,
    /// The best any occupied estate was ever provisioned, `0.0..=1.0`.
    best_fed: f64,
    /// The most buildings that were ever lit at once.
    most_lit: usize,
    /// The most people who ever lived here at once.
    peak: usize,
}

/// Found a republic, let the reference player run it, and report what it did.
///
/// Peaks and totals rather than a final reading, and that is not a detail: a
/// farm answers to the season, so sampling a republic in the March its run
/// happens to end in reports a stopped farm as a broken one.
fn live(seed: u64, climate: ClimateId, years: u32) -> Life {
    let mut world = World::new(WorldSpec {
        seed,
        extent: Metres(6_000.0),
        climate,
    });
    let centre = scenario::found(&mut world);
    let coal_at_founding = world.geology().remaining_of(Mineral::Coal);
    let mut director = Director::new(centre);

    let mut life = Life {
        mined: Tonnes::ZERO,
        hauled: Tonnes::ZERO,
        best_fed: 0.0,
        most_lit: 0,
        peak: 0,
    };

    for _ in 0..years * 12 {
        for _ in 0..TICKS_PER_DAY * 30 {
            for m in world.tick() {
                if let red_republic_sim::systems::Mutation::Unload { tonnes, .. } = m {
                    life.hauled += tonnes;
                }
            }
        }
        director.month(&mut world);
        // The runner accepts every tender offered, so this does too — an
        // obligation nobody accepts is a mechanism nobody exercises.
        let offers: Vec<_> = world.contracts().offers().map(|c| c.id).collect();
        for id in offers {
            let _ = world.issue(Command::AcceptContract { contract: id });
        }

        life.peak = life.peak.max(world.population().count());
        life.most_lit = life.most_lit.max(
            world
                .buildings()
                .all()
                .iter()
                .filter(|b| b.def().power_draw > 0.0 && b.powered)
                .count(),
        );
        for b in world.buildings().all() {
            if b.is_built()
                && b.def().residents > 0
                && !world.population().residents_of(b.id).is_empty()
            {
                life.best_fed = life.best_fed.max(b.provisioned);
            }
        }
    }

    life.mined = coal_at_founding.saturating_sub(world.geology().remaining_of(Mineral::Coal));
    life
}

/// The floor: a republic founded on empty ground does something in five years.
///
/// Five rather than ten because this runs in the ordinary gate and the failure
/// it exists to catch is total — an inert republic is inert from the first month
/// and no amount of extra years rescues it.
///
/// Watched to fail. Every one of these assertions reports zero against `main` as
/// it stood on 2026-07-30 before the labour fix, and the last two also fail with
/// only the labour fix in and the line-freight bug still present.
#[test]
fn a_founded_republic_is_not_inert() {
    let life = live(1, ClimateId::Plains, 5);

    assert!(
        life.hauled.is_positive(),
        "five years and the fleet never put a load down anywhere"
    );
    assert!(
        life.best_fed > 0.0,
        "five years and nobody in the republic was ever fed"
    );
    assert!(
        life.peak > 0,
        "five years and nobody ever came to live here"
    );
}

/// And eventually it lights itself and works its own seam, rather than living
/// on imports for ever.
///
/// **Eight years, and that number is measured rather than chosen.** On seed 1
/// the grid comes on in July 1966 — six years and four months after the
/// founding — and the first tonne of coal follows it the same month, because a
/// coal mine draws six megawatts and cannot cut anything until the wire
/// arrives. Both numbers are the reference player's pace rather than the game's
/// floor: it lays *one span a month* and waits for each to be built, so the
/// grid crawls out to a pit it sank in year one. A real player would string
/// several at once.
///
/// That slowness is worth arguing with — six years on imported coal is a long
/// opening — but it is a balance question, and this is not the place to answer
/// it. Eight years leaves room for the pace to drift before this fails; if it
/// drifts a lot, the number here is what to dispute rather than the assertion.
///
/// Kept apart from the inertness floor above because it is a different claim.
/// That one says the republic is running at all; this one says it can stop
/// living on imports, which is the whole arc of the opening. Splitting them also
/// keeps the fast floor fast.
#[test]
fn a_republic_eventually_lights_itself_and_works_its_own_seam() {
    let life = live(1, ClimateId::Plains, 8);
    assert!(
        life.most_lit > 0,
        "eight years and the republic never lit a single building"
    );
    assert!(
        life.mined.is_positive(),
        "eight years and not one tonne of coal came out of the ground — \
         the republic never got power to its own mine"
    );
}

/// And on every climate, because a winter is not supposed to be fatal.
///
/// One seed each rather than a sweep: this is a floor, not a balance pass, and
/// four foundings is already the slowest test in the repository.
#[test]
fn no_climate_is_fatal_on_its_own() {
    for climate in [
        ClimateId::Plains,
        ClimateId::Taiga,
        ClimateId::Steppe,
        ClimateId::Maritime,
    ] {
        let life = live(7, climate, 5);
        assert!(
            life.hauled.is_positive(),
            "a republic on the {} never hauled anything in five years",
            climate.def().name
        );
        assert!(
            life.best_fed > 0.0,
            "a republic on the {} never fed anybody in five years",
            climate.def().name
        );
    }
}

/// A republic that is fed is fed from somewhere, and the somewhere has to be
/// reachable.
///
/// The narrower half of the first test, kept apart because it is the one that
/// catches the specific failure that hid the longest: a customs house is where
/// everything a young republic eats comes from, and an unstaffed one has
/// `activity() == 0`, which switched off trade in **both** directions in
/// silence. Nothing about a republic slowly starving says "the border is shut".
#[test]
fn a_young_republic_can_actually_use_its_border() {
    let mut world = World::new(WorldSpec {
        seed: 1,
        extent: Metres(6_000.0),
        climate: ClimateId::Plains,
    });
    let centre = scenario::found(&mut world);
    let mut director = Director::new(centre);

    let mut ever_held = Tonnes::ZERO;
    for _ in 0..48 {
        for _ in 0..TICKS_PER_DAY * 30 {
            world.tick();
        }
        director.month(&mut world);
        let offers: Vec<_> = world.contracts().offers().map(|c| c.id).collect();
        for id in offers {
            let _ = world.issue(Command::AcceptContract { contract: id });
        }
        // Anything at all that the republic cannot yet make itself and so can
        // only have got by importing it.
        let imported: Tonnes = world
            .buildings()
            .all()
            .iter()
            .map(|b| b.stock.get(Resource::Fuel) + b.stock.get(Resource::Machinery))
            .sum();
        if imported.0 > ever_held.0 {
            ever_held = imported;
        }
    }

    assert!(
        ever_held.is_positive(),
        "four years in and the republic has never held a tonne of fuel or machinery, \
         neither of which it can make — so nothing ever crossed the border"
    );
}
