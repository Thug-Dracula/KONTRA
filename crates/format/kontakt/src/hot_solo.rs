//! Hot Solo (module 0x54, version 0x10) as the engine's amp stage: its saved
//! record (reader 0x140d01b50) and the tone response measured through Kontakt.
//!
//! What is modelled is the module's **linear** tone stage at one measured
//! operating point: Electric Mint's saved record, which is what the parity
//! render compares. The DSP class (`.?AVBEffectHotSolo@@`, vtable
//! 0x144fc0900) and its block process (slot 23, 0x140aa2ae0) are located, and
//! the per-block structure is read — 32-frame chunks, a pending-recalc flag at
//! +0x715c run through 0x140ad5440, a second at +0x715d through 0x140ad44f0
//! (whose body is the state shuffle, not a coefficient law), and the output
//! tail 0x140ac21f0 that ramps one gain per sample by `(target − now)/180`
//! (dat_144f38638 = 1/180) before the bus sum — but the coefficient law in
//! 0x140ad5440 and the piecewise shaper in kernel 0x140aab1e0 are not
//! recovered, so the bands below are the measured transfer function, not a
//! reconstruction of that code.
//!
//! The measurement (docs/re/engine/k-hotsolo.md): the stage is **ablated inside
//! Kontakt** — Kontakt with slot 3 active against Kontakt with slot 3's stored
//! bypass flag set, twelve fresh sessions, first render of each (round robin 0)
//! — so the curve is the stage and nothing else. Kontakt's own contribution is
//! **+7.00 dB** of level (floor 0.06 dB); the fitted stage below matches its
//! 1/3-octave curve to 0.40 dB RMS (0.98 dB worst band) — the residual
//! `tools/dsp-research/hot_solo_fit.py` prints for these constants.
//!
//! The previous curve was measured the other way: Kontakt's full chain against
//! a KONTRA that passed slot 3 through. That is not a differential — every
//! other stage's error, and the base error the 2026-10-31 liveness work found,
//! were inside it — and it read **1.73 dB low**, which is why this stage used
//! to price 3.9 dB under Kontakt's.

use kontra_ir as ir;

/// The record's eight floats and two flags (reader 0x140d01b50, v0x10).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Saved {
    pub prenormal: f32,
    pub preoverdrive: f32,
    pub master: f32,
    pub presence: f32,
    pub depth: f32,
    pub treble: f32,
    pub mid: f32,
    pub bass: f32,
    /// The overdrive channel select (the record's byte at +0x28).
    pub overdrive: bool,
    /// Mono (the record's byte at +0x29).
    pub mono: bool,
}

impl Saved {
    /// The saved record, or none when it is shorter than its reader needs.
    pub fn read(public: &[u8]) -> Option<Self> {
        let f = |n: usize| {
            let bytes = public.get(n..n + 4)?.try_into().ok()?;
            Some(f32::from_le_bytes(bytes)).filter(|v| v.is_finite())
        };
        Some(Self {
            prenormal: f(0)?,
            preoverdrive: f(4)?,
            master: f(8)?,
            presence: f(12)?,
            depth: f(16)?,
            treble: f(20)?,
            mid: f(24)?,
            bass: f(28)?,
            overdrive: public.get(32).is_some_and(|&b| b != 0),
            mono: public.get(33).is_some_and(|&b| b != 0),
        })
    }
}

/// The record the bands were measured at: Electric Mint's insert slot 3, as the
/// file stores it (the bits are the record's own; two knobs near 0.603 differ in
/// the last places, which is why comparisons below carry a tolerance).
pub const MEASURED: Saved = Saved {
    prenormal: f32::from_bits(0x3eaa_8ae7),
    preoverdrive: f32::from_bits(0x3ef0_eb46),
    master: f32::from_bits(0x3ed7_485e),
    presence: f32::from_bits(0x3f76_51a0),
    depth: f32::from_bits(0x3f1a_3476),
    treble: f32::from_bits(0x3f1a_8145),
    mid: f32::from_bits(0x3f40_5e92),
    bass: f32::from_bits(0x3f24_9a24),
    overdrive: false,
    mono: false,
};

/// The measured tone stage, in series: low shelf, presence/upper-mid peak,
/// treble shelf. A RBJ shelf's `q` is its slope (1 the steepest knee).
///
/// Least squares of the model below against the ablated-in-Kontakt curve
/// (`tools/dsp-research/hot_solo_fit.py` prints them); residual 0.40 dB RMS.
pub const BANDS: [ir::Bell; 4] = [
    ir::Bell {
        shape: ir::Shape::LowShelf,
        hz: 43.61,
        q: 1.0,
        gain_db: -6.569_262,
    },
    ir::Bell {
        shape: ir::Shape::Peak,
        hz: 3_654.338,
        q: 0.260_086,
        gain_db: 16.101_653,
    },
    ir::Bell {
        shape: ir::Shape::HighShelf,
        hz: 5_150.529,
        q: 1.0,
        gain_db: 2.546_798,
    },
    ir::Bell::FLAT,
];

/// The stage's own gain at [`MEASURED`], linear (+5.51 dB): the record's
/// prenormal and master make up more than they take away.
pub const MEASURED_GAIN: f32 = 1.885_888;

/// Hot Solo's measured response at [`MEASURED`], 1/3-octave centres and dB
/// (`docs/re/engine/k-hotsolo.md`): Kontakt with insert slot 3 active minus
/// Kontakt with it bypassed, so it is the stage alone and its level is
/// absolute. The 40 Hz band is absent — neither signal has energy there, so it
/// measures silence rather than response.
///
/// [`BANDS`] is a least-squares fit to this curve,
/// `tools/dsp-research/hot_solo_fit.py`.
pub const MEASURED_CURVE: [(f32, f32); 25] = [
    (50.0, 3.01),
    (63.0, 4.97),
    (80.0, 4.34),
    (101.0, 5.17),
    (127.0, 6.07),
    (160.0, 6.49),
    (202.0, 6.81),
    (254.0, 7.25),
    (320.0, 8.12),
    (403.0, 8.46),
    (508.0, 9.05),
    (640.0, 11.00),
    (806.0, 12.74),
    (1016.0, 14.38),
    (1280.0, 15.79),
    (1613.0, 18.78),
    (2032.0, 19.65),
    (2560.0, 20.39),
    (3225.0, 21.79),
    (4064.0, 21.97),
    (5120.0, 22.16),
    (6451.0, 21.15),
    (8127.0, 19.83),
    (10240.0, 18.02),
    (12902.0, 14.80),
];

/// The record as the engine's amp. `lost` names what the record asks for and
/// this model does not carry.
pub fn amp(saved: Saved, at: &str) -> (ir::Amp, Option<String>) {
    // Prenormal and master are gains, and the record's own run through them is
    // in the measured bands: their law is the ratio to the measured point.
    // Native evidence: master 0.5 -> 1.0 raised level 5 dB, where a
    // proportional law predicts 6 (that render clipped at 1.0); both mute the
    // output at 0.
    //
    // No test re-states this ratio. Beyond the measured point and the two
    // mutes it is unmeasured, and the one native pair argues the law is not
    // strictly proportional, so a test that pinned the arithmetic would block
    // the correction a taper needs. The measured point itself is pinned by
    // `the_measured_record_is_the_engine_amp_with_no_note`.
    let gain = if saved.prenormal > 0.0 && saved.master > 0.0 {
        let gain = MEASURED_GAIN
            * (saved.prenormal / MEASURED.prenormal)
            * (saved.master / MEASURED.master);
        // `Saved::read` admits any *finite* float, so two finite knobs can
        // still multiply past `f32::MAX` and the gain becomes `inf` — which
        // the amp then writes into every sample, poisoning the render with
        // `inf`/NaN. The native stage cannot carry such a value (its gain is
        // one product of bounded knob floats), so there is nothing to clamp
        // to: the record means nothing this model can price, and the answer
        // the record already gives at 0 applies — silence.
        if gain.is_finite() { gain } else { 0.0 }
    } else {
        0.0
    };
    let value = ir::Amp {
        input: 1.0,
        bands: BANDS,
        output: gain,
    };
    kontra_law::record(|| kontra_law::Application {
        id: kontra_law::LawId("kontakt.hot_solo.v1"),
        location: format!("{at}/hot_solo"),
        input: format!("{saved:?}"),
        output: format!("{value:?}"),
    });
    let mut lost = Vec::new();
    let tone = [
        ("presence", saved.presence, MEASURED.presence),
        ("depth", saved.depth, MEASURED.depth),
        ("treble", saved.treble, MEASURED.treble),
        ("mid", saved.mid, MEASURED.mid),
        ("bass", saved.bass, MEASURED.bass),
    ];
    let off: Vec<_> = tone
        .iter()
        .filter(|(_, value, measured)| (value - measured).abs() > 1e-3)
        .map(|(name, _, _)| *name)
        .collect();
    if !off.is_empty() {
        lost.push(format!(
            "Hot Solo tone knobs not modelled ({})",
            off.join(", ")
        ));
    }
    if saved.overdrive {
        // Pre-overdrive is inert while that channel is off (measured 2026-10-21:
        // 0, 0.5 and 1 give identical output), so only the channel itself is lost.
        lost.push("Hot Solo overdrive channel".to_owned());
    }
    if saved.mono {
        lost.push("Hot Solo mono".to_owned());
    }
    (value, lost.into_iter().reduce(|a, b| format!("{a}; {b}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The record as Electric Mint stores it (module 0x54, v0x10, insert slot 3).
    fn electric_mint() -> Vec<u8> {
        let mut out = Vec::new();
        for value in [
            MEASURED.prenormal,
            MEASURED.preoverdrive,
            MEASURED.master,
            MEASURED.presence,
            MEASURED.depth,
            MEASURED.treble,
            MEASURED.mid,
            MEASURED.bass,
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn the_reader_takes_the_eight_floats_and_two_flags() {
        let saved = Saved::read(&electric_mint()).unwrap();
        assert_eq!(saved, MEASURED);
        assert!(!saved.overdrive && !saved.mono);
        assert_eq!(
            Saved::read(&[0; 31]),
            None,
            "eight floats are the reader's minimum"
        );
        assert!(
            !Saved::read(&[0; 32]).unwrap().overdrive,
            "a missing flag reads as off"
        );
    }

    #[test]
    fn the_measured_record_is_the_engine_amp_with_no_note() {
        let (amp, lost) = amp(MEASURED, "rack");
        assert_eq!(amp.bands, BANDS);
        assert!((amp.gain() - MEASURED_GAIN).abs() < 1e-6);
        assert_eq!(lost, None);
    }

    #[test]
    fn a_record_at_another_tone_is_reported_not_silently_retuned() {
        let other = Saved {
            presence: 0.2,
            bass: 1.0,
            ..MEASURED
        };
        let (amp, lost) = amp(other, "rack");
        assert_eq!(amp.bands, BANDS, "the bands are the measured ones");
        let lost = lost.unwrap();
        assert!(lost.contains("presence") && lost.contains("bass"), "{lost}");
        assert!(!lost.contains("treble"), "{lost}");
    }

    #[test]
    fn the_overdrive_channel_is_reported() {
        let (_, lost) = amp(
            Saved {
                overdrive: true,
                ..MEASURED
            },
            "rack",
        );
        assert!(lost.unwrap().contains("overdrive channel"));
    }

    /// A record whose knobs are finite but whose product is not: the gain
    /// must not reach `inf`, because everything downstream would.
    #[test]
    fn a_gain_that_would_overflow_silences_the_stage() {
        let saved = Saved {
            prenormal: f32::MAX,
            master: 1.0,
            ..MEASURED
        };
        let gain = amp(saved, "rack").0.gain();
        assert!(gain.is_finite(), "the gain overflowed: {gain}");
        assert_eq!(gain, 0.0, "an unpriceable record mutes");
    }

    #[test]
    fn a_muted_gain_silences_the_stage() {
        for saved in [
            Saved {
                prenormal: 0.0,
                ..MEASURED
            },
            Saved {
                master: 0.0,
                ..MEASURED
            },
        ] {
            assert_eq!(amp(saved, "rack").0.gain(), 0.0);
        }
    }
}
