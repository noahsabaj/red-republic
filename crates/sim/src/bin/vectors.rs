//! Pinned generator output, for the Godot port to check itself against.
//!
//! **A throwaway extraction tool**, like `manifest`, and it dies with this
//! crate. It exists because map generation must reproduce *across machines* — a
//! shared seed is a promise between players — so a reimplementation of the
//! generator in another language is only as good as the evidence that it draws
//! the same stream. Six pinned values in a doc comment are not that evidence;
//! these are.
//!
//! `next_u64` is emitted as hex rather than decimal on purpose. GDScript's
//! `int` is signed, so half of these have no positive literal on the other
//! side, and a decimal that has to be sign-corrected by hand before it can be
//! compared is a transcription step — which is the thing being removed.

use red_republic_sim::rng::Rng;

fn main() {
    println!("{{");

    // The seeds cover the cases that have historically differed between
    // implementations: a small seed, zero, one whose SplitMix64 expansion sets
    // the top bit early, and the one the Rust suite already pins.
    let seeds: [u64; 5] = [1961, 0, 1, 42, u64::MAX];

    println!("  \"next_u64\": {{");
    let rows: Vec<String> = seeds
        .iter()
        .map(|&seed| {
            let mut rng = Rng::from_seed(seed);
            let vals: Vec<String> = (0..16)
                .map(|_| format!("\"{:016x}\"", rng.next_u64()))
                .collect();
            format!("    \"{seed:016x}\": [{}]", vals.join(", "))
        })
        .collect();
    println!("{}", rows.join(",\n"));
    println!("  }},");

    // Floats, emitted as their **bit patterns** rather than as decimals.
    //
    // Not caution for its own sake: this repository measured that serde_json
    // fails to round-trip f64, returning a different value for 91,767 of
    // 200,000 samples, because its parser is not correctly rounded even when
    // the digits it wrote were right (see the note in `crates/sim/Cargo.toml`).
    // Whether Godot's parser shares that defect is not something these vectors
    // should depend on — a test that silently checks the reader's arithmetic
    // instead of the generator's is worse than no test. Hex goes through as
    // an integer and is decoded to a double on the far side, so the only thing
    // under test here is the stream.
    println!("  \"next_f64_bits\": {{");
    let rows: Vec<String> = seeds
        .iter()
        .map(|&seed| {
            let mut rng = Rng::from_seed(seed);
            let vals: Vec<String> = (0..16)
                .map(|_| format!("\"{:016x}\"", rng.next_f64().to_bits()))
                .collect();
            format!("    \"{seed:016x}\": [{}]", vals.join(", "))
        })
        .collect();
    println!("{}", rows.join(",\n"));
    println!("  }},");

    // Bounded draws, including the bounds where the rejection threshold is not
    // zero — which is the half of `next_bounded` a naive `% n` gets wrong.
    println!("  \"next_bounded\": {{");
    let bounds: [u64; 6] = [1, 2, 7, 10, 1000, 1_000_003];
    let rows: Vec<String> = bounds
        .iter()
        .map(|&n| {
            let mut rng = Rng::from_seed(1961);
            let vals: Vec<String> = (0..24).map(|_| rng.next_bounded(n).to_string()).collect();
            format!("    \"{n}\": [{}]", vals.join(", "))
        })
        .collect();
    println!("{}", rows.join(",\n"));
    println!("  }},");

    // The save contract: a stream wound 500 draws in, its state carried, and
    // what it goes on to produce. A port that restores the seed but not the
    // position passes every test above and fails this one.
    let mut live = Rng::from_seed(7);
    for _ in 0..500 {
        live.next_u64();
    }
    //
    // The state itself is deliberately NOT emitted. `RngState` is opaque, and
    // pinning its four words here would make the port's internal representation
    // part of the contract — which it is not, and which would be the wrong
    // thing to freeze. What has to match is the *stream*: wind 500 draws in,
    // carry the position across a save, and produce these. A port that restores
    // the seed but not the position passes every test above and fails this one.
    let after: Vec<String> = (0..8)
        .map(|_| format!("\"{:016x}\"", live.next_u64()))
        .collect();
    println!("  \"resume\": {{");
    println!("    \"seed\": 7,");
    println!("    \"draws_before\": 500,");
    println!("    \"after\": [{}]", after.join(", "));
    println!("  }}");

    println!("}}");
}
