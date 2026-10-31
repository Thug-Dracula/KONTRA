//! A press has one owner: the front-most control that would take it. Electric
//! Mint's Sound Preset plate is the case: a `Menu` at (446,32,171,21) with a
//! `TextEdit` showing the current preset name painted over it.
//!
//! The name field is a `ui_text_edit`, which Kontakt edits in place on a
//! double click. A single click on it is therefore not the field's: it falls
//! through to the menu the field is painted over, which is the instrument's
//! own preset list. Before the rule the click opened a page-filling panel that
//! held a second copy of the field, and the list stayed open behind it.

use kontra_ui::input::{self, Edit, Gesture, Set};
use kontra_ui::{Pictures, view};
use kontra_ui_model::{
    self as model, Interface, Kind, MenuItem, Page, PageRef, Range, Rect, Size, Widget, WidgetRef,
};
use mui::prelude::{Buttons, Input, Mods, Point, PointerInput, Size as ViewSize, Ui};

/// The menu's own width, and the field's: as Electric Mint draws them, the
/// field is narrower and inset into the plate.
const MENU: Rect = Rect::new(220, 10, 171, 21);
const FIELD: Rect = Rect::new(229, 15, 128, 18);

struct Face {
    ui: Ui,
    tree: Interface,
    pictures: Pictures,
    gesture: Gesture,
    size: (f64, f64),
}

impl Face {
    fn new() -> Self {
        let mut tree = Interface::default();
        tree.pages.push(Page {
            size: Size {
                width: 400.,
                height: 300.,
            },
            ..Page::default()
        });
        let item = |text: &str, value| MenuItem {
            text: text.into(),
            value,
            visible: true,
        };
        let menu = Widget::new(
            "$preset",
            PageRef(0),
            MENU,
            Kind::Menu {
                items: vec![
                    item("Preset One", 0),
                    item("Preset Two", 1),
                    item("Preset Three", 2),
                    item("Preset Four", 3),
                ],
            },
        );
        let mut field = Widget::new("@name", PageRef(0), FIELD, Kind::TextEdit);
        field.text = "Preset Two".into();
        let mut knob = Widget::new(
            "$knob",
            PageRef(0),
            Rect::new(10, 10, 85, 52),
            Kind::Knob {
                range: Range {
                    min: 0.,
                    max: 100.,
                    default: 0.,
                    step: Some(1.),
                },
                display: model::Display::default(),
            },
        );
        knob.value = Some(model::Value::Integer(0));
        tree.widgets.extend([menu, field, knob]);
        Self {
            ui: kontra_ui::ui(),
            tree,
            pictures: Pictures::new(Box::new(|_| None)),
            gesture: Gesture::default(),
            size: (400., 300.),
        }
    }

    fn frame(&mut self, input: Input) -> Vec<Edit> {
        let (edits, popup) = input::read(
            &mut self.ui,
            0,
            &self.tree,
            PageRef(0),
            1.,
            &mut self.gesture,
        );
        input::apply(&mut self.tree, &edits);
        let (page, _) = view::page(
            &mut self.ui,
            0,
            &self.tree,
            PageRef(0),
            &mut self.pictures,
            (1., &[]),
        );
        let el = match popup {
            Some(popup) => mui::prelude::stack([page, popup]),
            None => page,
        };
        let (w, h) = self.size;
        assert!(
            self.ui
                .frame(el, Some(ViewSize::new(w, h)), input, 1. / 60.)
                .is_ok()
        );
        edits
    }

    fn settle(&mut self) {
        for _ in 0..6 {
            self.frame(Input::default());
        }
    }

    fn pointer(&mut self, pos: Point, down: bool) -> Vec<Edit> {
        let buttons = if down {
            Buttons::PRIMARY
        } else {
            Buttons::default()
        };
        self.frame(Input {
            pointer: PointerInput {
                pos: Some(pos),
                buttons,
                mods: Mods::default(),
            },
            ..Default::default()
        })
    }

    /// Down, up, then the quiet frame that reads them: what one click carried.
    fn click(&mut self, at: Point) -> Vec<Edit> {
        let mut out = self.pointer(at, true);
        out.extend(self.pointer(at, false));
        out.extend(self.frame(Input::default()));
        out
    }

    /// Two clicks in a row: the second lands inside mui's double-click window
    /// (0.4 s, three 60 Hz frames here).
    fn double_click(&mut self, at: Point) -> Vec<Edit> {
        let mut out = self.click(at);
        out.extend(self.click(at));
        out
    }

    fn frame_of(&self, id: &str) -> Option<(f64, f64, f64, f64)> {
        let f = self.ui.scene()?.surface(id)?.frame;
        Some((f.x, f.y, f.size.width, f.size.height))
    }

    fn at(&self, id: &str) -> Point {
        let (x, y, w, h) = self.frame_of(id).unwrap_or((0., 0., 0., 0.));
        Point::new(x + w / 2., y + h / 2.)
    }
}

/// A point on the plate the field does not cover: its left margin.
fn plate_corner() -> Point {
    Point::new(MENU.x + 3., MENU.y + 3.)
}

fn field_centre() -> Point {
    Point::new(FIELD.x + FIELD.width / 2., FIELD.y + FIELD.height / 2.)
}

#[test]
fn a_click_on_the_name_field_opens_the_list_it_sits_on() {
    let mut f = Face::new();
    f.settle();
    f.click(field_centre());

    // The field declines a single click (it edits on a double click), so the
    // press is the menu's, and the instrument's own list opens.
    assert_eq!(
        f.gesture.menu,
        Some(0),
        "the click on the name did not open the preset list"
    );
    assert_eq!(f.gesture.entry(), None, "the field opened an editor");

    // And the list it opened selects a preset.
    let row = f
        .frame_of("kontra-w-0-0-3")
        .expect("the fourth entry is drawn");
    let edits = f.click(Point::new(row.0 + row.2 / 2., row.1 + row.3 / 2.));
    assert_eq!(
        edits.iter().map(|e| e.to.clone()).collect::<Vec<_>>(),
        [Set::Value(3.)],
        "the list opened from the name did not pick an entry: {edits:?}"
    );
}

#[test]
fn a_click_on_the_name_field_while_the_list_is_open_closes_it() {
    let mut f = Face::new();
    f.settle();
    f.click(plate_corner());
    assert_eq!(
        f.gesture.menu,
        Some(0),
        "the plate's own corner opens the list"
    );

    f.click(field_centre());

    assert_eq!(f.gesture.menu, None, "the list stayed open under the click");
    assert_eq!(f.gesture.entry(), None, "the field opened an editor");
}

#[test]
fn the_field_edits_in_place_with_no_second_window() {
    let mut f = Face::new();
    f.settle();
    f.double_click(field_centre());

    assert_eq!(f.gesture.entry(), Some(1), "a double click edits the field");
    assert_eq!(f.gesture.menu, None, "editing the name left the list open");
    // The editor is the field, drawn exactly where the field is: no panel,
    // and nothing page-sized.
    let editor = f
        .frame_of("kontra-w-0-1-popup")
        .expect("the editor is drawn");
    let field = f
        .frame_of(&input::surface(0, 1))
        .expect("the field is drawn");
    let centre = |r: (f64, f64, f64, f64)| (r.0 + r.2 / 2., r.1 + r.3 / 2.);
    assert_eq!(
        (editor.2, centre(editor)),
        (field.2, centre(field)),
        "the editor is not the field's own width and centre"
    );
    assert!(
        editor.2 < f.size.0 / 2. && editor.3 < f.size.1 / 2.,
        "a {:.0}x{:.0} field drew a {:.0}x{:.0} window",
        field.2,
        field.3,
        editor.2,
        editor.3
    );
}

#[test]
fn the_field_over_the_plate_still_takes_typed_text() {
    let mut f = Face::new();
    f.settle();
    f.double_click(field_centre());
    assert_eq!(f.gesture.entry(), Some(1));
    f.frame(Input {
        text: "My Preset".into(),
        ..Default::default()
    });
    let key = |key| Input {
        keys: vec![mui::prelude::KeyPress {
            key,
            mods: Mods::default(),
        }],
        ..Default::default()
    };
    let mut edits = f.frame(key(mui::prelude::Key::Enter));
    edits.extend(f.frame(Input::default()));
    assert_eq!(
        edits.iter().map(|e| e.to.clone()).collect::<Vec<_>>(),
        [Set::Text("My Preset".into())],
        "the field did not take the typing: {edits:?}"
    );
    assert_eq!(f.tree.widgets[1].text, "My Preset");
}

#[test]
fn a_list_opened_on_a_covered_plate_still_picks_an_entry() {
    let mut f = Face::new();
    f.settle();
    f.click(plate_corner());
    assert_eq!(f.gesture.menu, Some(0), "the list opened");
    let row = f
        .frame_of("kontra-w-0-0-2")
        .expect("the third entry is drawn");
    let edits = f.click(Point::new(row.0 + row.2 / 2., row.1 + row.3 / 2.));
    assert_eq!(
        edits.iter().map(|e| e.to.clone()).collect::<Vec<_>>(),
        [Set::Value(2.)],
        "the pick writes the entry's value: {edits:?}"
    );
    assert_eq!(f.gesture.menu, None, "a pick closes the list");
}

#[test]
fn a_drag_keeps_the_control_it_was_pressed_on() {
    let mut f = Face::new();
    f.settle();
    let start = f.at(&input::surface(0, 2));
    let mut edits = f.pointer(start, true);
    // Far outside the knob's rect (and above it, which is the way its sweep
    // goes), which must not hand the drag to the page.
    edits.extend(f.pointer(Point::new(300., 0.), true));
    edits.extend(f.pointer(Point::new(300., 0.), true));
    edits.extend(f.pointer(Point::new(300., 0.), false));
    edits.extend(f.frame(Input::default()));
    assert!(
        edits.iter().all(|e| e.widget == WidgetRef(2)),
        "the drag left the knob: {edits:?}"
    );
    assert!(
        edits
            .iter()
            .any(|e| matches!(e.to, Set::Value(v) if v > 0.)),
        "the knob did not move: {edits:?}"
    );
}
