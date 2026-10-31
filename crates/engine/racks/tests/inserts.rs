//! A Choral slot in an instrument rack: the insert builds the effect and runs
//! it, and its knobs reach the DSP.

use kontra_ir as ir;
use kontra_racks::rack::Rack;

const RATE: u32 = 48_000;

fn choral(amount: f32, mix: f32) -> ir::Effect {
    ir::Effect {
        kind: ir::EffectKind::Chorus(ir::Chorus {
            mode: 0,
            amount,
            rate: 0.5,
            delay: 0.3,
            voices: 0.7,
            width: 0.5,
            feedback: 0.0,
            mix,
            scatter: true,
            invert: false,
        }),
        bypass: false,
        output: 1.0,
        dry: 0.0,
    }
}

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

fn render(slots: &[Option<ir::Effect>], io: &mut [[f32; 2]]) {
    let mut rack = Rack::new(slots, RATE, &[]);
    for block in io.chunks_mut(32) {
        rack.process(block, &mut [], 120.0);
    }
}

/// Choral's MIX at 0 is its dry signal, so the slot is then transparent: the
/// engine's routing runs the effect and the effect passes the input.
#[test]
fn a_choral_slot_with_no_wet_passes_the_input() {
    let mut io = stimulus(600);
    let want = io.clone();
    render(&[Some(choral(0.5, 0.0))], &mut io);
    assert_eq!(io, want);
}

/// With wet in the mix the slot changes the audio, and stays bounded.
#[test]
fn a_choral_slot_with_wet_changes_the_audio() {
    let mut io = stimulus(600);
    let dry = io.clone();
    render(&[Some(choral(0.5, 0.8))], &mut io);
    let changed = io
        .iter()
        .zip(&dry)
        .filter(|(a, b)| (a[0] - b[0]).abs() > 1e-4 || (a[1] - b[1]).abs() > 1e-4)
        .count();
    assert_eq!(changed, io.len(), "every frame differs");
    assert!(io.iter().flatten().all(|s| s.is_finite() && s.abs() < 4.0));
}

/// The knobs reach the DSP: the depth moves the read, so two amounts with
/// the same delay and mix render differently.
#[test]
fn the_amount_knob_reaches_the_kernel() {
    let run = |amount: f32| {
        let mut io = stimulus(600);
        render(&[Some(choral(amount, 1.0))], &mut io);
        io
    };
    // At amount 0 the lines barely sweep (3.5 samples at rate 0.5); at 0.8
    // they sweep about 240, so the wet is a different signal.
    assert_ne!(run(0.0), run(0.8), "the depth knob does something");
}

/// A bank move re-derives the voices: mode 2 (the longest bank) is not mode 0.
#[test]
fn the_bank_reaches_the_kernel() {
    let run = |mode: u8| {
        let mut io = stimulus(600);
        let mut slot = choral(0.5, 1.0);
        if let ir::EffectKind::Chorus(c) = &mut slot.kind {
            c.mode = mode;
        }
        render(&[Some(slot)], &mut io);
        io
    };
    assert_ne!(run(0), run(2));
}
