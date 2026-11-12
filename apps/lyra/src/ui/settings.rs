//! Settings as the shared card: Lyra's Playback group, then Appearance, Keyboard and About.

use super::{SHORTCUTS, ui};
use crate::prefs;
use gtk::prelude::*;
use nyx_ui::settings_card::{self, About, Spec, Standard};
use nyx_ui::widgets;

const GROUPS: &[(&str, &str, &str)] = &[
    ("Playback", "media-playback-start-symbolic", "Lyrics, the column, notifications, output"),
    ("Account", "avatar-default-symbolic", "Your YouTube Music channel"),
    ("Background", "weather-clear-night-symbolic", "Keeps playing when closed"),
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
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
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
        playback.append(
            &widgets::button_row("Output", "NyxOS picks the device for every app.", "Sound settings", |_| {
                let _ = std::process::Command::new("nyx-settings").args(["--section", "audio"]).spawn();
            })
            .0,
        );
        group(&body, "Account").append(&account_row());
        group(&body, "Background").append(
            &widgets::switch_row(
                "Keep playing when the window closes",
                "Closing Lyra while music plays hides it; open it again from the launcher or the dock.",
                p.keep_playing,
                |on| prefs::update(|p| p.keep_playing = on),
            )
            .0,
        );
        (body.clone().upcast(), body)
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
