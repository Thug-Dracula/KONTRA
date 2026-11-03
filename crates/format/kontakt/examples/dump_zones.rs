//! `dump_zones <instrument.nki>`: one line per zone — its group and the group's
//! source mode, its version, the lengths of its public and private sections, its
//! child chunk ids — then the hex of the public and private sections and of each
//! child. The zone's slice grid is the `0x35` child; this is the dump the search
//! for it was made with (docs/re/engine/zone-slices.md).

use ni_file::kontakt::Chunk;
use ni_file::kontakt::objects::{Zone, ZoneList};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A chunk and everything below it, as `(id, body)`.
fn collect(chunk: &Chunk, into: &mut Vec<(u16, Vec<u8>)>) {
    into.push((chunk.id, chunk.data.clone()));
    if let Ok(object) =
        ni_file::kontakt::StructuredObject::read(std::io::Cursor::new(chunk.data.as_slice()))
    {
        for child in object.children {
            collect(&child, into);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: dump_zones <instrument.nki>")?;
    let chunks = kontra_format_kontakt::chunks(std::path::Path::new(&path))?;
    let mut all = Vec::new();
    for chunk in &chunks.0 {
        collect(chunk, &mut all);
    }
    let modes: Vec<i64> = all
        .iter()
        .find(|(id, _)| *id == 0x33)
        .and_then(|(_, body)| {
            ni_file::kontakt::objects::GroupList::try_from(&Chunk {
                id: 0x33,
                data: body.clone(),
            })
            .ok()
        })
        .map(|l| {
            l.groups
                .iter()
                .map(|g| g.source_identity().map_or(-1, |s| i64::from(s.mode)))
                .collect()
        })
        .unwrap_or_default();
    let (_, body) = all
        .iter()
        .find(|(id, _)| *id == 0x34)
        .ok_or("no zone list")?;
    let list = ZoneList::read(std::io::Cursor::new(body.as_slice()))?;
    for (i, (zone, &group)) in list.zones().iter().zip(&list.group_ids).enumerate() {
        let Zone(obj) = zone;
        let params = zone.params().ok();
        let mode = modes.get(group as usize).copied().unwrap_or(-1);
        println!(
            "zone {i} group {group} mode {mode} version {:#x} public {} private {} children {:?}",
            obj.version,
            obj.public_data.len(),
            obj.private_data.len(),
            obj.children
                .iter()
                .map(|c| format!("{:#06x}:{}", c.id, c.data.len()))
                .collect::<Vec<_>>()
        );
        if let Some(p) = &params {
            println!(
                "  sample_start {} sample_end {} mod {} frames {} rate {} ch {} id {}",
                p.sample_start,
                p.sample_end,
                p.sample_start_mod_range,
                p.num_frames,
                p.sample_rate,
                p.num_channels,
                p.filename_id
            );
        }
        println!("  public {}", hex(&obj.public_data));
        println!("  private {}", hex(&obj.private_data));
        for child in &obj.children {
            println!("  child {:#06x} {}", child.id, hex(&child.data));
        }
    }
    Ok(())
}
