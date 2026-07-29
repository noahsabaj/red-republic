//! Build policy: where a site's materials come from when the republic cannot
//! make them.
//!
//! # No instant build, ever
//!
//! This is the one rule the whole module exists to serve, and it is the one 1.x
//! rule deliberately not carried forward. Paying hard currency to make a
//! construction site vanish is the *"click a button and it goes away"* shape
//! this build refuses. Nothing here shortens a build, waives a bill or skips a
//! journey.
//!
//! What auto-import does is answer a different question: **where does a tonne of
//! brick come from in a republic with no brickworks.** It buys the shortfall at
//! a border post of your choosing, in that post's own currency, and lands it
//! *at the post*. Your lorries still have to go and get it, over roads you still
//! have to build, and your crews still have to do the work.
//!
//! # The crossing is the decision
//!
//! Naming which post is the entire point, and it is why this is a policy rather
//! than a switch. A Western post settles in dollars and an Eastern one in
//! roubles, so choosing a post chooses a currency; and the post is a place, so
//! choosing it also chooses how far your fleet drives. A republic whose only
//! Western post is on the far side of the map pays for its dollars in
//! kilometres. That is the same geography the trade rules already answer to.
//!
//! # Global, with per-site overrides
//!
//! Noah's words: *"You can enable auto-import which will automatically import
//! materials from a customs office of your choosing, or auto-import on selected
//! construction sites only."* A default that every site follows, and a site may
//! say otherwise — including saying **off** when the default is on, which is why
//! an override is `Option<CrossingId>` rather than a crossing.

use crate::fleet::Destination;
use crate::resource::Resource;
use crate::trade::CrossingId;
use crate::units::Tonnes;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where sites get materials the republic has not made.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BuildPolicy {
    /// The post every site uses unless it says otherwise. `None` is the
    /// default: a republic imports nothing until somebody says to.
    global: Option<CrossingId>,
    /// Sites that disagree with the default. The inner `Option` is the point —
    /// `Some(None)` is a site opted **out** while the republic imports, which a
    /// bare map of crossings could not say.
    per_site: BTreeMap<Destination, Option<CrossingId>>,
    /// What has already been bought abroad on each site's account.
    ///
    /// **The Directorate buys a site's bill once**, and this is what makes that
    /// true. Without it auto-import chases a *shortfall*, and a shortfall is not
    /// the site's property: the goods land in a border yard and the republic's
    /// own freight ranking decides where they go from there. Measured — a six
    /// tonne bill bought forty-eight tonnes of machinery, because a
    /// Construction Office about to run dry outranks a foundation and the
    /// lorries kept taking it there instead.
    ///
    /// So the failure mode is now a stalled site rather than an emptied purse.
    /// That is the right way round: a site standing still is on the screen, and
    /// hard currency draining into a border post is not.
    bought: BTreeMap<(Destination, Resource), Tonnes>,
}

impl BuildPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// The post this site imports through, if it imports at all.
    pub fn crossing_for(&self, site: Destination) -> Option<CrossingId> {
        match self.per_site.get(&site) {
            Some(override_) => *override_,
            None => self.global,
        }
    }

    /// The republic's default post.
    pub fn global(&self) -> Option<CrossingId> {
        self.global
    }

    /// Whether this site has been given an instruction of its own.
    pub fn is_overridden(&self, site: Destination) -> bool {
        self.per_site.contains_key(&site)
    }

    /// How many sites disagree with the default.
    pub fn overrides(&self) -> usize {
        self.per_site.len()
    }

    /// What has been bought abroad on this site's account so far.
    pub fn bought_for(&self, site: Destination, resource: Resource) -> Tonnes {
        self.bought
            .get(&(site, resource))
            .copied()
            .unwrap_or(Tonnes::ZERO)
    }

    /// How much of a site's bill the Directorate will still buy.
    pub fn allowance(&self, site: Destination, resource: Resource, bill: Tonnes) -> Tonnes {
        bill.saturating_sub(self.bought_for(site, resource))
    }

    pub(crate) fn record_bought(&mut self, site: Destination, resource: Resource, tonnes: Tonnes) {
        *self.bought.entry((site, resource)).or_default() += tonnes;
    }

    pub(crate) fn set_global(&mut self, crossing: Option<CrossingId>) {
        self.global = crossing;
    }

    pub(crate) fn set_site(&mut self, site: Destination, crossing: Option<CrossingId>) {
        self.per_site.insert(site, crossing);
    }

    /// Drop a site's instruction, so it follows the default again.
    pub(crate) fn clear_site(&mut self, site: Destination) -> bool {
        self.per_site.remove(&site).is_some()
    }

    /// Forget sites that no longer exist.
    ///
    /// A building id is never reused, so a stale entry is harmless to read —
    /// but a map that only ever grows is a save that only ever grows, and this
    /// is one of the few structures keyed by something that stops existing.
    pub(crate) fn forget(&mut self, site: Destination) {
        self.per_site.remove(&site);
        self.bought.retain(|(at, _), _| *at != site);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingId;

    fn site(n: u32) -> Destination {
        Destination::Building(BuildingId(n))
    }

    /// A republic imports nothing until somebody says to. The default is not a
    /// convenience — auto-import spends hard currency, and a policy that was on
    /// by default would spend it before the player had chosen a post.
    #[test]
    fn nothing_is_imported_until_a_post_is_named() {
        let policy = BuildPolicy::new();
        assert_eq!(policy.global(), None);
        assert_eq!(policy.crossing_for(site(1)), None);
    }

    /// The case a bare map of crossings could not express: the republic imports
    /// through the western post, and this one site does not import at all.
    #[test]
    fn a_site_can_opt_out_of_a_republic_that_imports() {
        let mut policy = BuildPolicy::new();
        policy.set_global(Some(CrossingId(2)));
        assert_eq!(policy.crossing_for(site(1)), Some(CrossingId(2)));

        policy.set_site(site(1), None);
        assert_eq!(policy.crossing_for(site(1)), None, "opted out");
        assert_eq!(
            policy.crossing_for(site(9)),
            Some(CrossingId(2)),
            "and only this one"
        );
        assert!(policy.is_overridden(site(1)));

        // Clearing the override puts it back under the default rather than
        // leaving it off, which is the difference between "no instruction" and
        // "an instruction saying no".
        assert!(policy.clear_site(site(1)));
        assert_eq!(policy.crossing_for(site(1)), Some(CrossingId(2)));
        assert!(
            !policy.clear_site(site(1)),
            "there was nothing left to clear"
        );
    }

    #[test]
    fn a_site_can_import_through_a_different_post_from_the_rest() {
        let mut policy = BuildPolicy::new();
        policy.set_global(Some(CrossingId(1)));
        policy.set_site(site(4), Some(CrossingId(3)));
        assert_eq!(policy.crossing_for(site(4)), Some(CrossingId(3)));
        assert_eq!(policy.overrides(), 1);

        policy.forget(site(4));
        assert_eq!(policy.overrides(), 0);
        assert_eq!(policy.crossing_for(site(4)), Some(CrossingId(1)));
    }
}
