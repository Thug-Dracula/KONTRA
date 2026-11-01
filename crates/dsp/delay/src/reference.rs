//! The captured impulse response, and how the port is held to it.
//!
//! `docs/re/engine/vectors/replika-diffusion-ir-{l,r}.f32` are Kontakt 8.14.1's
//! own answer to a one-sample impulse through the eight-stage network
//! (`NI::DSP::Diffuser`, `docs/re/engine/k-replika-diffusion.md` §11.6),
//! captured with the live object's rotors and re-length clock where §11.7
//! leaves them. `docs/re/engine/vectors/replika-diffusion-taps.tsv` is the tap
//! table those rotors were at.

use crate::diffusion::{Diffusion, Rotor, STAGES, Settings, TAPS, Window};

/// The left channel of the capture, 4096 frames of `f32` little-endian.
const CAPTURED_IR_L: &[u8] =
    include_bytes!("../../../../docs/re/engine/vectors/replika-diffusion-ir-l.f32");
/// The right channel.
const CAPTURED_IR_R: &[u8] =
    include_bytes!("../../../../docs/re/engine/vectors/replika-diffusion-ir-r.f32");

/// The capture as `f32` frames, `(left, right)`.
pub fn captured() -> (Vec<f32>, Vec<f32>) {
    let read = |bytes: &[u8]| {
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect::<Vec<_>>()
    };
    (read(CAPTURED_IR_L), read(CAPTURED_IR_R))
}

/// Pearson correlation of two equal-length vectors; 1.0 is the same shape.
pub fn correlation(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len());
    let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0f64, 0.0f64, 0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b).take(n) {
        let (x, y) = (f64::from(*x), f64::from(*y));
        sa += x;
        sb += y;
        saa += x * x;
        sbb += y * y;
        sab += x * y;
    }
    let (n, sa, sb) = (n as f64, sa, sb);
    let num = sab - sa * sb / n;
    let den = ((saa - sa * sa / n) * (sbb - sb * sb / n)).sqrt();
    if den > 0.0 { num / den } else { 0.0 }
}

/// Energy per 256-frame block, `out / want`; 1.0 everywhere is a perfect fit.
pub fn block_energy_ratio(got: &[f32], want: &[f32]) -> Vec<f64> {
    got.chunks(256)
        .zip(want.chunks(256))
        .map(|(g, w)| {
            let e = |v: &[f32]| v.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>();
            let (g, w) = (e(g), e(w));
            if w > 0.0 { g / w } else { 0.0 }
        })
        .collect()
}

/// One impulse through a network built from `settings`, 4096 frames, both
/// channels.
pub fn impulse(settings: Settings) -> ([f32; 4096], [f32; 4096]) {
    let mut net = Diffusion::new(settings);
    let mut io = [[0.0f32; 2]; 4096];
    io[0] = [1.0, 1.0];
    net.process(&mut io);
    let mut l = [0.0; 4096];
    let mut r = [0.0; 4096];
    for (i, frame) in io.iter().enumerate() {
        l[i] = frame[0];
        r[i] = frame[1];
    }
    (l, r)
}

/// The network Kontakt ran for the capture: its stage lengths, its tap window,
/// and the two rotors' phases read out of `+0x1240`/`+0x1270` at that frame.
pub fn captured_settings() -> Settings {
    Settings {
        lengths: CAPTURED_LENGTHS,
        window: Window {
            floor: 0.414_201_74,
            spread: 0.003_758_912_2,
        },
        rotors: [
            Rotor {
                phase: 1.505_501_9,
                radians_per_frame: -0.000_137_487_49,
            },
            Rotor {
                phase: 1.665_127_1,
                radians_per_frame: 0.000_196_410_49,
            },
        ],
        angle: std::f32::consts::FRAC_PI_2 - 1.0,
        relength: 12_000,
        seed: 0x0f0e_29c0,
    }
}

/// The stage lengths the capture read (the ctor's table, `+0x158` onward).
pub const CAPTURED_LENGTHS: [[f32; TAPS]; STAGES] = [
    [577.0, 859.0, 587.0, 857.0],
    [643.0, 977.0, 647.0, 971.0],
    [877.0, 1301.0, 881.0, 1297.0],
    [1019.0, 1531.0, 1033.0, 1523.0],
    [1823.0, 2711.0, 1831.0, 2707.0],
    [2293.0, 3389.0, 2309.0, 3371.0],
    [3541.0, 2377.0, 3547.0, 2371.0],
    [3823.0, 2579.0, 3833.0, 2557.0],
];

/// The tap table in force during the capture, as `(length, whole, frac)` per
/// ring, from `docs/re/engine/vectors/replika-diffusion-taps.tsv`.
pub const CAPTURED_TAP_TABLE: [[(f32, f32, f32); 4]; 8] = [
    [
        (577.0, 241.0, 0.158_661),
        (859.0, 356.0, 0.009_979),
        (587.0, 245.0, 0.338_196),
        (857.0, 355.0, 0.181_091),
    ],
    [
        (643.0, 263.0, 0.919_891),
        (977.0, 404.0, 0.435_455),
        (647.0, 265.0, 0.561_707),
        (971.0, 401.0, 0.951_721),
    ],
    [
        (877.0, 363.0, 0.470_032),
        (1301.0, 543.0, 0.756_348),
        (881.0, 365.0, 0.127_808),
        (1297.0, 542.0, 0.084_534),
    ],
    [
        (1019.0, 421.0, 0.821_625),
        (1531.0, 628.0, 0.400_269),
        (1033.0, 427.0, 0.617_035),
        (1523.0, 625.0, 0.116_638),
    ],
    [
        (1823.0, 761.0, 0.911_804),
        (2711.0, 1121.0, 0.941_04),
        (1831.0, 765.0, 0.255_31),
        (2707.0, 1120.0, 0.285_645),
    ],
    [
        (2293.0, 941.0, 0.183_716),
        (3389.0, 1404.0, 0.929_565),
        (2309.0, 947.0, 0.751_099),
        (3371.0, 1397.0, 0.467_529),
    ],
    [
        (3541.0, 1465.0, 0.434_692),
        (2377.0, 993.0, 0.452_698),
        (3547.0, 1467.0, 0.917_725),
        (2371.0, 990.0, 0.945_068),
    ],
    [
        (3823.0, 1584.0, 0.846_802),
        (2579.0, 1058.0, 0.575_195),
        (3833.0, 1588.0, 0.992_31),
        (2557.0, 1049.0, 0.545_044),
    ],
];
