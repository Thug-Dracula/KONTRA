//! Regressions for the two instruments the parity sweep left silent:
//! Orange Tree's Cherry Electric Bass and Soundiron's Harpsichord. Each test
//! needs the installed library; with none it reports the skip and passes.

use std::path::{Path, PathBuf};

use kontra_host::render::{Note, Request, run};

/// The Kontakt library root the sweep used; override with `KONTRA_TEST_LIBRARY`.
fn library() -> PathBuf {
    std::env::var_os("KONTRA_TEST_LIBRARY").map_or_else(
        || PathBuf::from("/home/dracula/Armory/VSTs/Kontakt1"),
        PathBuf::from,
    )
}

#[expect(
    clippy::unwrap_used,
    reason = "a failed render is this regression test's failure"
)]
fn render(instrument: &Path, key: u8, name: &str) -> Option<(f64, String)> {
    if !instrument.is_file() {
        eprintln!(
            "SKIPPED (library-dependent): {} is not installed",
            instrument.display()
        );
        return None;
    }
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
    Some((
        summary.peak,
        String::from_utf8_lossy(&diagnostics).into_owned(),
    ))
}

#[test]
fn cherry_electric_bass_reports_its_saved_filter_and_renders_audio() {
    let instrument = library()
        .join("#Non NICNT/Orange Tree Samples - Cherry Electric Bass/Cherry Electric Bass.nki");
    let Some((peak, diagnostics)) = render(&instrument, 40, "cherry") else {
        return;
    };
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
    // A window, not "not silent". While the Kontakt 8 handlers failed to load
    // this instrument rendered only its buzzing group: peak 0.008685
    // (-41.2 dBFS), which `> 0.0` passed. That was the bug.
    assert!(
        peak > 0.1,
        "peak {peak:.6} is the buzzing-only render, not the bass:\n{diagnostics}"
    );
}

#[test]
fn harpsichord_compiles_its_script_and_renders_audio() {
    let instrument =
        library().join("#Non NICNT/Soundiron - Harpsichord/Instruments/Harpsichord.nki");
    let Some((peak, diagnostics)) = render(&instrument, 60, "harpsichord") else {
        return;
    };
    assert!(
        !diagnostics.contains("Broken script compile"),
        "its script still does not compile:\n{diagnostics}"
    );
    assert!(
        diagnostics.contains("voice filters: 576 groups"),
        "the filter group list is empty:\n{diagnostics}"
    );
    assert!(peak > 0.0, "the render is silent:\n{diagnostics}");
}
