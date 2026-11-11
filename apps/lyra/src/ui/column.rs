//! The now-playing column beside the pages (direction A): the cover, the song, and what
//! YouTube Music plays next. Hidden below 1100 px of window width.

use super::{open_now, play};
use crate::{art, prefs};
use gtk::prelude::*;
use nyx_lyra_core::model::Item;
use nyx_lyra_core::playback::Playback;
use nyx_lyra_core::youtube::Next;
use nyx_ui::{media::Artwork, widgets};
use std::cell::RefCell;

const WIDTH: i32 = 300;
const ART: i32 = 276;

pub struct Column {
    pub root: gtk::Box,
    art: Artwork,
    title: gtk::Label,
    artist: gtk::Label,
    list: gtk::Box,
    last_art: RefCell<Option<String>>,
}

impl Column {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("np-column");
        root.set_width_request(WIDTH);
        root.set_hexpand(false);
        root.set_visible(prefs::get().show_column);
        root.append(&group("Now playing"));
        let art = Artwork::new(ART, ART, "audio-x-generic-symbolic");
        art.root.add_css_class("np-art");
        let open = gtk::Button::new();
        open.add_css_class("flat");
        open.add_css_class("art-button");
        open.set_child(Some(&art.root));
        open.set_tooltip_text(Some("Now playing (F)"));
        open.connect_clicked(|_| open_now("queue"));
        root.append(&open);
        let title = widgets::label("Nothing playing", "np-title");
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.set_lines(2);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let artist = widgets::label("", "np-artist");
        artist.set_xalign(0.0);
        artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
        root.append(&title);
        root.append(&artist);
        root.append(&group("Up next"));
        let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
        let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&list).build();
        root.append(&scroll);
        Self { root, art, title, artist, list, last_art: RefCell::default() }
    }

    pub fn update(&self, s: &Playback) {
        match &s.track {
            Some(t) => {
                self.title.set_text(&t.title);
                self.artist.set_text(&t.artist);
                if *self.last_art.borrow() != t.artwork {
                    *self.last_art.borrow_mut() = t.artwork.clone();
                    // The page's artwork is small; YouTube's larger thumbnail of the same video reads better.
                    art::into(&self.art, t.artwork.as_deref());
                }
            }
            None => {
                self.title.set_text("Nothing playing");
                self.artist.set_text("");
            }
        }
    }

    pub fn set_queue(&self, next: Option<&Next>) {
        fill_queue(&self.list, next, 8);
    }

    /// Shown on wide windows only; the window calls this when its width changes.
    pub fn fit(&self, width: i32) {
        self.root.set_visible(prefs::get().show_column && width >= 1100);
    }
}

fn group(text: &str) -> gtk::Label {
    let l = widgets::label(&text.to_uppercase(), "nav-group");
    l.set_xalign(0.0);
    l
}

/// The songs after the one playing, as rows that play when clicked.
pub fn fill_queue(list: &gtk::Box, next: Option<&Next>, max: usize) {
    while let Some(c) = list.first_child() {
        list.remove(&c);
    }
    let Some(n) = next else { return };
    let start = n.current.map_or(0, |c| c + 1);
    let rest: Vec<&Item> = n.queue.iter().skip(start).take(max).collect();
    if rest.is_empty() {
        let l = widgets::label("YouTube Music picks what plays next when this ends.", "dim-label");
        l.set_wrap(true);
        l.set_xalign(0.0);
        list.append(&l);
        return;
    }
    for item in rest {
        let b = gtk::Button::new();
        b.add_css_class("flat");
        b.add_css_class("queue-row");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let a = Artwork::new(36, 36, "audio-x-generic-symbolic");
        art::into(&a, item.thumbnail(72).map(|t| t.url.as_str()));
        row.append(&a.root);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let t = widgets::label(&item.title, "queue-title");
        t.set_xalign(0.0);
        t.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let who = item.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
        let s = widgets::label(&who, "queue-sub");
        s.set_xalign(0.0);
        s.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&t);
        text.append(&s);
        row.append(&text);
        if let Some(d) = item.duration {
            row.append(&widgets::label(&super::clock(d as f64), "queue-time"));
        }
        b.set_child(Some(&row));
        let it = item.clone();
        b.connect_clicked(move |_| play(&it));
        list.append(&b);
    }
}
