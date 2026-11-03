//! The Solid G-EQ's script knobs (`$ENGINE_PAR_SEQ_*`).

use kontra_ir as ir;

use crate::script_knobs::{SlotKnob, sampled};

/// The Solid G-EQ's knobs through [`crate::laws::solid_eq`], the others at
/// their saved values: `saved` is LF gain and Hz, LMF and HMF gain, Hz and
/// Q, HF gain and Hz (each 0..=1, the KSP value in millionths, as v2
/// sampler-kontakt effects/eq.rs maps `$ENGINE_PAR_SEQ_*`), with the bell
/// switches' shapes.
// ponytail: a knob's law holds the other knobs at their saved values, so a
// script moving two coupled knobs (a shelf's gain also moves its corner)
// gets the second one's corner; the bell switches are not script knobs yet.
pub(crate) fn solid_knobs(saved: [f32; 10], shapes: [ir::Shape; 2]) -> Vec<SlotKnob> {
    use ir::EffectParam::{BellGain, BellHz, BellQ};
    use ir::SlotKnob::{BandGain, BandHz, BandWidth};
    let eq = |k: usize, x: f32| {
        let mut v = saved;
        *v.get_mut(k)? = x;
        let [lg, lh, mg, mh, mq, hg, hh, hq, fg, fh] = v;
        let [lf, hf] = shapes;
        let bands = crate::laws::solid_eq([lg, lh], lf, [mg, mh, mq], [hg, hh, hq], [fg, fh], hf);
        Some(ir::EffectKind::Eq(bands))
    };
    let knobs = [
        BandGain(0),
        BandHz(0),
        BandGain(1),
        BandHz(1),
        BandWidth(1),
        BandGain(2),
        BandHz(2),
        BandWidth(2),
        BandGain(3),
        BandHz(3),
    ];
    let mut out = Vec::new();
    for (k, knob) in knobs.into_iter().enumerate() {
        let band = match knob {
            BandGain(b) | BandHz(b) | BandWidth(b) => b,
            _ => continue,
        };
        // The knob's own field first: a read takes the first law's field, and
        // the gain also moves the Q and the Hz of a shelf, so a read of
        // `$ENGINE_PAR_SEQ_LMF_GAIN` must land on the gain, not on a coupled
        // field (`slot_x`). Fracture Sounds' Brass Band Soloists stores its
        // articulation and round-robin tags in these knobs and gates every
        // group on reading them back.
        let fields = match knob {
            BandGain(_) => [BellGain(band), BellQ(band), BellHz(band)],
            BandHz(_) => [BellHz(band), BellGain(band), BellQ(band)],
            _ => [BellQ(band), BellGain(band), BellHz(band)],
        };
        out.extend(sampled(knob, &fields, &|x| eq(k, x)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A read (`Engine::slot_x`) takes the **first** law's field, so a knob
    /// whose law moves several fields must list its own field first. The gain
    /// also moves the Q of a peak band, so with the fields in any other order
    /// `$ENGINE_PAR_SEQ_LMF_GAIN` reads back the Q's setting: Fracture Sounds'
    /// Brass Band Soloists stores its articulation tag in the LF gain and its
    /// round-robin index in the LMF gain, and gates every group on reading
    /// them back (measured 2026-11-03: the gate refused all 4,035 zones).
    #[test]
    fn a_knob_reads_back_through_its_own_field() {
        use ir::EffectParam::{BellGain, BellHz, BellQ};
        use ir::SlotKnob::{BandGain, BandHz, BandWidth};
        let saved = [9e-6, 1e-6, 1e-6, 0.5, 0.0, 0.5, 0.0, 0.5, 0.5, 0.5];
        let knobs = solid_knobs(saved, [ir::Shape::HighShelf, ir::Shape::Peak]);
        let first = |knob| {
            knobs
                .iter()
                .find(|(k, _, _)| *k == knob)
                .map(|(_, field, _)| *field)
        };
        for (knob, field) in [
            (BandGain(0), BellGain(0)),
            (BandHz(0), BellHz(0)),
            (BandGain(1), BellGain(1)),
            (BandWidth(1), BellQ(1)),
            (BandGain(3), BellGain(3)),
        ] {
            assert_eq!(first(knob), Some(field), "{knob:?}");
        }
    }

    #[test]
    fn a_solid_eq_knob_lands_where_its_law_puts_the_band() {
        let saved = [0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5];
        let shapes = [ir::Shape::LowShelf, ir::Shape::HighShelf];
        let knobs = solid_knobs(saved, shapes);
        let law = |knob, field| {
            let found = knobs.iter().find(|(k, f, _)| *k == knob && *f == field);
            found.unwrap().2.clone()
        };
        let band = |lmf_gain: f32| {
            let mid = [lmf_gain, 0.5, 0.5];
            crate::laws::solid_eq([0.5; 2], shapes[0], mid, [0.5; 3], [0.5; 2], shapes[1])[1]
        };
        let gain = law(ir::SlotKnob::BandGain(1), ir::EffectParam::BellGain(1));
        assert!((gain.eval(0.75) - band(0.75).gain_db).abs() < 1e-4);
        // The gain also moves the LMF's Q, as the core couples them.
        let q = law(ir::SlotKnob::BandGain(1), ir::EffectParam::BellQ(1));
        assert!((q.eval(0.75) - band(0.75).q).abs() < 1e-4);
        assert!(knobs.iter().any(|(k, _, _)| *k == ir::SlotKnob::BandHz(3)));
    }
}
