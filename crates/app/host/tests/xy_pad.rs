//! An XY pad's click through the rack: `Rack::axes` — what the plugin's editor
//! calls for an `Action::Axes` — must reach the script and run its callback.
//!
//! Electric Mint (Melody)'s articulation and pattern rows are `ui_xy` click
//! areas (`declare ui_xy ?rjhag[1]` …), 258×28, four in each of two columns.
//! Their `on ui_control` tells a press from a release by
//! `$NI_MOUSE_EVENT_TYPE`, and takes a different path for each column: a click
//! on the left (articulation) column sets the label `$kif3y` to "Bend", and
//! one on the right (pattern) column sets it to "Impact". A click that never
//! reaches the script leaves that label where it was.
#![cfg(test)]

use std::path::PathBuf;

use kontra_host::rack::{Rack, RackAudio};
use kontra_stream::{Store, StoreConfig};
use kontra_ui_model::{Kind, MouseEvent, UiModifiers};

const RATE: u32 = 48_000;
const BLOCK: usize = 512;

fn instrument() -> Option<PathBuf> {
    const LIBRARY: &str = "Armory/VSTs/Kontakt1/Native Instruments Session Guitarist Electric Mint 1.1.0/Session Guitarist - Electric Mint Library/Instruments";
    let path = std::env::var_os("KONTRA_EMINT_MELODY").map_or_else(
        || {
            std::env::var_os("HOME")
                .map_or_else(|| PathBuf::from("/home"), PathBuf::from)
                .join(LIBRARY)
                .join("Electric Mint (Melody).nki")
        },
        PathBuf::from,
    );
    if !path.exists() {
        eprintln!("skipped: no {}", path.display());
        return None;
    }
    Some(path)
}

/// Blocks and ticks, as the plugin's task runs them.
fn settle(rack: &mut Rack, audio: &mut RackAudio) {
    for _ in 0..6 {
        audio.process(BLOCK, &[]);
        rack.tick();
    }
}

/// The eight rows: their widget, their column's x and their row's y.
fn rows(rack: &Rack) -> Vec<(usize, f64, f64)> {
    let part = rack.parts()[0].as_ref().expect("part 0");
    let tree = part.view.as_ref().expect("an interface").tree();
    let mut found: Vec<(usize, f64, f64)> = (tree.widgets.iter().enumerate())
        .filter(|(_, w)| matches!(w.kind, Kind::Xy { .. }) && !w.hidden)
        .map(|(n, w)| (n, w.rect.x, w.rect.y))
        .collect();
    found.sort_by(|a, b| (a.2, a.1).partial_cmp(&(b.2, b.1)).expect("finite rects"));
    found
}

/// A press and a release at the middle of `widget`, as the editor sends them.
fn click(rack: &mut Rack, audio: &mut RackAudio, widget: usize) {
    for event in [MouseEvent::Down, MouseEvent::Up] {
        rack.axes(0, widget, 0, [0.5, 0.5], event, UiModifiers::default());
        settle(rack, audio);
    }
}

/// The selected index the page reads for `widget`.
fn active(rack: &Rack, widget: usize) -> Option<i32> {
    let part = rack.parts()[0].as_ref().expect("part 0");
    let tree = part.view.as_ref().expect("an interface").tree();
    tree.widgets.get(widget)?.active_index
}

#[test]
fn a_click_on_a_row_reaches_the_script_and_selects_that_row() {
    let Some(path) = instrument() else { return };
    let store = Store::new(StoreConfig::DEFAULT).expect("a store");
    let (mut rack, mut audio) = Rack::new(RATE, store, BLOCK);
    rack.load(0, &path).expect("the instrument loads");
    settle(&mut rack, &mut audio);

    let rows = rows(&rack);
    assert_eq!(rows.len(), 8, "the eight rows are Xy controls");
    let left = rows[0].1;
    assert_ne!(left, rows[1].1, "the rows sit in two columns");
    let column: Vec<usize> = (rows.iter().copied())
        .filter(|r| r.1 == left)
        .map(|r| r.0)
        .collect();
    assert_eq!(column.len(), 4, "four rows in the left column");

    // The instrument starts on its first row; the second is clicked first so
    // every step of the selection is a change.
    assert_eq!(
        active(&rack, column[1]),
        Some(-1),
        "the second row starts off"
    );
    click(&mut rack, &mut audio, column[1]);
    assert_eq!(
        active(&rack, column[1]),
        Some(0),
        "the row clicked must be the one the script marks selected"
    );
    assert_eq!(
        active(&rack, column[0]),
        Some(-1),
        "and the first row let go"
    );

    click(&mut rack, &mut audio, column[2]);
    assert_eq!(
        active(&rack, column[2]),
        Some(0),
        "the third row is selected"
    );
    assert_eq!(active(&rack, column[1]), Some(-1), "the second row let go");
}
