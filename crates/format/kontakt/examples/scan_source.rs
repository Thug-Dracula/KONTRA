//! `scan_source <root>`: instruments whose modulations a script drives.
//!
//! Reports, per instrument:
//!
//! - `ROUTE` / `ROUTE+SENT` — routes whose source is "from script"
//!   (`ModSource::Script`, written at runtime by
//!   `set_event_par_arr(id, $EVENT_PAR_MOD_VALUE_ID, value, index)`), marked
//!   with whether some script in the instrument actually sends one;
//! - `TEXT` — a script names that call but nothing parses as a script source;
//! - `INVERT` — a route with `$MOD_TARGET_INVERT_SOURCE` set, whatever its
//!   source, which is the census behind "no instrument renders through the
//!   invert law".
//!
//! Defaults to `$HOME/Armory/VSTs/Kontakt1`. Used to answer "is there an
//! instrument to probe this with" (`docs/re/kontakt-invert-source-2026-11-01.md`).

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| {
        format!(
            "{}/Armory/VSTs/Kontakt1",
            std::env::var("HOME").unwrap_or_default()
        )
    }));
    if !root.is_dir() {
        eprintln!(
            "usage: scan_source <root>   (no such directory: {})",
            root.display()
        );
        return Ok(());
    }
    let mut paths = Vec::new();
    walk(&root, &mut paths);
    eprintln!("{} instruments under {}", paths.len(), root.display());
    let (mut parsed, mut text, mut failed, mut unscripted, mut flagged) = (0, 0, 0, 0, 0);
    for path in &paths {
        let Ok(instrument) = kontra_format_kontakt::load(path) else {
            failed += 1;
            continue;
        };
        let sends = instrument
            .scripts
            .iter()
            .any(|s| s.source.contains("$EVENT_PAR_MOD_VALUE_ID"));
        let mut routes = Vec::new();
        let mut inverted = Vec::new();
        for (g, group) in instrument.groups.iter().enumerate() {
            for m in &group.mods {
                if m.invert {
                    inverted.push(format!("group {g} {} @{} {:?}", m.name, m.slot, m.kind));
                }
                if let Some(kontra_ir::Source::Script(id)) = source_of(&m.kind) {
                    routes.push(format!(
                        "group {g} {} @{} script {id} {:?}",
                        m.name, m.slot, m.kind
                    ));
                }
            }
        }
        if !inverted.is_empty() {
            flagged += 1;
            println!("INVERT {} :: {}", path.display(), inverted.join("; "));
        }
        if routes.is_empty() {
            if sends {
                text += 1;
                println!(
                    "TEXT  {} :: a script names $EVENT_PAR_MOD_VALUE_ID, no parsed route",
                    path.display()
                );
            }
        } else {
            parsed += 1;
            if !sends {
                unscripted += 1;
            }
            println!(
                "{} {} :: {}",
                if sends { "ROUTE+SENT" } else { "ROUTE" },
                path.display(),
                routes.join("; ")
            );
        }
    }
    eprintln!(
        "routes {parsed} (of which no script sends: {unscripted}), text-only {text}, \
         instruments with any inverted route: {flagged}, unreadable {failed}"
    );
    Ok(())
}

/// The route's source, over every [`kontra_ir::ModKind`] that carries one.
fn source_of(kind: &kontra_ir::ModKind) -> Option<kontra_ir::Source> {
    use kontra_ir::ModKind;
    Some(match *kind {
        ModKind::Envelope => return None,
        ModKind::Volume { source, .. }
        | ModKind::Pitch { source, .. }
        | ModKind::Pan { source, .. }
        | ModKind::Cutoff { source, .. }
        | ModKind::Resonance { source, .. }
        | ModKind::EqGain { source, .. }
        | ModKind::SampleStart { source, .. }
        | ModKind::Attack { source, .. }
        | ModKind::Release { source, .. }
        | ModKind::LfoPhase { source, .. }
        | ModKind::LfoRate { source, .. }
        | ModKind::LfoHz { source, .. }
        | ModKind::Intensity { source, .. }
        | ModKind::LoopShift { source, .. }
        | ModKind::Mix { source, .. }
        | ModKind::EffectParam { source, .. }
        | ModKind::Send { source, .. } => source,
    })
}

fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("nki"))
        {
            out.push(path);
        }
    }
}
