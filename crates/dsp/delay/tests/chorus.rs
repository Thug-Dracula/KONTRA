//! Choral against the native vectors.
//!
//! `tools/re/kemu_timemod.py choral` runs Kontakt 8.13.1's kernel
//! (`0x14193be50`) in the emulator over the stimulus below, 32-frame blocks at
//! 48 kHz, and prints the output's bits. Choral is a seven-voice feedback
//! network, so any coefficient that is merely close shows up as a slow drift
//! over the 3000 frames; the model lands within 3e-6 of the native everywhere
//! (mean 1e-7) and on the exact bits at most reference frames, the residual
//! being the emulator's own `sincos` in the width matrices.

use kontra_dsp_delay::chorus::{Chorus, Settings};

const RATE: f64 = 48_000.0;

fn stimulus(n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let t = i as f64;
            [
                ((t * 0.05).sin() * 0.5 + if i == 0 { 1. } else { 0. }) as f32,
                ((t * 0.031).sin() * 0.5) as f32,
            ]
        })
        .collect()
}

/// Runs `bank` over the native stimulus with `s`, in the kernel's 32-frame
/// blocks, and returns the output.
fn render(bank: usize, s: Settings) -> Vec<[f32; 2]> {
    let mut io = stimulus(3000);
    let mut chorus = Chorus::new(bank, RATE);
    chorus.set(s);
    for block in io.chunks_mut(32) {
        chorus.process(block);
    }
    io
}

fn check(name: &str, bank: usize, s: Settings, want: &[(usize, u32, u32)]) {
    let got = render(bank, s);
    let mut checked = 0;
    for (i, frame) in got.iter().enumerate() {
        let Some(&(_, l, r)) = want.iter().find(|&&(f, _, _)| f == i) else {
            continue;
        };
        let w = [f32::from_bits(l), f32::from_bits(r)];
        let e = (frame[0] - w[0]).abs().max((frame[1] - w[1]).abs());
        assert!(
            e < 2e-5,
            "{name} frame {i}: {frame:?} vs {w:?} (error {e:e})"
        );
        checked += 1;
    }
    assert_eq!(
        checked,
        want.len(),
        "{name}: every reference frame was compared"
    );
}

/// Amount .6, rate .5, delay .18, voices .7, width .4, feedback .5, mix .8,
/// scatter 1, inverted wet, mode 1 — the emulator's first case.
#[test]
fn choral_renders_the_first_native_vector() {
    check(
        "mode 1",
        1,
        Settings {
            amount: 0.6,
            rate: 0.5,
            delay: 0.18,
            voices: 0.7,
            width: 0.4,
            feedback: 0.5,
            mix: 0.8,
            scatter: 1.0,
            invert: 1.0,
        },
        &[
            (0, 0x3f32_c0a0, 0x0000_0000),
            (1, 0x3c71_45ad, 0x3c31_4b45),
            (150, 0x3ea4_9034, 0xbebc_e4dd),
            (777, 0x3e88_e9e0, 0xbe87_1bd9),
            (1500, 0xbe88_3379, 0x3e14_18bb),
            (2999, 0xbdc8_5a44, 0xbea4_ea2d),
        ],
    );
}

/// Amount .3, rate .8, delay .5, all voices, width 1, feedback .9, fully wet,
/// non-inverted, mode 2 — the emulator's second case.
#[test]
fn choral_renders_the_second_native_vector() {
    check(
        "mode 2",
        2,
        Settings {
            amount: 0.3,
            rate: 0.8,
            delay: 0.5,
            voices: 1.0,
            width: 1.0,
            feedback: 0.9,
            mix: 1.0,
            scatter: 1.0,
            invert: 0.0,
        },
        &[
            (0, 0x3e82_0df2, 0x0000_0000),
            (1, 0x3bc4_ddf0, 0x3b80_fe4e),
            (150, 0x3def_ac3e, 0xbe07_1ad4),
            (777, 0x3ded_953d, 0xbddf_899f),
            (1500, 0xbd45_a8a0, 0xbdf1_8584),
            (2999, 0xbe25_b166, 0x3d3d_b892),
        ],
    );
}
