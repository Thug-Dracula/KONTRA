//! Supercharger GT (module 0x60, saved v0x51) as the level it applies.
//!
//! The module's dynamics is `NI::DSP::CSL::Rockruepel_CompXT` (its wrapper
//! `0x1407f3d70` owns the core `0x141a820e0`, docs/re/engine/dyn-compressors.md
//! §"Supercharger GT (0x60, version 0x51)"); the CompXT's gain computer,
//! detector and ballistics are not recovered, so the *compression* is not
//! modelled. What is modelled is the one number that is: the module's static
//! make-up.
//!
//! Kontakt's own transfer at these settings is a flat gain, measured with the
//! Wine oracle over a 200 Hz ramp at every input level 0.2..1.0
//! (`docs/re/engine/vectors/supercharger-static.tsv`):
//!
//! | saved knob | transfer |
//! |---|---|
//! | TRIM 0 / 0.5 / 1.0 | 1.0716 … 1.0717, flat |
//! | SATURATION 0 / 0.5 | 1.0716 … 1.0717, flat |
//! | COMPRESS 0 / 0.5 / 1.0 | 1.0716 … 1.0717, flat (0.5 sags to 1.067 above 0.8) |
//!
//! The value is a constant in the binary, not a fit: `1.0716536` is the f32 at
//! `0x1479cb194` and `0x1479d1150`, and the f64 `1.0716535899579933` at
//! `0x1479b6048` and `0x1479bd140`
//! (`docs/re/kontakt-rdata/constants.tsv`). Electric Mint stores SATURATION 0,
//! COMPRESS 1.0 and CHARACTER 1.0, every one of which the vector reads as that
//! constant.
//!
//! The slot's stored Output is applied on top (`ir::Effect::output`), as for
//! every other module: Electric Mint's is 0.7035764, so the slot's own gain is
//! `0.7035764 × 1.0716536 = 0.7540` (−2.45 dB). Kontakt's slot measures
//! **−2.23 dB** on the parity note (`docs/re/electric-mint-slot-liveness-
//! 2026-10-31.md`, bypassing slot 2 moves the render +2.228 dB, spread 0.144);
//! the 0.22 dB difference is the CompXT's own level dependence and its MIX
//! blend, neither of which is recovered, and is reported rather than fitted.
//!
//! SATURATION above 0 makes the transfer level-dependent (1.0045 at 0.6, 1.0315
//! at 1.0 in the same vector); that is reported, not modelled.

use kontra_format::Notes;
use kontra_ir as ir;

/// The module's static make-up, Kontakt's own constant (`0x1479cb194` f32,
/// `0x1479b6048` f64); the measured flat transfer is this to four decimals.
pub(crate) const MAKEUP: f32 = 1.071_653_6;

/// The record's SATURATION byte (field 1, `$ENGINE_PAR_SUPERGT_SATURATION`).
const SATURATION: usize = 12;

/// The slot's effect: the module's make-up as a gain, at the reading the
/// measured vector covers.
pub(crate) fn effect(public: &[u8], at: &str, notes: &mut Notes) -> ir::EffectKind {
    let saturation = public
        .get(SATURATION..SATURATION + 4)
        .and_then(|b| b.try_into().ok())
        .map_or(0.0, f32::from_le_bytes);
    if saturation > 0.0 {
        notes.add(
            ir::Impact::Audible,
            "Supercharger saturation (level-dependent transfer)",
            at.to_owned(),
        );
    }
    notes.add(
        ir::Impact::Audible,
        "Supercharger gain only (CompXT compression not modelled)",
        at.to_owned(),
    );
    ir::EffectKind::Gain(MAKEUP)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Electric Mint's Supercharger record, as saved (45 bytes).
    const RECORD: &str = "f2d20d3f0000000054e3253e000000000000803f0000803f6021533e\
                          0200000001000000000000000200000000";

    fn bytes(hex: &str) -> Vec<u8> {
        let hex: String = hex.split_whitespace().collect();
        (0..hex.len() / 2)
            .filter_map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
            .collect()
    }

    /// The saved make-up is the constant, and the slot's stored output is what
    /// turns it into Kontakt's −2.23 dB: 0.7035764 × 1.0716536 = 0.7540.
    #[test]
    fn the_make_up_is_kontakts_constant() {
        assert!((MAKEUP - 1.071_653_6).abs() < 1e-7);
        let slot = 0.703_576_4f32 * MAKEUP;
        assert!((slot - 0.754_07).abs() < 1e-4, "{slot}");
        assert!(
            (20.0 * slot.log10() + 2.228).abs() < 0.25,
            "{}",
            20.0 * slot.log10()
        );
    }

    /// Electric Mint's record stores SATURATION 0, so the flat law holds and
    /// only the compression is reported missing.
    #[test]
    fn the_saved_saturation_is_off_and_only_the_compression_is_reported() {
        let record = bytes(RECORD);
        let mut notes = Notes::default();
        let kind = effect(&record, "instrument insert slot 2", &mut notes);
        assert_eq!(kind, ir::EffectKind::Gain(MAKEUP));
        let missing: Vec<_> = notes.finish().into_iter().map(|n| n.feature).collect();
        assert!(
            missing
                .iter()
                .any(|f| f.contains("compression not modelled"))
        );
        assert!(!missing.iter().any(|f| f.contains("saturation")));
    }
}
