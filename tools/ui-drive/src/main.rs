//! Layer 2 of `docs/self-test-loop.md`: drive a real instrument's page with no
//! window, and say which control each press lands on.
//!
//! For every visible control the page draws, it reads the control's rect from
//! the scene mui recorded when it drew it, presses the rect's centre through
//! the same path the editor uses (`input::read` and `Input`/`PointerInput`),
//! and checks the press selected that control and no other. A press two points
//! outside the rect must select nothing, so a "selects everything" hit test
//! cannot pass.
//!
//! `cargo run --release -p ui-drive -- [--scale <f>] [--out <dir>] <instrument>...`
//!
//! No display is opened: the page is drawn into mui's offscreen scene exactly
//! as `ui-shots` draws it, and painted with the CPU renderer only when `--out`
//! asks for the picture.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use kontra_engine::Engine;
use kontra_host::ui::lua::Page as _;
use kontra_host::ui::{Library, Live};
use kontra_stream::{Store, StoreConfig};
use kontra_ui::input::{self, Edit, Gesture, Set};
use kontra_ui::{Pictures, view};
use kontra_ui_model::{Interface, Kind, PageRef, Widget, WidgetRef};
use mui::prelude::{Buttons, Input, Mods, Point, PointerInput, Size as ViewSize, Ui};
use mui::vello;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scale = 1.0f64;
    let mut out: Option<PathBuf> = None;
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--scale") => {
                let value = args.next().ok_or("--scale needs a number")?;
                scale = value.to_string_lossy().parse()?;
            }
            Some("--out") => {
                out = Some(PathBuf::from(args.next().ok_or("--out needs a directory")?));
            }
            Some("-h" | "--help") => {
                usage();
                return Ok(());
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    if paths.is_empty() {
        usage();
        return Err("no instrument given".into());
    }
    if !(scale.is_finite() && scale > 0.) {
        return Err(format!("--scale must be a positive number, not {scale}").into());
    }
    if let Some(dir) = &out {
        std::fs::create_dir_all(dir)?;
    }
    let mut run = Run::default();
    for path in &paths {
        run.instrument(path, scale, out.as_deref())?;
    }
    match run.report() {
        Outcome::Passed => Ok(()),
        Outcome::CheckedNothing | Outcome::Failed => std::process::exit(1),
    }
}

/// What the run proved.
///
/// `CheckedNothing` is a failure of its own: a page that drew no control, or an
/// instrument with no page at all, satisfies every per-control check it never
/// ran. That is the "a check that cannot fail" class the gate exists to catch,
/// so it is reported as a failure rather than as a vacuous pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Passed,
    CheckedNothing,
    Failed,
}

fn usage() {
    eprintln!(
        "usage: ui-drive [--scale <f>] [--out <dir>] <instrument>...\n\
         \n\
         Presses every visible control of an instrument's page at the centre of the rect\n\
         it was drawn in, headless, and fails when the press does not select that control\n\
         or when a press two points outside its rect selects it anyway."
    );
}

/// One page's share of the run: what was proved where.
///
/// A single total over every page cannot answer "did *this* page check
/// anything", and a page that drew nothing is exactly what a second page hides:
/// 39 controls on one instrument and none on another summed to 39, and the run
/// read as a pass for both.
struct Page {
    label: String,
    checked: usize,
}

/// What the run found, across every instrument and page.
#[derive(Default)]
struct Run {
    pages: usize,
    checked: usize,
    skipped: usize,
    /// One entry per page driven, in the order they were driven.
    driven: Vec<Page>,
    /// Instruments that produced no page at all: an instrument that draws
    /// nothing satisfies every per-control check the same way an empty page does.
    unpaged: Vec<String>,
    /// Controls whose click produced an `Edit` (a value, a cell, a menu).
    edited: usize,
    /// Controls the page paints underneath another one: unreachable in
    /// Kontakt too, so not a hit-test failure.
    covered: usize,
    /// Text fields whose press is not theirs (they edit on a double click),
    /// so the control beneath takes it. A press there that opens the list is
    /// the instrument's own behaviour, not a hit-test failure.
    declining: usize,
    /// Text fields proven to edit in place: a double click, an editor drawn
    /// over the field itself.
    editing: usize,
    /// Overlapping pairs driven twice: the covered control opened, then a
    /// press on the control painted over it.
    overlaps: usize,
    failed: Vec<Fail>,
}

/// A control whose centre press did not select it, or whose outside press did.
struct Fail {
    page: String,
    widget: usize,
    source: String,
    name: String,
    rect: (f64, f64, f64, f64),
    point: Point,
    /// What the press selected instead; empty for nothing.
    selected: Vec<String>,
    why: &'static str,
}

impl Run {
    fn instrument(
        &mut self,
        path: &Path,
        scale: f64,
        out: Option<&Path>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut store = Store::new(StoreConfig::DEFAULT)?;
        let loaded = kontra_host::instrument::load(path, 48_000, &mut store)?;
        let mut engine = Engine::new(Arc::new(loaded.plan), store.client(), 48_000);
        let first = self.driven.len();
        // A KSP script's page: what `on init` built, then what the engine
        // says each control looks like now (ui-shots settles it the same way).
        for (slot, page) in loaded.pages.iter().enumerate() {
            let Some(page) = page else { continue };
            let s = u8::try_from(slot)?;
            let library = RefCell::new(Library::open(path));
            let meta = |p: &str| library.borrow_mut().meta(p);
            let now = engine
                .vm(s)
                .and_then(kontra_engine::Vm::own_page)
                .and_then(|p| p.downcast::<kontra_lua::Ui>().ok());
            let mut tree = kontra_host::ui::lua::tree(now.as_deref().unwrap_or(page), &meta);
            settle_script(&mut engine, s, &mut tree);
            let label = format!("{} — script {slot}", loaded.name);
            self.page(&label, tree, slot, scale, path, out)?;
        }
        // A library's own Interface face, when it has one.
        for (slot, face) in loaded.faces.iter().enumerate() {
            let Some(face) = face else { continue };
            let s = u8::try_from(slot)?;
            let library = RefCell::new(Library::open(path));
            let meta = |p: &str| library.borrow_mut().meta(p);
            let mut live = Live::new(face, s, &engine, &meta)?;
            let all = 0..live.tree.widgets.len();
            kontra_ui::layout::resolve(&mut live.tree, all);
            let mut block = vec![[0f32; 2]; 512];
            for _ in 0..10 {
                engine.process(&mut block, &[]);
                let changed = live.refresh(face, s, &engine, &meta);
                kontra_ui::layout::resolve(&mut live.tree, changed.widgets.iter().map(|w| w.0));
            }
            let label = format!("{} — face {slot}", loaded.name);
            self.page(&label, live.tree, slot, scale, path, out)?;
        }
        if self.driven.len() == first {
            // Nothing was drawn: every control check below it was skipped, and a
            // run that checks nothing proves nothing. Recorded per instrument,
            // because another instrument's pages must not stand in for this one.
            self.unpaged.push(loaded.name.clone());
        }
        Ok(())
    }

    fn page(
        &mut self,
        label: &str,
        tree: Interface,
        face: usize,
        scale: f64,
        path: &Path,
        out: Option<&Path>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.pages += 1;
        let mut checked_here = 0usize;
        let mut drive = Drive::new(tree, face, scale, path)?;
        drive.settle()?;
        let (w, h) = drive.size;
        println!("\n{label}, page 0: {w:.0}×{h:.0} at {scale}×");
        if let Some(dir) = out {
            let file = dir.join(format!("{}-page0.png", label.replace(['/', ' ', '—'], "_")));
            drive.save(&file)?;
            println!("  drawn to {}", file.display());
        }
        println!(
            "  {:>3}  {:<7} {:<24} {:<20} {:<20} {:<20} edit",
            "#", "source", "name", "drawn rect", "pressed", "selected"
        );
        let count = drive.tree.widgets.len();
        for n in 0..count {
            let wr = WidgetRef(n);
            if !drive.tree.visible(wr) {
                continue;
            }
            let Some(widget) = drive.tree.widgets.get(n) else {
                continue;
            };
            if !takes_input(widget) {
                continue;
            }
            let source = widget
                .source_id
                .map_or_else(|| "-".to_owned(), |s| s.to_string());
            let name = widget.name.clone();
            let Some(rect) = drive.rect(n) else {
                println!(
                    "  {n:>3}  {:<7} {:<24} {:<20} {:<20} {:<20} skip",
                    clip(&source, 7),
                    clip(&name, 24),
                    "-",
                    "-",
                    "not drawn",
                );
                self.skipped += 1;
                continue;
            };
            let point = Point::new(rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
            if !drive.on_page(point) {
                println!(
                    "  {n:>3}  {:<7} {:<24} {:<20} {:<20} {:<20} skip",
                    clip(&source, 7),
                    clip(&name, 24),
                    drawn(rect),
                    at(point),
                    "off the page",
                );
                self.skipped += 1;
                continue;
            }
            self.checked += 1;
            checked_here += 1;
            let probe = drive.press(point)?;
            let selected = drive.describe(&probe.selected);
            let edit = probe
                .edits
                .first()
                .map_or_else(|| "-".to_owned(), edit_text);
            if !probe.edits.is_empty() {
                self.edited += 1;
            }
            // One press, one owner: a click that reaches two controls is the
            // bug class that let the Sound Preset list open with the name
            // field's entry popup, so it fails here rather than shipping.
            let one = probe.reacted.len() == 1;
            let mine = probe.reacted.as_slice() == [n];
            // The reader can hand the press to this control through a text
            // edit above it that declines a single click; mui still reports
            // the text edit as what it captured.
            let forwarded = probe.selected.len() == 1
                && probe
                    .selected
                    .first()
                    .is_some_and(|&m| drive.kind(m) == "text edit" && drive.is_beneath(n, m));
            let ok = one
                && mine
                && (probe.selected == [n] || forwarded)
                && probe.edits.iter().all(|e| e.widget.0 == n);
            // A control the page paints under another one cannot be clicked in
            // Kontakt either: the widget above takes the press there, and that
            // says nothing about the hit test. It is reported, not failed.
            let covered = drive.covering(n, point);
            let unreachable = covered.filter(|&m| probe.selected == [m]);
            // A text edit is edited on a double click, so a single press on
            // it is the business of the control beneath. Electric Mint's
            // preset name field sits over its preset menu this way.
            let declining = one
                && !mine
                && drive.kind(n) == "text edit"
                && probe
                    .reacted
                    .first()
                    .is_some_and(|&m| drive.is_beneath(m, n));
            let verdict = if ok {
                "ok"
            } else if unreachable.is_some() && one {
                "covered"
            } else if declining {
                "declines"
            } else {
                "FAIL"
            };
            if unreachable.is_some() {
                self.covered += 1;
            }
            if declining {
                self.declining += 1;
            }
            if !ok && unreachable.is_none() && !declining {
                self.failed.push(Fail {
                    page: label.to_owned(),
                    widget: n,
                    source: source.clone(),
                    name: name.clone(),
                    rect,
                    point,
                    selected: probe
                        .selected
                        .iter()
                        .map(|&s| drive.describe(&[s]))
                        .collect(),
                    why: if !one {
                        "the press made more than one control react"
                    } else if !mine {
                        "the press selected it, but another control reacted"
                    } else if probe.selected.is_empty() {
                        "the centre press selected nothing"
                    } else if probe.selected == [n] {
                        "the press selected it, but an Edit named another control"
                    } else {
                        "the centre press selected another control"
                    },
                });
            }
            if let Some(dir) = out
                && forwarded
            {
                // A popup eases in; let its first frames play before the
                // picture, or it is drawn at zero opacity.
                drive.settle()?;
                let file = dir.join(format!(
                    "{}-press-{n}.png",
                    label.replace(['/', ' ', '—'], "_")
                ));
                drive.save(&file)?;
                println!("    forwarded press drawn to {}", file.display());
            }
            // Two points outside the rect, inside the page: that press must
            // not select this control, or the hit test is selecting everything.
            if let Some(outside) = drive.outside(rect) {
                let miss = drive.press(outside)?;
                if miss.selected.contains(&n) || miss.edits.iter().any(|e| e.widget.0 == n) {
                    self.failed.push(Fail {
                        page: label.to_owned(),
                        widget: n,
                        source: source.clone(),
                        name: name.clone(),
                        rect,
                        point: outside,
                        selected: miss
                            .selected
                            .iter()
                            .map(|&s| drive.describe(&[s]))
                            .collect(),
                        why: "a press 2 points outside the rect selected it anyway",
                    });
                }
            }
            let reached = if declining {
                format!(
                    " (the press is {})",
                    probe
                        .reacted
                        .iter()
                        .map(|&m| drive.describe(&[m]))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            } else if forwarded {
                let through = probe.selected.first().copied().unwrap_or(n);
                format!(
                    " (forwarded through {} {})",
                    drive.describe(&probe.selected),
                    drive.kind(through)
                )
            } else if one {
                String::new()
            } else {
                format!(
                    " (reacted: {})",
                    probe
                        .reacted
                        .iter()
                        .map(|&m| drive.describe(&[m]))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            println!(
                "  {n:>3}  {:<7} {:<24} {:<20} {:<20} {:<20} {}  {verdict}{}{reached}",
                clip(&source, 7),
                clip(&name, 24),
                drawn(rect),
                at(point),
                clip(&selected, 20),
                edit,
                unreachable.map_or_else(String::new, |m| format!(
                    " (painted under {} {})",
                    drive.describe(&[m]),
                    drive.kind(m)
                )),
            );
        }
        self.editing(&mut drive, label, out)?;
        self.overlapping(&mut drive, label)?;
        self.driven.push(Page {
            label: label.to_owned(),
            checked: checked_here,
        });
        Ok(())
    }

    /// Every text field on the page: a double click must edit it in place.
    /// The editor is the field itself, drawn exactly over the control, with
    /// no panel and no second copy in a window.
    fn editing(
        &mut self,
        drive: &mut Drive,
        label: &str,
        out: Option<&Path>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let count = drive.tree.widgets.len();
        for n in 0..count {
            let wr = WidgetRef(n);
            if !drive.tree.visible(wr) {
                continue;
            }
            let (name, source) = match drive.tree.widgets.get(n) {
                Some(w) if takes_input(w) && matches!(w.kind, Kind::TextEdit) => (
                    w.name.clone(),
                    w.source_id
                        .map_or_else(|| "-".to_owned(), |s| s.to_string()),
                ),
                _ => continue,
            };
            let Some(rect) = drive.rect(n) else {
                continue;
            };
            let centre = Point::new(rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
            if !drive.on_page(centre) {
                continue;
            }
            // Two presses inside mui's double-click window (0.4 s).
            drive.press(centre)?;
            drive.press_after(centre)?;
            let entered = drive.gesture.entry() == Some(n);
            let editor = drive.popup_rect(n);
            // The editor is the field's width, centred on it, one line tall:
            // a short field's text would scroll if its height were forced.
            let in_place = editor.is_some_and(|e| {
                (e.2 - rect.2).abs() <= 1.
                    && ((e.0 + e.2 / 2.) - (rect.0 + rect.2 / 2.)).abs() <= 1.
                    && ((e.1 + e.3 / 2.) - (rect.1 + rect.3 / 2.)).abs() <= 1.
                    && e.3 <= 2. * rect.3.max(18.)
            });
            self.editing += 1;
            println!(
                "  edit {name}: double click opened {entered}, editor {}{}",
                editor.map_or("not drawn".to_owned(), |e| format!(
                    "{:.0}x{:.0} at ({:.0},{:.0}) for the field's {:.0}x{:.0} at ({:.0},{:.0})",
                    e.2, e.3, e.0, e.1, rect.2, rect.3, rect.0, rect.1
                )),
                if entered && in_place { "" } else { "  FAIL" },
            );
            if let Some(dir) = out {
                let file = dir.join(format!(
                    "{}-edit-{n}.png",
                    label.replace(['/', ' ', '—'], "_")
                ));
                drive.save(&file)?;
                println!("    editor drawn to {}", file.display());
            }
            if !entered || !in_place {
                self.failed.push(Fail {
                    page: label.to_owned(),
                    widget: n,
                    source,
                    name,
                    rect,
                    point: centre,
                    selected: editor
                        .filter(|_| entered)
                        .map_or_else(Vec::new, |_| vec!["itself".to_owned()]),
                    why: if entered {
                        "the field's editor is not drawn in place"
                    } else {
                        "a double click did not edit the text field"
                    },
                });
            }
            // Leave the page as it was: the next check opens nothing.
            drive.gesture = Gesture::default();
        }
        Ok(())
    }

    /// Every control painted under another one that takes input: opens the
    /// one beneath (so whatever it opens is on screen), then presses the one
    /// on top where they overlap. One press, one owner — the control beneath
    /// must not keep a popup open over a press it does not own.
    ///
    /// Electric Mint's Sound Preset is the case: the list is open, the name
    /// field painted over its plate is clicked, and before the owner rule
    /// both the list (still open) and the field's entry popup reacted.
    fn overlapping(
        &mut self,
        drive: &mut Drive,
        label: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let count = drive.tree.widgets.len();
        for n in 0..count {
            let wr = WidgetRef(n);
            if !drive.tree.visible(wr) {
                continue;
            }
            let (under, source) = match drive.tree.widgets.get(n) {
                Some(w) if takes_input(w) => (
                    w.name.clone(),
                    w.source_id
                        .map_or_else(|| "-".to_owned(), |s| s.to_string()),
                ),
                _ => continue,
            };
            let Some(rect) = drive.rect(n) else {
                continue;
            };
            let centre = Point::new(rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
            let Some(m) = drive.covering(n, centre) else {
                continue;
            };
            // A point of the covered control the override does not cover, so
            // the press there is the covered control's own.
            let Some(exposed) = exposed_point(rect, |p| {
                drive.rect(m).is_none_or(|r| !holds(r, p)) && drive.on_page(p)
            }) else {
                continue;
            };
            let opened = drive.press(exposed)?;
            let over = drive
                .tree
                .widgets
                .get(m)
                .map_or_else(String::new, |w| w.name.clone());
            let popup = drive.gesture.menu.is_some() || drive.gesture.entry().is_some();
            println!(
                "  open {under} from {}, press {over} at {}: {} reaction(s), {}",
                at(exposed),
                at(centre),
                opened.reacted.len(),
                if popup { "left open" } else { "nothing open" },
            );
            if opened.reacted.as_slice() != [n] {
                self.failed.push(Fail {
                    page: label.to_owned(),
                    widget: n,
                    source: source.clone(),
                    name: under.clone(),
                    rect,
                    point: exposed,
                    selected: opened
                        .reacted
                        .iter()
                        .map(|&k| drive.describe(&[k]))
                        .collect(),
                    why: "opening the covered control made another control react",
                });
            }
            if !popup {
                continue;
            }
            self.overlaps += 1;
            let probe = drive.press_after(centre)?;
            if drive.gesture.entry() == Some(m)
                && let (Some(popup), Some(rect)) = (drive.popup_rect(m), drive.rect(m))
            {
                println!(
                    "    entry popup {:.0}x{:.0} for a {:.0}x{:.0} field",
                    popup.2, popup.3, rect.2, rect.3
                );
            }
            // The control over the plate may decline the press (a text edit
            // edits on a double click), in which case the covered control
            // takes it — that is one owner too.
            let one = matches!(probe.reacted.as_slice(), [only] if *only == m || *only == n);
            println!(
                "    press {over}: {} reaction(s){}",
                probe.reacted.len(),
                if one {
                    String::new()
                } else {
                    format!(
                        " ({})",
                        probe
                            .reacted
                            .iter()
                            .map(|&k| drive.describe(&[k]))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            );
            if !one {
                self.failed.push(Fail {
                    page: label.to_owned(),
                    widget: n,
                    source,
                    name: under,
                    rect,
                    point: centre,
                    selected: probe
                        .reacted
                        .iter()
                        .map(|&k| drive.describe(&[k]))
                        .collect(),
                    why: "a press the control above owned still made the control beneath react",
                });
            }
        }
        Ok(())
    }

    /// What this run proved, from its counts alone.
    ///
    /// Per page, not per run. One total over every page is the wrong question:
    /// an instrument whose page drew nothing checked nothing, and a second page
    /// that did check controls made the total look healthy. Both are failures of
    /// the same kind, so both land here.
    #[must_use]
    fn outcome(&self) -> Outcome {
        if !self.failed.is_empty() {
            Outcome::Failed
        } else if self.checked == 0
            || !self.unpaged.is_empty()
            || self.driven.iter().any(|p| p.checked == 0)
        {
            Outcome::CheckedNothing
        } else {
            Outcome::Passed
        }
    }

    /// Every page or instrument that checked nothing, named for the log.
    fn unchecked(&self) -> Vec<String> {
        self.driven
            .iter()
            .filter(|p| p.checked == 0)
            .map(|p| format!("{} (0 controls)", p.label))
            .chain(self.unpaged.iter().map(|i| format!("{i} (no page)")))
            .collect()
    }

    /// Prints the run's table of results and returns what it proved.
    #[must_use]
    fn report(&self) -> Outcome {
        let outcome = self.outcome();
        println!();
        match outcome {
            Outcome::CheckedNothing => {
                let empty = self.unchecked();
                if empty.is_empty() {
                    println!(
                        "FAIL: no control was checked on {} page(s), {} skipped — a page that drew \
                         no control cannot be shown to hit-test, so this run proves nothing",
                        self.pages, self.skipped
                    );
                } else {
                    println!(
                        "FAIL: {} of {} page(s) checked no control, {} skipped — a page that drew \
                         nothing satisfies every per-control check it never ran, so what the other \
                         pages checked cannot stand in for it: {}",
                        empty.len(),
                        self.pages,
                        self.skipped,
                        empty.join(", ")
                    );
                }
            }
            Outcome::Passed => println!(
                "PASS: {} controls checked on {} page(s), {} skipped; every control's centre \
                 selected itself and a press 2 points outside selected nothing. {} answered a \
                 click with an Edit (a knob or slider selects without changing, as Kontakt does; \
                 an XY pad reports both coordinates of the press). \
                 {} are painted under another control, so no click can reach them in Kontakt \
                 either; each is named in the table. {} single press(es) on a text field were \
                 the control beneath its business ({} field(s) edit in place on a double click). \
                 {} overlapping pairs were driven: the covered control opened, then the control \
                 over it pressed, and each press had exactly one owner.",
                self.checked,
                self.pages,
                self.skipped,
                self.edited,
                self.covered,
                self.declining,
                self.editing,
                self.overlaps,
            ),
            Outcome::Failed => {
                println!(
                    "FAIL: {} of {} controls checked did not hit-test where they are drawn",
                    self.failed.len(),
                    self.checked
                );
                for f in &self.failed {
                    println!(
                        "  {} #{} {} (source {}) drawn {}, pressed {} — {}{}",
                        f.page,
                        f.widget,
                        f.name,
                        f.source,
                        drawn(f.rect),
                        at(f.point),
                        f.why,
                        if f.selected.is_empty() {
                            String::new()
                        } else {
                            format!(" instead: {}", f.selected.join(", "))
                        },
                    );
                }
            }
        }
        outcome
    }
}

/// A page drawn offscreen, driven with the editor's own pointer path.
struct Drive {
    ui: Ui,
    tree: Interface,
    pictures: Pictures,
    gesture: Gesture,
    face: usize,
    scale: f64,
    size: (f64, f64),
    /// Page 0's widgets back to front, and each one's place in that order:
    /// what is painted above what.
    order: Vec<usize>,
    rank: Vec<usize>,
}

/// One press at one point: which widget mui says it landed on, the edits the
/// editor's reader made of it, and every widget that reacted to it (the press
/// itself, a popup it opened, an edit it reported).
struct Probe {
    selected: Vec<usize>,
    edits: Vec<Edit>,
    reacted: Vec<usize>,
}

impl Drive {
    fn new(
        tree: Interface,
        face: usize,
        scale: f64,
        path: &Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (w, h) = tree
            .pages
            .first()
            .map_or((0., 0.), |p| (p.size.width, p.size.height));
        if !(w > 0. && h > 0.) {
            return Err("the page has no size".into());
        }
        let (reader, library) = (path.to_path_buf(), std::sync::Mutex::new(None));
        let pictures = Pictures::new(Box::new(move |p: &str| {
            let mut library = library.lock().ok()?;
            library
                .get_or_insert_with(|| Library::open(&reader))
                .read(p)
        }));
        let order: Vec<usize> = tree.draw_order(PageRef(0)).iter().map(|w| w.0).collect();
        let mut rank = vec![usize::MAX; tree.widgets.len()];
        for (i, &n) in order.iter().enumerate() {
            if let Some(r) = rank.get_mut(n) {
                *r = i;
            }
        }
        Ok(Self {
            ui: kontra_ui::ui(),
            tree,
            pictures,
            gesture: Gesture::default(),
            face,
            scale,
            size: (w, h),
            order,
            rank,
        })
    }

    /// One frame the way the editor draws it: the reader first, against the
    /// rects the last frame recorded, then the page, then mui's hit test.
    fn frame(&mut self, input: Input) -> Result<Vec<Edit>, Box<dyn std::error::Error>> {
        let (edits, popup) = input::read(
            &mut self.ui,
            self.face,
            &self.tree,
            PageRef(0),
            self.scale,
            &mut self.gesture,
        );
        // The edits are reported, not applied: this harness asks where a press
        // landed, and applying one control's value would move the next probe's
        // starting point.
        let (page, _) = view::page(
            &mut self.ui,
            self.face,
            &self.tree,
            PageRef(0),
            &mut self.pictures,
            (self.scale, &[]),
        );
        let el = match popup {
            Some(popup) => mui::prelude::stack([page, popup]),
            None => page,
        };
        let (w, h) = self.size;
        self.ui
            .frame(
                el,
                Some(ViewSize::new(w * self.scale, h * self.scale)),
                input,
                1. / 60.,
            )
            .map_err(|e| format!("{e:?}"))?;
        Ok(edits)
    }

    /// Draws until the page's pictures are in (a page that never finishes
    /// loading is drawn as it is, not waited on forever).
    fn settle(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.frame(Input::default())?;
        for _ in 0..400 {
            if self.pictures.pending() == 0 {
                break;
            }
            if self.pictures.poll() {
                self.frame(Input::default())?;
            } else {
                std::thread::sleep(Duration::from_micros(500));
            }
        }
        self.frame(Input::default())?;
        self.frame(Input::default())?;
        Ok(())
    }

    /// The rect mui recorded when it drew widget `n`, in view points.
    fn rect(&self, n: usize) -> Option<(f64, f64, f64, f64)> {
        let id = input::surface(self.face, n);
        let s = self.ui.scene()?.surface(&id)?;
        Some((
            s.frame.x,
            s.frame.y,
            s.frame.size.width,
            s.frame.size.height,
        ))
    }

    /// The popup `n` floats, as mui drew it.
    fn popup_rect(&self, n: usize) -> Option<(f64, f64, f64, f64)> {
        let id = format!("{}-popup", input::surface(self.face, n));
        let s = self.ui.scene()?.surface(&id)?;
        Some((
            s.frame.x,
            s.frame.y,
            s.frame.size.width,
            s.frame.size.height,
        ))
    }

    fn on_page(&self, p: Point) -> bool {
        let (w, h) = self.size;
        (0. ..=w * self.scale).contains(&p.x) && (0. ..=h * self.scale).contains(&p.y)
    }

    /// Two points outside the drawn rect, at the middle of an edge, the first
    /// one still on the page.
    fn outside(&self, rect: (f64, f64, f64, f64)) -> Option<Point> {
        let (x, y, w, h) = rect;
        let (mid_x, mid_y) = (x + w / 2., y + h / 2.);
        [
            Point::new(x - 2., mid_y),
            Point::new(x + w + 2., mid_y),
            Point::new(mid_x, y - 2.),
            Point::new(mid_x, y + h + 2.),
        ]
        .into_iter()
        .find(|&p| self.on_page(p))
    }

    /// Down at `point`, a move held there, up, then a quiet frame — the
    /// editor's own sequence — and what the press selected.
    fn press(&mut self, point: Point) -> Result<Probe, Box<dyn std::error::Error>> {
        self.gesture = Gesture::default();
        self.press_after(point)
    }

    /// [`press`] without forgetting the gesture: the page is in the state the
    /// last press left it (a list still open), as it is for the next click.
    fn press_after(&mut self, point: Point) -> Result<Probe, Box<dyn std::error::Error>> {
        self.settle()?;
        let mut reacted = BTreeSet::new();
        let mut edits = self.frame(pointer(point, true))?;
        self.reacted(&mut reacted, &edits);
        // mui hit-tests the press against the last scene in the next frame:
        // the reader sees it on the move, and `ui.get` reports it now.
        let mut selected = self.pressed();
        edits.extend(self.frame(pointer(point, true))?);
        self.reacted(&mut reacted, &edits);
        if selected.is_empty() {
            selected = self.pressed();
        }
        edits.extend(self.frame(pointer(point, false))?);
        self.reacted(&mut reacted, &edits);
        edits.extend(self.frame(Input::default())?);
        self.reacted(&mut reacted, &edits);
        // The capture is released by the quiet frame above, so the next probe
        // starts with nothing held.
        reacted.extend(self.gesture.menu);
        reacted.extend(self.gesture.entry());
        Ok(Probe {
            selected,
            edits,
            reacted: reacted.into_iter().collect(),
        })
    }

    /// Adds every widget this frame's press reached: the one mui pressed,
    /// held, clicked or released, and the one an edit names. The popups still
    /// open at the end of the press are added by the caller: one that was
    /// already open and *did not* close under a press another control owns
    /// has reacted to it.
    fn reacted(&self, into: &mut BTreeSet<usize>, edits: &[Edit]) {
        into.extend(self.gesture.owner());
        into.extend(edits.iter().map(|e| e.widget.0));
    }

    /// The widget painted above `n` whose drawn rect holds `point`, if any:
    /// the press there belongs to it, in Kontakt as here.
    fn covering(&self, n: usize, point: Point) -> Option<usize> {
        let after = self.rank.get(n).copied().unwrap_or(usize::MAX);
        self.order
            .iter()
            .skip(after.saturating_add(1))
            .copied()
            .find(|&m| self.rect(m).is_some_and(|r| holds(r, point)))
    }

    /// Whether `m` is painted under `n`.
    fn is_beneath(&self, m: usize, n: usize) -> bool {
        self.rank.get(m).copied().unwrap_or(usize::MAX) < self.rank.get(n).copied().unwrap_or(0)
    }

    fn kind(&self, n: usize) -> &'static str {
        match self.tree.widgets.get(n).map(|w| &w.kind) {
            Some(Kind::Knob { .. }) => "knob",
            Some(Kind::Slider { .. }) => "slider",
            Some(Kind::ValueEdit { .. }) => "value edit",
            Some(Kind::Button { .. }) => "button",
            Some(Kind::Switch) => "switch",
            Some(Kind::Menu { .. }) => "menu",
            Some(Kind::Table { .. }) => "table",
            Some(Kind::Xy { .. }) => "xy pad",
            Some(Kind::TextEdit) => "text edit",
            Some(Kind::Label) => "label",
            Some(Kind::Image) => "image",
            Some(Kind::Panel) => "panel",
            _ => "widget",
        }
    }

    /// The widgets whose surface reports the press (or the capture) this
    /// frame, as mui resolved it.
    fn pressed(&self) -> Vec<usize> {
        (0..self.tree.widgets.len())
            .filter(|&n| {
                let r = self.ui.get(input::surface(self.face, n));
                r.pressed || r.held
            })
            .collect()
    }

    fn describe(&self, widgets: &[usize]) -> String {
        if widgets.is_empty() {
            return "nothing".to_owned();
        }
        widgets
            .iter()
            .filter_map(|&n| self.tree.widgets.get(n))
            .map(|w| w.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn save(&self, file: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let (w, h) = (
            (self.size.0 * self.scale).ceil() as u16,
            (self.size.1 * self.scale).ceil() as u16,
        );
        let mut ctx = vello::vello_cpu::RenderContext::new(w, h);
        let mut resources = vello::vello_cpu::Resources::default();
        let scene = self.ui.scene().ok_or("no scene")?;
        let cpu = &mut vello::Cpu {
            ctx: &mut ctx,
            resources: &mut resources,
            cache: &mut vello::Cache::default(),
        };
        vello::paint(cpu, scene, vello::kurbo::Affine::IDENTITY).map_err(|e| format!("{e:?}"))?;
        ctx.flush();
        let mut pix = vello::vello_cpu::Pixmap::new(w, h);
        ctx.render(&mut pix, &mut resources);
        let rgba: Vec<u8> = pix
            .take_unpremultiplied()
            .iter()
            .flat_map(|p| [p.r, p.g, p.b, p.a])
            .collect();
        let mut e = png::Encoder::new(
            std::io::BufWriter::new(std::fs::File::create(file)?),
            w.into(),
            h.into(),
        );
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()?.write_image_data(&rgba)?;
        Ok(())
    }
}

/// A widget the pointer can act on (the same set `input::read` reads).
fn takes_input(w: &Widget) -> bool {
    w.enabled
        && matches!(
            w.kind,
            Kind::Knob { .. }
                | Kind::Slider { .. }
                | Kind::ValueEdit { .. }
                | Kind::Button { .. }
                | Kind::Switch
                | Kind::Menu { .. }
                | Kind::Table { .. }
                | Kind::Xy { .. }
                | Kind::TextEdit
        )
}

/// A KSP script's page as the engine left it after `on init`, then with each
/// control's look and words applied (as `ui-shots` settles one).
fn settle_script(engine: &mut Engine, slot: u8, tree: &mut Interface) {
    let mut block = vec![[0f32; 2]; 512];
    for _ in 0..10 {
        engine.process(&mut block, &[]);
    }
    let mut text = kontra_engine::Text::EMPTY;
    for n in 0..tree.widgets.len() {
        let Ok(n32) = u32::try_from(n) else { continue };
        if let Some(look) = engine.widget(slot, n32) {
            let words = engine.widget_text(slot, n32, &mut text);
            kontra_host::ui::lua::apply(tree, n, look, words.then(|| text.as_str()));
        }
    }
    kontra_ui::layout::resolve(tree, 0..tree.widgets.len());
}

fn pointer(pos: Point, down: bool) -> Input {
    Input {
        pointer: PointerInput {
            pos: Some(pos),
            buttons: if down {
                Buttons::PRIMARY
            } else {
                Buttons::default()
            },
            mods: Mods::default(),
        },
        ..Default::default()
    }
}

fn edit_text(e: &Edit) -> String {
    let what = match &e.to {
        Set::Value(v) => format!("{v:.3}"),
        Set::Cell(k, v) => format!("cell {k} = {v:.3}"),
        Set::Axes { cell, at, event } => {
            format!("cursor {cell} = ({:.3}, {:.3}) {event:?}", at[0], at[1])
        }
        Set::Text(t) => format!("text {t:?}"),
    };
    format!("{} = {what}", e.widget.0)
}

/// A point of `rect` that `free` accepts, sampled across the rect from its
/// corners inward: the part of a covered control its override leaves visible.
fn exposed_point(rect: (f64, f64, f64, f64), free: impl Fn(Point) -> bool) -> Option<Point> {
    let (x, y, w, h) = rect;
    [0.05, 0.5, 0.95]
        .into_iter()
        .flat_map(|fx| {
            [0.05, 0.5, 0.95]
                .into_iter()
                .map(move |fy| Point::new(x + w * fx, y + h * fy))
        })
        .find(|&p| free(p))
}

/// Whether a drawn rect holds a point.
fn holds(r: (f64, f64, f64, f64), p: Point) -> bool {
    p.x >= r.0 && p.x <= r.0 + r.2 && p.y >= r.1 && p.y <= r.1 + r.3
}

fn drawn(r: (f64, f64, f64, f64)) -> String {
    format!("{:.0}×{:.0} @({:.0},{:.0})", r.2, r.3, r.0, r.1)
}

fn at(p: Point) -> String {
    format!("({:.1},{:.1})", p.x, p.y)
}

fn clip(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Drive, Outcome, Page, Run};
    use mui::prelude::Point;

    fn page(label: &str, checked: usize) -> Page {
        Page {
            label: label.to_owned(),
            checked,
        }
    }

    #[test]
    fn a_run_that_checked_nothing_is_not_a_pass() {
        // No instrument page, every control skipped, or a page with no drawn
        // control: each leaves `checked` at zero, and none of them proves the
        // hit test works. This is the case the gate must not read as "PASS".
        let empty = Run::default();
        assert_eq!(empty.outcome(), Outcome::CheckedNothing);
        let all_skipped = Run {
            pages: 2,
            skipped: 17,
            driven: vec![page("script 0", 0), page("face 0", 0)],
            ..Run::default()
        };
        assert_eq!(all_skipped.outcome(), Outcome::CheckedNothing);
    }

    #[test]
    fn one_page_that_checked_nothing_fails_the_run() {
        // The bug this covers: the summary kept one `checked` total for every
        // page and instrument, so 39 controls on one page and none on the next
        // summed to 39 and the run read as a pass for both. A page that drew
        // nothing checked nothing, and that is the answer for that page.
        let half = Run {
            pages: 2,
            checked: 39,
            driven: vec![
                page("Electric Mint — script 0", 39),
                page("Melody — face 0", 0),
            ],
            ..Run::default()
        };
        assert_eq!(half.outcome(), Outcome::CheckedNothing);
    }

    #[test]
    fn an_instrument_with_no_page_at_all_fails_the_run() {
        let silent = Run {
            pages: 1,
            checked: 39,
            driven: vec![page("Electric Mint — script 0", 39)],
            unpaged: vec!["Some Other Library".to_owned()],
            ..Run::default()
        };
        assert_eq!(silent.outcome(), Outcome::CheckedNothing);
    }

    #[test]
    fn checked_controls_with_no_failure_pass() {
        let run = Run {
            pages: 1,
            checked: 39,
            skipped: 3,
            driven: vec![page("Electric Mint — script 0", 39)],
            ..Run::default()
        };
        assert_eq!(run.outcome(), Outcome::Passed);
    }

    #[test]
    fn checked_controls_with_a_failure_fail() {
        let mut run = Run {
            pages: 1,
            checked: 39,
            driven: vec![page("Electric Mint — script 0", 39)],
            ..Run::default()
        };
        run.failed.push(super::Fail {
            page: "page".to_owned(),
            widget: 0,
            source: "0".to_owned(),
            name: "control".to_owned(),
            rect: (0., 0., 1., 1.),
            point: Default::default(),
            selected: Vec::new(),
            why: "test",
        });
        assert_eq!(run.outcome(), Outcome::Failed);
    }

    /// Electric Mint (Melody)'s articulation and pattern rows are `ui_xy`
    /// click areas — `declare ui_xy ?rjhag[1]` and its seven neighbours, each
    /// 258×28 at y = 320/346/372/398 in two columns — not buttons. A press at
    /// a row's centre must select *that* row: the script's `on ui_control` for
    /// it sets `$earko` and writes `$CONTROL_PAR_ACTIVE_INDEX` 0 on the row
    /// (the one `change_slot` marks selected) and -1 on the others. An edit
    /// that only says "something was clicked" cannot pass this.
    #[test]
    fn a_click_on_a_melody_row_selects_that_row() {
        use std::cell::RefCell;
        use std::path::PathBuf;
        use std::sync::Arc;

        use kontra_engine::Engine;
        use kontra_host::ui::{Library, Live};
        use kontra_stream::{Store, StoreConfig};
        use kontra_ui::input::Set;
        use kontra_ui_model::{Kind, MouseEvent};

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
            return;
        }

        let mut store = Store::new(StoreConfig::DEFAULT).expect("a store");
        let loaded = kontra_host::instrument::load(&path, 48_000, &mut store)
            .expect("the Melody instrument loads");
        let mut engine = Engine::new(Arc::new(loaded.plan), store.client(), 48_000);
        let face = loaded.faces[0].as_ref().expect("a script page");
        let library = RefCell::new(Library::open(&path));
        let meta = |p: &str| library.borrow_mut().meta(p);
        let mut live = Live::new(face, 0, &engine, &meta).expect("the page builds");
        let all = 0..live.tree.widgets.len();
        kontra_ui::layout::resolve(&mut live.tree, all);

        // The eight rows, in the order the page draws them down each column.
        let mut rows: Vec<usize> = (live.tree.widgets.iter().enumerate())
            .filter(|(n, w)| {
                matches!(w.kind, Kind::Xy { .. })
                    && !w.hidden
                    && live.tree.visible(kontra_ui_model::WidgetRef(*n))
            })
            .map(|(n, _)| n)
            .collect();
        rows.sort_by(|&a, &b| {
            let (w, v) = (&live.tree.widgets[a], &live.tree.widgets[b]);
            (w.rect.y, w.rect.x)
                .partial_cmp(&(v.rect.y, v.rect.x))
                .expect("finite rects")
        });
        assert_eq!(
            rows.len(),
            8,
            "the eight rows are Xy controls; found {}",
            rows.len()
        );
        let names: Vec<String> = rows
            .iter()
            .map(|&n| live.tree.widgets[n].name.clone())
            .collect();
        let value_cells: std::collections::BTreeMap<usize, u32> = (rows.iter().copied())
            .filter_map(|n| match live.tree.widgets[n].kind {
                Kind::Xy { cells, .. } => Some((n, cells)),
                _ => None,
            })
            .collect();

        let mut drive = Drive::new(live.tree.clone(), 0, 1.0, &path).expect("a page");
        drive.settle().expect("the page settles");

        // What the script says is selected: `change_slot` presses the
        // selected row's keyswitch and releases the rest
        // (`set_key_pressed`), so the keys down are the selection — read
        // without naming a parameter id, and different for every row.
        let pressed = |engine: &Engine| -> Vec<usize> {
            (engine.keys().iter().enumerate())
                .filter(|(_, k)| k.pressed)
                .map(|(n, _)| n)
                .collect()
        };
        // The instrument starts on one row, so it is clicked last: every
        // press then moves the selection, and a press that did nothing cannot
        // pass.
        let order: Vec<usize> = {
            let mut order = rows.clone();
            order.sort_by_key(|&n| live.tree.widgets[n].active_index != Some(-1));
            order
        };
        assert_eq!(
            pressed(&engine).len(),
            1,
            "the Melody instrument starts on one row"
        );

        let mut picked: Vec<(usize, usize)> = Vec::new();
        for &n in &order {
            let Some(rect) = drive.rect(n) else {
                panic!("row {n} was never drawn");
            };
            let point = Point::new(rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
            let probe = drive.press(point).expect("a press");
            assert_eq!(
                probe.selected,
                [n],
                "the press at {point:?} landed on {:?}, not on {}",
                probe.selected,
                names[rows.iter().position(|&r| r == n).expect("a row")]
            );
            let cells = value_cells.get(&n).copied().expect("a row's cells");
            let id = live.tree.widgets[n].source_id.expect("a script control");
            let bits = |v: f64| v.to_bits() as i64;
            // Every edit the click made, carried out in order the way the
            // editor carries it (`Action::Axes` -> `Rack::axes` -> the
            // engine): the press and the release are separate callbacks, and
            // a pattern row selects on the release.
            let mut events = Vec::new();
            for edit in &probe.edits {
                let Set::Axes { cell, at, event } = &edit.to else {
                    continue;
                };
                if edit.widget.0 != n {
                    continue;
                }
                assert!(
                    (at[0] - 0.5).abs() < 0.01 && (at[1] - 0.5).abs() < 0.01,
                    "the centre of row {n} reported {at:?}, not the middle of each axis"
                );
                engine.set_click(0, id, [false; 3], *cell);
                engine.set_axes(
                    0,
                    id,
                    *cell,
                    bits(at[0]),
                    (cells.saturating_sub(*cell) > 1).then(|| bits(at[1])),
                    *event,
                );
                events.push(*event);
            }
            assert!(
                events.contains(&MouseEvent::Down) && events.contains(&MouseEvent::Up),
                "a click on row {n} sent {events:?}, not both a press and a release"
            );
            let now = pressed(&engine);
            assert_eq!(
                now.len(),
                1,
                "clicking row {n} left {:?} down; one row's keyswitch should be",
                now
            );
            picked.push((n, now[0]));
        }
        // Every row of a column selects its own keyswitch: four rows, four
        // keys per column (`set_key_pressed($pyrwy + <row>)`), so a press
        // that repeated the previous row's cannot pass.
        for column in [40., 336.] {
            let of: Vec<(usize, usize)> = (picked.iter().copied())
                .filter(|&(n, _)| (live.tree.widgets[n].rect.x - column).abs() < 1.)
                .collect();
            assert_eq!(of.len(), 4, "four rows in the column at x={column}");
            let keys: std::collections::BTreeSet<usize> = of.iter().map(|&(_, k)| k).collect();
            assert_eq!(
                keys.len(),
                4,
                "the rows at x={column} pressed {of:?}; two share a keyswitch"
            );
        }
    }
}
