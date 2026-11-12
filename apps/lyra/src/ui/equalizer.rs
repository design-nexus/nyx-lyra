//! The equalizer, as Nyx Music's: a card over the window (from the header's button)
//! with a preamp and ten bands as vertical faders, and presets.

use super::ui;
use crate::{eq, prefs};
use gtk::prelude::*;
use nyx_ui::widgets;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct Card {
    veil: gtk::Box,
    card: gtk::Box,
    scroll: gtk::ScrolledWindow,
}

thread_local! {
    static CARD: RefCell<Option<Card>> = const { RefCell::new(None) };
    static REBUILD: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Open the card, building it the first time.
pub fn open() {
    let Some(shown) = ui() else { return };
    if CARD.with(|c| c.borrow().is_none()) {
        let card = make();
        shown.overlay.add_overlay(&card.veil);
        shown.overlay.set_measure_overlay(&card.veil, false);
        CARD.with(|c| *c.borrow_mut() = Some(card));
    }
    CARD.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            c.veil.set_visible(true);
        }
    });
    fit(&shown.window);
}

pub fn is_open() -> bool {
    CARD.with(|c| c.borrow().as_ref().is_some_and(|c| c.veil.is_visible()))
}

pub fn close() {
    CARD.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            c.veil.set_visible(false);
        }
    });
}

/// Size the card to the window: as wide as it allows, up to a limit, and as tall as its
/// page (scrolling when the window is shorter).
pub fn fit(window: &gtk::ApplicationWindow) {
    let (w, h) = (window.width(), window.height());
    if w <= 0 || h <= 0 {
        return;
    }
    CARD.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            let width = (w - 48).clamp(0, 860);
            if c.card.width_request() != width {
                c.card.set_size_request(width, -1);
            }
            // The card's header takes about 64 px of the height.
            c.scroll.set_max_content_height((h - 48 - 64).max(120));
        }
    });
}

fn make() -> Card {
    let veil = gtk::Box::new(gtk::Orientation::Vertical, 0);
    veil.add_css_class("settings-veil");
    veil.set_hexpand(true);
    veil.set_vexpand(true);
    veil.set_visible(false);
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("settings-dialog");
    card.add_css_class("panel-dialog");
    card.set_halign(gtk::Align::Center);
    card.set_valign(gtk::Align::Center);
    card.set_vexpand(true);
    card.set_overflow(gtk::Overflow::Hidden);
    veil.append(&card);

    // A click on the dimmed window around the card closes it.
    let click = gtk::GestureClick::new();
    let c = card.clone();
    click.connect_pressed(move |g, _, x, y| {
        let Some(veil) = g.widget() else { return };
        let inside = c.compute_bounds(&veil).is_some_and(|b| b.contains_point(&gtk::graphene::Point::new(x as f32, y as f32)));
        if !inside {
            close();
        }
    });
    veil.add_controller(click);

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    head.add_css_class("dialog-head");
    let title = widgets::label("Equalizer", "dialog-title");
    title.set_hexpand(true);
    head.append(&title);
    let x = widgets::bar_button("window-close-symbolic", "Close (Esc)");
    x.connect_clicked(|_| close());
    head.append(&x);
    card.append(&head);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.add_css_class("settings-page");
    body.set_hexpand(true);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .propagate_natural_height(true)
        .child(&body)
        .build();
    build(&body);
    let main = gtk::Box::new(gtk::Orientation::Vertical, 0);
    main.add_css_class("settings-dialog-main");
    main.append(&scroll);
    card.append(&main);
    Card { veil, card, scroll }
}

struct Fader {
    scale: gtk::Scale,
    value: gtk::Label,
}

fn fader(label: &str, value: f64, preamp: bool) -> (gtk::Box, Fader) {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
    b.add_css_class("fader");
    b.set_hexpand(true);
    if preamp {
        b.add_css_class("preamp");
    }
    let v = gtk::Label::new(Some(&eq::db(value)));
    v.add_css_class("fader-value");
    v.add_css_class("mono");
    let (lo, hi) = nyx_lyra_core::command::EQ_RANGE;
    let scale = gtk::Scale::with_range(gtk::Orientation::Vertical, lo, hi, 0.5);
    scale.add_css_class("fader-scale");
    scale.set_inverted(true);
    scale.set_draw_value(false);
    scale.set_value(value);
    scale.set_vexpand(true);
    scale.set_halign(gtk::Align::Center);
    // Fine steps with the keyboard, snap to 0.5 dB when dragging.
    scale.set_increments(0.5, 3.0);
    let l = gtk::Label::new(Some(label));
    l.add_css_class("fader-label");
    b.append(&v);
    b.append(&scale);
    b.append(&l);
    (b, Fader { scale, value: v })
}

/// On or off, a preset, and the faders: they apply as they move.
fn build(body: &gtk::Box) {
    let p = prefs::get();
    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("settings-group");
    let note =
        gtk::Label::new(Some("Boosting bands makes loud songs clip; lower the preamp by about as much as your biggest boost."));
    note.add_css_class("group-note");
    note.set_xalign(0.0);
    note.set_wrap(true);
    group.append(&note);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.add_css_class("group-list");
    group.append(&list);
    body.append(&group);

    let panel = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    panel.add_css_class("eq-panel");
    if !p.eq_enabled {
        panel.add_css_class("bypassed");
    }
    let pn = panel.clone();
    let (r, _) =
        widgets::switch_row("Equalizer", "Shape the sound. When it's off, music plays untouched.", p.eq_enabled, move |on| {
            prefs::update(|p| p.eq_enabled = on);
            if on {
                pn.remove_css_class("bypassed");
            } else {
                pn.add_css_class("bypassed");
            }
            eq::apply();
        });
    list.append(&r);

    // ----- Faders -----
    let (pre_box, pre) = fader("Preamp", p.eq_preamp, true);
    panel.append(&pre_box);
    let div = gtk::Box::new(gtk::Orientation::Vertical, 0);
    div.add_css_class("eq-divider");
    panel.append(&div);
    let mut bands = Vec::new();
    for (i, f) in nyx_lyra_core::command::EQ_BANDS.iter().enumerate() {
        let (b, fd) = fader(&eq::hz(*f), p.eq_bands[i], false);
        panel.append(&b);
        bands.push(fd);
    }
    let faders: Rc<(Fader, Vec<Fader>)> = Rc::new((pre, bands));

    // ----- Presets -----
    let preset_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let dd_holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    preset_row.append(&dd_holder);
    let save = gtk::Button::with_label("Save as preset…");
    preset_row.append(&save);
    let delete_holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    preset_row.append(&delete_holder);
    list.append(&widgets::row("Preset", "Start from a shape, then fine-tune the faders.", Some(preset_row.upcast_ref())));
    list.append(&panel);

    // Programmatic fader moves shouldn't count as hand edits.
    let syncing = Rc::new(Cell::new(false));
    let rebuild_presets: Rc<dyn Fn()> = {
        let (dd_holder, delete_holder, faders, syncing) =
            (dd_holder.clone(), delete_holder.clone(), faders.clone(), syncing.clone());
        Rc::new(move || {
            let p = prefs::get();
            let presets = eq::all(&p.eq_custom);
            let current = eq::matching(&p.eq_custom, p.eq_preamp, &p.eq_bands).unwrap_or_else(|| "manual".into());
            let mut options: Vec<(String, String)> = presets.iter().map(|(id, pr)| (id.clone(), pr.name.clone())).collect();
            if current == "manual" {
                options.push(("manual".into(), "Custom".into()));
            }
            while let Some(c) = dd_holder.first_child() {
                dd_holder.remove(&c);
            }
            let dd = widgets::dropdown(&options, &current);
            let (faders, syncing) = (faders.clone(), syncing.clone());
            dd.connect_selected_notify(move |d| {
                let Some((id, _)) = options.get(d.selected() as usize) else { return };
                let Some((_, preset)) = eq::all(&prefs::get().eq_custom).into_iter().find(|(i, _)| i == id) else { return };
                prefs::update(|p| {
                    p.eq_preamp = preset.preamp;
                    p.eq_bands = preset.bands;
                });
                syncing.set(true);
                faders.0.scale.set_value(preset.preamp);
                for (f, v) in faders.1.iter().zip(preset.bands.iter()) {
                    f.scale.set_value(*v);
                }
                syncing.set(false);
                eq::apply();
            });
            dd_holder.append(&dd);
            while let Some(c) = delete_holder.first_child() {
                delete_holder.remove(&c);
            }
            if let Some(name) = current.strip_prefix("custom:") {
                let name = name.to_string();
                let del = widgets::two_click("Delete", "Click again to delete", move || {
                    prefs::update(|p| p.eq_custom.retain(|c| c.name != name));
                    toast("Preset deleted.");
                    if let Some(f) = REBUILD.with(|r| r.borrow().clone()) {
                        f();
                    }
                });
                delete_holder.append(&del);
            }
        })
    };
    REBUILD.with(|r| *r.borrow_mut() = Some(rebuild_presets.clone()));
    rebuild_presets();

    // Hand edits apply live; the preset list follows after a moment.
    let pending: Rc<Cell<Option<gtk::glib::SourceId>>> = Rc::default();
    let on_fader = {
        let (faders, syncing, rebuild, pending) = (faders.clone(), syncing.clone(), rebuild_presets.clone(), pending.clone());
        Rc::new(move || {
            let pre = faders.0.scale.value();
            let mut b = [0.0; 10];
            for (i, f) in faders.1.iter().enumerate() {
                b[i] = f.scale.value();
                f.value.set_text(&eq::db(b[i]));
            }
            faders.0.value.set_text(&eq::db(pre));
            if syncing.get() {
                return;
            }
            prefs::update(|p| {
                p.eq_preamp = pre;
                p.eq_bands = b;
            });
            eq::apply();
            if let Some(id) = pending.take() {
                id.remove();
            }
            let (rebuild, pending2) = (rebuild.clone(), pending.clone());
            pending.set(Some(gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(450), move || {
                pending2.set(None);
                rebuild();
            })));
        })
    };
    let f = on_fader.clone();
    faders.0.scale.connect_value_changed(move |_| f());
    for fd in &faders.1 {
        let f = on_fader.clone();
        fd.scale.connect_value_changed(move |_| f());
    }

    save.connect_clicked(move |_| {
        let rebuild = rebuild_presets.clone();
        widgets::ask_text("Save preset", "Keep these fader settings under a name.", "", "Save", move |name| {
            let p = prefs::get();
            if eq::builtin().iter().any(|(_, b)| b.name.eq_ignore_ascii_case(&name)) {
                toast("A built-in preset already has that name.");
                return;
            }
            prefs::update(|pr| {
                pr.eq_custom.retain(|c| c.name != name);
                pr.eq_custom.push(prefs::EqPreset { name: name.clone(), preamp: p.eq_preamp, bands: p.eq_bands });
            });
            rebuild();
            toast(&format!("Saved preset “{name}”."));
        });
    });
}

fn toast(text: &str) {
    if let Some(ui) = ui() {
        nyx_ui::toast::show(&ui.overlay, text);
    }
}
