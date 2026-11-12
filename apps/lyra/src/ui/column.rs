//! The now-playing column beside the pages (direction A, the mockup's): the cover, the song
//! with its heart and menu, and what it plays from: an album's songs, numbered, with the one
//! playing marked, or YouTube Music's Up next. With nothing playing, a placeholder where the
//! cover goes. Hidden below 1100 px of window width.

use super::playerbar::{like_button, show_like};
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
    open: gtk::Button,
    title: gtk::Label,
    artist: gtk::Label,
    like: gtk::Button,
    more: gtk::MenuButton,
    from: gtk::Box,
    from_title: gtk::Label,
    from_count: gtk::Label,
    list: gtk::Box,
    last_art: RefCell<Option<String>>,
    /// The song playing and the queue it's in, kept to redraw the list's marks.
    playing: RefCell<Option<String>>,
    next: RefCell<Option<Next>>,
    album: RefCell<bool>,
}

impl Column {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("np-column");
        root.set_width_request(WIDTH);
        root.set_hexpand(false);
        root.set_visible(prefs::get().show_column);
        root.append(&group("Now playing"));
        let art = Artwork::new(ART, ART, "lyra-note-symbolic");
        art.root.add_css_class("np-art");
        art.placeholder.set_pixel_size(40);
        let open = gtk::Button::new();
        open.add_css_class("flat");
        open.add_css_class("art-button");
        open.set_child(Some(&art.root));
        open.set_tooltip_text(Some("Now playing (F)"));
        open.connect_clicked(|_| open_now("queue"));
        root.append(&open);
        let meta = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        meta.add_css_class("np-meta");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
        text.set_hexpand(true);
        let title = widgets::label("Nothing playing", "np-title");
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.set_lines(2);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        // The column keeps its width: long names wrap or end in "…" instead of widening it.
        title.set_max_width_chars(1);
        let artist = widgets::label("Pick something to play", "np-artist");
        artist.set_max_width_chars(1);
        artist.set_xalign(0.0);
        artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&title);
        text.append(&artist);
        meta.append(&text);
        let like = like_button();
        like.set_valign(gtk::Align::Start);
        meta.append(&like);
        let more = song_menu();
        more.set_valign(gtk::Align::Start);
        meta.append(&more);
        root.append(&meta);
        let from = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        from.add_css_class("np-from");
        let from_title = group("Up next");
        from_title.set_hexpand(true);
        from_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let from_count = widgets::label("", "np-count");
        from.append(&from_title);
        from.append(&from_count);
        root.append(&from);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
        list.add_css_class("np-list");
        let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&list).build();
        root.append(&scroll);
        let c = Self {
            root,
            art,
            open,
            title,
            artist,
            like,
            more,
            from,
            from_title,
            from_count,
            list,
            last_art: RefCell::default(),
            playing: RefCell::default(),
            next: RefCell::default(),
            album: RefCell::new(false),
        };
        c.update(&Playback::default());
        c
    }

    pub fn update(&self, s: &Playback) {
        show_like(&self.like, s);
        match &s.track {
            Some(t) => {
                self.title.set_text(&t.title);
                self.artist.set_text(&match &t.album {
                    Some(a) if !a.is_empty() => format!("{} · {a}", t.artist),
                    _ => t.artist.clone(),
                });
                self.art.root.remove_css_class("empty-art");
                self.open.set_sensitive(true);
                self.more.set_visible(true);
                if *self.last_art.borrow() != t.artwork {
                    *self.last_art.borrow_mut() = t.artwork.clone();
                    art::into(&self.art, t.artwork.as_deref());
                }
                // An album's own list (OLAK5uy_…) shows as the album, numbered.
                *self.album.borrow_mut() = t.playlist_id.as_deref().is_some_and(|l| l.starts_with("OLAK5uy_"));
                if *self.playing.borrow() != t.video_id {
                    *self.playing.borrow_mut() = t.video_id.clone();
                }
            }
            None => {
                self.title.set_text("Nothing playing");
                self.artist.set_text("Pick something to play");
                self.art.set_texture(None);
                self.art.root.add_css_class("empty-art");
                *self.last_art.borrow_mut() = None;
                self.open.set_sensitive(false);
                self.more.set_visible(false);
                *self.next.borrow_mut() = None;
                clear(&self.list);
            }
        }
        self.from.set_visible(s.track.is_some() && self.next.borrow().is_some());
    }

    pub fn set_queue(&self, next: Option<&Next>) {
        *self.next.borrow_mut() = next.cloned();
        let Some(n) = next else {
            clear(&self.list);
            self.from.set_visible(false);
            return;
        };
        self.from.set_visible(true);
        if *self.album.borrow() {
            self.from_title.set_text("FROM THE ALBUM");
            self.from_count.set_text(&songs(n.queue.len()));
            fill_numbered(&self.list, n, 40);
        } else {
            self.from_title.set_text("UP NEXT");
            self.from_count.set_text("Then autoplay");
            fill_queue(&self.list, Some(n), 8);
        }
    }

    /// Shown on wide windows only; the window calls this when its width changes.
    pub fn fit(&self, width: i32) {
        self.root.set_visible(prefs::get().show_column && width >= 1100);
    }
}

fn songs(n: usize) -> String {
    if n == 1 { "1 song".into() } else { format!("{n} songs") }
}

fn group(text: &str) -> gtk::Label {
    let l = widgets::label(&text.to_uppercase(), "nav-group");
    l.set_xalign(0.0);
    l
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

/// "…" by the song: go to its artist or album, or open Now playing.
pub fn song_menu() -> gtk::MenuButton {
    let m = gtk::MenuButton::new();
    m.set_icon_name("lyra-more-symbolic");
    m.set_has_frame(false);
    m.set_tooltip_text(Some("More"));
    m.set_create_popup_func(|button| {
        let pop = gtk::Popover::new();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 1);
        let add = |label: &str, act: Box<dyn Fn()>| {
            let b = gtk::Button::with_label(label);
            b.add_css_class("flat");
            b.add_css_class("menu-row");
            let p = pop.clone();
            b.connect_clicked(move |_| {
                p.popdown();
                act();
            });
            body.append(&b);
        };
        add("Open Now playing", Box::new(|| open_now("queue")));
        add("Lyrics", Box::new(|| open_now("lyrics")));
        add("Related", Box::new(|| open_now("related")));
        pop.set_child(Some(&body));
        button.set_popover(Some(&pop));
    });
    m
}

/// An album's songs, numbered; the one playing marked as the mockup's (accent, bars).
fn fill_numbered(list: &gtk::Box, n: &Next, max: usize) {
    clear(list);
    for (i, item) in n.queue.iter().take(max).enumerate() {
        let on = n.current == Some(i);
        let b = gtk::Button::new();
        b.add_css_class("flat");
        b.add_css_class("track-row");
        if on {
            b.add_css_class("on");
        }
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        if on {
            let eq = gtk::Image::from_icon_name("lyra-eq-symbolic");
            eq.add_css_class("eq");
            row.append(&eq);
        } else {
            let num = widgets::label(&(i + 1).to_string(), "track-number");
            num.set_width_chars(2);
            num.set_xalign(1.0);
            row.append(&num);
        }
        let t = widgets::label(&item.title, "track-name");
        t.set_xalign(0.0);
        t.set_hexpand(true);
        t.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&t);
        if let Some(d) = item.duration {
            row.append(&widgets::label(&super::clock(d as f64), "queue-time"));
        }
        b.set_child(Some(&row));
        let it = item.clone();
        b.connect_clicked(move |_| play(&it));
        list.append(&b);
    }
}

/// The songs after the one playing, with their covers, as rows that play when clicked.
pub fn fill_queue(list: &gtk::Box, next: Option<&Next>, max: usize) {
    clear(list);
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
        list.append(&queue_row(item, false));
    }
}

pub fn queue_row(item: &Item, on: bool) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("flat");
    b.add_css_class("queue-row");
    if on {
        b.add_css_class("on");
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let a = Artwork::new(30, 30, "lyra-note-symbolic");
    a.root.add_css_class("queue-art");
    art::into(&a, item.thumbnail(60).map(|t| t.url.as_str()));
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
    if on {
        let eq = gtk::Image::from_icon_name("lyra-eq-symbolic");
        eq.add_css_class("eq");
        row.append(&eq);
    } else if let Some(d) = item.duration {
        row.append(&widgets::label(&super::clock(d as f64), "queue-time"));
    }
    b.set_child(Some(&row));
    let it = item.clone();
    b.connect_clicked(move |_| play(&it));
    b
}
