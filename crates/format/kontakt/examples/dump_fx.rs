//! Print every effect slot's module, version, stored output/dry and raw public data, so a
//! module's saved record can be read byte by byte.
//!
//! `cargo run --release -p kontra-format-kontakt --example dump_fx -- <file.nki>`

use ni_file::kontakt::{Chunk, KontaktObject, StructuredObject, objects::BParFX};

fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: dump_fx <instrument>");
        std::process::exit(2);
    };
    let path = std::path::PathBuf::from(path);
    let Ok(chunks) = kontra_format_kontakt::chunks(&path) else {
        eprintln!("unreadable: {}", path.display());
        std::process::exit(1);
    };
    let mut n = 0usize;
    for c in &chunks.0 {
        walk(c, &mut n);
    }
}

fn walk(chunk: &Chunk, n: &mut usize) {
    if chunk.id == 0x25
        && let Ok(fx) = BParFX::try_from(chunk)
    {
        let params = fx.params().ok();
        let effect = fx.effect().and_then(|c| {
            StructuredObject::try_from(c)
                .ok()
                .map(|o| (c.id, o.version, o.public_data))
        });
        if let Some((id, version, public)) = effect {
            println!(
                "#{n:03} id={id:#06x} v={version:#x} out={:?} dry={:?} bypass={:?} bytes={}",
                params.map(|p| p.output_gain),
                params.map(|p| p.dry_level),
                params.map(|p| p.bypass),
                public.len()
            );
            for (i, group) in public.chunks(4).enumerate() {
                let mut w = [0u8; 4];
                let value = match w.get_mut(..group.len()) {
                    Some(head) => {
                        head.copy_from_slice(group);
                        format!("{:+.6}", f32::from_le_bytes(w))
                    }
                    None => "-".to_owned(),
                };
                let raw = group
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("   {:>3}  {raw:<12} {value}", i * 4);
            }
            *n += 1;
        }
    }
    if let Ok(o) = StructuredObject::try_from(chunk) {
        for c in &o.children {
            walk(c, n);
        }
    }
    if let Ok(KontaktObject::BParameterArraySerBParFX8(a)) = chunk.into_object() {
        for c in a.items.into_iter().flatten() {
            walk(&c, n);
        }
    }
}
