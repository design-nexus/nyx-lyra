//! The mockup's grids: as many columns as the width holds (each at least `min` wide, all
//! the same width), either every row or a page of `rows` rows that the shelf's arrows move
//! through; and tiles whose cover is a square as wide as the column.

use gtk::{glib, prelude::*};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone, Copy)]
pub struct Spec {
    pub min: i32,
    pub max_cols: i32,
    pub gap_x: i32,
    pub gap_y: i32,
    /// Rows a page shows; `None` shows them all.
    pub rows: Option<i32>,
}

pub const TILES: Spec = Spec { min: 128, max_cols: 8, gap_x: 18, gap_y: 20, rows: None };
pub const SONGS: Spec = Spec { min: 240, max_cols: 3, gap_x: 12, gap_y: 2, rows: None };

type PageListeners = Rc<RefCell<Vec<Box<dyn Fn(usize, usize)>>>>;

#[derive(Clone)]
pub struct Strip {
    pub root: gtk::Box,
    page: Rc<Cell<usize>>,
    /// (this page, pages), after each layout.
    pages: Rc<Cell<(usize, usize)>>,
    on_pages: PageListeners,
}

fn columns(spec: &Spec, width: i32) -> (i32, i32) {
    let cols = ((width + spec.gap_x) / (spec.min + spec.gap_x)).clamp(1, spec.max_cols);
    let w = ((width - spec.gap_x * (cols - 1)) / cols).max(1);
    (cols, w)
}

fn children(w: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    let mut c = w.first_child();
    while let Some(x) = c {
        c = x.next_sibling();
        out.push(x);
    }
    out
}

impl Strip {
    pub fn new(spec: Spec) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("strip");
        root.set_hexpand(true);
        let page = Rc::new(Cell::new(0usize));
        let pages = Rc::new(Cell::new((0usize, 1usize)));
        let on_pages: PageListeners = Rc::default();
        let measure = move |w: &gtk::Widget, o: gtk::Orientation, for_size: i32| -> (i32, i32, i32, i32) {
            if o == gtk::Orientation::Horizontal {
                return (spec.min, spec.min, -1, -1);
            }
            let width = if for_size > 0 { for_size } else { spec.min };
            let (cols, cw) = columns(&spec, width);
            let kids: Vec<_> = children(w).into_iter().filter(|c| c.is_visible()).collect();
            let rows = kids.len().div_ceil(cols as usize) as i32;
            let rows = spec.rows.map_or(rows, |r| r.min(rows));
            let row_h = kids.iter().map(|c| c.measure(gtk::Orientation::Vertical, cw).1).max().unwrap_or(0);
            let h = rows * row_h + (rows - 1).max(0) * spec.gap_y;
            (h, h, -1, -1)
        };
        let (pg, pgs, listeners) = (page.clone(), pages.clone(), on_pages.clone());
        let allocate = move |w: &gtk::Widget, width: i32, _height: i32, _baseline: i32| {
            let (cols, cw) = columns(&spec, width);
            let kids: Vec<_> = children(w).into_iter().filter(|c| c.is_visible()).collect();
            let per_page = spec.rows.map_or(kids.len().max(1), |r| (r * cols) as usize);
            let count = kids.len().div_ceil(per_page).max(1);
            let page = pg.get().min(count - 1);
            pg.set(page);
            let first = page * per_page;
            let row_h = kids.iter().map(|c| c.measure(gtk::Orientation::Vertical, cw).1).max().unwrap_or(0);
            for (i, c) in kids.iter().enumerate() {
                let shown = (first..first + per_page).contains(&i);
                c.set_child_visible(shown);
                if !shown {
                    continue;
                }
                let n = (i - first) as i32;
                let (col, row) = (n % cols, n / cols);
                let at = gtk::graphene::Point::new((col * (cw + spec.gap_x)) as f32, (row * (row_h + spec.gap_y)) as f32);
                c.allocate(cw, row_h, -1, Some(gtk::gsk::Transform::new().translate(&at)));
            }
            if pgs.get() != (page, count) {
                pgs.set((page, count));
                let listeners = listeners.clone();
                glib::idle_add_local_once(move || listeners.borrow().iter().for_each(|f| f(page, count)));
            }
        };
        root.set_layout_manager(Some(FnLayout::new(measure, allocate)));
        Self { root, page, pages, on_pages }
    }

    pub fn append(&self, child: &impl IsA<gtk::Widget>) {
        self.root.append(child);
    }

    /// Move a page on (`1`) or back (`-1`).
    pub fn turn(&self, by: i32) {
        let (page, count) = self.pages.get();
        let to = (page as i32 + by).clamp(0, count as i32 - 1) as usize;
        if to != page {
            self.page.set(to);
            self.root.queue_allocate();
        }
    }

    /// Hear which page shows and how many there are (for the arrows).
    pub fn connect_pages(&self, f: impl Fn(usize, usize) + 'static) {
        let (page, count) = self.pages.get();
        f(page, count);
        self.on_pages.borrow_mut().push(Box::new(f));
    }
}

/// A tile: `art` square at the column's width, the labels under it.
pub fn tile_layout(tile: &gtk::Box, art: &gtk::Widget) {
    const GAP: i32 = 8;
    let art_m = art.clone();
    let measure = move |w: &gtk::Widget, o: gtk::Orientation, for_size: i32| -> (i32, i32, i32, i32) {
        if o == gtk::Orientation::Horizontal {
            return (64, 160, -1, -1);
        }
        let width = if for_size > 0 { for_size } else { 160 };
        let text: i32 = children(w).iter().filter(|c| **c != art_m && c.is_visible()).map(|c| c.measure(o, width).1).sum();
        let h = width + GAP + text;
        (h, h, -1, -1)
    };
    let art_a = art.clone();
    let allocate = move |w: &gtk::Widget, width: i32, _h: i32, _b: i32| {
        let mut y = 0;
        for c in children(w).into_iter().filter(|c| c.is_visible()) {
            let is_art = c == art_a;
            let h = if is_art { width } else { c.measure(gtk::Orientation::Vertical, width).1 };
            c.allocate(width, h, -1, Some(gtk::gsk::Transform::new().translate(&gtk::graphene::Point::new(0.0, y as f32))));
            y += h + if is_art { GAP } else { 0 };
        }
    };
    tile.set_layout_manager(Some(FnLayout::new(measure, allocate)));
}

/// Turn a shelf's strip from its ‹ › buttons, greying them at the ends.
pub fn arrows(strip: &Strip) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    let back = nyx_ui::widgets::icon_button("lyra-chev-l-symbolic", "Back");
    let on = nyx_ui::widgets::icon_button("lyra-chev-r-symbolic", "More");
    for b in [&back, &on] {
        b.add_css_class("shelf-arrow");
        row.append(b);
    }
    let s = strip.clone();
    back.connect_clicked(move |_| s.turn(-1));
    let s = strip.clone();
    on.connect_clicked(move |_| s.turn(1));
    let (b2, o2, r2) = (back.clone(), on.clone(), row.clone());
    strip.connect_pages(move |page, count| {
        b2.set_sensitive(page > 0);
        o2.set_sensitive(page + 1 < count);
        r2.set_visible(count > 1);
    });
    row
}

/* ── A layout from two closures (gtk4-rs has no binding for GtkCustomLayout) ───── */

type Measure = Box<dyn Fn(&gtk::Widget, gtk::Orientation, i32) -> (i32, i32, i32, i32)>;
type Allocate = Box<dyn Fn(&gtk::Widget, i32, i32, i32)>;

mod imp {
    use super::{Allocate, Measure};
    use gtk::{glib, subclass::prelude::*};
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct FnLayout {
        pub measure: RefCell<Option<Measure>>,
        pub allocate: RefCell<Option<Allocate>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FnLayout {
        const NAME: &'static str = "LyraFnLayout";
        type Type = super::FnLayout;
        type ParentType = gtk::LayoutManager;
    }

    impl ObjectImpl for FnLayout {}

    impl LayoutManagerImpl for FnLayout {
        fn request_mode(&self, _widget: &gtk::Widget) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, widget: &gtk::Widget, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            self.measure.borrow().as_ref().map_or((0, 0, -1, -1), |f| f(widget, o, for_size))
        }

        fn allocate(&self, widget: &gtk::Widget, width: i32, height: i32, baseline: i32) {
            if let Some(f) = self.allocate.borrow().as_ref() {
                f(widget, width, height, baseline)
            }
        }
    }
}

glib::wrapper! {
    pub struct FnLayout(ObjectSubclass<imp::FnLayout>) @extends gtk::LayoutManager;
}

impl FnLayout {
    fn new(
        measure: impl Fn(&gtk::Widget, gtk::Orientation, i32) -> (i32, i32, i32, i32) + 'static,
        allocate: impl Fn(&gtk::Widget, i32, i32, i32) + 'static,
    ) -> Self {
        let l: Self = glib::Object::new();
        use gtk::subclass::prelude::ObjectSubclassIsExt;
        *l.imp().measure.borrow_mut() = Some(Box::new(measure));
        *l.imp().allocate.borrow_mut() = Some(Box::new(allocate));
        l
    }
}
