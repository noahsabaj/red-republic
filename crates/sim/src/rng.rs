//! The simulation's only source of randomness.
//!
//! # Why this generator
//!
//! **xoshiro256++**, seeded by **SplitMix64**. Chosen over the archived
//! build's `mulberry32` and over pulling in `rand`:
//!
//! - **Hand-implemented, zero dependencies.** A dependency bump must never be
//!   able to change a stream. Both algorithms are short, fully specified, and
//!   have public reference implementations, so the cost of owning them is
//!   about sixty lines and the benefit is that a seed means the same thing
//!   forever.
//! - **256 bits of state, not 32.** `mulberry32` was adequate for one map's
//!   worth of draws. This simulation runs tens of thousands of citizens, and
//!   short-period generators show their correlations exactly when you have
//!   many consumers drawing in lockstep.
//! - **Continuity with 1.x seeds is worth nothing.** Map generation is being
//!   rewritten around a 3D geology model, so archived seeds could not
//!   reproduce their maps regardless of which generator we kept.
//!
//! # The save contract
//!
//! [`Rng::state`] and [`Rng::set_state`] carry the *stream position*, not just
//! the seed. This is the contract the archived build established and proved:
//! a save that restores the seed but not the position resumes a different
//! future, and the failure is invisible until someone compares two runs.
//!
//! # Determinism
//!
//! Every operation here is integer arithmetic with wrapping semantics, so it
//! is bit-identical on any machine — which is what map generation needs, since
//! a shared seed is a promise between players. [`Rng::next_f64`] is the one
//! float, and its conversion is exact: a 53-bit integer scaled by a power of
//! two, with no rounding to disagree about.

/// The full internal state of an [`Rng`] — enough to resume a stream exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RngState([u64; 4]);

/// A seeded, resumable pseudo-random generator. See the module docs.
#[derive(Debug, Clone)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Expand a seed into a full state with SplitMix64, as the xoshiro authors
    /// specify. Seeding the state directly from a small integer leaves it
    /// mostly zero, and xoshiro needs several rounds to recover from that.
    pub fn from_seed(seed: u64) -> Self {
        let mut z = seed;
        let mut split = || {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^ (x >> 31)
        };
        let s = [split(), split(), split(), split()];
        // xoshiro is undefined for an all-zero state. SplitMix64 cannot
        // actually produce four zeros in a row, but relying on that silently
        // is how a generator becomes a constant.
        debug_assert!(s.iter().any(|&v| v != 0), "degenerate rng state");
        Self { s }
    }

    /// The next 64 random bits — the primitive every other method is built on.
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[0]
            .wrapping_add(self.s[3])
            .rotate_left(23)
            .wrapping_add(self.s[0]);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// A float in `[0, 1)`.
    ///
    /// Takes the top 53 bits — the exact width of an `f64` mantissa — and
    /// scales by 2⁻⁵³. Both steps are exact, so this introduces no rounding
    /// of its own and reproduces bit-for-bit anywhere `next_u64` does.
    pub fn next_f64(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / (1u64 << 53) as f64;
        (self.next_u64() >> 11) as f64 * SCALE
    }

    /// A uniform integer in `[0, n)`, with the bias removed.
    ///
    /// Plain `next_u64() % n` is skewed toward small values whenever `n` does
    /// not divide 2⁶⁴. Rejecting the short leading block costs one extra draw
    /// vanishingly often and makes the distribution exact — worth it, because
    /// a bias here is the kind of thing that shows up as a map that subtly
    /// prefers one corner and takes a week to find.
    ///
    /// # Panics
    /// If `n` is zero.
    pub fn next_bounded(&mut self, n: u64) -> u64 {
        assert!(n > 0, "next_bounded requires a positive bound");
        // (2^64) mod n, computed without overflowing.
        let threshold = (u64::MAX - n + 1) % n;
        loop {
            let x = self.next_u64();
            if x >= threshold {
                return x % n;
            }
        }
    }

    /// A float in `[lo, hi)`.
    pub fn next_range(&mut self, lo: f64, hi: f64) -> f64 {
        debug_assert!(lo <= hi, "next_range needs lo <= hi");
        lo + self.next_f64() * (hi - lo)
    }

    /// The current stream position. Persist this, not the seed.
    pub fn state(&self) -> RngState {
        RngState(self.s)
    }

    /// Resume a stream from a previously saved position.
    pub fn set_state(&mut self, state: RngState) {
        self.s = state.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tripwire on the generator itself.
    ///
    /// These numbers are xoshiro256++ seeded by SplitMix64 from seed 1961, and
    /// pinning them is the whole point: any "improvement" to the algorithm,
    /// the seeding, or the word order silently re-rolls every map and every
    /// simulated decision in the game. If this test fails, the change was
    /// either wrong or it invalidates every existing seed — and either way it
    /// is a decision, not a detail.
    ///
    /// They are not this implementation's word taken for itself: they were
    /// cross-checked against an independent arbitrary-precision implementation
    /// of both algorithms before being written down, so this pins the
    /// published algorithm rather than whatever was transcribed here.
    #[test]
    fn known_answers_for_a_pinned_seed() {
        let mut rng = Rng::from_seed(1961);
        let got: Vec<u64> = (0..6).map(|_| rng.next_u64()).collect();
        assert_eq!(
            got,
            vec![
                7_156_055_288_471_504_377,
                12_830_646_930_243_714_415,
                10_772_675_645_828_591_914,
                13_532_795_614_754_902_491,
                2_803_697_848_528_527_378,
                1_317_598_663_084_940_546,
            ]
        );
    }

    #[test]
    fn the_same_seed_replays_the_same_stream() {
        let mut a = Rng::from_seed(42);
        let mut b = Rng::from_seed(42);
        for _ in 0..1_000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge_immediately() {
        let mut a = Rng::from_seed(1);
        let mut b = Rng::from_seed(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    /// The save contract: restoring the seed is not enough, the POSITION has
    /// to come back too. A generator that fails this reloads into a different
    /// future and nothing complains.
    #[test]
    fn a_saved_position_resumes_the_same_future() {
        let mut live = Rng::from_seed(7);
        for _ in 0..500 {
            live.next_u64();
        }
        let saved = live.state();
        let expected: Vec<u64> = (0..100).map(|_| live.next_u64()).collect();

        // A fresh generator on the same seed, wound to the saved position,
        // must produce exactly what the live one went on to produce.
        let mut restored = Rng::from_seed(0);
        restored.set_state(saved);
        let actual: Vec<u64> = (0..100).map(|_| restored.next_u64()).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn floats_stay_in_the_half_open_unit_interval() {
        let mut rng = Rng::from_seed(99);
        for _ in 0..100_000 {
            let x = rng.next_f64();
            assert!((0.0..1.0).contains(&x), "{x} outside [0, 1)");
        }
    }

    #[test]
    fn bounded_draws_stay_in_range_and_reach_both_ends() {
        let mut rng = Rng::from_seed(5);
        let mut seen_low = false;
        let mut seen_high = false;
        for _ in 0..10_000 {
            let x = rng.next_bounded(7);
            assert!(x < 7);
            seen_low |= x == 0;
            seen_high |= x == 6;
        }
        assert!(
            seen_low && seen_high,
            "bounded draws did not cover the range"
        );
    }

    #[test]
    fn bounded_of_one_is_always_zero() {
        let mut rng = Rng::from_seed(3);
        for _ in 0..100 {
            assert_eq!(rng.next_bounded(1), 0);
        }
    }

    #[test]
    #[should_panic(expected = "positive bound")]
    fn bounded_rejects_a_zero_bound() {
        Rng::from_seed(1).next_bounded(0);
    }

    /// Not a quality test — a smoke test that the distribution is not visibly
    /// broken. A generator that returned a constant, or only even numbers,
    /// would pass every test above and fail this one.
    #[test]
    fn draws_are_roughly_uniform_across_buckets() {
        let mut rng = Rng::from_seed(2024);
        let mut buckets = [0u32; 10];
        const N: u32 = 200_000;
        for _ in 0..N {
            buckets[(rng.next_f64() * 10.0) as usize] += 1;
        }
        let expected = N / 10;
        for (i, &count) in buckets.iter().enumerate() {
            let drift = (count as i64 - expected as i64).abs();
            assert!(
                drift < expected as i64 / 10,
                "bucket {i} held {count}, expected about {expected}"
            );
        }
    }
}
