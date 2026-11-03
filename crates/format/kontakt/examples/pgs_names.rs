//! Does a preset store a PGS key name anywhere outside its script source?
//!
//! ```sh
//! cargo run -p kontra-format-kontakt --example pgs_names -- <instrument.nki>…
//! ```
//!
//! For each file, every key named by `pgs_create_key(NAME, …)` is counted in the
//! whole decompressed chunk stream and in the script sources alone, counting
//! whole identifiers only (`CONTROL` does not match `CONTROL_PAR_VALUE`). Equal
//! counts mean the file stores no PGS key table of its own; a name that occurs
//! outside the sources is the byte offset to start reading a stored table at.
//!
//! One line per file: `name, keys declared, names found outside the source, and
//! which`. See `docs/re/saved-pgs-probe-2026-11-03.md` for what it measured.

use std::collections::BTreeSet;
use std::io::Cursor;
use std::path::Path;

use ni_file::kontakt::objects::BParScript;
use ni_file::kontakt::{Chunk, StructuredObject};

/// Every `BParScript` source in `chunks`, at any depth.
fn sources(chunks: &[Chunk], out: &mut Vec<String>) {
    for chunk in chunks {
        if chunk.id == 0x06
            && let Ok(params) = StructuredObject::read(Cursor::new(chunk.data.as_slice()))
                .map(BParScript)
                .and_then(|script| script.params())
            && let Some(text) = params.text
        {
            out.push(text);
        }
        if let Ok(object) = StructuredObject::read(Cursor::new(chunk.data.as_slice())) {
            sources(&object.children, out);
        }
    }
}

/// Occurrences of `name` as a whole identifier.
fn whole(hay: &str, name: &str) -> usize {
    let edge = |b: Option<u8>| !b.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
    hay.match_indices(name)
        .filter(|(at, _)| {
            edge(hay.as_bytes().get(at.wrapping_sub(1)).copied())
                && edge(hay.as_bytes().get(at + name.len()).copied())
        })
        .count()
}

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(chunks) = kontra_format_kontakt::chunks(Path::new(&path)) else {
            println!("{path}: unreadable");
            continue;
        };
        let mut raw = Vec::new();
        let _ = chunks.write(&mut raw);
        let text = String::from_utf8_lossy(&raw);
        let mut scripts = Vec::new();
        sources(&chunks.0, &mut scripts);
        let sources = scripts.join("\n");

        let mut names = BTreeSet::new();
        for call in sources.split("pgs_create_key(").skip(1) {
            let name: String = call
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                names.insert(name);
            }
        }
        let outside: Vec<String> = names
            .iter()
            .filter_map(|name| {
                let (all, inside) = (whole(&text, name), whole(&sources, name));
                (all != inside).then(|| format!("{name}: {all} in file, {inside} in source"))
            })
            .collect();
        println!(
            "{}\tkeys {}\toutside {}\t{outside:?}",
            Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default(),
            names.len(),
            outside.len()
        );
    }
}
