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

const VERTIGO: &str = "/home/dracula/Armory/VSTs/Kontakt1/#Non NICNT/Cinematique \
                       Instruments Vertigo Violin/Vertigo Violin.nki";
const CHERRY: &str = "/home/dracula/Armory/VSTs/Kontakt1/#Non NICNT/Orange Tree Samples - \
                      Cherry Electric Bass/Cherry Electric Bass.nki";
const HAUNTED: &str = "/home/dracula/Armory/VSTs/Kontakt1/#Non NICNT/Haunted Strings 2 \
                       v2.5.1 [Insanity Samples]/1. Haunted Strings 2 - Twisted Chamber \
                       Strings - Main Articulations.nki";

/// The response `i` holds that the preset names `suffix`, if it resolved one.
fn response<'i>(i: &'i kontra_ir::Instrument, suffix: &str) -> Option<&'i kontra_ir::Sample> {
    i.samples.iter().find(|s| s.name.ends_with(suffix))
}

/// A stock Kontakt preset names its response `/presets/…`, outside the
/// instrument's library; without a factory root the load reports it missing,
/// which is not this test's failure on a machine with no Kontakt installed.
fn unplaced(i: &kontra_ir::Instrument) -> Option<String> {
    let missing: Vec<&str> = i
        .diagnostics
        .iter()
        .filter(|d| d.feature == "missing impulse response")
        .map(|d| d.detail.as_str())
        .collect();
    if !missing.is_empty() {
        return Some(missing.join("; "));
    }
    None
}

#[test]
fn vertigos_tape_echo_comes_from_the_kontakt_presets_folder() {
    let path = Path::new(VERTIGO);
    if !path.exists() {
        eprintln!("skipped: no {}", path.display());
        return;
    }
    let i = kontra_format_kontakt::load(path).unwrap();
    if let Some(missing) = unplaced(&i).filter(|d| d.contains("no Kontakt presets root")) {
        eprintln!("skipped: {missing}");
        return;
    }
    assert!(unplaced(&i).is_none(), "{:?}", unplaced(&i));
    let tape = response(&i, "Tape Echo.wav").expect("the insert's Tape Echo.wav resolved");
    assert!(tape.file.is_file(), "{}", tape.file.display());
    assert!(
        tape.name
            .to_lowercase()
            .starts_with("/presets/effects/convolution/"),
        "{}",
        tape.name
    );
    assert!(
        i.impulses.iter().any(|impulse| i
            .samples
            .get(impulse.sample)
            .is_some_and(|s| s.file == tape.file)),
        "no insert points at {}",
        tape.file.display()
    );
}

#[test]
fn haunted_strings_send_response_comes_out_of_kontakts_ir_archive() {
    let path = Path::new(HAUNTED);
    if !path.exists() {
        eprintln!("skipped: no {}", path.display());
        return;
    }
    let i = kontra_format_kontakt::load(path).unwrap();
    if let Some(missing) = unplaced(&i).filter(|d| d.contains("no Kontakt presets root")) {
        eprintln!("skipped: {missing}");
        return;
    }
    assert!(unplaced(&i).is_none(), "{:?}", unplaced(&i));
    let response = response(&i, "L300 Acousticas 8.7s.wav").expect("the send's response resolved");
    assert!(response.file.is_file(), "{}", response.file.display());
    assert!(
        response.file.to_string_lossy().ends_with("K4IR.nkx"),
        "{} is not the shipped archive",
        response.file.display()
    );
    assert!(response.len.is_some(), "an archive member has a length");
}

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
