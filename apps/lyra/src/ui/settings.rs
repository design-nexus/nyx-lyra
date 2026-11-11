//! Settings as the shared card: Lyra's Playback group, then Appearance, Keyboard and About.

use super::{SHORTCUTS, ui};
use crate::prefs;
use gtk::prelude::*;
use nyx_ui::settings_card::{self, About, Spec, Standard};
use nyx_ui::widgets;

const GROUPS: &[(&str, &str, &str)] =
    &[("Playback", "media-playback-start-symbolic", "Keep playing, the column, the mini-player")];

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
        let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
        group.add_css_class("settings-group");
        group.set_widget_name("Playback");
        let heading = gtk::Label::new(Some("PLAYBACK"));
        heading.add_css_class("group-title");
        heading.set_xalign(0.0);
        group.append(&heading);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.add_css_class("group-list");
        let p = prefs::get();
        list.append(
            &widgets::switch_row(
                "Keep playing when the window closes",
                "Closing Lyra while music plays hides it; open it again from the launcher or the dock.",
                p.keep_playing,
                |on| prefs::update(|p| p.keep_playing = on),
            )
            .0,
        );
        list.append(
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
        group.append(&list);
        body.append(&group);
        (body.clone().upcast(), body)
    });
}
