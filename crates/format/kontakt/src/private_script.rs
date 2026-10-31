//! The scripts Kontakt 8 keeps in a program's private data.
//!
//! Older programs save each `BParScript` as a `0x06` child chunk. Kontakt
//! 8.13's program private version `0x80` writes the same object inline
//! instead, once per script slot. A record is the emulated structured-object
//! header (flag `0`, version `0x60`) followed by exactly the fields
//! `BParScript::params` reads: the source (u32 length, any negative length is
//! "none"), source-editor-open, touched-but-not-applied, bypass, the password
//! hash (u32 length), the description and text-file name (optional strings),
//! and the saved persistent values (count, then u32 length + text each).
//!
//! Five slots follow the private scalars in slot order, and a slot with no
//! script is still written (no source, no names, so the record is 26 bytes).
//! `Cherry Electric Bass` (Orange Tree Samples) and `EEB Rick - Middle Only`
//! (Evolution) both open with `00 60 00` 0x41 bytes into the section and end
//! with the same 16-byte tail (`0, 0, 0, -1` as four u32); the tail is not a
//! record and this reader does not use it.
//!
//! The bypass flag is honoured as a `0x06` chunk's is: the script does not
//! run. A password hash protects a script only in Kontakt's own API — the
//! source is stored anyway — so a protected slot loads like any other, and
//! one whose source is missing is reported rather than run empty.

/// One saved script slot.
pub(crate) struct Slot {
    pub slot: usize,
    /// The description Kontakt shows, else the text-file name.
    pub name: String,
    pub source: String,
    pub bypass: bool,
    pub protected: bool,
    /// Saved persistent values, `"<name> <values>"` each.
    pub saved: Vec<String>,
}

/// The script slots a program's private data holds, in slot order.
///
/// `None` when the private carries no script section, or when one is present
/// but does not read cleanly. The section is found by its own framing, as
/// [`crate::links`] finds the MIDI assignments: the private scalars before it
/// are not decoded.
pub(crate) fn slots(private: &[u8]) -> Option<Vec<Slot>> {
    let start = find_start(private)?;
    let mut at = start;
    let mut out = Vec::with_capacity(SLOTS);
    while out.len() < SLOTS {
        let Some(mut slot) = record(private, &mut at) else {
            break;
        };
        slot.slot = out.len();
        out.push(slot);
    }
    // What follows the last slot is a short fixed tail, not a record.
    let left = private.len().checked_sub(at)?;
    (left <= TAIL).then_some(out).filter(|s| !s.is_empty())
}

/// The first offset whose records run out at the end of `private`.
///
/// A program without scripts keeps the scalars only, so every candidate that
/// is not the section fails before it reaches the end.
fn find_start(private: &[u8]) -> Option<usize> {
    (0..private.len().saturating_sub(MARKER.len() - 1))
        .filter(|&at| private.get(at..at + 3) == Some(MARKER))
        .find(|&at| {
            let mut end = at;
            let mut kept = 0;
            while kept < SLOTS && record(private, &mut end).is_some() {
                kept += 1;
            }
            kept > 0 && private.len().saturating_sub(end) <= TAIL
        })
}

/// One record at `*at`, which advances past it.
fn record(b: &[u8], at: &mut usize) -> Option<Slot> {
    let mut r = Reader(b, *at);
    if r.bytes(MARKER.len())? != MARKER {
        return None;
    }
    let source = match r.i32()? {
        len if len < 0 => String::new(),
        len => {
            let bytes = r.bytes(usize::try_from(len).ok()?)?;
            ni_file::kontakt::objects::BParScript::decode_source(bytes.to_vec())
        }
    };
    let _source_editor_open = r.u8()?;
    let _touched = r.u8()?;
    let bypass = r.u8()? != 0;
    let password = r.i32()?;
    let protected = password > 0;
    if protected {
        let len = usize::try_from(password).ok().filter(|&n| n <= 1024)?;
        r.bytes(len)?;
    }
    let description = r.optional_string()?;
    let textfile = r.optional_string()?;
    let count = r.u32()?;
    if count > MAX_PERSISTENT {
        return None;
    }
    let mut saved = Vec::with_capacity(usize::try_from(count).unwrap_or(0).min(64));
    for _ in 0..count {
        let len = r.i32()?;
        let bytes = r.bytes(usize::try_from(len).ok().filter(|&n| n <= MAX_TEXT)?)?;
        saved.push(String::from_utf8_lossy(bytes).into_owned());
    }
    *at = r.1;
    Some(Slot {
        slot: 0,
        name: description.or(textfile).unwrap_or_default(),
        source,
        bypass,
        protected,
        saved,
    })
}

/// The record header: structured flag `0`, then BParScript version `0x60`.
const VERSION: u16 = 0x60;
const MARKER: &[u8] = &[0, VERSION as u8, (VERSION >> 8) as u8];
/// Kontakt's per-instrument script slots.
const SLOTS: usize = 5;
/// The bytes after the last slot record: `0, 0, 0, -1` in every file read.
const TAIL: usize = 16;
/// A persistent value or name longer than this is not one.
const MAX_TEXT: usize = 1 << 16;
/// More saved values than this is not a script.
const MAX_PERSISTENT: u32 = 4096;

struct Reader<'a>(&'a [u8], usize);

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let b = self.0.get(self.1..self.1.checked_add(n)?)?;
        self.1 += n;
        Some(b)
    }
    fn u8(&mut self) -> Option<u8> {
        self.bytes(1)?.first().copied()
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    /// A sized string; any negative length is absent.
    fn optional_string(&mut self) -> Option<Option<String>> {
        let len = self.i32()?;
        if len < 0 {
            return Some(None);
        }
        let bytes = self.bytes(usize::try_from(len).ok().filter(|&n| n <= MAX_TEXT)?)?;
        Some(Some(String::from_utf8_lossy(bytes).into_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fields of a `BParScript` as Kontakt 8.13 writes them in a private.
    fn record_bytes(
        source: Option<&str>,
        description: Option<&str>,
        persistent: &[&str],
    ) -> Vec<u8> {
        let mut b = vec![0, 0x60, 0];
        let push = |b: &mut Vec<u8>, s: Option<&str>| match s {
            Some(s) => {
                b.extend((s.len() as u32).to_le_bytes());
                b.extend(s.as_bytes());
            }
            None => b.extend(u32::MAX.to_le_bytes()),
        };
        push(&mut b, source);
        b.extend([0, 0, 0]); // editor closed, untouched, not bypassed
        b.extend(0u32.to_le_bytes()); // no password
        push(&mut b, description);
        push(&mut b, None);
        b.extend((persistent.len() as u32).to_le_bytes());
        for p in persistent {
            b.extend((p.len() as u32).to_le_bytes());
            b.extend(p.as_bytes());
        }
        b
    }

    fn private(records: &[Vec<u8>], scalars: usize, tail: usize) -> Vec<u8> {
        let mut b = vec![0x5a; scalars];
        for r in records {
            b.extend(r);
        }
        b.resize(b.len() + tail, 0);
        let end = b.len();
        b[end - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
        b
    }

    #[test]
    fn the_slots_come_out_in_order_with_their_source_and_persistent_values() {
        let five = [
            record_bytes(Some("on init\n"), Some("Interface 1.09"), &["$igvla 2"]),
            record_bytes(Some("on note\n"), Some("Legato 1.03"), &[]),
            record_bytes(Some("on note\n"), Some("Sample 1.01"), &[]),
            record_bytes(None, None, &[]),
            record_bytes(None, None, &[]),
        ];
        let s = slots(&private(&five, 65, TAIL)).expect("five slots");
        assert_eq!(s.len(), 5);
        assert_eq!(s[0].name, "Interface 1.09");
        assert_eq!(s[0].saved, ["$igvla 2"]);
        assert_eq!(s[1].source, "on note\n");
        assert!(s[3].source.is_empty(), "an empty slot stores no source");
        assert!(s[3].name.is_empty());
    }

    #[test]
    fn a_private_without_a_script_section_has_no_slots() {
        let mut b = vec![0u8; 99];
        b[40] = 0x60;
        b[41] = 0x00;
        assert!(slots(&b).is_none());
    }

    #[test]
    fn a_protected_slot_still_loads_and_is_marked() {
        let mut r = record_bytes(Some("on init\n"), Some("Locked"), &[]);
        // Password length and bytes sit after the three flags.
        let at = 3 + 4 + "on init\n".len() + 3;
        r.splice(at..at + 4, 4u32.to_le_bytes());
        r.splice(at + 4..at + 4, [1, 2, 3, 4]);
        let s = slots(&private(&[r], 65, TAIL)).expect("one slot");
        assert!(s[0].protected);
        assert_eq!(s[0].source, "on init\n");
    }
}
