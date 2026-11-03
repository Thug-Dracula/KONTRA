//! Regressions for the two instruments the parity sweep left silent:
//! Orange Tree's Cherry Electric Bass and Soundiron's Harpsichord.
//!
//! Both need a Kontakt library that is installed outside the repository, so
//! they are `#[ignore]`d: a clean checkout reports them as ignored instead of
//! passing them without reading anything. `tools/verify.sh` runs them with
//! `--ignored` when the library is installed, and fails the gate if either
//! one fails. `KONTRA_TEST_LIBRARY` points at another library root, and
//! `KONTRA_CHERRY` / `KONTRA_HARPSICHORD` at an individual `.nki`, as
//! `KONTRA_EMINT` does for the other host tests.

use std::path::{Path, PathBuf};

use kontra_host::render::{Note, Request, run};

/// The library root, under `$HOME`; `KONTRA_TEST_LIBRARY` overrides it.
const LIBRARY: &str = "Armory/VSTs/Kontakt1";

fn library() -> PathBuf {
    std::env::var_os("KONTRA_TEST_LIBRARY")
        .filter(|v| !v.is_empty())
        .map_or_else(
            || {
                std::env::var_os("HOME")
                    .map_or_else(|| PathBuf::from("/home"), PathBuf::from)
                    .join(LIBRARY)
            },
            PathBuf::from,
        )
}

/// One instrument: `variable` when set, else `under` the library root.
fn instrument(variable: &str, under: &str) -> PathBuf {
    std::env::var_os(variable)
        .filter(|v| !v.is_empty())
        .map_or_else(|| library().join(under), PathBuf::from)
}

/// Renders one note. A missing instrument is a failure, not a skip: the test
/// only runs when it is asked for (it is `#[ignore]`d), so "missing" means the
/// library moved and the caller should be told where it looked.
#[expect(
    clippy::unwrap_used,
    reason = "a failed render is this regression test's failure"
)]
fn render(instrument: &Path, key: u8, name: &str) -> (f64, String) {
    assert!(
        instrument.is_file(),
        "{} is not installed; set KONTRA_TEST_LIBRARY or the per-instrument override",
        instrument.display()
    );
    let out = std::env::temp_dir().join(format!("kontra-parity-{name}.wav"));
    let request = Request {
        instrument: instrument.to_path_buf(),
        out,
        note: Note {
            key,
            velocity: 100,
            hold_s: 1.5,
        },
        ..Request::default()
    };
    let mut diagnostics = Vec::new();
    let summary = run(&request, &mut diagnostics).unwrap();
    (
        summary.peak,
        String::from_utf8_lossy(&diagnostics).into_owned(),
    )
}

/// Renders one note with the mod wheel moved to `wheel` one second in.
#[expect(
    clippy::unwrap_used,
    reason = "a failed render is this regression test's failure"
)]
fn render_wheel(instrument: &Path, key: u8, wheel: u8, name: &str) -> (f64, String) {
    assert!(
        instrument.is_file(),
        "{} is not installed; set KONTRA_TEST_LIBRARY or KONTRA_GLORY_DAYS",
        instrument.display()
    );
    let out = std::env::temp_dir().join(format!("kontra-parity-{name}.wav"));
    let request = Request {
        instrument: instrument.to_path_buf(),
        out,
        note: Note {
            key,
            velocity: 100,
            hold_s: 1.5,
        },
        wheel: Some(wheel),
        ..Request::default()
    };
    let mut diagnostics = Vec::new();
    let summary = run(&request, &mut diagnostics).unwrap();
    (
        summary.peak,
        String::from_utf8_lossy(&diagnostics).into_owned(),
    )
}

#[test]
#[ignore = "needs the installed Kontakt library; run with --ignored"]
fn cherry_electric_bass_reports_its_saved_filter_and_renders_audio() {
    let instrument = instrument(
        "KONTRA_CHERRY",
        "#Non NICNT/Orange Tree Samples - Cherry Electric Bass/Cherry Electric Bass.nki",
    );
    let (peak, diagnostics) = render(&instrument, 40, "cherry");
    // The saved filter is a low pass at 46.63 Hz before its velocity route
    // doubles it; the report must list that group at load, not an empty list.
    assert!(
        diagnostics.contains("voice filters: 1 groups"),
        "the filtered group is missing from the load report:\n{diagnostics}"
    );
    assert!(
        diagnostics.contains("(12, 0, 46.6"),
        "the saved cutoff is not reported:\n{diagnostics}"
    );
    // Its four handlers live only in Kontakt 8's private data, and this line
    // is what proves they were read: with none loaded the program keeps only
    // its buzzing group, which its own 93.65 Hz low pass then removes.
    assert!(
        diagnostics.contains(
            "scripts: 4 slots [\"Interface 1.09\", \"Legato 1.03\", \"Sample 1.01\", \"Zone 1.01\"]"
        ),
        "the private-data script slots are missing from the load report:\n{diagnostics}"
    );
    // Measured 2026-10-31: handlers loaded, peak 0.398 (about -8.0 dBFS);
    // with the private-data reader disabled (the bug this pins), peak 0.009
    // (about -40.9 dBFS). The window is 21 dB above the broken state and
    // 8 dB below the fixed one, so silence or a buzzing-only render cannot
    // pass through `peak > 0.0` again.
    assert!(
        (0.10..=1.00).contains(&peak),
        "peak {peak:.3} is outside 0.10..=1.00 (fixed 0.398, buzzing-only 0.009):\n{diagnostics}"
    );
}

#[test]
#[ignore = "needs the installed Kontakt library; run with --ignored"]
fn glory_days_sounds_at_its_saved_dynamics_and_tracks_the_wheel() {
    let instrument = instrument(
        "KONTRA_GLORY_DAYS",
        "Glory Days - Big Band Horns/Instruments/GDH - Glory Days - Big Band Horns 1.0/\
         Multi Instruments/01. Trumpet 1/01. Trumpet 1 Multi.nki",
    );
    // The instrument's dynamics knob is a saved persistent value of 66, so a
    // note with no controller sent is the "out of the box" case, and Kontakt
    // measures it at -28.20 dBFS.
    let (quiet, diagnostics) = render(&instrument, 60, "glory-days");
    assert!(
        diagnostics.contains("scripts: 5 slots [\"C.A.P.S.U.L.E.\", \""),
        "the five-slot engine is missing from the load report:\n{diagnostics}"
    );
    let (loud, _) = render_wheel(&instrument, 60, 127, "glory-days-wheel");
    // Measured 2026-10-31 against Kontakt: 0.039028 (-28.17 dBFS) with no
    // controller against Kontakt's -28.198, and 0.070683 (-23.01 dBFS) with
    // CC1 at 127 against Kontakt's -19.701 for a note held under it the whole
    // time (this render moves the wheel one second in).
    //
    // Both were exact silence before the fix: the crossfade curve lives in 409
    // PGS keys and the engine stopped creating them at 64, so only CC1 = 127
    // reached an endpoint the script sets itself. A window on one render alone
    // would not see that, so this pins the *difference* the wheel makes as well
    // as audibility.
    let peak_db = |p: f64| 20.0 * p.max(1e-12).log10();
    assert!(
        quiet > 0.02 && loud > 0.02,
        "peak {quiet:.6} / {loud:.6}: the instrument is silent (0.039028 / 0.070683 \
         measured, exact 0 before the PGS key table was fixed)"
    );
    assert!(
        peak_db(loud) - peak_db(quiet) > 2.0,
        "the mod wheel moves the level by {:.2} dB (measured 5.16 dB): the dynamics \
         curve is collapsed",
        peak_db(loud) - peak_db(quiet)
    );
}

#[test]
#[ignore = "needs the installed Kontakt library; run with --ignored"]
fn harpsichord_compiles_its_script_and_renders_audio() {
    let instrument = instrument(
        "KONTRA_HARPSICHORD",
        "#Non NICNT/Soundiron - Harpsichord/Instruments/Harpsichord.nki",
    );
    let (peak, diagnostics) = render(&instrument, 60, "harpsichord");
    assert!(
        !diagnostics.contains("Broken script compile"),
        "its script still does not compile:\n{diagnostics}"
    );
    assert!(
        diagnostics.contains("voice filters: 576 groups"),
        "the filter group list is empty:\n{diagnostics}"
    );
    assert!(
        diagnostics.contains("scripts: 2 slots [\"Performance\", \"FX Rack\"]"),
        "its scripts are missing from the load report:\n{diagnostics}"
    );
    // Measured 2026-10-31, key 60: 0.126 (one voice, -18.0 dBFS), against
    // Kontakt's own captures at 0.120/0.152 (-16.3..-18.4 dBFS). The
    // regression this pins rendered exact silence (0.0) because
    // `%NOTE_DURATION` failed the compile; the 157-voice overload that
    // `ignore_event($EVENT_ID)` reading 0 produced peaked at 2.475. The
    // window admits neither failure and the instrument's own level.
    assert!(
        (0.05..=0.5).contains(&peak),
        "peak {peak:.3} is outside 0.05..=0.5 (silent when the script fails to compile, \
         overloaded when the host note is not ignored):\n{diagnostics}"
    );
}

/// The Spitfire framework tags each group with the filter in one insert slot:
/// the cutoff holds a sound id, `$ENGINE_PAR_FILTER_RESC` an articulation
/// index. It only reads them when the slot answers
/// `$ENGINE_PAR_EFFECT_TYPE` = `$EFFECT_TYPE_FILTER` (7), so KONTRA answering
/// 0 silenced every Spitfire patch whose framework compiled.
///
/// Measured in Kontakt 8.14.1 (2026-11-02): a Versatile filter set into group
/// 0 insert 7 answers `EFFECT_TYPE` 7, `EFFECT_SUBTYPE` 19, `EFFECT_BYPASS` 1,
/// and the seven stored knobs exactly as saved (× 1e6). This file stores group
/// 0 slot 7 as cutoff 60, resonance 41, TYPEA 0, BYPA 0, SHIFTB 0, RESB 0,
/// RESC 0; the probe each group's values through the engine (the way the
/// framework reads them) and then runs its allow/disallow loop. Group 0 was
/// silent before the fix because both the type and every tag read 0.
#[test]
#[ignore = "needs the installed Kontakt library; run with --ignored"]
fn spitfire_filter_tags_answer_kontakt_and_reallow_their_groups() {
    use std::sync::Arc;

    use kontra_engine::{Engine, ir};
    use kontra_stream::{Store, StoreConfig};

    let instrument_path = instrument(
        "KONTRA_SPITFIRE",
        "Spitfire_Studio_Brass/Instruments/_Advanced_/Extended techniques/\
         b - Horns a4 - Decorative techniques.nki",
    );
    assert!(
        instrument_path.is_file(),
        "{} is not installed; set KONTRA_TEST_LIBRARY or KONTRA_SPITFIRE",
        instrument_path.display()
    );
    let mut store = Store::new(StoreConfig::DEFAULT).expect("store");
    let mut read = kontra_host::instrument::read(&instrument_path).expect("read");
    // The framework reads the saved tags and the layer selection in `on init`;
    // this probe runs after it and reports the getters plus the gate's counts.
    // Layer A's saved sound id in this patch is 42 (the render allows groups
    // 20 and 21, both tagged 42); `RESC` 0 or 1 is the gate's articulation
    // test with the round-robin counter at 0.
    read.scripts.push(ir::Script {
        name: "filter probe".into(),
        slot: read.scripts.len(),
        dialect: ir::Dialect::Ksp,
        source: r#"
on init
  declare $g
  declare $tag
  declare $rc
  declare $layer
  declare $early
  $layer := 0
  $early := 0
  $g := 0
  while ($g < $NUM_GROUPS)
    $tag := get_engine_par($ENGINE_PAR_CUTOFF,$g,7,-1)
    $rc := get_engine_par($ENGINE_PAR_FILTER_RESC,$g,7,-1)
    if (get_engine_par($ENGINE_PAR_EFFECT_TYPE,$g,7,-1) = $EFFECT_TYPE_FILTER and ($rc = 0 or $rc = 1))
      if ($tag = 42)
        inc($layer)
      end if
      if ($g = 20 or $g = 21)
        inc($early)
      end if
    end if
    inc($g)
  end while
  declare @s
  @s := "type=" & get_engine_par($ENGINE_PAR_EFFECT_TYPE,0,7,-1)
  @s := @s & " sub=" & get_engine_par($ENGINE_PAR_EFFECT_SUBTYPE,0,7,-1)
  @s := @s & " by=" & get_engine_par($ENGINE_PAR_EFFECT_BYPASS,0,7,-1)
  @s := @s & " cut=" & get_engine_par($ENGINE_PAR_CUTOFF,0,7,-1)
  @s := @s & " res=" & get_engine_par($ENGINE_PAR_RESONANCE,0,7,-1)
  @s := @s & " ta=" & get_engine_par($ENGINE_PAR_FILTER_TYPEA,0,7,-1)
  @s := @s & " wa=" & get_engine_par($ENGINE_PAR_FILTER_BYPA,0,7,-1)
  @s := @s & " sb=" & get_engine_par($ENGINE_PAR_FILTER_SHIFTB,0,7,-1)
  @s := @s & " rb=" & get_engine_par($ENGINE_PAR_FILTER_RESB,0,7,-1)
  @s := @s & " rc=" & get_engine_par($ENGINE_PAR_FILTER_RESC,0,7,-1)
  @s := @s & " layer=" & $layer & " early=" & $early
  message(@s)
end on
"#
        .into(),
        saved: Vec::new(),
        data: Vec::new(),
        links: Vec::new(),
    });
    let loaded = kontra_host::instrument::build(read, 48_000, &mut store);
    let broken: Vec<_> = loaded
        .diagnostics
        .iter()
        .filter(|d| d.impact == ir::Impact::Broken)
        .map(|d| format!("{}: {}", d.feature, d.detail))
        .collect();
    assert!(
        broken.is_empty(),
        "the probe or the framework did not compile: {broken:?}"
    );
    let engine = Engine::new(Arc::new(loaded.plan), store.client(), 48_000);
    assert_eq!(
        engine.last_message(),
        "type=7 sub=19 by=1 cut=60 res=41 ta=0 wa=0 sb=0 rb=0 rc=0 layer=14 early=2",
        "the filter tags do not answer the values Kontakt returns"
    );
}
