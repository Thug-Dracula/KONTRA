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

/// The Kontakt library root, under `$HOME`; `KONTRA_TEST_LIBRARY` overrides it, so
/// these tests also run where the library lives somewhere else.
const LIBRARY: &str = "Armory/VSTs/Kontakt1";

fn library() -> std::path::PathBuf {
    std::env::var_os("KONTRA_TEST_LIBRARY")
        .filter(|v| !v.is_empty())
        .map_or_else(
            || {
                std::env::var_os("HOME")
                    .map_or_else(
                        || std::path::PathBuf::from("/home"),
                        std::path::PathBuf::from,
                    )
                    .join(LIBRARY)
            },
            std::path::PathBuf::from,
        )
}

/// One `.nki` under the library root: `variable` when set, else `under` the root.
fn instrument(variable: &str, under: &str) -> std::path::PathBuf {
    std::env::var_os(variable)
        .filter(|v| !v.is_empty())
        .map_or_else(|| library().join(under), std::path::PathBuf::from)
}

fn vertigo() -> std::path::PathBuf {
    instrument(
        "KONTRA_VERTIGO",
        "#Non NICNT/Cinematique Instruments Vertigo Violin/Vertigo Violin.nki",
    )
}

fn cherry() -> std::path::PathBuf {
    instrument(
        "KONTRA_CHERRY",
        "#Non NICNT/Orange Tree Samples - Cherry Electric Bass/Cherry Electric Bass.nki",
    )
}

fn haunted() -> std::path::PathBuf {
    instrument(
        "KONTRA_HAUNTED",
        "#Non NICNT/Haunted Strings 2 v2.5.1 [Insanity Samples]/1. Haunted Strings 2 - \
         Twisted Chamber Strings - Main Articulations.nki",
    )
}

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
    let path = vertigo();
    if !path.exists() {
        eprintln!(
            "skipped: no {} (set KONTRA_TEST_LIBRARY if the library is elsewhere)",
            path.display()
        );
        return;
    }
    let i = kontra_format_kontakt::load(&path).unwrap();
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
    let path = haunted();
    if !path.exists() {
        eprintln!(
            "skipped: no {} (set KONTRA_TEST_LIBRARY if the library is elsewhere)",
            path.display()
        );
        return;
    }
    let i = kontra_format_kontakt::load(&path).unwrap();
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
    let path = cherry();
    if !path.exists() {
        return;
    }
    let i = kontra_format_kontakt::load(&path).unwrap();
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

/// A Vocalise pedal: 65 slice markers 7200 frames apart over the whole sample,
/// so Kontakt answers 64 slices whose last one ends at the sample's length.
fn vocalise_pedal() -> std::path::PathBuf {
    instrument(
        "KONTRA_VOCALISE_PEDAL",
        "Heavyocity Vocalise 3/Instruments/III Rhythmic Pedals/Single Pedals (Mono)/\
         Straight Pedal 01 Mono (Carla).nki",
    )
}

/// A Spitfire patch whose grid stops 34119 frames before its sample does.
fn spitfire_horn() -> std::path::PathBuf {
    instrument(
        "KONTRA_SPITFIRE_HORN",
        "Spitfire_Studio_Brass/Instruments/a - Horn Solo.nki",
    )
}

/// What a zone's slice grid must look like once read, whatever the instrument:
/// starts ascend strictly, every slice is non-empty, and none runs past the
/// sample. These hold only on a grid read from the file.
fn assert_slice_invariants(i: &kontra_ir::Instrument) {
    let mut seen = 0;
    for group in &i.groups {
        for zone in &group.zones {
            let frames = i.samples.get(zone.sample).map_or(0, |s| s.frames);
            let mut previous = None;
            for slice in &zone.slices {
                assert!(
                    slice.start < slice.end,
                    "{:?} frames {frames}: {slice:?} is empty",
                    i.name
                );
                assert!(
                    slice.end <= frames,
                    "{:?} frames {frames}: {slice:?} runs past the sample",
                    i.name
                );
                if let Some(start) = previous {
                    assert!(
                        slice.start > start,
                        "{:?}: {slice:?} does not start after {start}",
                        i.name
                    );
                }
                previous = Some(slice.start);
                seen += 1;
            }
        }
    }
    assert!(seen > 0, "{:?} read no slices at all", i.name);
}

#[test]
fn a_vocalise_grid_ends_at_its_sample_and_holds_sixty_four_slices() {
    let path = vocalise_pedal();
    if !path.exists() {
        eprintln!("skipped: no {}", path.display());
        return;
    }
    let i = kontra_format_kontakt::load(&path).unwrap();
    assert_slice_invariants(&i);
    let zone = &i.groups[0].zones[0];
    let frames = i.samples[zone.sample].frames;
    assert_eq!(frames, 460_800);
    assert_eq!(zone.slices.len(), 64, "65 markers, so 64 slices");
    assert_eq!(zone.slices[0].start, 0);
    assert_eq!(zone.slices[1].start, 7_200);
    assert_eq!(zone.slices[63].start, 453_600);
    assert_eq!(
        zone.slices[63].end, frames,
        "the last marker is the grid's end, which here is the sample's length"
    );
}

#[test]
fn a_spitfire_grid_stops_at_its_own_end_before_the_sample_does() {
    let path = spitfire_horn();
    if !path.exists() {
        eprintln!("skipped: no {}", path.display());
        return;
    }
    let i = kontra_format_kontakt::load(&path).unwrap();
    assert_slice_invariants(&i);
    // Every key of the patch stores the same 33-marker grid; a zone whose
    // sample is longer than the grid's end keeps its 32 slices.
    let (zone, frames) = i
        .groups
        .iter()
        .flat_map(|g| &g.zones)
        .filter(|z| !z.slices.is_empty())
        .map(|z| (z, i.samples[z.sample].frames))
        .find(|(z, frames)| z.slices.last().is_some_and(|s| s.end == 475_686) && *frames > 475_686)
        .expect("a zone whose sample outlasts the grid");
    assert_eq!(zone.slices.len(), 32, "33 markers, so 32 slices");
    assert_eq!(zone.slices[0].start, 0);
    assert_eq!(zone.slices[1].start, 14_865);
    assert_eq!(zone.slices[31].start, 460_820);
    assert_eq!(
        zone.slices[31].end,
        475_686,
        "the grid's last marker, which is {} frames short of the sample's {frames}",
        frames - 475_686
    );
    // The zones whose sample is shorter than the grid keep no slices: Kontakt
    // answers 0 for such a grid (measured on AVANT Modern Keys).
    assert!(
        i.groups
            .iter()
            .flat_map(|g| &g.zones)
            .filter(|z| i.samples[z.sample].frames < 475_686)
            .all(|z| z.slices.is_empty()),
        "a grid past its sample must read as no slices"
    );
}

#[test]
fn a_groups_stored_start_criteria_are_kept_beside_the_rules_they_lower_to() {
    let path = instrument(
        "KONTRA_ACCORDIONS",
        "Accordions 2 Library/Instruments/Steirisches Accordion.nki",
    );
    if !path.exists() {
        eprintln!("skipped: no {}", path.display());
        return;
    }
    let i = kontra_format_kontakt::load(&path).unwrap();
    // Accordions 2 stores a register number in the criterion's controller
    // minimum and a family in its controller, then names groups by reading
    // them back: `%yrudk[get_engine_par($ENGINE_PAR_START_CRITERIA_CC_MIN, g, 0, -1)] := g`.
    let sus = i
        .groups
        .iter()
        .find(|g| g.name == "SUS_VEL1_RR1")
        .expect("the first sustain group");
    assert_eq!(sus.criteria.len(), 1);
    let c = sus.criteria[0];
    assert_eq!((c.mode, c.controller, c.cc), (2, 2, (0, 64)));
    assert_eq!(c.par(kontra_ir::StartPar::Controller), 2);
    assert_eq!(c.par(kontra_ir::StartPar::CcMin), 0);
    assert_eq!(c.par(kontra_ir::StartPar::CcMax), 64);
    assert!(sus.start.iter().any(|(_, r)| *r
        == kontra_ir::StartRule::Controller {
            cc: 2,
            range: 0..=64
        }));
    // The keyswitch group stores an empty range (11..=0), which lowers to no
    // rule at all: the row is kept because the tag is what a script reads.
    let ks = i
        .groups
        .iter()
        .find(|g| g.name == "KS")
        .expect("the keyswitch group");
    assert_eq!(ks.criteria[0].cc, (11, 0));
    assert!(ks.start.is_empty());
}
