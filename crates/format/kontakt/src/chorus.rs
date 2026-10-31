//! Choral (module `0x5d`, saved v0x51) onto [`ir::Chorus`].
//!
//! The kernel is read in `docs/re/engine/k-choral-modes.md`; this is only the
//! saved record. The reader is `0x140d00630` (`docs/re/kontakt-catalog/
//! reader_schemas.tsv` line 197: `struct BParFXChoral 0x5D 0x51 0x51 Fx7BBI`),
//! and it takes only v0x51 — any other version throws. It reads, in this
//! order, seven f32 into the parameter object's `+0x08`, `+0x0c`, `+0x10`,
//! `+0x14`, `+0x18`, `+0x1c`, `+0x20`, two bytes into `+0x24` and `+0x25`,
//! then an enum into `+0x28` (0..=3, anything else throws). Which parameter
//! each of those object offsets is comes from
//! `docs/re/kontakt-bindings/fx_other.tsv` lines 65..73, the engine-par rows
//! for `BParFXChoral`:
//!
//! | record | object | parameter | law |
//! | --- | --- | --- | --- |
//! | `0x00` | `+0x08` | `$ENGINE_PAR_CHORAL_AMOUNT` | 0..=1, identity |
//! | `0x04` | `+0x0c` | `$ENGINE_PAR_CHORAL_RATE` | 0..=1, identity |
//! | `0x08` | `+0x10` | `$ENGINE_PAR_CHORAL_DELAY` | 0..=1, identity |
//! | `0x0c` | `+0x14` | `$ENGINE_PAR_CHORAL_VOICES` | 0..=1, identity |
//! | `0x10` | `+0x18` | `$ENGINE_PAR_CHORAL_WIDTH` | 0..=1, identity |
//! | `0x14` | `+0x1c` | `$ENGINE_PAR_CHORAL_FEEDBACK` | 0..=1, identity |
//! | `0x18` | `+0x20` | `$ENGINE_PAR_CHORAL_MIX` | 0..=1, identity |
//! | `0x1c` | `+0x24` | `$ENGINE_PAR_CHORAL_INVERT_PHASE` | bool |
//! | `0x1d` | `+0x25` | `$ENGINE_PAR_CHORAL_SCATTER` | bool |
//! | `0x1e` | `+0x28` | the bank (mode) | 0..=3 |
//!
//! A record is 34 bytes and every field is present in every version, so
//! nothing is defaulted.

use kontra_ir as ir;

/// One saved Choral record, as the reader lays it out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Choral {
    /// Which of the four banks; `Chorus::new` takes it.
    pub mode: u8,
    pub amount: f32,
    pub rate: f32,
    pub delay: f32,
    pub voices: f32,
    pub width: f32,
    pub feedback: f32,
    pub mix: f32,
    pub invert: bool,
    pub scatter: bool,
}

/// The record at v0x51, the only version the reader takes.
pub(crate) fn saved(version: u16, public: &[u8]) -> Option<Choral> {
    if version != 0x51 {
        return None;
    }
    let f = |n: usize| {
        let bytes = public.get(n..n + 4)?.try_into().ok()?;
        Some(f32::from_le_bytes(bytes)).filter(|v: &f32| v.is_finite())
    };
    let flag = |n: usize| public.get(n).map(|&b| b != 0);
    let mode = i32::from_le_bytes(public.get(30..34)?.try_into().ok()?);
    // The reader throws on a mode outside 0..=3.
    let mode = u8::try_from(mode).ok().filter(|m| *m < 4)?;
    Some(Choral {
        // The par table gives every knob 0..=1; a record outside it is clamped
        // rather than played at a value the effect cannot take.
        amount: f(0)?.clamp(0.0, 1.0),
        rate: f(4)?.clamp(0.0, 1.0),
        delay: f(8)?.clamp(0.0, 1.0),
        voices: f(12)?.clamp(0.0, 1.0),
        width: f(16)?.clamp(0.0, 1.0),
        feedback: f(20)?.clamp(0.0, 1.0),
        mix: f(24)?.clamp(0.0, 1.0),
        invert: flag(28)?,
        scatter: flag(29)?,
        mode,
    })
}

/// The effect the engine runs for this record.
pub(crate) fn kind(c: Choral) -> ir::EffectKind {
    ir::EffectKind::Chorus(ir::Chorus {
        mode: c.mode,
        amount: c.amount,
        rate: c.rate,
        delay: c.delay,
        voices: c.voices,
        width: c.width,
        feedback: c.feedback,
        mix: c.mix,
        scatter: c.scatter,
        invert: c.invert,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(hex: &str) -> Vec<u8> {
        let hex: String = hex.split_whitespace().collect();
        (0..hex.len() / 2)
            .filter_map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
            .collect()
    }

    /// A record built from the layout above, read back field by field: the
    /// offsets are the point of this binding, so they are pinned.
    #[test]
    fn the_record_reads_every_field_at_its_offset() {
        let mut record = Vec::new();
        for v in [0.25f32, 0.5, 0.75, 0.2, 0.4, 0.6, 0.8] {
            record.extend_from_slice(&v.to_le_bytes());
        }
        record.push(1); // invert
        record.push(0); // scatter
        record.extend_from_slice(&2i32.to_le_bytes()); // mode
        assert_eq!(record.len(), 34);
        let c = saved(0x51, &record).expect("v0x51");
        assert_eq!(
            c,
            Choral {
                mode: 2,
                amount: 0.25,
                rate: 0.5,
                delay: 0.75,
                voices: 0.2,
                width: 0.4,
                feedback: 0.6,
                mix: 0.8,
                invert: true,
                scatter: false,
            }
        );
    }

    /// Electric Mint's Choral, saved v0x51 at instrument insert slot 6: 34
    /// bytes, mode 0, and no feedback.
    const ELECTRIC_MINT: &str =
        "a4c3bb3e54e3253e0952993e4d4b943e69e46b3f00000000ab5bcd3e000100000000";

    #[test]
    fn the_record_reads_electric_mints_choral() {
        let record = bytes(ELECTRIC_MINT);
        assert_eq!(record.len(), 34);
        let c = saved(0x51, &record).expect("v0x51");
        assert_eq!(c.mode, 0);
        assert!((c.amount - 0.366727).abs() < 1e-6);
        assert!((c.rate - 0.162).abs() < 1e-6);
        assert!((c.delay - 0.299454).abs() < 1e-6);
        assert!((c.voices - 0.289637).abs() < 1e-6);
        assert!((c.width - 0.921454).abs() < 1e-6);
        assert_eq!(c.feedback, 0.0);
        assert!((c.mix - 0.40109).abs() < 1e-6);
        assert!(!c.invert && c.scatter);
    }

    #[test]
    fn only_v51_and_only_banks_zero_to_three_read() {
        let mut record =
            bytes("0000803e0000803e0000803e0000803e0000803e0000803e0000803e00000100000000");
        assert!(saved(0x50, &record).is_none());
        assert!(saved(0x51, &record).is_some());
        // A mode of 4 is outside the four banks and the reader throws on it.
        record[30..34].copy_from_slice(&4i32.to_le_bytes());
        assert!(saved(0x51, &record).is_none());
        // A short record is not a record.
        assert!(saved(0x51, &record[..20]).is_none());
    }
}
