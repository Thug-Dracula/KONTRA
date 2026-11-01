//! Choral (Kontakt module `0x5d`): a seven-voice modulated delay bank.
//!
//! The bank is Kontakt's, recovered from the 8.13.1 initialiser and read back
//! from the emulator (`docs/re/engine/k-choral-modes.md`): seven base delays,
//! seven gains and seven pans per mode, with mode 4 the same as mode 3.
//!
//! Measured laws (emulator, 2026-10-30):
//! - a voice's delay is `base × (delay · 5 + 0.1)` seconds. Checked against the
//!   first tap at delay 0.5 (374 frames predicted 374) and 1.0 (734 vs 734).
//! - the LFO is a triangle, and its rate is `0.25 + 7.4 · rate³` Hz
//!   ([`lfo_hz`], seven rates checked exactly).
//! - its per-voice phase offset is the mode table's `pan` field
//!   ([`Voice::pan`]), read as the offset in `voice+(-0x158)`.
//! - `amount` sets the modulation depth in samples as
//!   `(fs / hz) · ½ · (2^(0.025 · m) − 1)`, `m = 0.99 · amount² + 0.01`
//!   ([`modulation_depth`]); measured 3.54 / 25.46 / 91.35 / 201.63 / 357.03
//!   samples at amount 0 / .25 / .5 / .75 / 1, rate 0.5, 48 kHz.
//!
//! [`Chorus`] is the kernel at `0x14193be50`: per frame a seven-tap cyclic mix
//! of the previous frame's reads, then per voice a two-pole loop filter
//! (`0x14193c300`) written into a ring, an LFO with a smoother, a Catmull-Rom
//! read at the modulated delay, a width rotation and the voice gains, then a
//! global two-pole tone filter and the dry/wet mix. Every coefficient below
//! cites the address it was read from or the measurement that fixed it; the
//! full account is in `docs/re/engine/k-choral-modes.md`.

/// One voice of a mode's bank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voice {
    /// Base delay in seconds, before the `delay` parameter scales it
    /// (bank `+0x00`).
    pub base_s: f32,
    /// The voice's gain: it scales the LFO rate and the output (bank `+0x1c`).
    pub gain: f32,
    /// 0 left, 1 right; 0.0 is a voice the mode leaves centred. In the kernel
    /// this is the voice's LFO phase offset (bank `+0x38` → `voice+(-0x158)`).
    pub pan: f32,
    /// The loop filter's value for the voice (bank `+0x54`). It scales the
    /// second LFO's rate and divides its tap (`voice+(-0x1d8)`).
    pub filter: f32,
    /// The loop filter's corner in Hz (bank `+0x8c` → `voice+(-0x80)`).
    pub hz: f32,
    /// The second LFO's depth (bank `+0x70` → `voice+(-0x1e0)`). Zero switches
    /// the second LFO off (`voice+(-0x1a0)` is `tap > 1e-8`).
    pub tap: f32,
    /// The smoother's triangle coefficient (bank `+0xc4` → `voice+(-0x1f8)`).
    pub c1: f32,
    /// The smoother's one-pole coefficient (bank `+0xe0` → `voice+(-0x200)`).
    pub c2: f32,
    /// 0 takes the loop filter's lowpass tap, 1 its bandpass tap
    /// (bank `+0xa8` → `voice+(-0x1c0)`, which holds `1 − blend`).
    pub blend: f32,
}

/// A mode's seven voices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mode {
    pub voices: [Voice; 7],
}

#[expect(
    clippy::too_many_arguments,
    reason = "one entry of the bank's nine columns, named as the binary stores them"
)]
const fn voice(
    base_s: f32,
    gain: f32,
    pan: f32,
    filter: f32,
    hz: f32,
    tap: f32,
    c1: f32,
    c2: f32,
    blend: f32,
) -> Voice {
    Voice {
        base_s,
        gain,
        pan,
        filter,
        hz,
        tap,
        c1,
        c2,
        blend,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "a bank is nine columns of seven floats, one per table in the binary"
)]
const fn mode(
    delays: [f32; 7],
    gains: [f32; 7],
    pans: [f32; 7],
    filters: [f32; 7],
    hz: [f32; 7],
    taps: [f32; 7],
    c1: [f32; 7],
    c2: [f32; 7],
    blends: [f32; 7],
) -> Mode {
    let [d0, d1, d2, d3, d4, d5, d6] = delays;
    let [g0, g1, g2, g3, g4, g5, g6] = gains;
    let [p0, p1, p2, p3, p4, p5, p6] = pans;
    let [f0, f1, f2, f3, f4, f5, f6] = filters;
    let [h0, h1, h2, h3, h4, h5, h6] = hz;
    let [t0, t1, t2, t3, t4, t5, t6] = taps;
    let [a0, a1, a2, a3, a4, a5, a6] = c1;
    let [b0, b1, b2, b3, b4, b5, b6] = c2;
    let [m0, m1, m2, m3, m4, m5, m6] = blends;
    let voices = [
        voice(d0, g0, p0, f0, h0, t0, a0, b0, m0),
        voice(d1, g1, p1, f1, h1, t1, a1, b1, m1),
        voice(d2, g2, p2, f2, h2, t2, a2, b2, m2),
        voice(d3, g3, p3, f3, h3, t3, a3, b3, m3),
        voice(d4, g4, p4, f4, h4, t4, a4, b4, m4),
        voice(d5, g5, p5, f5, h5, t5, a5, b5, m5),
        voice(d6, g6, p6, f6, h6, t6, a6, b6, m6),
    ];
    Mode { voices }
}

/// Kontakt 8.13.1's four banks, read from `[core+0x6c8]` after the mode setter.
///
/// The columns are the effective bank's floats: delays `+0x00`, gains `+0x1c`,
/// pans `+0x38`, loop-filter values `+0x54`, taps `+0x70`, cutoffs `+0x8c`,
/// blend weights `+0xa8`, smoother coefficients `+0xc4`/`+0xe0`. Where a mode's
/// block does not write a column the constructor's defaults stand, which is why
/// mode 1's cutoffs (4000/6000/10000 Hz), and every mode's voice 6 (50 Hz, tap
/// 0, blend 1), differ from the raw immediates.
pub const MODES: [Mode; 4] = [
    mode(
        [0.003, 0.003, 0.007, 0.007, 0.011, 0.011, 0.013],
        [1.0, 1.0, 1.42857, 1.43, 1.7242, 1.72765, 0.7423],
        [0.0, 0.5, 0.0, 0.5, 0.0, 0.5, 0.0],
        [6.1, 6.1, 6.1, 6.1, 0.1, 0.1, 1.0],
        [5000.0, 5000.0, 5000.0, 5000.0, 10000.0, 10000.0, 50.0],
        [0.0, 0.0, 0.0, 0.0, 0.01, 0.01, 0.0],
        [0.2, 0.21, 0.21, 0.2, 0.4, 0.4, 0.0],
        [0.4, 0.4, 0.4, 0.4, 0.4, 0.4, 0.4],
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
    ),
    mode(
        [0.007, 0.007, 0.007, 0.007, 0.013, 0.013, 0.029],
        [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.7423],
        [0.0, 0.5, 0.33333, 0.83333, 0.66666, 0.16666, 0.0],
        [4.1, 4.1, 4.0, 4.0, 4.12, 4.1241, 1.0],
        [4000.0, 4000.0, 6000.0, 6000.0, 10000.0, 10000.0, 50.0],
        [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0],
        [0.01, 0.01, 0.01, 0.013, 0.014, 0.012, 0.0],
        [0.3, 0.32, 0.32, 0.3, 0.33, 0.3, 0.4],
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
    ),
    mode(
        [0.011, 0.011, 0.011, 0.011, 0.011, 0.011, 0.053],
        [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.7423],
        [0.0, 0.5, 0.5, 0.0, 0.25, 0.75, 0.0],
        [6.1, 6.1, 6.1, 6.1, 6.1, 6.1, 1.0],
        [12000.0, 12000.0, 150.0, 150.0, 20000.0, 20000.0, 50.0],
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [0.1, 0.1, 0.101, 0.101, 0.4, 0.4, 0.0],
        [0.2, 0.22, 0.22, 0.2, 0.2, 0.2, 0.99],
        [0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0],
    ),
    mode(
        [0.007, 0.007, 0.011, 0.011, 0.013, 0.013, 0.029],
        [1.0, 1.0, 1.42857, 1.42857, 1.7242, 1.7242, 0.7423],
        [0.0, 0.0, 0.0, 0.25, 0.0, 0.25, 0.0],
        [6.1, 6.1, 6.1, 6.1, 6.1, 6.1, 1.0],
        [20000.0, 20000.0, 20000.0, 20000.0, 20000.0, 20000.0, 50.0],
        [0.0, 0.0, 0.3, 0.3, 0.6, 0.6, 0.0],
        [0.0, 0.2, 0.0, 0.0, 0.0, 0.0, 0.0],
        [0.99, 0.99, 0.99, 0.99, 0.99, 0.99, 0.99],
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
    ),
];

/// The mode a `mode` parameter picks: `(int)(p × 4)` clamped to 0..=3, and
/// mode 4 is mode 3 (the initialiser's switch falls through to it).
#[must_use]
pub fn mode_at(p: f32) -> usize {
    let k = (p * 4.0) as i32;
    match k.clamp(0, 4) {
        4 => 3,
        n => n as usize,
    }
}

/// A voice's delay in seconds at `delay` 0..=1: `base × (delay · 5 + 0.1)`.
#[must_use]
pub fn voice_delay_s(base_s: f32, delay: f32) -> f32 {
    base_s * (delay * 5.0 + 0.1)
}

/// The LFO rate in Hz: `0.25 + 7.4 · rate³`.
///
/// Measured from `voice+(-0x220)` after prepare, 48 kHz: rate 0, 0.05, 0.1,
/// 0.25, 0.5, 0.75 and 1 give 0.25, 0.250925, 0.2574, 0.365625, 1.175,
/// 3.371875 and 7.65 Hz, which is the cubic exactly (7.4 · 0.05³ = 0.000925).
#[must_use]
pub fn lfo_hz(rate: f32) -> f32 {
    0.25 + 7.4 * rate * rate * rate
}

/// The LFO's waveform at `phase` in 0..1: a triangle, `4φ − 1` on the rising
/// half and `3 − 4φ` on the falling half.
///
/// Measured at 48 kHz, rate 0.5, amount 0.6: the value at frame `n` is
/// `4 · n · (hz / fs) − 1` to the last bit up to the peak at frame 20429
/// (`hz / fs` = 2.4479e-5, half a period), then the mirror.
#[must_use]
pub fn lfo_triangle(phase: f32) -> f32 {
    let p = phase.rem_euclid(1.0);
    if p < 0.5 {
        4.0 * p - 1.0
    } else {
        3.0 - 4.0 * p
    }
}

/// The modulation depth in samples at `amount`, for an LFO at `hz` and `fs`:
/// `(fs / hz) · ½ · (2^(0.025 · m) − 1)`, `m = 0.99 · amount² + 0.01`, the
/// distance [`lfo_triangle`] swings the line either side of its centre (so the
/// peak-to-peak swing is twice this).
///
/// Read out of the kernel: `0x14193a2f0` (amount setter) stores
/// `m = 0.99 · amount² + 0.01` at `voice+(-0x218)` and
/// `1 / (2^(0.025 m) − 0.9999)` at `voice+0x58`, and `0x14193dc00` stores
/// `(fs / hz) · ½ · (2^(0.025 m) − 1)` at `voice+(-0x210)`, which `0x14193be50`
/// adds to the base delay. Measured: 3.5398, 25.4558, 91.3451, 201.6327 and
/// 357.0320 samples at amount 0, 0.25, 0.5, 0.75, 1 (rate 0.5, 48 kHz), and
/// 611.4650 at amount 0.6, rate 0 (0.25 Hz).
#[must_use]
pub fn modulation_depth(amount: f32, hz: f32, fs: f32) -> f32 {
    // The setter squares in f32 and does the rest in f64.
    let m = 0.99 * f64::from(amount * amount) + 0.01;
    let p = 2.0f64.powf(0.025 * m);
    ((f64::from(fs) / f64::from(hz)) * 0.5 * (p - 1.0)) as f32
}

/// The seven parameters the effect's knobs carry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// 0..=1, the modulation depth (the "amount" knob).
    pub amount: f32,
    /// 0..=1, the LFO rate through [`lfo_hz`].
    pub rate: f32,
    /// 0..=1, the delay through [`voice_delay_s`].
    pub delay: f32,
    /// 0..=1, how many of the seven voices are heard (see [`Chorus::set`]).
    pub voices: f32,
    /// 0..=1, the width of the stereo rotation.
    pub width: f32,
    /// 0..=1, the feedback into the delay lines.
    pub feedback: f32,
    /// 0..=1, dry (0) to wet (1).
    pub mix: f32,
    /// 0..=1, how the voices mix into each other (0 is diagonal).
    pub scatter: f32,
    /// 0 or 1, the wet path's polarity.
    pub invert: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            amount: 0.6,
            rate: 0.5,
            delay: 0.18,
            voices: 0.7,
            width: 0.4,
            feedback: 0.5,
            mix: 0.8,
            scatter: 1.0,
            invert: 0.0,
        }
    }
}

/// The kernel's ring length per voice in seconds: 32768 frames at 48 kHz.
///
/// `voice+0x48` holds the mask `0x7fff` and `voice+0x18`/`voice+0x28` the two
/// base pointers, 32768 frames apart. The ring is written twice a frame (at
/// `widx` and `widx + size`) and read one frame ahead of the write, so the
/// length has to hold every read the banks can ask for: the longest is
/// 0.053 s × 5.1 = 0.27 s plus the deepest swing, 0.047 s.
const RING_S: f64 = 32768.0 / 48_000.0;

/// `x + x³/3 + x⁵/7.5`, the kernel's `tan` at `0x14193e321` and `0x14193adc4`
/// (the coefficients are `1/3` at `0x147697b50` and `1/7.5` at `0x147697b40`).
fn tan_poly(x: f64) -> f64 {
    let x2 = x * x;
    x + x2 * x / 3.0 + x2 * x2 * x / 7.5
}

/// The second LFO's waveform: the triangle shaped by
/// `(π/2)y − 0.641592653589793 y³ + 0.0707963267948966 y⁵`.
///
/// That is `0x1419393d0`, the lambda the voice constructor installs as the
/// second LFO's function object (`0x141939440` is the plain triangle the first
/// one gets). The constants are `0x147697aa8`, `0x147697a90` and `0x147697a70`.
fn shaped_triangle(y: f64) -> f64 {
    const C3: f64 = 0.641_592_653_589_793;
    const C5: f64 = 0.070_796_326_794_896_6;
    let y2 = y * y;
    std::f64::consts::FRAC_PI_2 * y - C3 * y2 * y + C5 * y2 * y2 * y
}

/// One voice's running state, and the coefficients the settings pin on it.
#[derive(Clone)]
struct Line {
    ring: Vec<[f64; 2]>,
    widx: usize,
    /// The loop filter's two state variables, `voice+0x00` and `voice-0x10`.
    s1: [f64; 2],
    s2: [f64; 2],
    /// The smoothed LFO, `voice-0x1c8`.
    smooth: f64,
    /// The two LFOs' phases (`voice-0x198+0x08`, `voice-0x110+0x08`).
    ph: f64,
    ph2: f64,
    base: f64,
    depth: f64,
    hz: f64,
    hz2: f64,
    pan: f64,
    k: f64,
    k2: f64,
    c1: f64,
    c2: f64,
    blend: f64,
    /// `tap / filter`, `voice-0x1d0`, which scales the second LFO.
    inv: f64,
    tap: f64,
}

impl std::fmt::Debug for Line {
    /// The ring is summarised: printing 32768 frames helps nobody.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Line")
            .field("widx", &self.widx)
            .field("s1", &self.s1)
            .field("s2", &self.s2)
            .field("smooth", &self.smooth)
            .field("ph", &self.ph)
            .field("ph2", &self.ph2)
            .field("base", &self.base)
            .field("depth", &self.depth)
            .field("ring_frames", &self.ring.len())
            .finish()
    }
}

/// Kontakt's Choral: seven voices of modulated delay in one feedback ring.
///
/// The kernel is `0x14193be50`; every coefficient cites the address it came
/// from in `docs/re/engine/k-choral-modes.md`.
#[derive(Clone, Debug)]
pub struct Chorus {
    bank: Mode,
    /// The knobs last applied, so a bank change can re-derive the voices.
    settings: Settings,
    fs: f64,
    /// `A`, the wet polarity, `core+0x1b80`.
    a: f64,
    /// `0x1b90` and `0x1b88`: the wet and dry gains.
    wet: f64,
    dry: f64,
    /// `0x1b78`: the input makeup, `(1 − min(g, 0.99)⁴)^¼`.
    makeup: f64,
    /// `0x1b70`, the output trim from the voices law.
    trim: f64,
    /// `0x1b90 · 0x1b78 · (0x1b70 · 0.707)`, the wet path's whole multiplier.
    out_gain: f64,
    /// `core+0x748` and `core+0x780`: the output and input gains per voice.
    g1: [f64; 7],
    g2: [f64; 7],
    /// The width rotations, two rows per voice at `core+0x8a0`.
    rot: [[f64; 4]; 7],
    /// The seven-tap cyclic mix's coefficients at `core+0x7c0`.
    mix: [f64; 7],
    /// The global tone filter: `core+0x1c20`, `0x1c30`, `0x1bc0`, `0x1bd0`.
    kg: f64,
    k2g: f64,
    g_avg: f64,
    g_tap: f64,
    gs1: [f64; 2],
    gs2: [f64; 2],
    /// The previous frame's reads, `core+0x830`.
    prev: [[f64; 2]; 7],
    lines: [Line; 7],
}

/// `1/√2`, the damping the kernel gives both loop filters (`0x147697b60`).
const SQRT_HALF: f64 = 0.707_106_186_186_548;
/// `√2`, the loop filter's output tap (`0x147697b70`, `voice-0x40`).
const SQRT_2: f64 = 1.414_213_562_373_096;
/// The scatter matrix's three weights, `0x147697a80`, `0x147697a88`, `0x147697ac0`.
const SCATTER_DIAG: f64 = 0.445_902_906_222_806;
const SCATTER_ZERO: f64 = 0.554_097_093_777_194_1;
const SCATTER_OFF: f64 = -0.261_203_874_963_741;

impl Chorus {
    /// Builds the effect for `bank` at `fs`.
    ///
    /// `bank` is an index into [`MODES`] (see [`mode_at`]); the ring starts
    /// empty.
    #[must_use]
    #[expect(
        clippy::indexing_slicing,
        reason = "the voice index comes from MODES itself"
    )]
    pub fn new(bank: usize, fs: f64) -> Self {
        let mode = MODES[bank.min(MODES.len() - 1)];
        let frames = ((RING_S * fs).ceil() as usize)
            .clamp(4, 1 << 20)
            .next_power_of_two();
        let mut c = Self {
            bank: mode,
            settings: Settings::default(),
            fs,
            a: 1.0,
            wet: 0.0,
            dry: 1.0,
            makeup: 1.0,
            trim: 1.0,
            out_gain: 0.0,
            g1: [0.0; 7],
            g2: [1.0; 7],
            rot: [[1.0, 0.0, 0.0, 1.0]; 7],
            mix: [0.0; 7],
            kg: 0.0,
            k2g: 1.0,
            g_avg: 0.0,
            g_tap: 0.0,
            gs1: [0.0; 2],
            gs2: [0.0; 2],
            prev: [[0.0; 2]; 7],
            lines: std::array::from_fn(|i| Line {
                ring: vec![[0.0; 2]; frames],
                widx: 0,
                s1: [0.0; 2],
                s2: [0.0; 2],
                smooth: 0.0,
                ph: 0.0,
                ph2: 0.0,
                base: f64::from(mode.voices[i].base_s),
                depth: 0.0,
                hz: f64::from(mode.voices[i].gain),
                hz2: 0.0,
                pan: f64::from(mode.voices[i].pan),
                k: 0.0,
                k2: 1.0,
                c1: f64::from(mode.voices[i].c1),
                c2: f64::from(mode.voices[i].c2),
                blend: f64::from(mode.voices[i].blend),
                inv: 0.0,
                tap: f64::from(mode.voices[i].tap),
            }),
        };
        // `set` would early out here (the knob record already holds the
        // defaults), and an underived bank has no coefficients at all.
        c.derive();
        c
    }

    /// Applies the knobs, as the kernel's setters do, leaving the ring alone.
    ///
    /// `0x14193b140` (rate), `0x14193ac00` (delay), `0x14193a2f0` (amount),
    /// `0x14193a3e0` (voices), `0x14193a4a0` (width), `0x14193ae70` (feedback),
    /// `0x14193a620` (scatter), `0x14193a6a0` (invert), `0x14193a6d0` (mix) and
    /// `0x14193e160`, which maps the bank onto the voices.
    pub fn set(&mut self, s: Settings) {
        // `Wet::set` calls this every block; re-deriving seven voices' worth of
        // `pow`/`tan`/`sincos` for unchanged knobs is wasted work.
        if self.settings == s {
            return;
        }
        self.settings = s;
        self.derive();
    }

    /// Swaps the bank, as the native's mode setter does, keeping the ring and
    /// the filters' state; the knobs it was last given are re-applied.
    #[expect(
        clippy::indexing_slicing,
        reason = "voices are indexed over MODES[..].voices and the arrays sized 7"
    )]
    pub fn set_bank(&mut self, bank: usize) {
        self.bank = MODES[bank.min(MODES.len() - 1)];
        self.derive();
    }

    /// The bank in use, 0..=3.
    #[must_use]
    pub fn bank(&self) -> usize {
        MODES
            .iter()
            .position(|m| m == &self.bank)
            .unwrap_or_default()
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "voices are indexed over MODES[..].voices and the arrays sized 7"
    )]
    fn derive(&mut self) {
        let s = self.settings;
        let fs = self.fs;
        // The kernel evaluates these laws in f64 from the f32 knobs; the f32
        // helpers above are the same laws rounded, and are not used here.
        let rate = f64::from(s.rate);
        let base_hz = 0.25 + 7.4 * rate * rate * rate;
        let m = 0.99 * f64::from(s.amount * s.amount) + 0.01;
        let p = 2.0f64.powf(0.025 * m);
        // The voices law: sqrt(min(2v, 1)) and sqrt(max(2v - 1, 0)).
        let voices = f64::from(s.voices);
        let g = (2.0 * voices).clamp(0.0, 1.0).sqrt();
        let h = (2.0 * voices - 1.0).clamp(0.0, 1.0).sqrt();
        self.g1 = [1.0, 1.0, g, g, h, h, 0.0];
        // `0x14193a3e0`: the output trim from 2 + 2(g + h), `0x144760130` is 0.6.
        let sum = 2.0 + 2.0 * (g + h);
        self.trim = (1.0 / sum) * (0.2 * sum + 0.6).sqrt();
        // `0x14193ae70`: (0.999 · feedback^¼) raised to each voice's delay ratio.
        let d0 = f64::from(self.bank.voices[0].base_s);
        let gain = 0.999 * f64::from(s.feedback).sqrt().sqrt();
        for (i, line) in self.lines.iter_mut().enumerate() {
            let ratio = f64::from(self.bank.voices[i].base_s) / d0;
            self.g2[i] = gain.powf(ratio);
            // `0x14193b140`: the rate, scaled by the voice's gain.
            let hz = base_hz * f64::from(self.bank.voices[i].gain);
            // `0x14193e160`: the second LFO runs at the first's rate times the
            // voice's filter value, and its tap is `tap / filter`.
            let hz2 = hz * f64::from(self.bank.voices[i].filter);
            line.hz = hz;
            line.hz2 = hz2;
            line.depth = fs / hz * 0.5 * (p - 1.0);
            line.base =
                f64::from(self.bank.voices[i].base_s) * (f64::from(s.delay) * 5.0 + 0.1) * fs;
            line.inv = f64::from(self.bank.voices[i].tap) / f64::from(self.bank.voices[i].filter);
            line.tap = f64::from(self.bank.voices[i].tap);
            // The loop filter's two-pole coefficients, `0x14193e2f6`: a `tan`
            // of π·hz/fs, then 1/(1 + √2·k + k²).
            let k = tan_poly(f64::from(self.bank.voices[i].hz) * std::f64::consts::PI / fs);
            line.k = k;
            line.k2 = 1.0 / (1.0 + 2.0 * SQRT_HALF * k + k * k);
        }
        // The width rotations, `0x14193a4a0`: ±width·π/4·(1 − i/7) per pair.
        for (i, rot) in self.rot.iter_mut().enumerate() {
            if i == 6 {
                *rot = [1.0, 0.0, 0.0, 1.0];
                continue;
            }
            let theta = f64::from(s.width)
                * (std::f64::consts::PI / 4.0)
                * (1.0 - (2 * (i / 2)) as f64 / 7.0);
            let (sin, cos) = theta.sin_cos();
            *rot = if i % 2 == 0 {
                [cos, sin, -sin, cos]
            } else {
                [cos, -sin, sin, cos]
            };
        }
        // The seven-tap cyclic mix, `0x14193a620`: a circulant whose row sums
        // are one, diagonal-then-scatter.
        let sc = f64::from(s.scatter);
        self.mix = [
            1.0 - sc * SCATTER_ZERO,
            sc * SCATTER_OFF,
            sc * SCATTER_OFF,
            sc * SCATTER_DIAG,
            sc * SCATTER_OFF,
            sc * SCATTER_DIAG,
            sc * SCATTER_DIAG,
        ];
        self.a = 1.0 - 2.0 * f64::from(s.invert);
        self.wet = f64::from(s.mix);
        self.dry = 1.0 - self.wet;
        // The global tone filter, `0x14193ae70` and `0x14193ac00`: its corner
        // is 0.25 / the first voice's delay in seconds, damped by the feedback.
        let g0 = self.g2[0];
        let w = (1.0 - g0).max(0.2);
        let first = f64::from(self.bank.voices[0].base_s) * (f64::from(s.delay) * 5.0 + 0.1);
        let f1 = (0.25 / first).min(50.0);
        self.kg = tan_poly(f1 / w.sqrt().sqrt() * std::f64::consts::PI / fs);
        self.k2g = 1.0 / (1.0 + 2.0 * SQRT_HALF * self.kg + self.kg * self.kg);
        self.g_avg = w - 1.0;
        self.g_tap = 2.0 * SQRT_HALF * (w.sqrt() - 1.0);
        self.makeup = (1.0 - g0.min(0.99).powi(4)).sqrt().sqrt();
        // The association is the kernel's: (wet · makeup) · (trim · 0.707).
        self.out_gain = (self.wet * self.makeup) * (self.trim * 0.707);
    }

    /// Renders `io` in place (stereo frames).
    #[expect(
        clippy::indexing_slicing,
        reason = "the ring index is masked to the ring's length and every other index is 0..7 or 0..2"
    )]
    pub fn process(&mut self, io: &mut [[f32; 2]]) {
        for frame in io.iter_mut() {
            let inp = [f64::from(frame[0]), f64::from(frame[1])];
            // The seven-tap cyclic mix of the previous frame's reads:
            // out[j] = Σᵢ C[(i − j) mod 7] · prev[i] (`0x141939270`).
            let mut mix = [[0.0f64; 2]; 7];
            for (j, m) in mix.iter_mut().enumerate() {
                for (i, prev) in self.prev.iter().enumerate() {
                    let c = self.mix[(i + 7 - j) % 7];
                    m[0] += c * prev[0];
                    m[1] += c * prev[1];
                }
            }
            let mut wet = [[0.0f64; 2]; 7];
            for (j, line) in self.lines.iter_mut().enumerate() {
                let y = loop_filter(line, mix[j], self.a * self.g2[j], self.makeup, inp);
                line.ring[line.widx] = y;
                line.widx = (line.widx + 1) & (line.ring.len() - 1);
                // The LFOs: the first is a triangle at the voice's phase offset,
                // the second a shaped one whose tap the bank may zero.
                let x1 = wrap(line.pan + line.ph);
                let t1 = triangle(x1);
                line.ph = wrap(line.ph + line.hz / self.fs);
                // The second LFO only runs when the bank gives the voice a tap
                // (`voice+(-0x1a0)`), and its phase stands still otherwise.
                let alt = if line.tap > 1e-8 {
                    let t2 = shaped_triangle(triangle(wrap(line.ph2)));
                    line.ph2 = wrap(line.ph2 + line.hz2 / self.fs);
                    (0.5 * t2 + 1.0) * line.inv
                } else {
                    0.0
                };
                // The smoother: (1 − c2)·[(2 − c1(1 − t))·(t + 1)/4 + alt]
                // + c2·previous, `0x14193c431`.
                line.smooth = (1.0 - line.c2)
                    * ((2.0 - line.c1 * (1.0 - t1)) * (0.25 * (t1 + 1.0)) + alt)
                    + line.c2 * line.smooth;
                // The read, `0x14193c4c8`: trunc(d) frames back, Catmull-Rom.
                // The ring is a power of two, so the distance wraps through it
                // by masking: `wrapping_sub` keeps the frame that a ring that
                // long would hold, where `len − whole` went negative once the
                // distance passed the ring (a panic in debug, an arbitrary
                // frame in release).
                let d = line.depth * line.smooth + line.base;
                let whole = d as usize;
                let f = d - d.floor();
                let len = line.ring.len();
                let at = line.widx.wrapping_sub(whole) & (len - 1);
                let (p0, pm1, p1, p2) = (
                    line.ring[at],
                    line.ring[(at + len - 1) & (len - 1)],
                    line.ring[(at + 1) & (len - 1)],
                    line.ring[(at + len - 2) & (len - 1)],
                );
                for ch in 0..2 {
                    wet[j][ch] = p0[ch]
                        + 0.5 * f * (pm1[ch] - p1[ch])
                        + f * f * (p1[ch] - 2.5 * p0[ch] + 2.0 * pm1[ch] - 0.5 * p2[ch])
                        + f * f * f * (1.5 * p0[ch] - 1.5 * pm1[ch] + 0.5 * p2[ch] - 0.5 * p1[ch]);
                }
            }
            // The output sum, `0x14193c191`: each voice's previous read plus
            // its share of the mix, rotated and gained, then the global filter.
            let mut sum = [0.0f64; 2];
            for (j, rot) in self.rot.iter().enumerate() {
                let t0 = self.a * (self.prev[j][0] + self.g2[j] * mix[j][0]) + self.makeup * inp[0];
                let t1 = self.a * (self.prev[j][1] + self.g2[j] * mix[j][1]) + self.makeup * inp[1];
                sum[0] += self.g1[j] * (rot[0] * t0 + rot[1] * t1);
                sum[1] += self.g1[j] * (rot[2] * t0 + rot[3] * t1);
            }
            let mut out = [0.0f32; 2];
            for ch in 0..2 {
                let x = sum[ch];
                let s1 = self.gs1[ch];
                let s2 = self.gs2[ch];
                let q = (s2 + self.kg * (x - s1)) * self.k2g;
                let s2n = 2.0 * q - s2;
                let s1n = s1 + 2.0 * q * self.kg;
                self.gs1[ch] = s1n;
                self.gs2[ch] = s2n;
                let avg = (s1 + s1n) * 0.5;
                let y =
                    (x + avg * self.g_avg + q * self.g_tap) * self.out_gain + self.dry * inp[ch];
                out[ch] = y as f32;
            }
            *frame = out;
            self.prev = wet;
        }
    }
}

/// `φ` folded into 0..1, as the kernel's `phase >= 1 → phase − 1` does.
fn wrap(phase: f64) -> f64 {
    if phase >= 1.0 { phase - 1.0 } else { phase }
}

/// The plain triangle in f64: `1 − |2 − 4φ|` (`0x141939440`).
fn triangle(phase: f64) -> f64 {
    1.0 - (2.0 - 4.0 * phase).abs()
}

/// One voice's two-pole loop filter and its output tap (`0x14193c300`).
///
/// `x` is the voice's share of the mix plus its share of the input; the
/// returned pair is what the ring stores this frame.
#[expect(
    clippy::indexing_slicing,
    reason = "the channel loop runs over 0..2, the length of every pair here"
)]
fn loop_filter(
    line: &mut Line,
    mix: [f64; 2],
    again: f64,
    input_gain: f64,
    inp: [f64; 2],
) -> [f64; 2] {
    let mut y = [0.0; 2];
    for ch in 0..2 {
        let x = again * mix[ch] + input_gain * inp[ch];
        let s1 = line.s1[ch];
        let s2 = line.s2[ch];
        let q = (s2 + line.k * (x - s1)) * line.k2;
        let s2n = 2.0 * q - s2;
        let s1n = s1 + 2.0 * q * line.k;
        line.s1[ch] = s1n;
        line.s2[ch] = s2n;
        let avg = (s1 + s1n) * 0.5;
        let band = x - q * SQRT_2 - avg;
        y[ch] = (1.0 - line.blend) * avg + line.blend * band;
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The measured taps: mode 0's first voice at delay 0.5 landed on frame 374
    /// and at delay 1.0 on frame 734 (48 kHz, emulator 2026-10-30).
    #[test]
    fn a_voice_delay_is_the_base_scaled_by_the_delay_knob() {
        let base = MODES[0].voices[0].base_s;
        assert_eq!(voice_delay_s(base, 0.5) * 48_000.0, 374.4);
        assert_eq!(voice_delay_s(base, 1.0) * 48_000.0, 734.4);
    }

    #[test]
    fn the_mode_parameter_maps_four_ways_and_four_is_three() {
        assert_eq!(mode_at(0.0), 0);
        assert_eq!(mode_at(0.3), 1);
        assert_eq!(mode_at(0.55), 2);
        assert_eq!(mode_at(0.8), 3);
        assert_eq!(mode_at(1.0), 3);
    }

    #[test]
    fn every_bank_has_seven_voices_and_a_centred_voice_is_pan_zero() {
        for (n, m) in MODES.iter().enumerate() {
            assert_eq!(m.voices.len(), 7, "mode {n}");
            assert!(m.voices.iter().any(|v| v.pan == 0.0), "mode {n}");
        }
    }

    /// The seven rates read back from `voice+(-0x220)` after prepare.
    #[test]
    fn the_lfo_rate_is_cubic_in_the_rate_knob() {
        let measured = [
            (0.0, 0.25),
            (0.05, 0.250925),
            (0.1, 0.2574),
            (0.25, 0.365625),
            (0.5, 1.175),
            (0.75, 3.371875),
            (1.0, 7.65),
        ];
        for (rate, hz) in measured {
            assert!(
                (lfo_hz(rate) - hz).abs() < 1e-6,
                "rate {rate}: {} vs {hz}",
                lfo_hz(rate)
            );
        }
    }

    /// The triangle: -1 at phase 0, +1 at the half period, and the value the
    /// kernel produced on the rising half (frame 2000 at rate 0.5 is -0.804167).
    #[test]
    fn the_lfo_is_a_triangle() {
        assert_eq!(lfo_triangle(0.0), -1.0);
        assert!((lfo_triangle(0.5) - 1.0).abs() < 1e-6);
        assert!(lfo_triangle(0.25).abs() < 1e-6);
        assert!((lfo_triangle(0.75) + 0.0).abs() < 1e-6);
        let phase = 2000.0 * lfo_hz(0.5) / 48_000.0;
        assert!((lfo_triangle(phase) - -0.8041667).abs() < 1e-6);
        assert_eq!(lfo_triangle(1.0), -1.0);
    }

    /// The five depths read out of `voice+(-0x210)` at rate 0.5, 48 kHz.
    #[test]
    fn the_modulation_depth_is_the_kernels_law() {
        let hz = lfo_hz(0.5);
        let measured = [3.5398, 25.4558, 91.3451, 201.6327, 357.032];
        for (n, amount) in [0.0, 0.25, 0.5, 0.75, 1.0].into_iter().enumerate() {
            let depth = modulation_depth(amount, hz, 48_000.0);
            assert!(
                (depth - measured[n]).abs() < 1e-3,
                "amount {amount}: {depth} vs {}",
                measured[n]
            );
        }
        // rate 0 is 0.25 Hz, amount 0.6.
        assert!((modulation_depth(0.6, lfo_hz(0.0), 48_000.0) - 611.465).abs() < 1e-3);
    }

    /// The shaped second LFO is odd, lands on ±1, and is within 4e-4 of
    /// `sin(πy/2)` — the kernel's fit, not a sine.
    #[test]
    fn the_second_lfo_is_the_kernels_shaped_triangle() {
        assert!((shaped_triangle(-1.0) + 1.0).abs() < 1e-9);
        assert!((shaped_triangle(1.0) - 1.0).abs() < 1e-9);
        assert!((shaped_triangle(0.0)).abs() < 1e-12);
        assert!((shaped_triangle(0.5) - 0.707_411_466_911_064_7).abs() < 1e-12);
        for i in 0..=100 {
            let y = -1.0 + 2.0 * f64::from(i) / 100.0;
            let sine = (std::f64::consts::FRAC_PI_2 * y).sin();
            assert!(
                (shaped_triangle(y) - sine).abs() < 4e-4,
                "y {y}: {} vs {sine}",
                shaped_triangle(y)
            );
        }
    }

    /// A silent input stays silent, and the dry path alone is just the input.
    #[test]
    fn silence_in_is_silence_out() {
        let mut c = Chorus::new(1, 48_000.0);
        let mut io = vec![[0.0f32; 2]; 256];
        c.process(&mut io);
        assert!(io.iter().all(|f| f == &[0.0, 0.0]));
    }

    /// A fresh effect is already initialised: its defaults are audible. An
    /// underived bank would have zero coefficients and pass the dry signal.
    #[test]
    fn a_new_effect_is_derived() {
        let mut c = Chorus::new(0, 48_000.0);
        let mut io: Vec<[f32; 2]> = (0..256).map(|i| [((i as f32) * 0.1).sin(), 0.0]).collect();
        let want = io.clone();
        c.process(&mut io);
        assert_ne!(io, want, "the default knobs give a wet signal");
    }

    /// A read distance longer than the ring reads the frame its remainder
    /// names. `widx + len − whole` underflowed as soon as the distance passed
    /// the ring: a panic in debug, an arbitrary frame in release.
    #[test]
    fn a_read_past_the_ring_reads_frames_remainder() {
        let ring = Chorus::new(1, 48_000.0).lines[0].ring.len() as f64;
        let run = |base: f64| {
            let mut c = Chorus::new(1, 48_000.0);
            for line in &mut c.lines {
                line.base = base;
                line.depth = 0.0;
            }
            let mut io: Vec<[f32; 2]> = (0..512)
                .map(|i| {
                    [
                        ((i as f32) * 0.1).sin() * 0.5,
                        ((i as f32) * 0.07).cos() * 0.5,
                    ]
                })
                .collect();
            c.process(&mut io);
            io
        };
        // Three rings and 100 frames back is 100 frames back in this ring, so
        // the two runs read the same frames throughout.
        assert_eq!(
            run(3.0 * ring + 100.0),
            run(100.0),
            "a distance of 3 rings + 100 frames, {ring} to the ring"
        );
    }

    /// `mix` 0 is the dry signal, to the last bit the kernel produced.
    #[test]
    fn a_dry_setting_passes_the_input() {
        let mut c = Chorus::new(1, 48_000.0);
        c.set(Settings {
            mix: 0.0,
            ..Settings::default()
        });
        let mut io: Vec<[f32; 2]> = (0..64)
            .map(|i| {
                [
                    ((i as f32) * 0.1).sin() * 0.5,
                    ((i as f32) * 0.07).cos() * 0.5,
                ]
            })
            .collect();
        let want = io.clone();
        c.process(&mut io);
        assert_eq!(io, want);
    }
}
