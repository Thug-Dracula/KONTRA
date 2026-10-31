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

    /// Hot Solo's measured response at Electric Mint's saved record, 1/3-octave,
    /// dB (docs/re/engine/k-hotsolo.md): Kontakt with insert slot 3 active minus
    /// Kontakt with it bypassed. The 40 Hz band is absent — neither signal has
    /// energy there, so it measures silence rather than response.
    const MEASURED: [(f32, f32); 25] = [
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

    #[test]
    fn an_amp_stage_reproduces_hot_solos_measured_response() {
        // The bands the Kontakt translator builds (crates/format/kontakt/src/hot_solo.rs).
        let amp = ir::Amp {
            input: 1.0,
            bands: [
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
            ],
            output: 1.885_888,
        };
        let mut stages = stages(&[ir::EffectKind::Amp(amp)], 48_000);
        let ach: Vec<f32> = MEASURED
            .iter()
            .map(|&(hz, _)| response(&mut stages, hz))
            .collect();
        // The level is asserted, not absorbed. An earlier version subtracted
        // this offset before checking the shape, which meant the stage gain --
        // the number that was miscalibrated twice against the wrong reference,
        // and the whole reason this curve exists -- could move by decibels with
        // the suite green. Measured today: +0.0101 dB.
        let offset = MEASURED
            .iter()
            .zip(&ach)
            .map(|((_, want), got)| got - want)
            .sum::<f32>()
            / ach.len() as f32;
        let worst = MEASURED
            .iter()
            .zip(&ach)
            .map(|((_, want), got)| (got - want - offset).abs())
            .fold(0.0f32, f32::max);
        assert!(
            offset.abs() < 0.2,
            "the stage's level is {offset:+.2} dB off Kontakt's curve; the gain is not a free parameter"
        );
        assert!(
            worst < 1.2,
            "worst band error {worst:.2} dB (offset {offset:+.2} dB)"
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
