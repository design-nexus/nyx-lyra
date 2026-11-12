//! Settings as the shared card: Lyra's Playback and Account groups, then Appearance,
//! Keyboard and About.

use super::{SHORTCUTS, ui};
use crate::prefs;
use gtk::prelude::*;
use nyx_ui::settings_card::{self, About, Spec, Standard};
use nyx_ui::widgets;

const GROUPS: &[(&str, &str, &str)] = &[
    ("Playback", "media-playback-start-symbolic", "Lyrics, the column, notifications, output"),
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
            "Signed in. Like a browser, Lyra keeps the Google session in its own folder (~/.local/share/nyx-lyra), readable by your programs and copied by backups of your home folder. Signing out forgets it; your Google account and Online Accounts stay as they are.",
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
