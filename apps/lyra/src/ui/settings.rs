//! Settings as the shared card: Lyra's Playback group, then Appearance, Keyboard and About.

use super::{SHORTCUTS, ui};
use crate::{eq, prefs};
use gtk::prelude::*;
use nyx_ui::settings_card::{self, About, Spec, Standard};
use nyx_ui::widgets;
use std::cell::Cell;
use std::rc::Rc;

const GROUPS: &[(&str, &str, &str)] = &[
    ("Playback", "media-playback-start-symbolic", "Lyrics, the column, notifications, output"),
    ("Equalizer", "lyra-eq-symbolic", "Ten bands, a preamp and presets"),
    ("Account", "avatar-default-symbolic", "Your YouTube Music channel"),
];

const SPEC: Spec = Spec {
    groups: GROUPS,
    search_text: None,
    standard: Some(Standard {
        app: "lyra",
        about: About {
            name: "Lyra",
            icon: "io.github.design_nexus.nyx.Lyra",
            version: env!("CARGO_PKG_VERSION"),
            description: "YouTube Music for NyxOS, through YouTube Music's own web player.",
            licence: "MIT",
            website: "https://github.com/design-nexus/nyx-lyra",
            issues: "https://github.com/design-nexus/nyx-lyra/issues",
            credits: &[("WebKitGTK", "YouTube Music's web player"), ("GStreamer", "media")],
        },
        shortcuts: SHORTCUTS,
        reduce_motion: || prefs::get().reduce_motion,
        set_reduce_motion: |on| prefs::update(|p| p.reduce_motion = on),
    }),
};

pub fn open() {
    let Some(shown) = ui() else { return };
    settings_card::open(&shown.overlay, &shown.window, SPEC, || {
        // The card keeps one size: its page scrolls, as every Nyx app's settings do.
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.add_css_class("settings-page");
        body.set_hexpand(true);
        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .child(&body)
            .vexpand(true)
            .build();
        let p = prefs::get();
        let playback = group(&body, "Playback");
        playback.append(
            &widgets::switch_row(
                "Show lyrics when available",
                "Now playing opens on Lyrics when YouTube Music has them.",
                p.show_lyrics,
                |on| prefs::update(|p| p.show_lyrics = on),
            )
            .0,
        );
        playback.append(
            &widgets::switch_row(
                "Show the now-playing column",
                "The cover and what plays next beside the pages, on wide windows.",
                p.show_column,
                |on| {
                    prefs::update(|p| p.show_column = on);
                    if let Some(ui) = ui() {
                        ui.fit_column();
                    }
                },
            )
            .0,
        );
        playback.append(
            &widgets::switch_row("Notify when the song changes", "Only while Lyra's window isn't in front.", p.notify, |on| {
                prefs::update(|p| p.notify = on)
            })
            .0,
        );
        playback.append(&quality_row());
        playback.append(
            &widgets::button_row("Output", "NyxOS picks the device for every app.", "Sound settings", |_| {
                let _ = std::process::Command::new("nyx-settings").args(["--section", "audio"]).spawn();
            })
            .0,
        );
        equalizer(&group(&body, "Equalizer"));
        group(&body, "Account").append(&account_row());
        (root.upcast(), body)
    });
}

/// A group of the card: its heading and the list its rows go in.
fn group(body: &gtk::Box, name: &str) -> gtk::Box {
    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("settings-group");
    group.set_widget_name(name);
    let heading = gtk::Label::new(Some(&name.to_uppercase()));
    heading.add_css_class("group-title");
    heading.set_xalign(0.0);
    group.append(&heading);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.add_css_class("group-list");
    group.append(&list);
    body.append(&group);
    list
}

/// Signed in: Sign out (Lyra's session only). Signed out: Sign in.
fn account_row() -> gtk::Box {
    let signed_in = ui().is_some_and(|u| u.signed_in.get() == Some(true));
    if signed_in {
        let out = widgets::two_click("Sign out", "Click again to sign out", || {
            settings_card::close();
            crate::cache::clear();
            if let Some(p) = crate::player() {
                p.sign_out();
            }
        });
        widgets::row(
            "YouTube Music account",
            "Signed in. Signing out forgets the session in Lyra; your Google account and Online Accounts stay as they are.",
            Some(out.upcast_ref()),
        )
    } else {
        widgets::button_row(
            "YouTube Music account",
            "Signed out. YouTube Music plays only signed in; you sign in on Google's own page.",
            "Sign in",
            |_| {
                settings_card::close();
                if let Some(ui) = ui() {
                    ui.show_signin(true, None);
                }
            },
        )
        .0
    }
}

/// YouTube Music's Audio quality for the account, with its own options (High needs
/// Premium); a change applies from the next song.
fn quality_row() -> gtk::Box {
    let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    slot.append(&gtk::Spinner::builder().spinning(true).build());
    let row = widgets::row(
        "Audio quality",
        "Kept by YouTube Music for your account; a change applies from the next song.",
        Some(slot.upcast_ref()),
    );
    crate::quality::read(move |q| {
        while let Some(c) = slot.first_child() {
            slot.remove(&c);
        }
        let Some(q) = q else {
            slot.append(&widgets::label("YouTube Music didn't say", "dim-label"));
            return;
        };
        let current = q.selected.clone().or_else(|| crate::quality::wanted(&q)).unwrap_or_default();
        let options: Vec<(String, String)> = q.options.iter().map(|(name, v)| (v.clone(), name.clone())).collect();
        let drop = widgets::dropdown(&options, &current);
        let item = q.item.clone();
        drop.connect_selected_notify(move |d| {
            let Some((value, _)) = options.get(d.selected() as usize).cloned() else { return };
            prefs::update(|p| p.audio_quality = Some(value.clone()));
            crate::quality::set(&item, &value, |ok| {
                if !ok && let Some(ui) = ui() {
                    nyx_ui::toast::show(&ui.overlay, "YouTube Music didn't change the audio quality");
                }
            });
        });
        slot.append(&drop);
    });
    row
}

/* ── The equalizer (Nyx Music's) ───────────────────────────────────────────────── */

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
    let (lo, hi) = nyx_lyra_core::command::EQ_RANGE;
    let scale = gtk::Scale::with_range(gtk::Orientation::Vertical, lo, hi, 0.5);
    scale.add_css_class("fader-scale");
    scale.set_inverted(true);
    scale.set_draw_value(false);
    scale.set_value(value);
    scale.set_vexpand(true);
    scale.set_halign(gtk::Align::Center);
    scale.set_increments(0.5, 3.0);
    let l = gtk::Label::new(Some(label));
    l.add_css_class("fader-label");
    b.append(&v);
    b.append(&scale);
    b.append(&l);
    (b, Fader { scale, value: v })
}

thread_local! {
    static REBUILD: std::cell::RefCell<Option<Rc<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
}

/// On or off, a preset, and the faders: they apply as they move.
fn equalizer(list: &gtk::Box) {
    let p = prefs::get();
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

    let preset_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let dd_holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    preset_row.append(&dd_holder);
    let save = gtk::Button::with_label("Save as preset…");
    preset_row.append(&save);
    let delete_holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    preset_row.append(&delete_holder);
    list.append(&widgets::row("Preset", "Start from a shape, then fine-tune the faders.", Some(preset_row.upcast_ref())));
    list.append(&panel);
    let note = widgets::label(
        "Boosting bands makes loud songs clip; lower the preamp by about as much as your biggest boost.",
        "dim-label",
    );
    note.set_wrap(true);
    note.set_xalign(0.0);
    note.add_css_class("eq-note");
    list.append(&note);

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
                if let Some(ui) = ui() {
                    nyx_ui::toast::show(&ui.overlay, "A built-in preset already has that name.");
                }
                return;
            }
            prefs::update(|pr| {
                pr.eq_custom.retain(|c| c.name != name);
                pr.eq_custom.push(prefs::EqPreset { name: name.clone(), preamp: p.eq_preamp, bands: p.eq_bands });
            });
            rebuild();
        });
    });
}
