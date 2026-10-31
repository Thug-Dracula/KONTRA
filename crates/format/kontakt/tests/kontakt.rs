//! Real instruments, skipped where the library is not installed.
#![cfg(test)]

use std::path::Path;

const AREIA: &str = "/data/main/Libraries/Kontakt/Areia 1.2.0 [Audio Imperia]/Instruments/01 Core Technique Patches/01 Areia - 16 Violins - Core Techniques.nki";

#[test]
fn areia_violins_load_with_their_samples_and_script() {
    let path = Path::new(AREIA);
    if !path.exists() {
        return;
    }
    let i = kontra_format_kontakt::load(path).unwrap();
    let zones: usize = i.groups.iter().map(|g| g.zones.len()).sum();
    eprintln!(
        "{}: {} groups, {} zones, {} samples, {} scripts, stream {:?}",
        i.name,
        i.groups.len(),
        zones,
        i.samples.len(),
        i.scripts.len(),
        i.groups.first().and_then(|g| g.stream)
    );
    for d in &i.diagnostics {
        eprintln!("  {:?} {}: {}", d.impact, d.feature, d.detail);
    }
    if let Some(s) = i.samples.first() {
        eprintln!("  first sample {s:?}");
    }
    assert!(zones > 0 && !i.samples.is_empty() && !i.scripts.is_empty());
    assert!(!i.diagnostics.iter().any(|d| d.feature == "missing sample"));
}

const ANALOG: &str =
    "/data/main/Libraries/Kontakt/ANALOG STRINGS/Snapshots/ANALOG STRINGS/Factory/9001 Punch.nksn";

#[test]
fn an_analog_strings_snapshot_is_its_instrument_with_its_saved_values() {
    let path = Path::new(ANALOG);
    if !path.exists() {
        return;
    }
    let snap = kontra_format_kontakt::load(path).unwrap();
    let base = Path::new("/data/main/Libraries/Kontakt/ANALOG STRINGS/Instruments");
    let nki = std::fs::read_dir(base).unwrap().flatten().map(|e| e.path());
    let nki = nki
        .into_iter()
        .find(|p| p.extension().is_some_and(|e| e == "nki"))
        .unwrap();
    let base = kontra_format_kontakt::load(&nki).unwrap();
    assert_eq!(snap.name, "9001 Punch");
    assert_eq!(snap.groups.len(), base.groups.len());
    let saved = |i: &kontra_ir::Instrument| {
        i.scripts
            .iter()
            .map(|s| s.saved.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(snap.scripts.len(), base.scripts.len());
    assert_ne!(saved(&snap), saved(&base));
}

const CHERRY: &str = "/home/dracula/Armory/VSTs/Kontakt1/#Non NICNT/Orange Tree Samples - \
                      Cherry Electric Bass/Cherry Electric Bass.nki";

/// Kontakt 8 keeps the handlers inline in the program's private data rather
/// than in `0x06` chunks; a slot's saved values ride with its source.
#[test]
fn a_kontakt_8_program_keeps_its_scripts_in_its_private_data() {
    let path = Path::new(CHERRY);
    if !path.exists() {
        return;
    }
    let i = kontra_format_kontakt::load(path).unwrap();
    let slots: Vec<(usize, &str)> = i
        .scripts
        .iter()
        .map(|s| (s.slot, s.name.as_str()))
        .collect();
    assert_eq!(
        slots,
        [
            (0, "Interface 1.09"),
            (1, "Legato 1.03"),
            (2, "Sample 1.01"),
            (3, "Zone 1.01"),
        ],
        "one script per slot, in slot order"
    );
    // The Interface's saved values are the record's persistent list.
    assert_eq!(i.scripts[0].saved[0], "$hkbb2 0");
    assert!(i.scripts[0].saved.iter().any(|s| s.starts_with("%4ikw1")));
    assert!(i.scripts.iter().all(|s| !s.source.trim().is_empty()));
}
