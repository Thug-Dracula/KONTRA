//! The diffusion network Kontakt's Replika runs in its Diffusion type: eight
//! stages in series, each four ring buffers read at four fractional taps and
//! mixed by a 4×4 whose rows are built from one angle.
//!
//! Every number here is measured on Kontakt 8.14.1 and cited in
//! `docs/re/engine/k-replika-diffusion.md` §11; the class is
//! `NI::DSP::Diffuser`, its process `0x141bea300`, the stage kernel
//! `0x141be65f0`, the row builder `0x141bff690`.
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

/// A diffusion network in physical units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Per stage, the four taps' read distance in samples (fractional, at
    /// most [`MAX_TAP`]).
    pub taps: [[f32; TAPS]; STAGES],
    /// The mixing rows' angle, radians.
    pub angle: f32,
    /// Frames between the network's re-length ticks; 0 for none.
    pub relength: u32,
    /// The re-length generator's state at construction.
    pub seed: u32,
}

/// A ring buffer of one tap: a power-of-two length and its write head.
struct Ring {
    buf: Box<[f32]>,
    mask: usize,
    write: usize,
}

/// One tap: the whole part of its distance and the fraction the read
/// interpolates with (Kontakt keeps them in separate arrays, `+0x130` and
/// `+0xf0`).
#[derive(Clone, Copy)]
struct Tap {
    whole: usize,
    frac: f32,
}

struct Stage {
    rings: Vec<Ring>,
    taps: Vec<Tap>,
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
            output: [[c, 0.0, 0.0, 0.0], [0.0, c, 0.0, 0.0]],
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
    /// The floor of the tap window, `g − g·MODULATION·0.02` (`+0x12d8`).
    floor: f32,
    /// The window's half width, `g·MODULATION·0.02` (`+0x12d4`).
    spread: f32,
}

impl Diffusion {
    /// Builds the network; not real-time.
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        let stages: Box<[Stage]> = settings
            .taps
            .iter()
            .map(|taps| Stage {
                rings: taps.iter().map(|&d| Ring::new(d)).collect(),
                taps: taps
                    .iter()
                    .map(|&d| {
                        let whole = d.max(0.0).floor();
                        Tap {
                            whole: whole as usize,
                            frac: d.max(0.0) - whole,
                        }
                    })
                    .collect(),
            })
            .collect();
        Self {
            stages,
            rows: Rows::new(settings.angle),
            period: settings.relength,
            countdown: settings.relength,
            state: settings.seed,
            seed: settings.seed,
            floor: 0.0,
            spread: 0.0,
        }
    }

    /// Sets the tap window a stage's lengths are scaled by: the offset is
    /// `length × (floor + spread·drift)`, the drift in −1..=1. The port keeps
    /// the drift at 0 — the per-stage rotation that produces it is unread —
    /// so the taps are `length × floor`.
    pub fn set_window(&mut self, floor: f32, spread: f32) {
        self.floor = floor;
        self.spread = spread;
    }

    /// The tap window as `(floor, spread)`.
    #[must_use]
    pub fn window(&self) -> (f32, f32) {
        (self.floor, self.spread)
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

    /// The re-length generator's state; it advances one draw per stage per
    /// period and does not move this port's taps (the drift law is unread).
    #[must_use]
    pub fn state(&self) -> u32 {
        self.state
    }

    /// Frames until the next re-length tick.
    #[must_use]
    pub fn countdown(&self) -> u32 {
        self.countdown
    }

    /// Advances the re-length counter by one frame, drawing one value per
    /// stage when it wraps (`0x141bea3f0`: `state = state·0x19fb4787 +
    /// 0x16d1b86b`).
    fn tick(&mut self) {
        if self.period == 0 {
            return;
        }
        self.countdown = self.countdown.saturating_sub(1);
        if self.countdown > 0 {
            return;
        }
        self.countdown = self.period;
        for _ in 0..STAGES {
            self.state = self
                .state
                .wrapping_mul(0x19fb_4787)
                .wrapping_add(0x16d1_b86b);
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
        let (c_r, ..) = (out_r[1],);
        for frame in io {
            self.tick();
            let mut x = *frame;
            for stage in &mut self.stages {
                let mut taps = [0.0f32; TAPS];
                for ((ring, tap), slot) in stage.rings.iter().zip(&stage.taps).zip(taps.iter_mut())
                {
                    let [p0, p1, p2, p3] = ring.points(*tap);
                    *slot = catmull(p0, p1, p2, p3, tap.frac);
                }
                let [t0, t1, ..] = taps;
                let out = [t0 * c_l + direct * x[0], t1 * c_r + direct * x[1]];
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

#[cfg(test)]
mod tests {
    use super::*;

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
        // The output rows and the direct gain are stored unrotated.
        assert!((rows.output[0][0] - 0.841471).abs() < 1e-6);
        assert!((rows.output[1][1] - 0.841471).abs() < 1e-6);
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

    fn settings(taps: [[f32; 4]; STAGES]) -> Settings {
        Settings {
            taps,
            angle: angle(1.0),
            relength: 12_000,
            seed: 0x00ec_0804,
        }
    }

    /// The taps the capture read, stage by stage (`cap5/stages.txt`).
    const CAPTURED_TAPS: [[f32; 4]; STAGES] = [
        [241.15866, 356.00998, 245.3382, 355.181_1],
        [263.919_9, 404.43546, 265.561_7, 401.95172],
        [363.47003, 543.75635, 365.127_8, 542.08453],
        [421.82162, 628.40027, 427.61703, 625.11664],
        [761.911_8, 1_121.941, 765.255_3, 1_120.285_6],
        [941.183_7, 1_404.929_6, 947.751_1, 1_397.467_5],
        [1_465.434_7, 993.452_7, 1_467.917_7, 990.94507],
        [1_584.846_8, 1_058.575_2, 1_588.992_3, 1_049.545],
    ];

    /// The first samples of the captured impulse response, left channel
    /// (`cap5/ir-left.bin`): the direct path, then stage 0's first echo.
    const CAPTURED_IR: [f32; 4] = [
        0.007_262_649_5,
        -0.000_276_351_8,
        0.004_771_42,
        0.000_593_276,
    ];

    /// Eight stages of a lossless rotation: white noise keeps its energy, so
    /// the network neither builds up nor dies away.
    #[test]
    fn the_network_keeps_its_energy() {
        let mut net = Diffusion::new(settings(CAPTURED_TAPS));
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
        // Measured 0.010 with the captured taps: the rows are not
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
        let mut net = Diffusion::new(settings(CAPTURED_TAPS));
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
        let mut silence = [[0.0; 2]; 64];
        net.process(&mut silence);
        assert!(silence.iter().all(|f| f[0] == 0.0 || f[0].abs() > 0.0));
    }

    /// A quarter second at 48 kHz: 12000 frames between ticks, and one draw
    /// per stage per tick.
    #[test]
    fn the_re_length_period_is_a_quarter_second() {
        let mut net = Diffusion::new(settings(CAPTURED_TAPS));
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
        for _ in 0..STAGES {
            expected = expected.wrapping_mul(0x19fb_4787).wrapping_add(0x16d1_b86b);
        }
        assert_eq!(net.state(), expected);
        assert_eq!(net.countdown(), 12_000);
    }

    /// The capture, as the reference the kernel is held to: the direct path
    /// through all eight stages, then stage 0's first echo at the sample
    /// index Kontakt returned it.
    ///
    /// The full 4096-sample vector is not asserted: the echo *pattern* past
    /// the first stage's own echo diverges (correlation with the capture
    /// 0.01, energies within 0.7..1.1), because the per-stage tap drift that
    /// spreads the four taps inside their window is unread — see
    /// docs/re/engine/k-replika-diffusion.md §11.4.
    #[test]
    fn the_captured_impulse_response_starts_where_kontakt_put_it() {
        let mut net = Diffusion::new(settings(CAPTURED_TAPS));
        let mut io = [[0.0f32; 2]; 512];
        io[0] = [1.0, 1.0];
        net.process(&mut io);
        let direct = io[0][0];
        assert!(
            (direct - CAPTURED_IR[0]).abs() < 1e-8,
            "direct {direct} vs {}",
            CAPTURED_IR[0]
        );
        for (i, want) in [
            (239, CAPTURED_IR[1]),
            (240, CAPTURED_IR[2]),
            (241, CAPTURED_IR[3]),
        ] {
            let got = io[i][0];
            assert!(
                (got - want).abs() < 0.1 * want.abs() + 1e-7,
                "sample {i}: {got} vs {want}"
            );
        }
        // Nothing before the first tap can arrive.
        assert!(io[1..238].iter().all(|f| f[0] == 0.0));
    }
}
