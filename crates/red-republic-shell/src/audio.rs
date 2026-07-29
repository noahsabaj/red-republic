//! Procedural audio: every sound the republic makes, synthesised.
//!
//! Ambience, weather, machinery, vehicles and interface feedback are generated
//! rather than recorded. Two reasons, and the second is why the code is here
//! rather than in GDScript.
//!
//! 1. **Nobody working on this repository can hear it.** Musical and tonal
//!    quality is only ever Noah's verdict, so what *can* be established here is
//!    everything else: that a sound is the same every time, that it is the length
//!    it claims, that it loops without a click, and that it does not clip. Those
//!    are properties a test can hold, and the tests at the bottom of this file
//!    hold them.
//! 2. Determinism. A generated sound must be **bit-identical** run to run, or the
//!    audit above is auditing something other than what ships. Synthesis in
//!    GDScript would put it beyond `cargo test` entirely — the same argument that
//!    made `reference::document` return `Vec<String>` instead of a Godot type.
//!
//! # The State Radio is deliberately not here
//!
//! The radio wants **real fixed composed songs** a player can replay identically,
//! not generative improvisation. That is a content decision and a taste decision,
//! and neither is answerable by synthesis. Its panel and transport are built
//! empty so composed tracks are a content drop rather than a retrofit.
//!
//! # Sample format
//!
//! Mono 16-bit PCM at [`SAMPLE_RATE`]. Mono because every one of these is either
//! diegetic — positioned in the world, where Godot does the panning from a
//! listener — or a full-width interface sound where stereo buys nothing. Halving
//! the bytes matters more than it looks: a four-second loop is 176 KB mono and
//! 353 KB stereo, and there are a dozen of them.

use godot::prelude::*;

/// Samples per second. 22,050 rather than 44,100.
///
/// Every sound here is noise, rumble or a soft transient, and none of them has
/// content above 11 kHz worth keeping — a wind loop and a diesel drone are
/// low-frequency by nature. Halves the memory for no audible loss on this
/// material. An actual music track would not tolerate it, which is another
/// reason the radio is a content drop rather than a generator.
pub const SAMPLE_RATE: u32 = 22_050;

/// What a generated sound is for. Drives which bus it plays on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice {
    /// Open-air wind. The bed everything else sits on.
    Wind,
    /// Rain on the ground.
    Rain,
    /// Snow, which is quieter than rain and duller. Its own voice rather than
    /// rain at a lower volume, because the difference between them is spectral
    /// and not one of level.
    Snow,
    /// Industry: a low rumble with a slow beat in it.
    Machinery,
    /// A diesel engine, for a vehicle passing.
    Engine,
    /// Interface: a click, for anything selected.
    Click,
    /// Interface: an action carried out.
    Confirm,
    /// Interface: an action refused. Not a buzzer — see [`refusal`].
    Refuse,
}

impl Voice {
    pub const ALL: [Voice; 8] = [
        Voice::Wind,
        Voice::Rain,
        Voice::Snow,
        Voice::Machinery,
        Voice::Engine,
        Voice::Click,
        Voice::Confirm,
        Voice::Refuse,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Voice::Wind => "wind",
            Voice::Rain => "rain",
            Voice::Snow => "snow",
            Voice::Machinery => "machinery",
            Voice::Engine => "engine",
            Voice::Click => "click",
            Voice::Confirm => "confirm",
            Voice::Refuse => "refuse",
        }
    }

    /// Which mixer bus this belongs on, matching the names `settings_store.gd`
    /// sets volumes for.
    pub fn bus(self) -> &'static str {
        match self {
            Voice::Wind | Voice::Rain | Voice::Snow => "Ambience",
            Voice::Machinery | Voice::Engine => "Machinery",
            Voice::Click | Voice::Confirm | Voice::Refuse => "Interface",
        }
    }

    /// Whether this is a loop or a one-shot.
    ///
    /// A loop is crossfaded end-to-start by [`generate`]; a one-shot is not, and
    /// a one-shot that were crossfaded would have its attack chewed off.
    pub fn loops(self) -> bool {
        matches!(
            self,
            Voice::Wind | Voice::Rain | Voice::Snow | Voice::Machinery | Voice::Engine
        )
    }

    /// How long, in seconds.
    ///
    /// The loops are deliberately not all the same length. Wind and rain are
    /// noise, and a short noise loop is audible *as* a loop because the ear
    /// latches onto the repeat; four seconds is long enough that it does not.
    /// Machinery has a beat in it, so its length is a whole number of beats or
    /// the beat stutters at the seam.
    pub fn seconds(self) -> f64 {
        match self {
            Voice::Wind => 4.0,
            Voice::Rain => 4.0,
            Voice::Snow => 4.0,
            Voice::Machinery => 3.0,
            Voice::Engine => 2.0,
            Voice::Click => 0.045,
            Voice::Confirm => 0.16,
            Voice::Refuse => 0.22,
        }
    }
}

/// How much of a loop is crossfaded back onto its own start, in seconds.
///
/// Without this a noise loop clicks audibly at the seam, because sample zero and
/// the last sample are unrelated values and the discontinuity is a step. A step in
/// a waveform is a click; this is the whole reason the fade exists.
const CROSSFADE_SECONDS: f64 = 0.35;

/// The periodic components inside each looping voice, in hertz.
///
/// Authored here rather than as literals inside the synthesis functions, because
/// **they are load-bearing and were silently so.** Removing the crossfade fails
/// wind and snow and leaves rain, machinery and engine passing — and the reason
/// the last two pass is that every frequency in them completes a whole number of
/// cycles across the loop, so they join without help. That is a real property and
/// it is one edit from being false: 48 Hz over 3 s is 144 cycles, and 47.5 Hz is
/// 142.5. Verified by sabotage, and the first attempt at that sabotage picked
/// 47 Hz -- which is 141 cycles exactly and passed. An integer frequency over a
/// whole-second loop always closes, so only a fractional one tests anything.
///
/// `every_tone_in_a_loop_completes_whole_cycles` reads this table. A frequency
/// changed in a synthesis function and not here would make that guard check the
/// wrong number, which is why the functions read the table rather than the other
/// way round.
///
/// Rain is absent deliberately: it has no periodic component. It joins because
/// dense white noise masks its own seam, which is a different argument and not one
/// a frequency table can make.
const TONES: &[(Voice, &[f64])] = &[
    // The gust swell, twice across the loop. Not a tone anybody hears as pitch,
    // and still a periodic component that has to close.
    (Voice::Wind, &[0.5]),
    (Voice::Snow, &[]),
    // Fundamental, its fifth, and the mechanical beat.
    (Voice::Machinery, &[48.0, 72.0, 2.0]),
    // Sawtooth fundamental and the cylinder firing over it.
    (Voice::Engine, &[34.0, 102.0]),
];

/// The frequencies one voice is built from.
fn tones_of(voice: Voice) -> &'static [f64] {
    TONES
        .iter()
        .find(|(v, _)| *v == voice)
        .map_or(&[], |(_, tones)| *tones)
}

/// A voice as 16-bit PCM samples.
///
/// A pure function of the voice alone. No seed parameter, deliberately: two
/// identical wind loops are the same wind, and letting a caller vary them would
/// make "the audio is deterministic" a claim about the caller rather than about
/// this function. Variation at playback is Godot's job — pitch scale, volume and
/// position — which is also where it belongs, because it is free there and
/// baked-in here.
pub fn generate(voice: Voice) -> Vec<i16> {
    let count = (voice.seconds() * f64::from(SAMPLE_RATE)).round() as usize;
    let mut samples = vec![0.0f64; count];
    // Its own generator per voice, so adding a voice never shifts an existing
    // one. The same discipline as the simulation's substreams, for the same
    // reason: a sound that changes because something unrelated was added is a
    // sound nobody can pin.
    let mut noise = Noise::new(0x5245_4400 ^ voice as u64);

    match voice {
        Voice::Wind => wind(&mut samples, &mut noise),
        Voice::Rain => rain(&mut samples, &mut noise),
        Voice::Snow => snow(&mut samples, &mut noise),
        Voice::Machinery => machinery(&mut samples, &mut noise),
        Voice::Engine => engine(&mut samples, &mut noise),
        Voice::Click => click(&mut samples),
        Voice::Confirm => confirm(&mut samples),
        Voice::Refuse => refusal(&mut samples),
    }

    if voice.loops() {
        seam(&mut samples);
    } else {
        // A one-shot still needs its tail taken to zero, or stopping the voice
        // is itself a click.
        let fade = (samples.len() / 8).max(1);
        let start = samples.len() - fade;
        for i in 0..fade {
            samples[start + i] *= 1.0 - (i as f64 / fade as f64);
        }
    }

    normalise(&mut samples, peak_for(voice));
    samples
        .iter()
        .map(|v| (v.clamp(-1.0, 1.0) * f64::from(i16::MAX)) as i16)
        .collect()
}

/// The peak each voice is normalised to, as a share of full scale.
///
/// Set here rather than left to the mixer because the *relative* levels are a
/// property of the material: a click that arrives at the same peak as a wind bed
/// is a click that startles. The player's volume controls scale all of it
/// together, so these have to be right before any slider is touched.
fn peak_for(voice: Voice) -> f64 {
    match voice {
        Voice::Wind => 0.55,
        Voice::Rain => 0.62,
        Voice::Snow => 0.34,
        Voice::Machinery => 0.58,
        Voice::Engine => 0.66,
        Voice::Click => 0.30,
        Voice::Confirm => 0.42,
        Voice::Refuse => 0.44,
    }
}

/// Wind: brown-ish noise with a slow swell over it.
///
/// Two low-passes in series rather than one. A single pole leaves too much hiss
/// and reads as static; the second one is what turns it into air.
fn wind(samples: &mut [f64], noise: &mut Noise) {
    let rate = f64::from(SAMPLE_RATE);
    let swell = tones_of(Voice::Wind)[0];
    let mut a = 0.0;
    let mut b = 0.0;
    for (i, out) in samples.iter_mut().enumerate() {
        a += 0.020 * (noise.next() - a);
        b += 0.060 * (a - b);
        // A slow gust swell, phased so it is at its mean at both ends —
        // otherwise the crossfade has to fight a level difference as well as a
        // waveform step.
        let t = i as f64 / rate;
        let gust = 0.62 + 0.38 * (t * std::f64::consts::TAU * swell).sin();
        *out = b * gust * 14.0;
    }
}

/// Rain: dense high noise with a body under it.
fn rain(samples: &mut [f64], noise: &mut Noise) {
    let mut low = 0.0;
    for out in samples.iter_mut() {
        let white = noise.next();
        low += 0.30 * (white - low);
        // The high component is what makes it rain rather than wind; the low one
        // stops it being a hiss.
        *out = white * 0.45 + low * 0.85;
    }
}

/// Snow: rain's spectrum with the top taken off and the density dropped.
///
/// Its own function rather than rain at a lower gain, because falling snow is
/// quieter *and* duller, and only the second of those is a volume change.
fn snow(samples: &mut [f64], noise: &mut Noise) {
    let mut a = 0.0;
    let mut b = 0.0;
    for out in samples.iter_mut() {
        a += 0.10 * (noise.next() - a);
        b += 0.10 * (a - b);
        *out = b * 8.0;
    }
}

/// Machinery: a low fundamental, a fifth above it, and a slow mechanical beat.
///
/// The beat is what makes it read as a works rather than as a drone, and it is
/// why this loop's length is a whole number of beats.
fn machinery(samples: &mut [f64], noise: &mut Noise) {
    let rate = f64::from(SAMPLE_RATE);
    // Fundamental, fifth, beat — in `TONES` order. Low enough to feel
    // industrial, high enough to survive a laptop speaker that rolls off below
    // 100 Hz.
    let tones = tones_of(Voice::Machinery);
    let (fundamental, fifth, beat_hz) = (tones[0], tones[1], tones[2]);
    let mut rumble = 0.0;
    for (i, out) in samples.iter_mut().enumerate() {
        let t = i as f64 / rate;
        rumble += 0.05 * (noise.next() - rumble);
        let hum = (t * std::f64::consts::TAU * fundamental).sin() * 0.55
            + (t * std::f64::consts::TAU * fifth).sin() * 0.22;
        let beat = (t * std::f64::consts::TAU * beat_hz).sin().max(0.0).powi(3);
        *out = hum * (0.7 + 0.3 * beat) + rumble * 3.0;
    }
}

/// A diesel engine: a rough fundamental with cylinder firing on top.
fn engine(samples: &mut [f64], noise: &mut Noise) {
    let rate = f64::from(SAMPLE_RATE);
    let tones = tones_of(Voice::Engine);
    let (crank, firing_hz) = (tones[0], tones[1]);
    let mut grit = 0.0;
    for (i, out) in samples.iter_mut().enumerate() {
        let t = i as f64 / rate;
        grit += 0.12 * (noise.next() - grit);
        // A sawtooth rather than a sine: a diesel is all harmonics.
        let saw = (t * crank).fract() * 2.0 - 1.0;
        let firing = (t * std::f64::consts::TAU * firing_hz).sin() * 0.18;
        *out = saw * 0.5 + firing + grit * 2.2;
    }
}

/// A click: a short filtered impulse.
///
/// Not a beep. Every interface sound in this game is a mechanical noise — a
/// switch, a stamp, a drawer — because the register is a state instrument and a
/// synthesised tone reads as a phone notification.
fn click(samples: &mut [f64]) {
    let rate = f64::from(SAMPLE_RATE);
    for (i, out) in samples.iter_mut().enumerate() {
        let t = i as f64 / rate;
        let decay = (-t * 220.0).exp();
        *out = (t * std::f64::consts::TAU * 1_400.0).sin() * decay;
    }
}

/// Confirm: two short knocks, low then high. A stamp coming down.
fn confirm(samples: &mut [f64]) {
    let rate = f64::from(SAMPLE_RATE);
    for (i, out) in samples.iter_mut().enumerate() {
        let t = i as f64 / rate;
        let first = (-t * 90.0).exp() * (t * std::f64::consts::TAU * 420.0).sin();
        let second = if t > 0.055 {
            let u = t - 0.055;
            (-u * 70.0).exp() * (u * std::f64::consts::TAU * 640.0).sin()
        } else {
            0.0
        };
        *out = first * 0.7 + second * 0.9;
    }
}

/// Refuse: one flat, dull thud that does not resolve.
///
/// Deliberately **not** a descending two-tone buzzer. A refusal in this game
/// always carries a sentence explaining itself, so the sound's job is to draw the
/// eye to that sentence, not to scold. A harsh sound on a refusal makes a player
/// avoid trying things, and trying things is how this game is learned.
fn refusal(samples: &mut [f64]) {
    let rate = f64::from(SAMPLE_RATE);
    for (i, out) in samples.iter_mut().enumerate() {
        let t = i as f64 / rate;
        let decay = (-t * 26.0).exp();
        let body = (t * std::f64::consts::TAU * 150.0).sin() * 0.8
            + (t * std::f64::consts::TAU * 151.7).sin() * 0.5;
        *out = body * decay;
    }
}

/// Fold the tail of a loop back onto its head so the seam is inaudible.
///
/// Takes a `Vec` because it **shortens** the buffer: the tail is mixed into the
/// head and then discarded, so a loop plays `seconds() - CROSSFADE_SECONDS` of
/// audio. That is the crossfade working rather than a length bug — the discarded
/// samples are still heard, at the start of the next cycle.
fn seam(samples: &mut Vec<f64>) {
    let fade = ((CROSSFADE_SECONDS * f64::from(SAMPLE_RATE)) as usize).min(samples.len() / 3);
    if fade == 0 {
        return;
    }
    let tail_start = samples.len() - fade;
    for i in 0..fade {
        let mix = i as f64 / fade as f64;
        let tail = samples[tail_start + i];
        samples[i] = samples[i] * mix + tail * (1.0 - mix);
    }
    samples.truncate(tail_start);
}

/// Scale so the loudest sample sits at `peak`.
fn normalise(samples: &mut [f64], peak: f64) {
    let loudest = samples.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if loudest <= f64::EPSILON {
        return;
    }
    let gain = peak / loudest;
    for sample in samples.iter_mut() {
        *sample *= gain;
    }
}

/// A deterministic white-noise source.
///
/// Its own generator rather than anything from the simulation crate: audio must
/// never draw on a stream a republic depends on, and the sim's `Rng` is not
/// exported for this. SplitMix64, which is enough for noise and is exactly
/// reproducible.
struct Noise {
    state: u64,
}

impl Noise {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next sample in `-1.0..1.0`.
    fn next(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // The top 32 bits, mapped to -1..1. Deliberately not modulo, which
        // biases.
        (((z >> 32) as f64) / 2_147_483_648.0) - 1.0
    }
}

/// A voice as bytes Godot can wrap in an `AudioStreamWAV`.
///
/// Little-endian 16-bit, which is what `AudioStreamWAV::FORMAT_16_BITS` expects.
pub fn generate_for_godot(voice: Voice) -> PackedByteArray {
    let samples = generate(voice);
    let mut out = PackedByteArray::new();
    out.resize(samples.len() * 2);
    for (i, sample) in samples.iter().enumerate() {
        let [low, high] = sample.to_le_bytes();
        out[i * 2] = low;
        out[i * 2 + 1] = high;
    }
    out
}

/// The voice at an index into [`Voice::ALL`], for the Godot side.
pub fn voice_at(index: i64) -> Option<Voice> {
    Voice::ALL.get(index.max(0) as usize).copied()
}

/// The synthesiser, as a node Godot can hold.
///
/// Its own class rather than methods on [`crate::republic::Republic`], because a
/// sound has nothing to do with a republic: the voices are the same in the main
/// menu, on the founding screen and in a game, and hanging them off the republic
/// would mean no interface sound worked until one was founded.
#[derive(GodotClass)]
#[class(base = Node, init)]
pub struct Sounds {
    base: godot::obj::Base<godot::classes::Node>,
}

#[godot_api]
impl Sounds {
    /// How many voices there are.
    #[func]
    fn voice_count(&self) -> i64 {
        Voice::ALL.len() as i64
    }

    #[func]
    fn voice_name(&self, index: i64) -> GString {
        voice_at(index).map_or_else(|| GString::from(""), |v| GString::from(v.name()))
    }

    /// Which bus a voice belongs on.
    #[func]
    fn voice_bus(&self, index: i64) -> GString {
        voice_at(index).map_or_else(|| GString::from("Master"), |v| GString::from(v.bus()))
    }

    #[func]
    fn voice_loops(&self, index: i64) -> bool {
        voice_at(index).is_some_and(Voice::loops)
    }

    /// A voice's samples as little-endian 16-bit PCM.
    #[func]
    fn voice_samples(&self, index: i64) -> PackedByteArray {
        voice_at(index).map_or_else(PackedByteArray::new, generate_for_godot)
    }

    #[func]
    fn sample_rate(&self) -> i64 {
        i64::from(SAMPLE_RATE)
    }

    /// Suppresses the unused-field warning while nothing needs the base.
    #[allow(dead_code)]
    fn base_node(&self) -> &godot::obj::Base<godot::classes::Node> {
        &self.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The claim the whole module rests on, because it is the only one anybody
    /// here can check: the same sound, every time.
    ///
    /// Nobody working on this repository can hear the audio, so tonal quality is
    /// Noah's verdict alone. What that leaves testable is everything structural —
    /// and if generation were not reproducible, none of the checks below would be
    /// testing what ships.
    #[test]
    fn a_generated_voice_is_the_same_every_time() {
        for voice in Voice::ALL {
            let once = generate(voice);
            let twice = generate(voice);
            assert_eq!(
                once,
                twice,
                "{} came out differently on a second generation",
                voice.name()
            );
            // The premise. Two empty buffers are also equal, and a synthesiser
            // that returned silence would satisfy every other test in this file.
            assert!(
                once.iter().any(|&s| s != 0),
                "{} generated pure silence",
                voice.name()
            );
        }
    }

    /// Every voice reaches its intended level and none of them clips.
    ///
    /// Clipping is the one audio fault that is unambiguously a fault rather than
    /// a matter of taste, so it is worth a test even though the levels themselves
    /// are not.
    #[test]
    fn every_voice_is_audible_and_none_of_it_clips() {
        for voice in Voice::ALL {
            let samples = generate(voice);
            let peak = samples.iter().map(|s| s.abs() as i32).max().unwrap_or(0);
            let intended = (peak_for(voice) * f64::from(i16::MAX)) as i32;

            assert!(
                peak <= i32::from(i16::MAX),
                "{} clips at {peak}",
                voice.name()
            );
            // Within a percent of the target. Normalisation is exact; the
            // tolerance is for the float-to-int truncation.
            assert!(
                (peak - intended).abs() < intended / 100 + 2,
                "{} peaks at {peak} where {intended} was intended",
                voice.name()
            );
        }
    }

    /// A loop has to join to itself without a step, or it clicks once a cycle.
    ///
    /// The step from the last sample back to the first is what the ear hears as a
    /// click. Measured against the material's own typical sample-to-sample
    /// movement rather than against an absolute figure: a wind bed moves slowly
    /// and a diesel engine moves fast, so one threshold for both would either
    /// pass everything or fail the engine for being an engine.
    #[test]
    fn every_loop_joins_to_itself() {
        // Collected rather than asserted per voice, so a failure names every
        // loop that clicks instead of only the first. That mattered while
        // calibrating this: removing the crossfade fails all five, and a
        // fail-fast version reported one and left the other four unproven.
        let mut clicking: Vec<String> = Vec::new();
        for voice in Voice::ALL.iter().copied().filter(|v| v.loops()) {
            let samples = generate(voice);
            assert!(samples.len() > 2, "{} is too short to loop", voice.name());

            let typical = samples
                .windows(2)
                .map(|w| (i32::from(w[1]) - i32::from(w[0])).abs())
                .max()
                .unwrap_or(0);
            let seam_step = (i32::from(samples[0]) - i32::from(samples[samples.len() - 1])).abs();

            if seam_step > typical {
                clicking.push(format!(
                    "{} steps {seam_step} at the seam against {typical} anywhere else",
                    voice.name()
                ));
            }
        }
        assert!(
            clicking.is_empty(),
            "these loops step further at the seam than they ever do inside \
             themselves, which is an audible click once a cycle: {}",
            clicking.join("; ")
        );
    }

    /// A one-shot ends at silence, or stopping it is itself a click.
    #[test]
    fn every_one_shot_ends_quietly() {
        for voice in Voice::ALL.iter().copied().filter(|v| !v.loops()) {
            let samples = generate(voice);
            let last = samples[samples.len() - 1].abs();
            let peak = samples.iter().map(|s| s.abs()).max().unwrap_or(0);
            assert!(
                i32::from(last) * 50 < i32::from(peak).max(1),
                "{} ends at {last} against a peak of {peak}",
                voice.name()
            );
        }
    }

    /// Every voice has a bus, and every bus is one the settings screen sets.
    ///
    /// A voice on a bus nothing controls is a sound the player cannot turn down,
    /// which on a game with ambience running permanently is the difference
    /// between shipping and not.
    #[test]
    fn every_voice_lands_on_a_bus_the_player_controls() {
        let settings = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(|p| p.parent())
                .expect("the shell crate sits two levels under the repository root")
                .join("godot/settings_store.gd"),
        )
        .expect("the settings store");

        for voice in Voice::ALL {
            let bus = voice.bus();
            assert!(
                settings.contains(&format!("\"{bus}\"")),
                "{} plays on the {bus} bus and settings_store.gd never names it, \
                 so nothing on the settings screen can turn it down",
                voice.name()
            );
        }
    }

    /// Every periodic component of a loop closes across the loop's length.
    ///
    /// This replaced a test that checked one frequency of one voice, and the
    /// widening was prompted by measurement rather than tidiness. Removing the
    /// crossfade fails wind and snow and leaves **machinery and engine passing**,
    /// and the reason those two survive is exactly this property: every tone in
    /// them completes a whole number of cycles, so they join with no help at all.
    ///
    /// Which means the crossfade is not what keeps them clean, and a frequency
    /// changed from 48 Hz to 47 Hz would break them in a way
    /// `every_loop_joins_to_itself` would report as a seam problem and send
    /// somebody to look at the crossfade. Guarding the class rather than the one
    /// instance is what makes the failure point at the cause.
    ///
    /// The seam test cannot cover this. A crossfade hides a step; it does not hide
    /// a rhythm that stutters, because the stutter is a whole cycle wide.
    #[test]
    fn every_tone_in_a_loop_completes_whole_cycles() {
        // The premise: the table must actually describe the looping voices, or
        // this passes by having nothing to check. Rain is the deliberate
        // exception and is named as such.
        let described: Vec<Voice> = TONES.iter().map(|(v, _)| *v).collect();
        for voice in Voice::ALL.iter().copied().filter(|v| v.loops()) {
            assert!(
                described.contains(&voice) || voice == Voice::Rain,
                "{} loops and TONES does not describe it, so nothing checks \
                 whether its components close",
                voice.name()
            );
        }

        let mut fractional: Vec<String> = Vec::new();
        for (voice, tones) in TONES {
            for &hz in *tones {
                let cycles = voice.seconds() * hz;
                if (cycles - cycles.round()).abs() > 1e-9 {
                    fractional.push(format!(
                        "{} at {hz} Hz is {cycles} cycles over {} s",
                        voice.name(),
                        voice.seconds()
                    ));
                }
            }
        }
        assert!(
            fractional.is_empty(),
            "these components do not close across their loop, so the loop \
             stutters every time it wraps: {}",
            fractional.join("; ")
        );
    }
}
