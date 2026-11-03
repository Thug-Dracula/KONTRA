//! Real instruments end to end: file → IR → plan → engine → sound.
#![cfg(test)]

use std::path::Path;
use std::sync::Arc;

use kontra_engine::{Engine, Event, HostNote, HostPattern, SampleTime, Timed};
use kontra_stream::{Store, StoreConfig};

const RATE: u32 = 48_000;
const AREIA: &str = "/data/main/Libraries/Kontakt/Areia 1.2.0 [Audio Imperia]/Instruments/01 Core Technique Patches/01 Areia - 16 Violins - Core Techniques.nki";

fn note(key: u8, on: bool) -> Timed<Event> {
    let body = if on {
        Event::NoteOn {
            note: HostNote {
                port: 0,
                channel: 0,
                key,
                id: -1,
                clap: true,
            },
            velocity: 0.8,
            tune: 0.0,
        }
    } else {
        Event::NoteOff(HostPattern {
            port: 0,
            channel: 0,
            key: i32::from(key),
            id: -1,
            clap: true,
        })
    };
    Timed {
        at: SampleTime(0),
        body,
    }
}

#[test]
fn areia_violins_sound_on_the_fresh_engine() {
    let path = Path::new(AREIA);
    if !path.exists() {
        return;
    }
    let t = std::time::Instant::now();
    let mut store = Store::new(StoreConfig::DEFAULT).unwrap();
    let loaded = kontra_host::instrument::load(path, RATE, &mut store).unwrap();
    let load = t.elapsed();
    eprintln!(
        "{}: {} zones, {} scripts, loaded in {load:?}",
        loaded.name,
        loaded.plan.zones.len(),
        loaded.plan.scripts.len()
    );
    for d in &loaded.diagnostics {
        eprintln!("  {:?} {}: {}", d.impact, d.feature, d.detail);
    }
    let mut e = Engine::new(Arc::new(loaded.plan), store.client(), RATE);
    let mut peak = 0f32;
    let mut out = vec![[0.0; 2]; 512];
    e.process(&mut out, &[note(60, true)]);
    for _ in 0..(RATE as usize / 512) {
        e.process(&mut out, &[]);
        peak = out
            .iter()
            .fold(peak, |p, f| p.max(f[0].abs()).max(f[1].abs()));
    }
    e.process(&mut out, &[note(60, false)]);
    eprintln!(
        "peak {peak}, stats {:?}, faults {:?}",
        e.stats(),
        e.script_faults()
    );
    assert!(peak > 1e-3, "a held C4 sounds");
}

/// The Session family's quick-start pane: `%welcome_state` decides whether
/// `on note` plays or only dismisses the pane, and its second cell comes from
/// `welcome_state.nka` in the library's Kontakt user data folder.
const UPRIGHT_BASS: &str = "Armory/VSTs/Kontakt1/Native Instruments Session Bassist Upright Bass/Instruments/Upright Bass (Melody).nki";

#[test]
fn upright_bass_plays_through_its_quick_start_pane_state() {
    let Some(path) =
        std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(UPRIGHT_BASS))
    else {
        return;
    };
    if !path.exists() {
        eprintln!("skipped: {} is not installed", path.display());
        return;
    }
    let mut store = Store::new(StoreConfig::DEFAULT).unwrap();
    let loaded = kontra_host::instrument::load(&path, RATE, &mut store).unwrap();
    let after_restore = loaded
        .plan
        .scripts
        .first()
        .map_or(0, |s| s.after_restore.len());
    if after_restore == 0 {
        // The mode-1 file is in the machine's Kontakt user data, which a
        // machine without Kontakt installed does not have.
        eprintln!("skipped: no Kontakt user data folder for this library");
        return;
    }
    let mut e = Engine::new(Arc::new(loaded.plan), store.client(), RATE);
    let mut peak = 0f32;
    let mut out = vec![[0.0; 2]; 512];
    e.process(&mut out, &[note(48, true)]);
    for _ in 0..(RATE as usize / 512) {
        e.process(&mut out, &[]);
        peak = out
            .iter()
            .fold(peak, |p, f| p.max(f[0].abs()).max(f[1].abs()));
    }
    eprintln!("{after_restore} cells from the library's data folder, peak {peak}");
    assert!(
        peak > 1e-3,
        "a held C3 sounds instead of dismissing the pane"
    );
}
