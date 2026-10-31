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
    // Measured 2026-10-31: 2.475 with both scripts compiled; the regression
    // this pins rendered exact silence (0.0) because `%NOTE_DURATION` failed
    // the compile. 1.0..=4.0 lets the instrument be loud (it clips today) but
    // not silent.
    assert!(
        (1.0..=4.0).contains(&peak),
        "peak {peak:.3} is outside 1.0..=4.0 (silent when the script fails to compile):\n{diagnostics}"
    );
}
