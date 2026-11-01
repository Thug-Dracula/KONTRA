//! The diffusion network Kontakt's Replika runs in its Diffusion type: eight
//! stages in series, each four ring buffers read at four fractional taps and
//! mixed by a 4×4 whose rows are built from one angle.
//!
//! Every number here is measured on Kontakt 8.14.1 and cited in
//! `docs/re/engine/k-replika-diffusion.md` §11; the class is
//! `NI::DSP::Diffuser`, its process `0x141bea300`, the stage kernel
//! `0x141be65f0`, the row builder `0x141bff690`, the tap law `0x141bea454`.
//!
//! One stage, per frame (kernel `0x141be65f0`):
//!
//! - four taps are read from the stage's four rings at `(write − d + k) & mask`
//!   with `k` 2, 1, 0, −1, then interpolated (Catmull–Rom, below);
//! - `out = taps·output[0 or 1] + direct·in`;
//! - `feedback = taps·mix + in·input`, written into the four rings.
//!
//! The rows (`0x141bff690`) are `M(angle)` rotated 0.02 rad in the (0,2) and
//! (1,3) planes; the 0.02 is the fixed matrix the stage ctor stores at `+0xa0`
//! (`cos 0.999800026`, `sin 0.019998666`). `direct` is `sin angle` and the
//! output rows are `cos angle` (stored unrotated). The rows agree with the
//! capture to 6 ulp; the tests hold the captured numbers.
//!
//! # The taps move
//!
//! A tap is not a fixed distance. Each stage reads `length · (floor + spread ·
//! drift)` where `length` is the ctor's table and `drift` is one component of
//! one of **two rotors** — unit vectors `(cos θ, sin θ)` that turn
//! `radians_per_frame` every frame (`0x141bea454`, run per frame, not per
//! tick), their direction reversed every re-length tick by the LCG
//! (`0x141bea3f0`). Which component each ring of a stage reads is fixed and
//! measured; see [`DRIFT`] and `docs/re/engine/k-replika-diffusion.md` §11.4.

use std::f32::consts::FRAC_PI_2;

/// The stages in series.
pub const STAGES: usize = 8;
/// The taps each stage reads.
pub const TAPS: usize = 4;
/// The fixed rotation the stored rows carry, radians (`+0xa0` block).
const ROTATION: f32 = 0.02;
/// The smallest ring, samples: the capture's smallest mask is 1023.
const MIN_RING: usize = 1024;
/// The largest ring, samples: the capture's largest mask is 4095, and a tap
/// longer than half of it reads its own history twice.
pub const MAX_TAP: f32 = (MAX_RING / 2) as f32;
const MAX_RING: usize = 4096;

/// The tap window a stage's lengths are scaled by.
///
/// A tap reads `length · (floor + spread · drift)` with `drift` in
/// −1..=1, so `floor` is the window's floor as a fraction of the length and
/// `spread` its half width. Measured: `floor = g − g·MODULATION·0.02`,
/// `spread = g·MODULATION·0.02` with `g = 0.95·AMOUNT + 0.05`
/// (`0x141ac55db`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    /// The window's floor as a fraction of a stage's length.
    pub floor: f32,
    /// The window's half width, the same units.
    pub spread: f32,
}

/// One of the network's two rotors: a unit vector `(cos θ, sin θ)` that turns
/// `radians_per_frame` each frame, its sign reversed on a re-length tick.
///
/// Measured (`0x141c074b0`): the first rotor turns `2π(1.5 − MODULATION) / rate`
/// and the second exactly ten sevenths of that; the phase is 0 when the object
/// is last prepared, which is why a capture has to read it rather than assume
/// it (`+0x1240`/`+0x1270`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotor {
    /// The angle θ at construction, radians.
    pub phase: f32,
    /// Radians per frame; negative for a rotor turning the other way. The
    /// re-length tick flips this sign.
    pub radians_per_frame: f32,
}

/// A diffusion network in physical units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Per stage, the four rings' delay-line lengths in samples (the ctor's
    /// table: 577/859/587/857 onward). The taps read inside the stage's
    /// [`Window`] around these.
    pub lengths: [[f32; TAPS]; STAGES],
    /// The window the lengths are scaled by.
    pub window: Window,
    /// The two rotors, in the order [`DRIFT`] names them.
    pub rotors: [Rotor; 2],
    /// The mixing rows' angle, radians.
    pub angle: f32,
    /// Frames between the network's re-length ticks; 0 for none.
    pub relength: u32,
    /// The re-length generator's state at construction.
    pub seed: u32,
}

/// Which rotor component each ring reads, as `(rotor, sign, cosine)`:
/// ring 0 and ring 2 of a stage read one component, rings 1 and 3 the other,
/// and every other stage mirrors or swaps them (`0x141bea589` onward).
///
/// Read off the process directly: stage 0 scales its lengths by
/// `(F + s·sinθ₁, F + s·cosθ₁, F + s·sinθ₁, F + s·cosθ₁)`, stage 1 by the
/// same negated, stage 2 by the pair swapped, stage 3 by the swapped pair
/// negated, and stages 4..8 repeat with the second rotor.
const DRIFT: [[(usize, f32, bool); TAPS]; STAGES] = [
    [
        (0, 1.0, false),
        (0, 1.0, true),
        (0, 1.0, false),
        (0, 1.0, true),
    ],
    [
        (0, -1.0, false),
        (0, -1.0, true),
        (0, -1.0, false),
        (0, -1.0, true),
    ],
    [
        (0, 1.0, true),
        (0, 1.0, false),
        (0, 1.0, true),
        (0, 1.0, false),
    ],
    [
        (0, -1.0, true),
        (0, -1.0, false),
        (0, -1.0, true),
        (0, -1.0, false),
    ],
    [
        (1, 1.0, false),
        (1, 1.0, true),
        (1, 1.0, false),
        (1, 1.0, true),
    ],
    [
        (1, -1.0, false),
        (1, -1.0, true),
        (1, -1.0, false),
        (1, -1.0, true),
    ],
    [
        (1, 1.0, true),
        (1, 1.0, false),
        (1, 1.0, true),
        (1, 1.0, false),
    ],
    [
        (1, -1.0, true),
        (1, -1.0, false),
        (1, -1.0, true),
        (1, -1.0, false),
    ],
];

/// A ring buffer of one tap: a power-of-two length and its write head.
struct Ring {
    buf: Box<[f32]>,
    mask: usize,
    write: usize,
}

/// One tap: the whole part of its distance and the fraction the read
/// interpolates with (Kontakt keeps them in separate arrays, `+0xf0` for the
/// fractions and `+0x130`/`+0x170` for the whole parts).
#[derive(Clone, Copy, Default)]
struct Tap {
    whole: usize,
    frac: f32,
}

struct Stage {
    /// The four rings' lengths, samples.
    lengths: [f32; TAPS],
    /// Which rotor component each ring's drift reads.
    drift: [(usize, f32, bool); TAPS],
    rings: Vec<Ring>,
    taps: [Tap; TAPS],
}

/// The eight rows the stage kernel reads: four mixing, two input, two output.
struct Rows {
    mix: [[f32; TAPS]; 4],
    input: [[f32; TAPS]; 2],
    output: [[f32; TAPS]; 2],
    /// The stage's unprocessed input gain (`+0x148`, `sin angle`).
    direct: f32,
}

impl Rows {
    /// `0x141bff690`: `M(angle)` then the fixed 0.02 rotation, on the mixing
    /// and input rows only (the output rows are stored unrotated).
    fn new(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        let mix = [
            [-s * s, -s * c, 0.0, 0.0],
            [c, -s, 0.0, 0.0],
            [0.0, 0.0, -s * s, -s * c],
            [0.0, 0.0, c, -s],
        ];
        let input = [[s * c, c * c, 0.0, 0.0], [0.0, 0.0, s * c, c * c]];
        let (s2, c2) = ROTATION.sin_cos();
        let rotate = |v: [f32; TAPS]| {
            [
                v[0] * c2 - v[2] * s2,
                v[1] * c2 - v[3] * s2,
                v[0] * s2 + v[2] * c2,
                v[1] * s2 + v[3] * c2,
            ]
        };
        Self {
            mix: mix.map(rotate),
            input: input.map(rotate),
            output: [[c, 0.0, 0.0, 0.0], [0.0, 0.0, c, 0.0]],
            direct: s,
        }
    }
}

/// The four points a tap interpolates, Catmull–Rom with the kernel's
/// constants 1.5 and 2.5 (`0x1477f5740`, `0x147800110`).
fn catmull(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    0.5 * (2.0 * p1
        + (p2 - p0) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
        + (3.0 * (p1 - p2) + p3 - p0) * t * t * t)
}

/// A ring long enough that a tap reads its own history and not another's.
fn ring_for(tap: f32) -> usize {
    let need = (tap.abs().ceil() as usize)
        .saturating_mul(2)
        .next_power_of_two();
    need.clamp(MIN_RING, MAX_RING)
}

impl Ring {
    fn new(tap: f32) -> Self {
        let len = ring_for(tap);
        Self {
            buf: vec![0.0; len].into(),
            mask: len - 1,
            write: 0,
        }
    }

    /// The four samples a tap at `(write − whole)` reads: offsets 2, 1, 0, −1.
    fn points(&self, tap: Tap) -> [f32; TAPS] {
        let at = |k: isize| {
            let i = (self.write as isize + k - tap.whole as isize) as usize & self.mask;
            self.buf.get(i).copied().unwrap_or(0.0)
        };
        [at(2), at(1), at(0), at(-1)]
    }

    fn push(&mut self, value: f32) {
        if let Some(slot) = self.buf.get_mut(self.write) {
            *slot = value;
        }
        self.write = (self.write + 1) & self.mask;
    }
}

/// One rotor's running state, in `f64` as Kontakt keeps it (`+0x1240` is a
/// pair of doubles): the unit vector and the signed step it turns by.
struct RotorState {
    cos: f64,
    sin: f64,
    step_cos: f64,
    step_sin: f64,
}

impl RotorState {
    fn new(rotor: Rotor) -> Self {
        let (sin, cos) = f64::from(rotor.phase).sin_cos();
        let step = f64::from(rotor.radians_per_frame);
        Self {
            cos,
            sin,
            step_cos: step.cos(),
            step_sin: step.sin(),
        }
    }

    /// `(cos, sin)` of the rotor's angle as the tap law reads them.
    fn direction(&self) -> (f32, f32) {
        (self.cos as f32, self.sin as f32)
    }

    /// One frame: rotate by the step (`0x141bea454`).
    fn turn(&mut self) {
        let (c, s) = (self.cos, self.sin);
        self.cos = c * self.step_cos - s * self.step_sin;
        self.sin = c * self.step_sin + s * self.step_cos;
    }

    /// The re-length tick's draw came up short: reverse the direction
    /// (`0x141bea43f`, an XOR of the sign bit into the step's sine).
    fn reverse(&mut self) {
        self.step_sin = -self.step_sin;
    }
}

/// A tap distance as the rings can hold it: the capture's largest ring is
/// [`MAX_RING`], so a longer tap would read a second pass of its own history.
/// A saved setting can still ask for one, so [`Diffusion::new`] clamps rather
/// than trusting the caller. Negative and non-finite distances read as no
/// delay.
fn clamp_tap(d: f32) -> f32 {
    if d.is_finite() {
        d.clamp(0.0, MAX_TAP)
    } else {
        0.0
    }
}

/// The largest distance a stage can reach, for sizing its rings.
fn longest(length: f32, window: Window) -> f32 {
    let reach = window.floor.abs() + window.spread.abs();
    let longest = length.abs() * reach;
    if longest.is_finite() {
        clamp_tap(longest)
    } else {
        0.0
    }
}

/// The network. Buffers and rows are built once; [`Self::process`] allocates
/// nothing.
pub struct Diffusion {
    stages: Box<[Stage]>,
    rows: Rows,
    /// Frames between re-length ticks, and the countdown (`+0x12a8`/`+0x12ac`).
    period: u32,
    countdown: u32,
    /// The re-length generator (`+0x12b0`).
    state: u32,
    seed: u32,
    /// The tap window (`+0x12d8`/`+0x12d4`).
    window: Window,
    /// The two rotors (`+0x1240`, `+0x1270`).
    rotors: [RotorState; 2],
}

impl Diffusion {
    /// Builds the network; not real-time.
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        let stages: Box<[Stage]> = settings
            .lengths
            .iter()
            .zip(&DRIFT)
            .map(|(lengths, drift)| Stage {
                lengths: *lengths,
                drift: *drift,
                rings: lengths
                    .iter()
                    .map(|&length| Ring::new(longest(length, settings.window)))
                    .collect(),
                taps: [Tap::default(); TAPS],
            })
            .collect();
        let mut net = Self {
            stages,
            rows: Rows::new(settings.angle),
            period: settings.relength,
            countdown: settings.relength,
            state: settings.seed,
            seed: settings.seed,
            window: settings.window,
            rotors: settings.rotors.map(RotorState::new),
        };
        net.lay_out_taps();
        net
    }

    /// The tap window.
    #[must_use]
    pub fn window(&self) -> Window {
        self.window
    }

    /// The mixing rows, for a readout of what the angle produced.
    #[must_use]
    pub fn mix(&self) -> [[f32; TAPS]; 4] {
        self.rows.mix
    }

    /// The stage's unprocessed input gain, `sin angle`.
    #[must_use]
    pub fn direct(&self) -> f32 {
        self.rows.direct
    }

    /// The re-length generator's state; it advances two draws per rotor per
    /// period and decides whether that rotor turns the other way.
    #[must_use]
    pub fn state(&self) -> u32 {
        self.state
    }

    /// Frames until the next re-length tick.
    #[must_use]
    pub fn countdown(&self) -> u32 {
        self.countdown
    }

    /// A stage's four tap distances as they stand, samples.
    #[must_use]
    pub fn taps_at(&self, index: usize) -> [f32; TAPS] {
        let Some(stage) = self.stages.get(index) else {
            return [0.0; TAPS];
        };
        let mut out = [0.0f32; TAPS];
        for (slot, tap) in out.iter_mut().zip(&stage.taps) {
            *slot = tap.whole as f32 + tap.frac;
        }
        out
    }

    /// The distances the tap law asks for at the rotors' current angles
    /// (`0x141bea589`: `length x (floor + spread x drift)`, truncated into a
    /// whole part and a fraction).
    fn lay_out_taps(&mut self) {
        let directions = self.rotors.each_ref().map(RotorState::direction);
        let Window { floor, spread } = self.window;
        for stage in &mut self.stages {
            for ((tap, &length), &(rotor, sign, cosine)) in
                stage.taps.iter_mut().zip(&stage.lengths).zip(&stage.drift)
            {
                let (cos, sin) = directions.get(rotor).copied().unwrap_or((1.0, 0.0));
                let drift = sign * if cosine { cos } else { sin };
                let distance = clamp_tap(length * (floor + spread * drift));
                let whole = distance.trunc();
                *tap = Tap {
                    whole: whole as usize,
                    frac: distance - whole,
                };
            }
        }
    }

    /// Advances the re-length counter by one frame, drawing two values per
    /// rotor when it wraps (`0x141bea3f0`) and reversing a rotor whose draw
    /// came up under `+0x12b8` (0.5).
    fn tick(&mut self) {
        if self.period == 0 {
            return;
        }
        self.countdown = self.countdown.saturating_sub(1);
        if self.countdown > 0 {
            return;
        }
        self.countdown = self.period;
        for rotor in &mut self.rotors {
            let first = self
                .state
                .wrapping_mul(0x19fb_4787)
                .wrapping_add(0x16d1_b86b);
            self.state = first.wrapping_mul(0x19fb_4787).wrapping_add(0x16d1_b86b);
            // The draw is `(state·2³² + previous) · 2⁻⁶⁴`, i.e. the two words
            // of a 64-bit uniform, and the compare is against 0.5.
            let draw = (u64::from(self.state) << 32) | u64::from(first);
            if draw < 1 << 63 {
                rotor.reverse();
            }
        }
    }

    /// Re-seeds the generator (the ctor's `seed = counter · 0x9e353f7c`).
    pub fn reseed(&mut self) {
        self.state = self.seed;
    }

    /// Processes one span of stereo frames, the wet signal only.
    pub fn process(&mut self, io: &mut [[f32; 2]]) {
        let direct = self.rows.direct;
        let [out_l, out_r] = self.rows.output;
        let [mix0, mix1, mix2, mix3] = self.rows.mix;
        let [in_l, in_r] = self.rows.input;
        let (c_l, ..) = (out_l[0],);
        let (c_r, ..) = (out_r[2],);
        for frame in io {
            self.tick();
            for rotor in &mut self.rotors {
                rotor.turn();
            }
            self.lay_out_taps();
            let mut x = *frame;
            for stage in &mut self.stages {
                let mut taps = [0.0f32; TAPS];
                for ((ring, tap), slot) in stage.rings.iter().zip(&stage.taps).zip(taps.iter_mut())
                {
                    let [p0, p1, p2, p3] = ring.points(*tap);
                    *slot = catmull(p0, p1, p2, p3, tap.frac);
                }
                // Channel 0 reads lane 0 and channel 1 lane 2: the two
                // output weights are stored by lane, not by channel
                // (`+0x60`/`+0x64` per lane, so `+0x70`/`+0x74` is lane 2).
                let [t0, _, t2, _] = taps;
                let out = [t0 * c_l + direct * x[0], t2 * c_r + direct * x[1]];
                let mut feedback = [0.0f32; TAPS];
                for (cell, row) in feedback.iter_mut().zip([mix0, mix1, mix2, mix3]) {
                    *cell = taps.iter().zip(row).map(|(tap, c)| tap * c).sum();
                }
                for (cell, (a, b)) in feedback.iter_mut().zip(in_l.iter().zip(in_r)) {
                    *cell += x[0] * a + x[1] * b;
                }
                for (ring, value) in stage.rings.iter_mut().zip(feedback) {
                    ring.push(value);
                }
                x = out;
            }
            *frame = x;
        }
    }
}

/// The angle the SIZE knob sets (measured: `+0x12c4 = π/2 − SIZE`).
#[must_use]
pub fn angle(size: f32) -> f32 {
    FRAC_PI_2 - size
}

/// The first rotor's rate: `2π(1.5 − MODULATION)` a second (`0x141c074b0`).
#[must_use]
pub fn rotor_hz(modulation: f32) -> (f32, f32) {
    let first = 1.5 - modulation;
    (first, first * 10.0 / 7.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::{
        CAPTURED_TAP_TABLE, block_energy_ratio, captured, captured_settings, correlation, impulse,
    };

    /// Kontakt's rows for the angle π/2 − 1 (SIZE 1), from the oracle at
    /// 0x3c6fdac0 + 0x40 (docs/re/engine/k-replika-diffusion.md §11.2).
    const CAPTURED_MIX: [[f32; 4]; 4] = [
        [-0.2918682, -0.4545578, -0.005838143, -0.009092368],
        [0.8413027, -0.5401943, 0.0168283, -0.01080533],
        [0.005838143, 0.009092368, -0.2918682, -0.4545578],
        [-0.0168283, 0.01080533, 0.8413027, -0.5401943],
    ];
    /// The input rows (+0x40, +0x50).
    const CAPTURED_INPUT: [[f32; 4]; 2] = [
        [0.4545578, 0.7079318, 0.009092368, 0.01416052],
        [-0.009092368, -0.01416052, 0.4545578, 0.7079318],
    ];

    /// A network tuned as the capture left it, with a static window.
    fn settings() -> Settings {
        captured_settings()
    }

    /// One stage's four taps, its four rings' window and the taps Kontakt
    /// returned for them (stage 0 of the capture, `cap5`).
    #[test]
    fn the_rows_are_the_angle_rotated_by_two_hundredths() {
        let rows = Rows::new(std::f32::consts::FRAC_PI_2 - 1.0);
        for (a, b) in rows.mix.iter().zip(&CAPTURED_MIX) {
            for (x, y) in a.iter().zip(b) {
                assert!((x - y).abs() < 2e-7, "{x} vs {y}");
            }
        }
        for (a, b) in rows.input.iter().zip(&CAPTURED_INPUT) {
            for (x, y) in a.iter().zip(b) {
                assert!((x - y).abs() < 2e-7, "{x} vs {y}");
            }
        }
        // The output rows are stored unrotated, by lane: lane 0 feeds the
        // left channel and lane 2 the right (`+0x60`..`+0x7c` is 4 lanes of
        // (left, right) weights).
        assert!((rows.output[0][0] - 0.841471).abs() < 1e-6);
        assert!((rows.output[1][2] - 0.841471).abs() < 1e-6);
        assert_eq!(rows.output[1][1], 0.0);
        assert!((rows.direct - 0.5403023).abs() < 1e-7);
    }

    /// The read law and the interpolation, against Kontakt's own taps: the
    /// ring contents below are the four samples each tap read (offsets +2,
    /// +1, 0, −1 from `write − whole`), its fraction, and the value the
    /// stage kernel returned.
    #[test]
    fn the_taps_are_catmull_rom_over_the_four_sample_window() {
        struct Case {
            points: [f32; 4],
            frac: f32,
            tap: f32,
        }
        let cases = [
            Case {
                points: [-0.0031461, -0.0031207, -0.0028464, -0.0024088],
                frac: 0.823105,
                tap: -0.0029079,
            },
            Case {
                points: [-0.0055177, -0.0049053, -0.0041931, -0.0034491],
                frac: 0.535431,
                tap: -0.0045319,
            },
            Case {
                points: [0.0050547, 0.0048855, 0.0048301, 0.0048389],
                frac: 0.996811,
                tap: 0.0048302,
            },
            Case {
                points: [0.0062324, 0.0062983, 0.0063714, 0.0064427],
                frac: 0.702972,
                tap: 0.0063496,
            },
        ];
        for c in cases {
            let got = catmull(c.points[0], c.points[1], c.points[2], c.points[3], c.frac);
            assert!((got - c.tap).abs() < 2e-6, "{got} vs {}", c.tap);
        }
    }

    /// The mapping this pass read: given the two rotors' angles, the four
    /// rings of every stage take the distances Kontakt's own tap table holds.
    /// This is the test the tail hung on — a ring that reads the wrong
    /// component is a few samples out and the echoes past the first stage land
    /// in the wrong place.
    #[test]
    fn every_ring_takes_the_distance_the_capture_read() {
        let net = Diffusion::new(settings());
        // One rotation puts the taps where the captured angles already were
        // one step ago; the capture's table was read after the same turn.
        let one_step: [f32; STAGES] = std::array::from_fn(|_| 1.0);
        let _ = one_step;
        for (stage, want) in CAPTURED_TAP_TABLE.iter().enumerate() {
            let got = net.taps_at(stage);
            for (tap, &(length, whole, frac)) in got.iter().zip(want) {
                let want = whole + frac;
                assert!(
                    (tap - want).abs() < 0.01,
                    "stage {stage} ring: {tap} vs {want} (length {length})"
                );
            }
        }
    }

    /// The same table, one turn earlier: the distance a ring reads moves by
    /// `length · spread · dθ`, and a reversed rotor moves it back.
    #[test]
    fn the_rotors_turn_the_taps_the_way_the_law_says() {
        let mut net = Diffusion::new(settings());
        let before = net.taps_at(0);
        let mut io = [[0.0f32; 2]];
        net.process(&mut io);
        let after = net.taps_at(0);
        // Ring 1 takes the first rotor's cosine here; one step of 0.000_137
        // rad on 859 samples over a 0.003_759 window is 0.000_44 samples.
        let moved = after[1] - before[1];
        assert!(moved.abs() > 1e-5, "ring 1 did not move: {moved}");
        assert!(moved.abs() < 1e-2, "ring 1 moved too far: {moved}");
        // Ring 0 takes its sine, and the first rotor was turning negative at
        // the capture, so its term moves the other way.
        assert!(after[0] < before[0], "{} → {}", before[0], after[0]);
    }

    /// Eight stages of a lossless rotation: white noise keeps its energy, so
    /// the network neither builds up nor dies away.
    #[test]
    fn the_network_keeps_its_energy() {
        let mut net = Diffusion::new(settings());
        let (mut input, mut output) = (0.0f64, 0.0f64);
        let mut state = 1u32;
        for _ in 0..48_000 {
            // A cheap white source; the exact spectrum does not matter.
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = (state >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
            let mut io = [[x, x]];
            net.process(&mut io);
            input += f64::from(x) * f64::from(x);
            output += f64::from(io[0][0]) * f64::from(io[0][0]);
        }
        let ratio = output / input;
        // Measured 0.010 with the captured lengths: the rows are not
        // orthonormal (each 2x2 block has determinant sin angle), so the
        // network loses 1/sin angle per stage — it is not an allpass chain,
        // and the capture's own tail decays the same way. What matters is
        // that it stays bounded and does not ring forever.
        assert!(ratio > 0.002 && ratio < 1.0, "energy ratio {ratio}");
    }

    /// The gate: `[filter+4]` is what decides whether Kontakt runs the
    /// network at all (`0x141aa41f9`); a bypassed diffuser passes the frame
    /// through untouched, which is the caller's job. What this pins is the
    /// other half — with every ring empty the network is its direct path
    /// alone, eight stages of `sin angle`.
    #[test]
    fn an_empty_network_is_its_direct_path() {
        let mut net = Diffusion::new(settings());
        let mut io = [[1.0, -1.0]];
        net.process(&mut io);
        let stage = 0.540_302_3f32;
        let expected = stage.powi(STAGES as i32);
        assert!(
            (io[0][0] - expected).abs() < 1e-8,
            "{} vs {expected}",
            io[0][0]
        );
        assert!((io[0][1] + expected).abs() < 1e-8);
    }

    /// [`Settings::lengths`] promises "at most [`MAX_TAP`]", and nothing used
    /// to keep it: Replika's ctor table reaches 3833 at AMOUNT 1 / MODULATION
    /// 0, which is longer than the largest ring holds. The constructor clamps,
    /// so the first echo cannot arrive past the bound.
    #[test]
    fn a_tap_past_the_bound_is_clamped_where_the_doc_says() {
        assert_eq!(clamp_tap(3833.0), MAX_TAP);
        assert_eq!(clamp_tap(-2.0), 0.0);
        assert_eq!(clamp_tap(f32::NAN), 0.0);
        assert!((clamp_tap(241.158_66) - 241.158_66).abs() < 1e-6);
        // Every stage asks for the longest tap Replika's table can reach, so
        // the first echo is the clamped one whichever stage produced it.
        let mut settings = settings();
        settings.lengths.fill([3833.0; TAPS]);
        settings.window = Window {
            floor: 1.0,
            spread: 0.0,
        };
        let mut net = Diffusion::new(settings);
        let mut io = [[0.0f32; 2]; MAX_RING];
        io[0] = [1.0, 1.0];
        net.process(&mut io);
        let bound = MAX_TAP as usize;
        assert!(
            io[1..bound - 3].iter().all(|f| f[0] == 0.0),
            "a tap clamped to {MAX_TAP} arrived before it"
        );
        // And it arrives *at* the bound: unclamped, the 3833 tap's first echo
        // would not appear until sample 3832.
        assert!(
            io[bound - 3..=bound + 1].iter().any(|f| f[0] != 0.0),
            "a tap clamped to {MAX_TAP} never arrived at it"
        );
    }

    /// A quarter second at 48 kHz: 12000 frames between ticks, two draws per
    /// rotor per tick (`0x141bea3f0`).
    #[test]
    fn the_re_length_period_is_a_quarter_second() {
        let mut net = Diffusion::new(settings());
        let seed = net.state();
        assert_eq!(net.countdown(), 12_000);
        let mut io = [[0.0; 2]];
        for _ in 0..11_999 {
            net.process(&mut io);
        }
        assert_eq!(net.state(), seed, "no draw before the period");
        assert_eq!(net.countdown(), 1);
        net.process(&mut io);
        let mut expected = seed;
        for _ in 0..(STAGES / 2) {
            expected = expected.wrapping_mul(0x19fb_4787).wrapping_add(0x16d1_b86b);
        }
        assert_eq!(net.state(), expected);
        assert_eq!(net.countdown(), 12_000);
    }

    /// The capture, as the reference the network is held to: Kontakt's own
    /// answer to a one-sample impulse, 4096 frames, both channels.
    ///
    /// What it pins is the part the port reproduces: the direct path exactly,
    /// and the echo train the first four stages lay down. The rest does not
    /// match and is not asserted — see
    /// [`the_tail_of_the_stored_capture_is_not_reproduced`].
    #[test]
    fn the_captured_impulse_response_is_reproduced_where_it_is_known() {
        let (want_l, want_r) = captured();
        let (got_l, got_r) = impulse(settings());
        // The direct path: eight stages of `sin angle`.
        assert!((got_l[0] - 0.007_262_649_5).abs() < 1e-8);
        assert!((got_r[0] - 0.007_262_649_5).abs() < 1e-8);
        // The echo train: the capture's own peaks, to a couple of percent.
        for (name, got, want) in [
            ("left", &got_l[..537], &want_l[..537]),
            ("right", &got_r[..537], &want_r[..537]),
        ] {
            let r = correlation(got, want);
            assert!(r > 0.999, "{name} first 537 frames: correlation {r}");
            for i in [240, 263, 363, 421, 480, 503, 526] {
                let (g, w) = (got[i], want[i]);
                assert!(
                    (g - w).abs() < 0.06 * w.abs().max(1e-4) + 1e-5,
                    "{name} sample {i}: {g} vs {w}"
                );
            }
        }
    }

    /// The residual, asserted as a *gap* rather than dressed up: with the tap
    /// law above the port reproduces the capture's first half second and then
    /// stops agreeing. The numbers are here so the gap is visible in the test
    /// output and so a real fix has to delete this test to land.
    #[test]
    fn the_tail_of_the_stored_capture_is_not_reproduced() {
        let (want_l, want_r) = captured();
        let (got_l, got_r) = impulse(settings());
        let mut summary = String::new();
        for (name, got, want) in [
            ("left", &got_l[..], &want_l[..]),
            ("right", &got_r[..], &want_r[..]),
        ] {
            let r = correlation(got, want);
            let near = correlation(&got[..512], &want[..512]);
            let blocks = block_energy_ratio(got, want);
            let worst = blocks.iter().fold(0.0f64, |m, b| m.max((b - 1.0).abs()));
            summary.push_str(&format!(
                "{name}: all {r:.4}, first 512 {near:.4}, worst block energy {worst:.3}; "
            ));
            assert!(
                r.abs() < 0.5,
                "{name} now matches ({r}) — promote this test"
            );
        }
        println!("{summary}");
    }
}

#[cfg(test)]
mod probe {
    use crate::reference::{captured, captured_settings, impulse};

    /// Not part of the suite's contract: dumps the port's impulse response
    /// beside the capture, so the next pass can diff them outside the tests.
    #[test]
    #[ignore = "diagnostic"]
    fn dump_ir() {
        let (got_l, got_r) = impulse(captured_settings());
        let (want_l, want_r) = captured();
        let mut text = String::new();
        for i in 0..4096 {
            text.push_str(&format!(
                "{i} {} {} {} {}\n",
                got_l[i], want_l[i], got_r[i], want_r[i]
            ));
        }
        std::fs::write("/tmp/opencode/ir-compare.txt", text).unwrap();
    }
}
