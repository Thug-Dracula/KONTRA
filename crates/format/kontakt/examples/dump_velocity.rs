//! `dump_velocity <instrument.nki> [group]`: the stored velocity and key
//! low/high pairs of every zone, raw, plus a histogram of the velocity pairs.
//! Prints field offsets alongside so an off-by-one can be seen rather than
//! assumed.

use std::collections::BTreeMap;

use ni_file::kontakt::objects::ZoneList;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: dump_velocity <instrument.nki> [group]")?;
    let only: Option<u32> = std::env::args().nth(2).map(|g| g.parse()).transpose()?;
    let chunks = kontra_format_kontakt::chunks(std::path::Path::new(&path))?;
    let mut body = None;
    for chunk in &chunks.0 {
        if let Ok(object) =
            ni_file::kontakt::StructuredObject::read(std::io::Cursor::new(chunk.data.as_slice()))
        {
            for child in &object.children {
                if child.id == 0x34 {
                    body = Some(child.data.clone());
                }
            }
        }
    }
    let body = body.ok_or("no zone list")?;
    let list = ZoneList::read(std::io::Cursor::new(body.as_slice()))?;
    let mut hist: BTreeMap<(i16, i16), usize> = BTreeMap::new();
    let mut keyhist: BTreeMap<(i16, i16), usize> = BTreeMap::new();
    let mut total = 0;
    for (zone, &group) in list.zones().iter().zip(&list.group_ids) {
        if let Some(g) = only
            && g != group
        {
            continue;
        }
        let Ok(p) = zone.params() else { continue };
        total += 1;
        *hist.entry((p.low_velocity, p.high_velocity)).or_default() += 1;
        *keyhist.entry((p.low_key, p.high_key)).or_default() += 1;
        if only.is_some() && total <= 40 {
            println!(
                "zone group {group} vel {}..{} fade {}..{} key {}..{} fadekey {}..{} root {} start {} end {} frames {}",
                p.low_velocity,
                p.high_velocity,
                p.fade_low_velocity,
                p.fade_high_velocity,
                p.low_key,
                p.high_key,
                p.fade_low_key,
                p.fade_high_key,
                p.root_key,
                p.sample_start,
                p.sample_end,
                p.num_frames,
            );
        }
    }
    println!("zone list length: {}", list.zones().len());
    println!("zones read: {total}");
    println!("velocity pairs (low..high -> count):");
    for ((lo, hi), n) in &hist {
        println!("  {lo}..{hi} -> {n}");
    }
    println!("key pairs (low..high -> count):");
    for ((lo, hi), n) in &keyhist {
        println!("  {lo}..{hi} -> {n}");
    }
    let cover = |v: i16| {
        hist.iter()
            .filter(|((lo, hi), _)| *lo <= v && v <= *hi)
            .map(|(_, n)| *n)
            .sum::<usize>()
    };
    for v in [1, 5, 6, 32, 64, 100, 127] {
        println!(
            "zones whose stored velocity range contains {v}: {}",
            cover(v)
        );
    }
    // The note in question: key 60, velocity 100. Which zones admit it, and
    // which of those the key filter keeps.
    for (key, vel) in [(60u8, 100u8), (60, 5), (60, 127)] {
        let mut hit = Vec::new();
        for (zone, &group) in list.zones().iter().zip(&list.group_ids) {
            let Ok(p) = zone.params() else { continue };
            if (p.low_key as i32..=p.high_key as i32).contains(&i32::from(key))
                && (p.low_velocity as i32..=p.high_velocity as i32).contains(&i32::from(vel))
            {
                hit.push((group, p));
            }
        }
        println!("key {key} velocity {vel}: {} zone(s) admit it", hit.len());
        for (group, p) in hit.iter().take(12) {
            println!(
                "  group {group} vel {}..{} key {}..{} root {} frames {} file {}",
                p.low_velocity,
                p.high_velocity,
                p.low_key,
                p.high_key,
                p.root_key,
                p.num_frames,
                p.filename_id
            );
        }
        if hit.len() > 12 {
            println!("  … {} more", hit.len() - 12);
        }
    }
    Ok(())
}
