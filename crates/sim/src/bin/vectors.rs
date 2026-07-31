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
    println!("  }},");

    terrain_vectors();
    geology_vectors();

    println!("}}");
}

/// The geology fingerprint, per seed.
///
/// Hashes every authored field of every body in draw order. It fails if the
/// draw order changes, if the plan changes, or — the case it really exists for —
/// if generation ever picks up a float operation allowed to differ in its last
/// bit between platforms. Any of those means two players with the same seed get
/// different republics.
fn geology_vectors() {
    use red_republic_sim::mapgen::{DEFAULT_PLAN, generate_geology};
    use red_republic_sim::units::Metres;

    println!("  \"geology\": [");
    let mut rows: Vec<String> = Vec::new();
    for &seed in &[1961_u64, 7, 42] {
        for &extent in &[3000.0_f64, 10_000.0] {
            let g = generate_geology(seed, Metres(extent), &DEFAULT_PLAN);
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            let mut eat = |v: f64, h: &mut u64| {
                for byte in v.to_bits().to_le_bytes() {
                    *h ^= u64::from(byte);
                    *h = h.wrapping_mul(0x100_0000_01b3);
                }
            };
            let deposits = g.all();
            let mut layers = 0usize;
            let mut total = 0.0_f64;
            for d in deposits {
                eat(f64::from(d.id.0), &mut h);
                eat(d.mineral as u8 as f64, &mut h);
                eat(d.centre.x.0, &mut h);
                eat(d.centre.y.0, &mut h);
                eat(d.radius.0, &mut h);
                eat(d.top.0, &mut h);
                for l in &d.layers {
                    eat(l.thickness.0, &mut h);
                    eat(l.initial.0, &mut h);
                    layers += 1;
                    total += l.initial.0;
                }
            }
            rows.push(format!(
                "    {{ \"seed\": {seed}, \"extent\": {extent:?}, \"deposits\": {}, \"layers\": {layers}, \"fnv\": \"{h:016x}\", \"total_tonnes_bits\": \"{:016x}\" }}",
                deposits.len(),
                total.to_bits()
            ));
        }
    }
    println!("{}", rows.join(",\n"));
    println!("  ]");
}

/// FNV-1a over bytes, matching `Bits.fnv` on the far side.
fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Worldgen, checked in three layers rather than one.
///
/// A generated map is a million cells and the only practical comparison is a
/// hash — but a single red hash cannot tell a wrong hash function from wrong
/// hydrology, and the fill and accumulation passes are where the subtle
/// mistakes live. So: the integer cell hash, then the fractal field, then the
/// finished surfaces and heights. The first that disagrees is the one to look
/// at.
fn terrain_vectors() {
    use red_republic_sim::terrain::{
        DEFAULT_TERRAIN, Surface, fractal_noise, generate_terrain, hash_cell,
    };
    use red_republic_sim::units::{Metres, Point};

    println!("  \"terrain\": {{");

    // Layer 1: the integer lattice hash, including negative coordinates —
    // GDScript's `>>` sign-extends, and this is where that shows.
    let mut rows: Vec<String> = Vec::new();
    for &(x, y) in &[
        (0_i64, 0_i64),
        (1, 0),
        (0, 1),
        (-1, -1),
        (12345, -67890),
        (-1, 9_007_199_254_740_993),
    ] {
        // Coordinates as hex too, not decimal. A JSON number is a double on
        // the far side, so 9007199254740993 arrives as ...992 and the vector
        // silently tests a different cell than the one it pinned — which is
        // exactly the case chosen to exercise the high bits.
        rows.push(format!(
            "      [\"{:016x}\", \"{:016x}\", \"{:016x}\"]",
            x as u64,
            y as u64,
            hash_cell(1961, x, y)
        ));
    }
    println!("    \"hash_cell\": [\n{}\n    ],", rows.join(",\n"));

    // Layer 2: the fractal field, as bits.
    let mut rows: Vec<String> = Vec::new();
    for &(x, y) in &[
        (0.0_f64, 0.0_f64),
        (5.0, 5.0),
        (1234.5, 6789.25),
        (2000.0, 2000.0),
        (5995.0, 5995.0),
    ] {
        let v = fractal_noise(
            1961,
            Point::new(Metres(x), Metres(y)),
            DEFAULT_TERRAIN.feature_size,
            DEFAULT_TERRAIN.octaves,
        );
        rows.push(format!("      [{x:?}, {y:?}, \"{:016x}\"]", v.to_bits()));
    }
    println!("    \"fractal_noise\": [\n{}\n    ],", rows.join(",\n"));

    // Layer 3: whole generated maps. A 1.2 km map is 120 cells a side, which is
    // small enough to generate in a test and large enough that the fill and the
    // accumulation both have real work to do.
    let mut rows: Vec<String> = Vec::new();
    for &seed in &[1961_u64, 7, 42] {
        for &extent in &[1200.0_f64, 3000.0] {
            let t = generate_terrain(seed, Metres(extent), &DEFAULT_TERRAIN);
            let cells = t.cells();
            let mut surfaces: Vec<u8> = Vec::new();
            let mut heights: Vec<u8> = Vec::new();
            let mut counts = [0_u32; 4];
            for cy in 0..cells {
                for cx in 0..cells {
                    let p = t.cell_centre(cx, cy);
                    let s = t.surface_at(p).expect("cell centre is on the map");
                    let code = match s {
                        Surface::Grass => 0_u8,
                        Surface::Forest => 1,
                        Surface::Rock => 2,
                        Surface::Water => 3,
                    };
                    counts[code as usize] += 1;
                    surfaces.push(code);
                    let h = t.height_at(p).expect("cell centre is on the map").0 as f32;
                    heights.extend_from_slice(&h.to_le_bytes());
                }
            }
            rows.push(format!(
                "      {{ \"seed\": {seed}, \"extent\": {extent:?}, \"cells\": {cells}, \"surface_fnv\": \"{:016x}\", \"height_fnv\": \"{:016x}\", \"counts\": [{}, {}, {}, {}] }}",
                fnv(&surfaces),
                fnv(&heights),
                counts[0],
                counts[1],
                counts[2],
                counts[3]
            ));
        }
    }
    println!("    \"maps\": [\n{}\n    ]", rows.join(",\n"));

    // A trailing comma: `geology_vectors` follows.
    println!("  }},");
}
