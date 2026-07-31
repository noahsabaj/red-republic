//! The reference player: what a competent republic-builder buys, in the order
//! they would buy it.
//!
//! **It lives here rather than inside the trajectory binary because two things
//! need it and a second copy would be a second answer.** The runner reads it to
//! show what an opening looks like; `tests/opening.rs` asserts against it so the
//! gate can tell a republic that works from one that does not. Before this move
//! the only thing that had ever played the opening was a binary nothing ran in
//! CI, and four defects that between them left every republic producing nothing
//! at all sat on `main` through 423 green tests and three green jobs.
//!
//! It touches the simulation only through [`World::issue`] and reads, which is
//! what makes it a player rather than a fixture. Nothing here may reach into
//! state directly; if it needs something it cannot ask for, that is a gap in the
//! player's surface and belongs there.
//!
//! It is deliberately dumb: one thing at a time, in a fixed order, paid for
//! however it can be. A cleverer director would be a better player and a worse
//! instrument, because what this is for is finding out whether the opening is
//! *possible* rather than how well it can be played. **A failure under it is
//! therefore not proof the game is unwinnable** — but a republic that cuts no
//! coal, moves no freight and feeds nobody for ten years under a sensible fixed
//! order is not a hard game, it is a broken one, and telling those two apart is
//! the whole reason this is reachable from a test.

use crate::building::BuildingKind;
use crate::command::Command;
use crate::resource::Resource;
use crate::roadworks::Grade;
use crate::scenario;
use crate::trade::{Market, TradeAction};
use crate::units::{Metres, Point, Tonnes};
use crate::world::World;

/// The autopilot: what a competent player would buy, in the order they would
/// buy it.
///
/// **It exists because condition 3 became unmeasurable the day the map went
/// empty.** "A republic survives a decade" is a question about a republic, and
/// a runner that stands the old founding hand up is measuring a fixture — so
/// every figure this tool printed described a town nobody is given. It has to
/// play the opening or it is not reading the game.
///
/// It is deliberately dumb: one thing at a time, in a fixed order, paid for
/// however it can be. A cleverer director would be a better player and a worse
/// instrument, because what this is for is finding out whether the opening is
/// *possible* rather than how well it can be played. If a fixed sensible order
/// cannot get a republic off the ground, no player order will.
pub struct Director {
    centre: Point,
    /// How far through [`Director::PLAN`] it has got.
    step: usize,
    /// Whether builders have been hired from abroad yet.
    hired: bool,
    /// Whether the track to the crossing has been ordered.
    road: bool,
    /// Whether coal has been put on sale.
    selling: bool,
    /// Whether the standing import rules have been set.
    buying: bool,
    /// How many months running the current step has been refused.
    stuck: u32,
    /// What it has said out loud, so a decade does not print the same line
    /// three hundred times.
    said: Vec<String>,
}

impl Director {
    /// The order a republic gets built in, and every entry is load-bearing.
    ///
    /// A **Construction Office** first, because everything after it is cheaper
    /// once the republic has crews of its own — contractors cost several times
    /// what your own builders do. A **Motor Depot** second, because a republic
    /// without lorries starves beside its own full bins. Then somewhere to
    /// live, power, and a **Customs House**, which is the only way a rouble
    /// ever comes back in.
    const PLAN: &'static [BuildingKind] = &[
        BuildingKind::ConstructionOffice,
        BuildingKind::MotorDepot,
        BuildingKind::Apartment,
        BuildingKind::CoalMine,
        BuildingKind::PowerPlant,
        BuildingKind::TransformerStation,
        BuildingKind::Customs,
        // The materials chain, and it comes early for a reason the first run
        // found the hard way: until the republic can make gravel, brick and
        // planks, a site it builds *itself* has nothing to be built out of, and
        // every one of them stalls. See `MATERIALS` below.
        BuildingKind::GravelQuarry,
        BuildingKind::Woodcutter,
        BuildingKind::Sawmill,
        BuildingKind::Brickworks,
        BuildingKind::Store,
        BuildingKind::Farm,
        BuildingKind::FoodFactory,
        BuildingKind::HeatingPlant,
        BuildingKind::BusDepot,
        BuildingKind::Apartment,
        BuildingKind::Clinic,
        BuildingKind::School,
    ];

    /// What the republic has to be able to make before its own crews are worth
    /// using.
    ///
    /// **The first run switched to own crews the moment the office had staff,
    /// and everything after that stalled.** Contractors bring their own
    /// materials; your crews do not, so a republic that owns no quarry, no
    /// sawmill and no brickworks puts down foundations nothing will ever
    /// deliver to. Three buildings in three years, against eleven when it kept
    /// paying. A player would have seen it in a week.
    /// What the republic buys in until it can make it, and how much it keeps at
    /// the crossing.
    ///
    /// A stock level rather than an order: `Buy { up_to }` tops the customs
    /// house back up to this figure as lorries take it away, which is what a
    /// standing import policy is. The figures are a few weeks of consumption at
    /// the rates the building table authors — enough that a lorry breaking down
    /// does not stop the plant, small enough that the grant is not spent on a
    /// mountain of coal in month one.
    const IMPORTS: &'static [(Resource, f64)] = &[
        (Resource::Food, 120.0),
        (Resource::Coal, 200.0),
        (Resource::Fuel, 60.0),
        (Resource::Machinery, 20.0),
        // Six tonnes a kilometre, and without it the grid is an order that
        // never becomes a wire: the first span was accepted, sat as a site for
        // ten years waiting for steel that was never bought, and every building
        // in the republic read `no power` beside a fully staffed power station.
        (Resource::Steel, 80.0),
        // A heat main is eighteen tonnes of steel and twelve of brick to the
        // kilometre, and a republic whose brickworks has no gravel and no crew
        // makes none of the second. Without this the mains are ordered and never
        // laid, which is the same shape as the grid before steel was bought.
        (Resource::Bricks, 60.0),
    ];

    const MATERIALS: &'static [BuildingKind] = &[
        BuildingKind::GravelQuarry,
        BuildingKind::Sawmill,
        BuildingKind::Brickworks,
    ];

    pub fn new(centre: Point) -> Self {
        Self {
            centre,
            step: 0,
            hired: false,
            road: false,
            selling: false,
            buying: false,
            stuck: 0,
            said: Vec::new(),
        }
    }

    fn say(&mut self, line: String) {
        if !self.said.contains(&line) {
            println!("  {line}");
            self.said.push(line);
        }
    }

    /// One month of decisions.
    pub fn month(&mut self, world: &mut World) {
        self.build_next(world);
        self.hire(world);
        self.trade(world);
        self.grid(world);
    }

    /// String the power line, and then keep stringing it.
    ///
    /// **A plant that is not wired to anything lights nothing, including
    /// itself**, and this runner never laid a single span — so a republic that
    /// had bought a power station, a transformer and a coal mine sat dark for
    /// ten years, and the mine never cut a tonne because a mine draws six
    /// megawatts. Every building read `no power` while a fully staffed plant
    /// stood beside them.
    ///
    /// One span a month, and the town centre is the hub. That is not the
    /// cheapest grid a player could draw, but a star from the middle of the town
    /// is what somebody actually builds, and this is meant to be a plain player
    /// rather than a good one. A span is a *site* like any other — it wants
    /// steel and builder-days — so ordering one at a time is also what keeps
    /// the queue honest.
    fn grid(&mut self, world: &mut World) {
        use crate::utility::Utility;

        // One span under construction at a time, across both networks: a
        // republic with one office and one gang cannot work two.
        if !world.lineworks().is_empty() {
            return;
        }
        // Current before heat. A dark republic produces nothing at all where a
        // cold one merely suffers, and the boiler house that carries the heat
        // draws off the grid itself.
        if self.string(world, Utility::Power) {
            return;
        }
        self.string(world, Utility::Heat);
    }

    /// Order one span of one network, and say whether anything was ordered.
    ///
    /// Source to the hub first, then the hub outward to whichever consumer is
    /// nearest and not yet joined. Written once for both networks because a
    /// heat main is the same problem as a power line in every respect that
    /// matters here — and because the first version of this did power only,
    /// which left every home in the republic reading `cold` through ten winters
    /// beside a fully staffed boiler house.
    fn string(&mut self, world: &mut World, kind: crate::utility::Utility) -> bool {
        use crate::Building;
        use crate::utility::Utility;

        let makes: fn(&Building) -> bool = match kind {
            Utility::Heat => |b| b.def().heat_output > 0.0,
            _ => |b| b.def().power_output > 0.0,
        };
        let wants: fn(&Building) -> bool = match kind {
            Utility::Heat => |b| b.def().heat > 0.0,
            _ => |b| b.def().power_draw > 0.0,
        };

        // Nothing to carry until something makes it.
        let Some(source) = world
            .buildings()
            .all()
            .iter()
            .find(|b| b.is_built() && makes(b))
            .map(|b| (b.id, b.centre))
        else {
            return false;
        };
        // The source to the hub first: until that span exists there is no
        // network for anything else to join.
        if world.utilities().network_of(source.0, kind).is_none() {
            match world.issue(Command::OrderLine {
                kind,
                from: source.1,
                to: self.centre,
            }) {
                Ok(_) => self.say(format!("laying the {} in from the works", kind.def().name)),
                Err(why) => self.say(format!("no {} from the works: {why}", kind.def().name)),
            }
            return true;
        }
        // Then outward, nearest first, so it grows out of the town rather than
        // leaping across the map.
        let mut waiting: Vec<_> = world
            .buildings()
            .all()
            .iter()
            .filter(|b| b.is_built() && wants(b))
            .filter(|b| world.utilities().network_of(b.id, kind).is_none())
            .map(|b| (b.centre, b.def().name))
            .collect();
        waiting.sort_by(|a, b| {
            self.centre
                .distance_to(a.0)
                .0
                .total_cmp(&self.centre.distance_to(b.0).0)
        });
        let Some(&(at, name)) = waiting.first() else {
            return false;
        };
        match world.issue(Command::OrderLine {
            kind,
            from: self.centre,
            to: at,
        }) {
            Ok(_) => self.say(format!("running the {} out to the {name}", kind.def().name)),
            Err(why) => self.say(format!("no {} to the {name}: {why}", kind.def().name)),
        }
        true
    }

    /// Put the next thing on the plan up, if there is nothing already going.
    ///
    /// **One site at a time**, which is what a republic with one office and one
    /// gang can actually work — queuing five would only mean four of them
    /// standing empty, and would hide whether the first one ever finished.
    fn build_next(&mut self, world: &mut World) {
        if self.step >= Self::PLAN.len() {
            return;
        }
        // **Two on the go, not one.** The first version waited for every site to
        // finish, so a single site that could never finish — one the republic
        // ordered itself with no materials to build it from — stopped the plan
        // dead for the rest of the run and every column after it printed a flat
        // line. Two is still a queue a single office can work, and it means a
        // stuck foundation costs the republic momentum rather than everything.
        let going = world
            .buildings()
            .all()
            .iter()
            .filter(|b| !b.is_built())
            .count();
        if going >= 2 {
            return;
        }
        let kind = Self::PLAN[self.step];
        // **A customs house goes to the border, not to the town.** It is the
        // one building in the table that has to stand somewhere the republic
        // did not choose, and looking for a site near the centre found ground
        // that would take it and a command that refused it — which stalled the
        // whole plan on step seven for two and a half simulated years, with the
        // director printing one line about it and then nothing at all.
        // **And it goes to an EASTERN post, because the grant is roubles.**
        // This asked for the nearest crossing of any bloc, and on most seeds
        // that is a Western one — where a purse holding 2,500,000 roubles and
        // no dollars can buy precisely nothing. Trade then failed silently in
        // both directions for a decade: `affordable` came out zero on every
        // tick, no goods ever crossed, and the republic sat inert with a
        // customs house it could not use. `scenario::GRANT_ROUBLES` says which
        // bloc's posts the land reaches is the first thing that matters about a
        // posting, and until now the one thing that plays the opening ignored
        // it. Falling back to any post keeps a republic with no Eastern
        // crossing playable, and says so rather than pretending.
        let around = if kind == BuildingKind::Customs {
            let east = world
                .frontier()
                .nearest_crossing(self.centre, Some(Market::East));
            if east.is_none() {
                self.say("no Eastern post in reach — this posting must earn dollars first".into());
            }
            match east.or_else(|| world.frontier().nearest_crossing(self.centre, None)) {
                Some(post) => post.at,
                None => {
                    self.say("this republic has no frontier post at all".into());
                    self.step += 1;
                    return;
                }
            }
        } else {
            self.centre
        };
        let Some(at) = scenario::find_site(world, kind, around, Metres(1_400.0)) else {
            self.say(format!("nowhere within reach takes a {}", kind.def().name));
            self.step += 1;
            return;
        };
        // Build it with your own crews if there is an office with people in it,
        // and pay a Bloc firm otherwise. That is the whole shape of the opening
        // and the reason contractors exist at all.
        let own = world
            .buildings()
            .all()
            .iter()
            .any(|b| b.kind == BuildingKind::ConstructionOffice && b.is_built() && b.staff > 0)
            && Self::MATERIALS.iter().all(|&want| {
                world
                    .buildings()
                    .all()
                    .iter()
                    .any(|b| b.kind == want && b.is_built() && b.staff > 0)
            });
        let outcome = if own {
            world.issue(Command::Place { kind, at })
        } else {
            world.issue(Command::ContractBuild {
                kind,
                at,
                market: Market::East,
            })
        };
        match outcome {
            Ok(_) => {
                self.say(format!(
                    "{} a {}",
                    if own { "building" } else { "contracting" },
                    kind.def().name
                ));
                self.step += 1;
                self.stuck = 0;
            }
            Err(why) => {
                // **Give up on it rather than trying for ever.** The first
                // version stayed on a refused step, so one building the
                // director could not site stopped the plan dead and every
                // column after it printed a flat line that read as balance.
                // A player would move on; so does this.
                self.say(format!("cannot start a {}: {why}", kind.def().name));
                self.stuck += 1;
                if self.stuck >= 3 {
                    self.stuck = 0;
                    self.step += 1;
                }
            }
        }
    }

    /// Buy builders from the Eastern Bloc once there is an office to put them
    /// in. A blank map has nobody at all, so the first crews are always bought.
    fn hire(&mut self, world: &mut World) {
        if self.hired {
            return;
        }
        let Some(office) = world
            .buildings()
            .all()
            .iter()
            .find(|b| b.kind == BuildingKind::ConstructionOffice && b.is_built())
            .map(|b| b.id)
        else {
            return;
        };
        match world.issue(Command::HireForeign {
            market: Market::East,
            office,
            heads: 20,
        }) {
            Ok(_) => {
                self.hired = true;
                self.say("hired twenty builders from the Eastern Bloc".into());
            }
            Err(why) => self.say(format!("cannot hire abroad: {why}")),
        }
    }

    /// Sell coal, and lay a track to the post that clears it.
    ///
    /// **Not a fixed market.** A customs house clears only for the bloc whose
    /// post it stands at, and which post is nearest is decided by the land.
    /// Selling east from a Western crossing earns nothing at all — which is
    /// what this runner reported, in its first run after trade became
    /// geographic, as a flat zero in the roubles column for two years.
    fn trade(&mut self, world: &mut World) {
        let Some(house) = world
            .buildings()
            .all()
            .iter()
            .find(|b| b.kind == BuildingKind::Customs && b.is_built())
            .map(|b| b.centre)
        else {
            return;
        };
        let bloc = world.bloc_near(house);
        // **The opening is an import problem, and this runner never once
        // treated it as one.** A coal mine draws six megawatts, a power plant
        // burns coal to make them, and a republic founded on empty ground has
        // neither — so the circle cannot be broken from the inside and the
        // first tonne has to come over the border. Fuel and machinery are the
        // same shape: the construction office and both depots consume them from
        // the day they open, and nothing in the republic makes either for
        // years. Without these three rules the republic is inert for a decade,
        // which is exactly what it was.
        if !self.buying {
            let mut all = true;
            for &(resource, up_to) in Self::IMPORTS {
                if world
                    .issue(Command::AddTradeRule {
                        resource,
                        market: bloc,
                        action: TradeAction::Buy {
                            up_to: Tonnes(up_to),
                        },
                    })
                    .is_err()
                {
                    all = false;
                }
            }
            if all {
                self.buying = true;
                self.say(format!("buying coal, fuel and machinery from the {bloc:?}"));
            }
        }
        // Coal goes on sale only once the republic actually digs it. Selling it
        // from the day the house opens is what the old plan did, and it is
        // worse than useless now that coal is also bought: the sell rule and
        // the buy rule meet at the same customs house, so the republic would
        // buy a tonne over the border and sell it straight back.
        if !self.selling
            && world
                .buildings()
                .all()
                .iter()
                .any(|b| b.kind == BuildingKind::CoalMine && b.is_built() && b.staff > 0)
            && world
                .issue(Command::AddTradeRule {
                    resource: Resource::Coal,
                    market: bloc,
                    action: TradeAction::Sell,
                })
                .is_ok()
        {
            self.selling = true;
            self.say(format!("selling coal to the {bloc:?}ern bloc"));
        }
        if !self.road {
            match world.issue(Command::OrderRoad {
                from: self.centre,
                to: house,
                grade: Grade::Dirt,
                lamps: false,
            }) {
                Ok(_) => {
                    self.road = true;
                    self.say(format!(
                        "ordered a dirt track to the crossing: {:.1} km",
                        self.centre.distance_to(house).as_km()
                    ));
                }
                Err(why) => self.say(format!("no track to the crossing: {why}")),
            }
        }
    }
}
