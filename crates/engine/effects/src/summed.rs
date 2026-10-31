//! EQ bands and filters as stages on summed audio.

use kontra_dsp::eq::{Bell, Biquad, Side};
use kontra_dsp::filter::{Formant, LadderKind, Pass, Shape, Svf, VowelSet, Vowels};
use kontra_ir as ir;

/// The rack's active EQs (their audible bands) and filters, in slot order.
pub fn sections(rack: &[Option<ir::Effect>]) -> Vec<ir::EffectKind> {
    rack.iter()
        .flatten()
        .filter(|e| !e.bypass)
        .filter_map(|e| match e.kind {
            ir::EffectKind::Eq(bands) => bands.iter().any(|b| b.gain_db != 0.0).then_some(e.kind),
            kind @ (ir::EffectKind::Amp(_)
            | ir::EffectKind::Filter(_)
            | ir::EffectKind::Formant(_)) => Some(kind),
            _ => None,
        })
        .collect()
}

/// One EQ band as its biquad at `rate`.
fn band(b: &ir::Bell, rate: u32) -> Biquad {
    match b.shape {
        ir::Shape::Peak => {
            let band = Bell {
                hz: b.hz,
                q: b.q,
                gain_db: b.gain_db,
            };
            kontra_dsp::eq::bell(band, rate)
        }
        ir::Shape::LowShelf => kontra_dsp::eq::shelf(b.hz, b.gain_db, b.q, Side::Low, rate),
        ir::Shape::HighShelf => kontra_dsp::eq::shelf(b.hz, b.gain_db, b.q, Side::High, rate),
    }
}

/// A group's EQ run on one voice: its four bands, moved by their routes.
#[derive(Clone)]
pub struct VoiceEq {
    biquads: [Biquad; 4],
}

impl VoiceEq {
    pub fn new(bells: [ir::Bell; 4], rate: u32) -> Self {
        Self {
            biquads: bells.map(|b| band(&b, rate)),
        }
    }

    /// Moves the bands' coefficients and keeps their state.
    pub fn retune(&mut self, bells: [ir::Bell; 4], rate: u32) {
        for (f, b) in self.biquads.iter_mut().zip(&bells) {
            f.retune(&band(b, rate));
        }
    }

    pub fn process(&mut self, io: &mut [[f32; 2]]) {
        for f in &mut self.biquads {
            f.process(io);
        }
    }
}

/// A section's runner: biquads in series, a formant's parallel bands, or a
/// ladder (not a set of biquads).
pub enum Stage {
    Biquad(Biquad),
    /// A linear gain (an amp's input times its output).
    Gain(f32),
    // Boxed: its glide state is six times a biquad; built at load.
    Formant(Box<Formant>),
    Running(Box<Svf>),
}

impl Stage {
    pub fn is_silent(&self) -> bool {
        match self {
            Self::Gain(g) => *g == 0.0,
            Self::Biquad(b) => b.is_silent(),
            Self::Formant(f) => f.is_silent(),
            Self::Running(f) => f.is_silent(),
        }
    }
}

pub fn stages(sections: &[ir::EffectKind], rate: u32) -> Box<[Stage]> {
    let mut out = Vec::new();
    for section in sections {
        match section {
            ir::EffectKind::Eq(bands) => {
                out.extend(
                    bands
                        .iter()
                        .filter(|b| b.gain_db != 0.0)
                        .map(|b| Stage::Biquad(band(b, rate))),
                );
            }
            ir::EffectKind::Amp(a) => {
                out.extend(
                    a.bands
                        .iter()
                        .filter(|b| b.gain_db != 0.0)
                        .map(|b| Stage::Biquad(band(b, rate))),
                );
                out.push(Stage::Gain(a.gain()));
            }
            ir::EffectKind::Filter(f) => {
                let shape = shape(f);
                if shape.ladder.is_some() {
                    let mut svf = Svf::new(shape);
                    svf.set_cutoff(shape.hz, rate);
                    out.push(Stage::Running(Box::new(svf)));
                    continue;
                }
                out.extend(
                    kontra_dsp::filter::sections(&shape, rate)
                        .into_iter()
                        .map(Stage::Biquad),
                );
            }
            ir::EffectKind::Formant(f) => {
                let v = Vowels {
                    talk: f.talk,
                    sharp: f.sharp,
                    size: f.size,
                    set: match f.set {
                        ir::VowelSet::First => VowelSet::First,
                        ir::VowelSet::Second => VowelSet::Second,
                    },
                };
                out.push(Stage::Formant(Box::new(Formant::new(v, rate))));
            }
            _ => {}
        }
    }
    out.into()
}

pub fn shape(f: &ir::Filter) -> Shape {
    let pass = match f.pass {
        ir::Pass::Low => Pass::Low,
        ir::Pass::High => Pass::High,
        ir::Pass::Band => Pass::Band,
        ir::Pass::Notch => Pass::Notch,
    };
    Shape {
        pass,
        hz: f.hz,
        stages: f.stages,
        ladder: f.ladder.map(|l| match l {
            ir::Ladder::OnePole => LadderKind::OnePole,
            ir::Ladder::ZeroDelay(taps) => LadderKind::ZeroDelay(taps),
            ir::Ladder::Explicit { taps, fat } => LadderKind::Explicit { taps, fat },
            ir::Ladder::SallenKey { morph, curve } => LadderKind::SallenKey { morph, curve },
        }),
        damping: f.damping,
        gain: f.gain,
    }
}

pub fn run(stages: &mut [Stage], io: &mut [[f32; 2]]) {
    for stage in stages {
        match stage {
            Stage::Biquad(b) => b.process(io),
            Stage::Gain(g) => {
                let g = *g;
                for frame in io.iter_mut() {
                    frame[0] *= g;
                    frame[1] *= g;
                }
            }
            Stage::Formant(f) => f.process(io),
            Stage::Running(f) => f.process(io),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stage's magnitude at `hz`, from its impulse response.
    fn magnitude(stages: &mut [Stage], hz: f32) -> f32 {
        let mut io = vec![[0.0f32; 2]; 16_384];
        io[0] = [1.0, 1.0];
        run(stages, &mut io);
        let w = std::f64::consts::TAU * f64::from(hz) / 48_000.0;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (n, frame) in io.iter().enumerate() {
            re += f64::from(frame[0]) * (w * n as f64).cos();
            im -= f64::from(frame[0]) * (w * n as f64).sin();
        }
        (20.0 * (re * re + im * im).sqrt().max(1e-30).log10()) as f32
    }

    /// The band's level: the power average over a 1/3 octave, as a band analyser
    /// reads it (a single centre frequency would miss a shelf's slope).
    fn response(stages: &mut [Stage], centre: f32) -> f32 {
        let (lo, hi) = (centre / 2f32.powf(1.0 / 6.0), centre * 2f32.powf(1.0 / 6.0));
        let points = 15;
        let mut power = 0.0f64;
        for n in 0..points {
            let hz = lo * (hi / lo).powf(n as f32 / (points - 1) as f32);
            let db = magnitude(stages, hz);
            power += 10f64.powf(f64::from(db) / 10.0);
        }
        (10.0 * (power / f64::from(points)).log10()) as f32
    }

    /// How far the modelled stage's level may sit from the measured curve. The
    /// measurement's own floor is 0.06 dB and the fit lands at 0.01 dB; the
    /// stage gain that was shipped wrong is 3.9 dB of error.
    const LEVEL_TOLERANCE_DB: f32 = 0.35;

    /// Hot Solo's measured response, against the amp the Kontakt translator
    /// builds for it (`crates/format/kontakt/src/hot_solo.rs`): the translator's
    /// own stage, not a copy, so its gain is what this test prices.
    #[test]
    fn an_amp_stage_reproduces_hot_solos_measured_response() {
        let (amp, lost) =
            kontra_format_kontakt::hot_solo::amp(kontra_format_kontakt::hot_solo::MEASURED, "rack");
        assert_eq!(lost, None, "the measured record is the modelled one");
        let mut stages = stages(&[ir::EffectKind::Amp(amp)], 48_000);
        let curve = kontra_format_kontakt::hot_solo::MEASURED_CURVE;
        let ach: Vec<f32> = curve
            .iter()
            .map(|&(hz, _)| response(&mut stages, hz))
            .collect();
        // The curve is the stage alone — Kontakt with the slot active minus
        // Kontakt with it bypassed — so its level is absolute: the model has to
        // sit on the curve, not on a shifted copy of it. A gain error moves
        // every band together, and the stage gain is the number that has been
        // wrong twice; a free offset (the mean error) used to absorb exactly
        // that. Both this level and the parity render's price (+6.963 dB here
        // against Kontakt's +7.002, docs/re/electric-mint-base-gain-2026-10-31.md)
        // move one for one with it.
        let level = ach.iter().sum::<f32>() / ach.len() as f32
            - curve.iter().map(|&(_, db)| db).sum::<f32>() / curve.len() as f32;
        assert!(
            level.abs() < LEVEL_TOLERANCE_DB,
            "the stage's level is {level:+.2} dB off the measured curve (tolerance {LEVEL_TOLERANCE_DB})"
        );
        // The shape, its level removed: what the fit fixes. Measured residual
        // of the fit: 0.40 dB RMS, 0.98 worst band.
        let worst = curve
            .iter()
            .zip(&ach)
            .map(|((_, want), got)| (got - want - level).abs())
            .fold(0.0f32, f32::max);
        assert!(
            worst < 1.2,
            "worst band error {worst:.2} dB (level {level:+.2} dB)"
        );
    }

    #[test]
    fn an_amp_gain_scales_the_stage() {
        let amp = ir::Amp {
            input: 2.0,
            bands: [ir::Bell::FLAT; 4],
            output: 0.5,
        };
        let mut stages = stages(&[ir::EffectKind::Amp(amp)], 48_000);
        let mut io = [[1.0f32, 1.0]];
        run(&mut stages, &mut io);
        assert!((io[0][0] - 1.0).abs() < 1e-6, "{:?}", io[0]);
    }
}
