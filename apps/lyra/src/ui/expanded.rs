//! Now playing, expanded (the art-and-tabs player): the cover large with the song, the
//! seek bar and the controls on the left; Up next, Lyrics and Related on the right.

use super::column::fill_queue;
use super::playerbar::{Clock, send};
use super::{clock, close_now, pages_for, ui};
use crate::{art, player};
use gtk::{glib, prelude::*};
use nyx_lyra_core::command::Command;
use nyx_lyra_core::playback::{Connection, Playback, Status};
use nyx_lyra_core::youtube::{self, Next};
use nyx_ui::{media::Artwork, widgets};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct Expanded {
    pub root: gtk::Box,
    art: Artwork,
    title: gtk::Label,
    artist: gtk::Label,
    play: gtk::Button,
    prev: gtk::Button,
    next: gtk::Button,
    seek: gtk::Scale,
    elapsed: gtk::Label,
    total: gtk::Label,
    tabs: gtk::Stack,
    queue: gtk::Box,
    lyrics: gtk::Box,
    related: gtk::Box,
    clock: Rc<Clock>,
    dragging: Rc<Cell<bool>>,
    last_art: RefCell<Option<String>>,
    lyrics_for: RefCell<Option<String>>,
    related_for: RefCell<Option<String>>,
    next_ids: RefCell<(Option<String>, Option<String>)>,
}

impl Expanded {
    pub fn new() -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 40);
        root.add_css_class("expanded");
        let left = gtk::Box::new(gtk::Orientation::Vertical, 10);
        left.set_width_request(460);
        let art = Artwork::new(460, 460, "audio-x-generic-symbolic");
        art.root.add_css_class("big-art");
        left.append(&art.root);
        let meta = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text.set_hexpand(true);
        let title = widgets::label("Nothing playing", "big-title");
        title.set_xalign(0.0);
        title.set_wrap(true);
        let artist = widgets::label("", "dim-label");
        artist.set_xalign(0.0);
        text.append(&title);
        text.append(&artist);
        meta.append(&text);
        let collapse = widgets::icon_button("go-down-symbolic", "Back to the library (Esc)");
        collapse.connect_clicked(|_| close_now());
        meta.append(&collapse);
        left.append(&meta);
        let seek_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let elapsed = widgets::label("", "bar-time");
        elapsed.set_width_chars(5);
        let seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
        seek.set_hexpand(true);
        seek.set_draw_value(false);
        seek.add_css_class("seek");
        let total = widgets::label("", "bar-time");
        total.set_width_chars(5);
        seek_row.append(&elapsed);
        seek_row.append(&seek);
        seek_row.append(&total);
        left.append(&seek_row);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        buttons.set_halign(gtk::Align::Center);
        let prev = widgets::icon_button("media-skip-backward-symbolic", "Previous (Ctrl+←)");
        let play = widgets::icon_button("media-playback-start-symbolic", "Play (Space)");
        play.add_css_class("play-button");
        play.add_css_class("large");
        let next = widgets::icon_button("media-skip-forward-symbolic", "Next (Ctrl+→)");
        prev.connect_clicked(|_| send(Command::Previous));
        play.connect_clicked(|_| send(Command::Toggle));
        next.connect_clicked(|_| send(Command::Next));
        buttons.append(&prev);
        buttons.append(&play);
        buttons.append(&next);
        left.append(&buttons);
        root.append(&left);

        let right = gtk::Box::new(gtk::Orientation::Vertical, 12);
        right.set_hexpand(true);
        let tabs = gtk::Stack::new();
        tabs.set_vexpand(true);
        let queue = gtk::Box::new(gtk::Orientation::Vertical, 1);
        let lyrics = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let related = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (name, child) in [("queue", &queue), ("lyrics", &lyrics), ("related", &related)] {
            let s = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(child).build();
            tabs.add_named(&s, Some(name));
        }
        let me = Rc::new(Self {
            root,
            art,
            title,
            artist,
            play,
            prev,
            next,
            seek,
            elapsed,
            total,
            tabs: tabs.clone(),
            queue,
            lyrics,
            related,
            clock: Rc::default(),
            dragging: Rc::default(),
            last_art: RefCell::default(),
            lyrics_for: RefCell::default(),
            related_for: RefCell::default(),
            next_ids: RefCell::default(),
        });
        let weak = Rc::downgrade(&me);
        let seg = widgets::segmented(&widgets::opts(&[("queue", "Up next"), ("lyrics", "Lyrics"), ("related", "Related")]), "queue", move |v| {
            if let Some(m) = weak.upgrade() {
                m.show_tab(&v);
            }
        });
        seg.set_halign(gtk::Align::Start);
        seg.set_widget_name("tabs");
        right.append(&seg);
        right.append(&tabs);
        me.root.append(&right);

        let drag = gtk::GestureClick::new();
        let d = me.dragging.clone();
        drag.connect_pressed(move |_, _, _, _| d.set(true));
        let (d, s) = (me.dragging.clone(), me.seek.clone());
        drag.connect_released(move |_, _, _, _| {
            d.set(false);
            send(Command::Seek { position: s.value() });
        });
        me.seek.add_controller(drag);
        let (c, seek, el, dr) = (me.clock.clone(), me.seek.clone(), me.elapsed.clone(), me.dragging.clone());
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            if !dr.get() && seek.adjustment().upper() > 1.0 && seek.is_mapped() {
                let now = c.now().min(seek.adjustment().upper());
                seek.set_value(now);
                el.set_text(&clock(now));
            }
            glib::ControlFlow::Continue
        });
        me
    }

    pub fn update(&self, s: &Playback) {
        self.clock.set(s);
        let playing = s.is_playing();
        self.play.set_icon_name(if playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });
        let ready = s.connection == Connection::Ready;
        self.play.set_sensitive(ready && s.track.is_some());
        self.prev.set_sensitive(ready && s.controls.previous);
        self.next.set_sensitive(ready && s.controls.next);
        self.seek.set_sensitive(ready && s.controls.seek);
        if let Some(t) = &s.track {
            self.title.set_text(&t.title);
            self.artist.set_text(&match &t.album {
                Some(a) if !a.is_empty() => format!("{} • {a}", t.artist),
                _ => t.artist.clone(),
            });
            if *self.last_art.borrow() != t.artwork {
                *self.last_art.borrow_mut() = t.artwork.clone();
                art::into(&self.art, t.artwork.as_deref());
            }
        }
        let d = s.duration.unwrap_or(0.0);
        self.seek.set_range(0.0, d.max(1.0));
        self.total.set_text(&if d > 0.0 { clock(d) } else { String::new() });
        if !self.dragging.get() {
            self.seek.set_value(s.position.unwrap_or(0.0));
        }
        let _ = s.status == Status::Playing;
    }

    pub fn set_queue(self: &Rc<Self>, next: Option<&Next>) {
        fill_queue(&self.queue, next, 40);
        *self.next_ids.borrow_mut() = (next.and_then(|n| n.lyrics.clone()), next.and_then(|n| n.related.clone()));
        // A tab that's showing follows the new song.
        if let Some(name) = self.tabs.visible_child_name() {
            if name != "queue" && self.root.is_mapped() {
                self.show_tab(&name);
            }
        }
    }

    pub fn set_tab(self: &Rc<Self>, tab: &str) {
        // The segmented control follows too.
        if let Some(seg) = self.root.last_child().and_then(|r| r.first_child()) {
            let mut b = seg.first_child();
            let names = ["queue", "lyrics", "related"];
            let mut i = 0;
            while let Some(w) = b {
                if let Some(t) = w.downcast_ref::<gtk::ToggleButton>() {
                    t.set_active(names.get(i) == Some(&tab));
                }
                i += 1;
                b = w.next_sibling();
            }
        }
        self.show_tab(tab);
    }

    fn show_tab(self: &Rc<Self>, tab: &str) {
        self.tabs.set_visible_child_name(tab);
        let (lyrics_id, related_id) = self.next_ids.borrow().clone();
        match tab {
            "lyrics" => {
                if *self.lyrics_for.borrow() == lyrics_id && lyrics_id.is_some() {
                    return;
                }
                *self.lyrics_for.borrow_mut() = lyrics_id.clone();
                let b = self.lyrics.clone();
                clear(&b);
                let Some(req) = lyrics_id.as_deref().and_then(youtube::browse) else {
                    b.append(&note("No lyrics for this song."));
                    return;
                };
                let Some(p) = player() else { return };
                p.request(
                    &req,
                    Box::new(move |r| {
                        clear(&b);
                        match r.ok().map(|v| youtube::lyrics(&v)) {
                            Some(Ok(Some(l))) => {
                                let text = widgets::label(&l.text, "lyrics");
                                text.set_xalign(0.0);
                                text.set_wrap(true);
                                text.set_selectable(true);
                                b.append(&text);
                                b.append(&note(&l.source));
                            }
                            Some(Ok(None)) => b.append(&note("YouTube Music has no lyrics for this song.")),
                            _ => b.append(&note("Lyra couldn't read the lyrics.")),
                        }
                    }),
                );
            }
            "related" => {
                if *self.related_for.borrow() == related_id && related_id.is_some() {
                    return;
                }
                *self.related_for.borrow_mut() = related_id.clone();
                clear(&self.related);
                match related_id {
                    Some(id) => self.related.append(&pages_for(&id)),
                    None => self.related.append(&note("Nothing related for this song.")),
                }
            }
            _ => {}
        }
        let _ = ui();
    }
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

fn note(text: &str) -> gtk::Label {
    let l = widgets::label(text, "dim-label");
    l.set_xalign(0.0);
    l.set_wrap(true);
    l
}
